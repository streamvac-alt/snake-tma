use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{env, sync::Arc};
use tower_http::services::ServeDir;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct CreateOrderPayload {
    user_id: i64,
    user_name: String,
    items: Vec<OrderItemPayload>,
    comment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderItemPayload {
    variant_id: i32,
    qty: i32,
}

#[derive(Debug, Serialize)]
struct OrderResponse { ok: bool, order_number: Option<i32>, error: Option<String> }

#[derive(Debug, Deserialize)]
struct AdminQuery { key: String }

#[derive(Serialize)]
struct SimpleResponse { ok: bool, data: Option<serde_json::Value> }

struct AppState {
    bot_token: String,
    admin_chat_id: String,
    admin_key: String,
    db: PgPool,
}

async fn create_order(State(state): State<Arc<AppState>>, Json(payload): Json<CreateOrderPayload>) -> impl IntoResponse {
    let shift_res = sqlx::query("SELECT id, is_open FROM shifts WHERE is_open = true LIMIT 1")
        .fetch_optional(&state.db).await;
    
    let shift = match shift_res {
        Ok(Some(s)) => {
            use sqlx::Row;
            s.get::<i32, _>("id")
        },
        _ => return (StatusCode::BAD_REQUEST, Json(OrderResponse { ok: false, order_number: None, error: Some("Смена закрыта".into()) })),
    };

    let _ = sqlx::query("INSERT INTO profiles (id, first_name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET first_name = EXCLUDED.first_name")
        .bind::<i64>(payload.user_id)
        .bind::<String>(payload.user_name.clone())
        .execute(&state.db).await;

    let row: (i32,) = sqlx::query_as("SELECT COALESCE(MAX(order_number), 0) + 1 FROM orders WHERE shift_id = $1")
        .bind::<i32>(shift).fetch_one(&state.db).await.unwrap();
    let order_num = row.0;

    let order_id = Uuid::new_v4();
    let comment_text = payload.comment.clone().unwrap_or_default();
    
    let _ = sqlx::query("INSERT INTO orders (id, order_number, user_id, shift_id, status, comment) VALUES ($1, $2, $3, $4, 'new', $5)")
        .bind::<Uuid>(order_id)
        .bind::<i32>(order_num)
        .bind::<i64>(payload.user_id)
        .bind::<i32>(shift)
        .bind::<String>(comment_text.clone())
        .execute(&state.db).await;

    let mut total = 0;
    let mut items_text = String::new();
    for item in payload.items {
        let var: Option<(String, String, i32)> = sqlx::query_as("SELECT p.name, v.name, v.price FROM product_variants v JOIN products p ON v.product_id = p.id WHERE v.id = $1")
            .bind::<i32>(item.variant_id).fetch_optional(&state.db).await.unwrap();
        
        if let Some(v) = var {
            total += v.2 * item.qty;
            items_text.push_str(&format!("▪️ {} ({}) x{} — {} GEL\n", v.0, v.1, item.qty, v.2 * item.qty));
            
            let _ = sqlx::query("INSERT INTO order_items (order_id, variant_id, product_name, variant_name, price, qty) VALUES ($1, $2, $3, $4, $5, $6)")
                .bind::<Uuid>(order_id)
                .bind::<i32>(item.variant_id)
                .bind::<String>(v.0)
                .bind::<String>(v.1)
                .bind::<i32>(v.2)
                .bind::<i32>(item.qty)
                .execute(&state.db).await;
        }
    }

    let _ = sqlx::query("UPDATE orders SET total_price = $1 WHERE id = $2")
        .bind::<i32>(total)
        .bind::<Uuid>(order_id)
        .execute(&state.db).await;

    let comment_display = if comment_text.is_empty() { "".to_string() } else { format!("📝 **Комментарий:** {}", comment_text) };
    let msg = format!("🔥 **НОВЫЙ ЗАКАЗ №{}**\n\n👤 **Гость:** {}\n🛒 **Состав:**\n{}\n💰 **Итого:** {} GEL\n{}",
        order_num, payload.user_name, items_text, total, comment_display
    );
    send_tg(&state.bot_token, &state.admin_chat_id, &msg).await;

    (StatusCode::OK, Json(OrderResponse { ok: true, order_number: Some(order_num), error: None }))
}

async fn get_active_orders(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let orders: Vec<(Uuid, i32, i64, String, Option<i32>)> = sqlx::query_as("SELECT id, order_number, user_id, status, total_price FROM orders WHERE status IN ('new', 'accepted', 'ready') ORDER BY created_at ASC")
        .fetch_all(&state.db).await.unwrap_or_default();
    
    let mut result = Vec::new();
    for o in orders {
        result.push(serde_json::json!({
            "id": o.0, "order_number": o.1, "user_id": o.2,
            "status": o.3, "total": o.4
        }));
    }
    Json(result)
}

async fn toggle_shift(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>) -> impl IntoResponse {
    if query.key != state.admin_key { return (StatusCode::UNAUTHORIZED, Json(SimpleResponse{ok: false, data: None})); }
    
    let current: Option<(i32, bool)> = sqlx::query_as("SELECT id, is_open FROM shifts WHERE is_open = true LIMIT 1").fetch_optional(&state.db).await.unwrap();
    
    if let Some(shift) = current {
        let _ = sqlx::query("UPDATE shifts SET is_open = false, closed_at = now() WHERE id = $1").bind::<i32>(shift.0).execute(&state.db).await;
        let _ = sqlx::query("INSERT INTO shifts (is_open) VALUES (false)").execute(&state.db).await;
    } else {
        let _ = sqlx::query("UPDATE shifts SET is_open = true, opened_at = now() WHERE is_open = false ORDER BY id DESC LIMIT 1").execute(&state.db).await;
    }
    (StatusCode::OK, Json(SimpleResponse{ok: true, data: None}))
}

#[derive(Deserialize)]
struct StatusPayload { id: String, status: String }
async fn update_status(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>, Json(payload): Json<StatusPayload>) -> impl IntoResponse {
    if query.key != state.admin_key { 
        return (StatusCode::UNAUTHORIZED, Json(SimpleResponse{ok: false, data: None})); 
    }
    
    let _ = sqlx::query("UPDATE orders SET status = $1 WHERE id = $2")
        .bind::<String>(payload.status) 
        .bind::<Uuid>(Uuid::parse_str(&payload.id).unwrap())
        .execute(&state.db).await;
        
    (StatusCode::OK, Json(SimpleResponse{ok: true, data: None}))
}

async fn send_tg(token: &str, chat_id: &str, text: &str) {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    let _ = reqwest::Client::new().post(&url)
        .json(&serde_json::json!({ "chat_id": chat_id, "text": text, "parse_mode": "Markdown" }))
        .send().await;
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let supabase_url = env::var("SUPABASE_URL").expect("Нет SUPABASE_URL");
    let supabase_key = env::var("SUPABASE_KEY").expect("Нет SUPABASE_KEY");
    
    let connection_string = format!("postgresql://postgres:{}@db.{}.supabase.co:5432/postgres", supabase_key, supabase_url.replace("https://", ""));
    
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await.expect("Ошибка подключения к БД");

    let state = Arc::new(AppState {
        bot_token: env::var("BOT_TOKEN").unwrap_or_default(),
        admin_chat_id: env::var("ADMIN_CHAT_ID").unwrap_or_default(),
        admin_key: env::var("ADMIN_KEY").unwrap_or_else(|_| "boss123".to_string()),
        db,
    });

    let app = Router::new()
        .route("/api/order", post(create_order))
        .route("/api/orders", get(get_active_orders))
        .route("/api/shift", post(toggle_shift))
        .route("/api/order/status", post(update_status))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    tracing::info!("Сервер запущен на порту {}", port);
    axum::serve(listener, app).await.unwrap();
}
