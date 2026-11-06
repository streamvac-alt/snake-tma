use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::{Arc, Mutex}};
use std::env;
use tower_http::services::ServeDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderItem {
    name: String,
    price: u32,
    qty: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Order {
    id: String,
    user_id: i64,
    user_name: String,
    items: Vec<OrderItem>,
    total: u32,
    status: String, // "new", "accepted", "done"
}

#[derive(Debug, Deserialize)]
struct OrderPayload {
    user_id: i64,
    user_name: String,
    items: Vec<OrderItem>,
}

#[derive(Debug, Serialize)]
struct OrderResponse { ok: bool }

#[derive(Debug, Deserialize)]
struct StatusPayload { id: String, status: String }

struct AppState {
    bot_token: String,
    admin_chat_id: String,
    orders: Mutex<HashMap<String, Order>>,
}

async fn make_order(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OrderPayload>,
) -> impl IntoResponse {
    let total: u32 = payload.items.iter().map(|i| i.price * i.qty).sum();
    let id = format!("{}_{}", chrono::Utc::now().timestamp_millis(), payload.user_id);
    
    let order = Order {
        id: id.clone(),
        user_id: payload.user_id,
        user_name: payload.user_name.clone(),
        items: payload.items.clone(),
        total,
        status: "new".to_string(),
    };

    state.orders.lock().unwrap().insert(id.clone(), order);

    let mut items_text = String::new();
    for item in &payload.items {
        items_text.push_str(&format!("▪️ {} x{} — {} руб.\n", item.name, item.qty, item.price * item.qty));
    }

    let message = format!(
        "🔥 **НОВЫЙ ЗАКАЗ**\n\n👤 **Гость:** {}\n🛒 **Состав:**\n{}\n💰 **Итого:** {} руб.\n\n_Для смены статуса зайдите в админку._",
        payload.user_name, items_text, total
    );

    send_tg(&state.bot_token, &state.admin_chat_id, &message).await;
    (StatusCode::OK, Json(OrderResponse { ok: true }))
}

async fn get_orders(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let orders = state.orders.lock().unwrap();
    Json(orders.values().cloned().collect::<Vec<Order>>())
}

async fn update_status(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StatusPayload>,
) -> impl IntoResponse {
    let mut orders = state.orders.lock().unwrap();
    if let Some(order) = orders.get_mut(&payload.id) {
        order.status = payload.status.clone();
    }
    (StatusCode::OK, Json(OrderResponse { ok: true }))
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
    let state = Arc::new(AppState {
        bot_token: env::var("BOT_TOKEN").unwrap_or_default(),
        admin_chat_id: env::var("ADMIN_CHAT_ID").unwrap_or_default(),
        orders: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/api/order", post(make_order))
        .route("/api/orders", get(get_orders))
        .route("/api/order/status", post(update_status))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
