use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::env;
use tower_http::services::ServeDir;

#[derive(Debug, Deserialize)]
struct OrderItem {
    name: String,
    price: u32,
    qty: u32,
}

#[derive(Debug, Deserialize)]
struct OrderPayload {
    table: String,
    items: Vec<OrderItem>,
}

#[derive(Debug, Serialize)]
struct OrderResponse {
    ok: bool,
}

async fn make_order(Json(payload): Json<OrderPayload>) -> impl IntoResponse {
    let bot_token = env::var("BOT_TOKEN").unwrap_or_default();
    let admin_chat_id = env::var("ADMIN_CHAT_ID").unwrap_or_default();

    if bot_token.is_empty() || admin_chat_id.is_empty() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(OrderResponse { ok: false }));
    }

    let mut total_price = 0;
    let mut items_text = String::new();

    for item in payload.items {
        let sum = item.price * item.qty;
        total_price += sum;
        items_text.push_str(&format!("▪️ {} x{} — {} руб.\n", item.name, item.qty, sum));
    }

    let message = format!(
        "🔥 **НОВЫЙ ЗАКАЗ!**\n\n📍 **Столик:** {}\n\n🛒 **Состав:**\n{}\n💰 **Итого к оплате:** {} руб.",
        payload.table, items_text, total_price
    );

    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
    let client = reqwest::Client::new();
    
    let _ = client.post(&url)
        .json(&serde_json::json!({
            "chat_id": admin_chat_id,
            "text": message,
            "parse_mode": "Markdown"
        }))
        .send()
        .await;

    (StatusCode::OK, Json(OrderResponse { ok: true }))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/api/order", post(make_order))
        .fallback_service(ServeDir::new("static"));

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    
    tracing::info!("Сервер запущен на порту {}", port);
    axum::serve(listener, app).await.unwrap();
}
