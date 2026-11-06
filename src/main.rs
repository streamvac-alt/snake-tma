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
struct CreateOrderPayload { user_id: i64, user_name: String, items: Vec<OrderItemPayload>, comment: Option<String> }
#[derive(Debug, Deserialize)]
struct OrderItemPayload { variant_id: i32, qty: i32 }
#[derive(Debug, Deserialize)]
struct AdminQuery { key: String }
#[derive(Debug, Deserialize)]
struct StatusPayload { id: String, status: String }
#[derive(Debug, Deserialize)]
struct StopPayload { variant_id: i32, is_stopped: bool }

struct AppState { bot_token: String, admin_chat_id: String, admin_key: String, db: PgPool }

async fn get_menu(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cats: Vec<(i32, String)> = sqlx::query_as("SELECT id, name FROM categories ORDER BY sort_order ASC").fetch_all(&state.db).await.unwrap_or_default();
    let mut categories = Vec::new();
    for c in cats { categories.push(serde_json::json!({ "id": c.0, "name": c.1 })); }

    let prods: Vec<(i32, i32, String, Option<String>, Option<String>)> = sqlx::query_as("SELECT id, category_id, name, description, image_url FROM products WHERE is_archived = false").fetch_all(&state.db).await.unwrap_or_default();
    let mut products = Vec::new();
    for p in prods {
        let vars: Vec<(i32, String, i32)> = sqlx::query_as("SELECT id, name, price FROM product_variants WHERE product_id = $1 AND is_stopped = false").bind(p.0).fetch_all(&state.db).await.unwrap_or_default();
        let mut variants = Vec::new();
        for v in vars { variants.push(serde_json::json!({ "id": v.0, "name": v.1, "price": v.2 })); }
        products.push(serde_json::json!({ "id": p.0, "category_id": p.1, "name": p.2, "description": p.3, "image_url": p.4, "variants": variants }));
    }
    (StatusCode::OK, Json(serde_json::json!({ "categories": categories, "products": products })))
}

async fn create_order(State(state): State<Arc<AppState>>, Json(payload): Json<CreateOrderPayload>) -> impl IntoResponse {
    let shift: Option<i32> = sqlx::query_scalar("SELECT id FROM shifts WHERE is_open = true LIMIT 1").fetch_optional(&state.db).await.unwrap_or(None);
    let shift_id = match shift { Some(id) => id, None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": "Смена закрыта" }))) };

    let _ = sqlx::query("INSERT INTO profiles (id, first_name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET first_name = EXCLUDED.first_name").bind(payload.user_id).bind(&payload.user_name).execute(&state.db).await;
    let row: (i32,) = sqlx::query_as("SELECT COALESCE(MAX(order_number), 0) + 1 FROM orders WHERE shift_id = $1").bind(shift_id).fetch_one(&state.db).await.unwrap();
    let order_num = row.0;
    let order_id = Uuid::new_v4();
    let comment_text = payload.comment.unwrap_or_default();
    
    let _ = sqlx::query("INSERT INTO orders (id, order_number, user_id, shift_id, status, comment) VALUES ($1, $2, $3, $4, 'new', $5)").bind::<Uuid>(order_id).bind(order_num).bind(payload.user_id).bind(shift_id).bind(&comment_text).execute(&state.db).await;

    let mut total = 0;
    let mut items_text = String::new();
    for item in payload.items {
        let var: Option<(String, String, i32)> = sqlx::query_as("SELECT p.name, v.name, v.price FROM product_variants v JOIN products p ON v.product_id = p.id WHERE v.id = $1").bind(item.variant_id).fetch_optional(&state.db).await.unwrap_or(None);
        if let Some(v) = var {
            total += v.2 * item.qty;
            items_text.push_str(&format!("▪️ {} ({}) x{} — {} GEL\n", v.0, v.1, item.qty, v.2 * item.qty));
            let _ = sqlx::query("INSERT INTO order_items (order_id, variant_id, product_name, variant_name, price, qty) VALUES ($1, $2, $3, $4, $5, $6)").bind::<Uuid>(order_id).bind(item.variant_id).bind(&v.0).bind(&v.1).bind(v.2).bind(item.qty).execute(&state.db).await;
        }
    }
    let _ = sqlx::query("UPDATE orders SET total_price = $1 WHERE id = $2").bind(total).bind::<Uuid>(order_id).execute(&state.db).await;

    let comment_display = if comment_text.is_empty() { "".to_string() } else { format!("📝 **Комментарий:** {}", comment_text) };
    let msg = format!("🔥 **НОВЫЙ ЗАКАЗ №{}**\n\n👤 **Гость:** {}\n🛒 **Состав:**\n{}\n💰 **Итого:** {} GEL\n{}", order_num, payload.user_name, items_text, total, comment_display);
    send_tg(&state.bot_token, &state.admin_chat_id, &msg).await;

    (StatusCode::OK, Json(serde_json::json!({ "ok": true, "order_number": order_num })))
}

async fn get_active_orders(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let orders: Vec<(Uuid, i32, i64, String, Option<i32>, Option<String>)> = sqlx::query_as("SELECT id, order_number, user_id, status, total_price, comment FROM orders WHERE status IN ('new', 'accepted', 'ready') ORDER BY created_at ASC").fetch_all(&state.db).await.unwrap_or_default();
    let mut result = Vec::new();
    for o in orders {
        let items: Vec<(String, String, i32, i32)> = sqlx::query_as("SELECT product_name, variant_name, price, qty FROM order_items WHERE order_id = $1").bind(o.0).fetch_all(&state.db).await.unwrap_or_default();
        let mut items_json = Vec::new();
        for i in items { items_json.push(serde_json::json!({ "product_name": i.0, "variant_name": i.1, "price": i.2, "qty": i.3 })); }
        result.push(serde_json::json!({ "id": o.0, "order_number": o.1, "user_id": o.2, "status": o.3, "total_price": o.4, "comment": o.5, "items": items_json }));
    }
    (StatusCode::OK, Json(serde_json::json!(result)))
}

async fn get_shift_status(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>) -> impl IntoResponse {
    if query.key != state.admin_key { return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "is_open": false }))); }
    let current: Option<(i32, bool)> = sqlx::query_as("SELECT id, is_open FROM shifts WHERE is_open = true LIMIT 1").fetch_optional(&state.db).await.unwrap_or(None);
    (StatusCode::OK, Json(serde_json::json!({ "is_open": current.is_some() })))
}

async fn toggle_shift(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>) -> impl IntoResponse {
    if query.key != state.admin_key { return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "ok": false }))); }
    let current: Option<(i32, bool)> = sqlx::query_as("SELECT id, is_open FROM shifts WHERE is_open = true LIMIT 1").fetch_optional(&state.db).await.unwrap_or(None);
    if let Some(shift) = current {
        let _ = sqlx::query("UPDATE shifts SET is_open = false, closed_at = now() WHERE id = $1").bind(shift.0).execute(&state.db).await;
        let _ = sqlx::query("INSERT INTO shifts (is_open) VALUES (false)").execute(&state.db).await;
    } else {
        let _ = sqlx::query("UPDATE shifts SET is_open = true, opened_at = now() WHERE is_open = false ORDER BY id DESC LIMIT 1").execute(&state.db).await;
    }
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn update_status(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>, Json(payload): Json<StatusPayload>) -> impl IntoResponse {
    if query.key != state.admin_key { return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "ok": false }))); }
    let order_id = Uuid::parse_str(&payload.id).unwrap();
    let _ = sqlx::query("UPDATE orders SET status = $1 WHERE id = $2").bind(&payload.status).bind::<Uuid>(order_id).execute(&state.db).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn delete_order(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>, Json(payload): Json<StatusPayload>) -> impl IntoResponse {
    if query.key != state.admin_key { return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "ok": false }))); }
    let order_id = Uuid::parse_str(&payload.id).unwrap();
    let _ = sqlx::query("UPDATE orders SET status = 'deleted' WHERE id = $1").bind::<Uuid>(order_id).execute(&state.db).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn toggle_stop(State(state): State<Arc<AppState>>, Query(query): Query<AdminQuery>, Json(payload): Json<StopPayload>) -> impl IntoResponse {
    if query.key != state.admin_key { return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "ok": false }))); }
    let _ = sqlx::query("UPDATE product_variants SET is_stopped = $1 WHERE id = $2").bind(payload.is_stopped).bind(payload.variant_id).execute(&state.db).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn send_tg(token: &str, chat_id: &str, text: &str) {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
    let _ = reqwest::Client::new().post(&url).json(&serde_json::json!({ "chat_id": chat_id, "text": text, "parse_mode": "Markdown" })).send().await;
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let connection_string = env::var("SUPABASE_URL").expect("Нет SUPABASE_URL");
    let db = match PgPoolOptions::new().max_connections(5).connect(&connection_string).await {
        Ok(pool) => pool,
        Err(e) => { tracing::error!("ОШИБКА БД: {}", e); std::process::exit(1); }
    };
    let state = Arc::new(AppState {
        bot_token: env::var("BOT_TOKEN").unwrap_or_default(),
        admin_chat_id: env::var("ADMIN_CHAT_ID").unwrap_or_default(),
        admin_key: env::var("ADMIN_KEY").unwrap_or_else(|_| "boss123".to_string()),
        db,
    });

    let app = Router::new()
        .route("/api/menu", get(get_menu))
        .route("/api/order", post(create_order))
        .route("/api/orders", get(get_active_orders))
        .route("/api/shift", post(toggle_shift))
        .route("/api/shift/status", get(get_shift_status))
        .route("/api/order/status", post(update_status))
        .route("/api/order/delete", post(delete_order))
        .route("/api/variant/stop", post(toggle_stop))
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    tracing::info!("Сервер запущен на порту {}", port);
    axum::serve(listener, app).await.unwrap();
}
