use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower_http::services::ServeDir;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User { id: i64, first_name: String }

#[derive(Debug, Deserialize)]
struct InitDataPayload { init_data: String }

#[derive(Debug, Serialize)]
struct AuthResponse { ok: bool, user: Option<User>, high_score: u32, error: Option<String> }

#[derive(Debug, Deserialize)]
struct ScorePayload { init_data: String, score: u32 }

#[derive(Debug, Serialize)]
struct ScoreResponse { ok: bool, high_score: u32 }

#[derive(Clone)]
struct AppState {
    bot_token: String,
    scores: Arc<Mutex<HashMap<i64, u32>>>,
}

fn validate_init_data(init_data: &str, bot_token: &str) -> Result<User, String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut hash: Option<String> = None;

    for part in init_data.split('&') {
        let mut kv = part.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let value = kv.next().unwrap_or("");
        let key_dec = urlencoding::decode(key).map_err(|e| e.to_string())?.into_owned();
        let val_dec = urlencoding::decode(value).map_err(|e| e.to_string())?.into_owned();

        if key_dec == "hash" { hash = Some(val_dec); } 
        else { pairs.push((key_dec, val_dec)); }
    }

    let hash = hash.ok_or("initData не содержит hash")?;
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let data_check_string = pairs.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("\n");

    let mut secret_mac = HmacSha256::new_from_slice(b"WebAppData").map_err(|e| e.to_string())?;
    secret_mac.update(bot_token.as_bytes());
    let secret_key = secret_mac.finalize().into_bytes();

    let mut mac = HmacSha256::new_from_slice(&secret_key).map_err(|e| e.to_string())?;
    mac.update(data_check_string.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());

    if expected != hash { return Err("Неверная подпись initData".into()); }

    let user_json = pairs.iter().find(|(k, _)| k == "user").map(|(_, v)| v.clone()).ok_or("Нет поля user")?;
    let user: User = serde_json::from_str(&user_json).map_err(|e| e.to_string())?;
    Ok(user)
}

async fn auth(State(state): State<AppState>, Json(payload): Json<InitDataPayload>) -> impl IntoResponse {
    match validate_init_data(&payload.init_data, &state.bot_token) {
        Ok(user) => {
            let high_score = state.scores.lock().unwrap().get(&user.id).copied().unwrap_or(0);
            Json(AuthResponse { ok: true, user: Some(user), high_score, error: None })
        }
        Err(e) => (StatusCode::UNAUTHORIZED, Json(AuthResponse { ok: false, user: None, high_score: 0, error: Some(e) }))
    }
}

async fn submit_score(State(state): State<AppState>, Json(payload): Json<ScorePayload>) -> impl IntoResponse {
    match validate_init_data(&payload.init_data, &state.bot_token) {
        Ok(user) => {
            let mut scores = state.scores.lock().unwrap();
            let entry = scores.entry(user.id).or_insert(0);
            if payload.score > *entry { *entry = payload.score; }
            Json(ScoreResponse { ok: true, high_score: *entry })
        }
        Err(_) => (StatusCode::UNAUTHORIZED, Json(ScoreResponse { ok: false, high_score: 0 }))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let bot_token = std::env::var("BOT_TOKEN").expect("Укажите BOT_TOKEN в переменных окружения");
    
    let state = AppState {
        bot_token,
        scores: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/api/auth", post(auth))
        .route("/api/score", post(submit_score))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    
    tracing::info!("Сервер запущен на порту {}", port);
    axum::serve(listener, app).await.unwrap();
}