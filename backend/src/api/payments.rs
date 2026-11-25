//! Stars & Payments (v1.2).
//!
//! - Produk: barang digital yang dijual bot (Stars atau fiat).
//! - Invoice: `sendInvoice` dengan `provider_token` "" (wajib kosong untuk
//!   Stars). v1.2 TIDAK menyimpan provider token nyata untuk fiat —
//!   invoice non-XTR tetap dikirim dengan provider_token "" dan
//!   didokumentasikan sebagai keterbatasan (lihat README v1.2); pembayaran
//!   fiat nyata membutuhkan provider payment token dari BotFather.
//! - `pre_checkout_query` dijawab ok=true; `successful_payment` menandai
//!   order lunas (lihat update_processor).

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::bot::get_bot_token;
use crate::bot::telegram::SendInvoiceParams;
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, Order, Product};
use crate::state::AppState;

/// Payload invoice: gabungan product payload + id order (unik per order).
/// Parsing kembali ke product payload: bagian sebelum `#` terakhir.
/// Dipakai update_processor saat successful_payment tiba.
pub fn build_invoice_payload(product_payload: &str, order_id: &str) -> String {
    format!("{product_payload}#{order_id}")
}

#[allow(dead_code)]
pub fn product_payload_from_invoice(invoice_payload: &str) -> &str {
    match invoice_payload.rfind('#') {
        Some(i) => &invoice_payload[..i],
        None => invoice_payload,
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateProductRequest {
    pub bot_id: String,
    pub title: String,
    pub description: Option<String>,
    /// Bila kosong: digenerate `product_<uuid>`.
    pub payload: Option<String>,
    pub price: i64,
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProductRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub price: Option<i64>,
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListProductsQuery {
    pub bot_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListOrdersQuery {
    pub bot_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceRequest {
    #[serde(deserialize_with = "crate::models::de_chat_id")]
    pub chat_id: i64,
}

async fn fetch_product(state: &AppState, id: &str) -> AppResult<Product> {
    sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("produk {id} tidak ditemukan")))
}

pub async fn list_products(
    State(state): State<AppState>,
    Query(q): Query<ListProductsQuery>,
) -> AppResult<Json<Envelope<Vec<Product>>>> {
    let products = match &q.bot_id {
        Some(b) => {
            sqlx::query_as::<_, Product>(
                "SELECT * FROM products WHERE bot_id = ? ORDER BY created_at DESC",
            )
            .bind(b)
            .fetch_all(&state.pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Product>("SELECT * FROM products ORDER BY created_at DESC")
                .fetch_all(&state.pool)
                .await?
        }
    };
    Ok(Envelope::ok(products))
}

pub async fn create_product(
    State(state): State<AppState>,
    Json(req): Json<CreateProductRequest>,
) -> AppResult<Json<Envelope<Product>>> {
    if req.title.trim().is_empty() {
        return Err(AppError::Validation("title tidak boleh kosong".to_string()));
    }
    if req.price <= 0 {
        return Err(AppError::Validation("price harus > 0".to_string()));
    }
    let currency = req.currency.unwrap_or_else(|| "XTR".to_string());
    if currency.trim().is_empty() {
        return Err(AppError::Validation("currency tidak boleh kosong".to_string()));
    }
    // Bot harus ada.
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM bots WHERE id = ?")
        .bind(&req.bot_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("bot {} tidak ditemukan", req.bot_id)));
    }
    let payload = req
        .payload
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| format!("product_{}", new_id()));
    let now = now_rfc3339();
    let id = new_id();
    let res = sqlx::query(
        "INSERT INTO products (id, bot_id, title, description, payload, price, currency, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.bot_id)
    .bind(req.title.trim())
    .bind(req.description.as_deref())
    .bind(&payload)
    .bind(req.price)
    .bind(&currency)
    .bind(&now)
    .execute(&state.pool)
    .await;
    if let Err(e) = res {
        if e.to_string().contains("UNIQUE") {
            return Err(AppError::Conflict(format!(
                "payload '{payload}' sudah dipakai produk lain"
            )));
        }
        return Err(e.into());
    }
    Ok(Envelope::ok(fetch_product(&state, &id).await?))
}

pub async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProductRequest>,
) -> AppResult<Json<Envelope<Product>>> {
    let product = fetch_product(&state, &id).await?;
    if let Some(p) = req.price {
        if p <= 0 {
            return Err(AppError::Validation("price harus > 0".to_string()));
        }
    }
    sqlx::query(
        "UPDATE products SET title = ?, description = ?, price = ?, currency = ? WHERE id = ?",
    )
    .bind(req.title.as_deref().unwrap_or(&product.title))
    .bind(req.description.or(product.description))
    .bind(req.price.unwrap_or(product.price))
    .bind(req.currency.unwrap_or(product.currency))
    .bind(&id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(fetch_product(&state, &id).await?))
}

pub async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_product(&state, &id).await?;
    sqlx::query("DELETE FROM products WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

pub async fn list_orders(
    State(state): State<AppState>,
    Query(q): Query<ListOrdersQuery>,
) -> AppResult<Json<Envelope<Vec<Order>>>> {
    let mut qb = sqlx::QueryBuilder::new("SELECT * FROM orders WHERE 1=1");
    if let Some(b) = &q.bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(s) = &q.status {
        qb.push(" AND status = ").push_bind(s);
    }
    qb.push(" ORDER BY created_at DESC LIMIT 200");
    let orders = qb.build_query_as::<Order>().fetch_all(&state.pool).await?;
    Ok(Envelope::ok(orders))
}

pub async fn create_invoice(
    State(state): State<AppState>,
    Path(product_id): Path<String>,
    Json(req): Json<CreateInvoiceRequest>,
) -> AppResult<Json<Envelope<Order>>> {
    let product = fetch_product(&state, &product_id).await?;
    let token = get_bot_token(&state, &product.bot_id).await?;
    let order_id = new_id();
    let now = now_rfc3339();
    let invoice_payload = build_invoice_payload(&product.payload, &order_id);
    let sent = state
        .telegram
        .send_invoice(&token, &SendInvoiceParams {
            chat_id: req.chat_id,
            title: product.title.clone(),
            description: product.description.clone().unwrap_or_default(),
            payload: invoice_payload.clone(),
            provider_token: String::new(), // wajib "" untuk Stars
            currency: product.currency.clone(),
            prices: vec![json!({"label": product.title, "amount": product.price})],
        })
        .await?;
    let _ = sent;
    sqlx::query(
        "INSERT INTO orders (id, bot_id, product_id, chat_id, invoice_payload, amount, currency, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(&order_id)
    .bind(&product.bot_id)
    .bind(&product.id)
    .bind(req.chat_id.to_string())
    .bind(&invoice_payload)
    .bind(product.price)
    .bind(&product.currency)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let order = sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE id = ?")
        .bind(&order_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Envelope::ok(order))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_payload_round_trip() {
        let payload = build_invoice_payload("product_abc", "order-123");
        assert_eq!(payload, "product_abc#order-123");
        assert_eq!(product_payload_from_invoice(&payload), "product_abc");
        // Payload lama tanpa '#' tetap kembali utuh.
        assert_eq!(product_payload_from_invoice("product_abc"), "product_abc");
    }

    #[test]
    fn successful_payment_json_parses() {
        // Bentuk successful_payment dari Telegram Bot API.
        let v: serde_json::Value = serde_json::from_str(
            r#"{"currency":"XTR","total_amount":50,"invoice_payload":"product_abc#order-123",
                "telegram_payment_charge_id":"charge_1","provider_payment_charge_id":"prov_1"}"#,
        )
        .unwrap();
        assert_eq!(v["currency"], "XTR");
        assert_eq!(v["total_amount"], 50);
        assert_eq!(
            product_payload_from_invoice(v["invoice_payload"].as_str().unwrap()),
            "product_abc"
        );
    }

    #[test]
    fn pre_checkout_query_json_parses() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"id":"query-1","from":{"id":111,"is_bot":false,"first_name":"Astr"},
                "currency":"XTR","total_amount":50,"invoice_payload":"product_abc#order-123"}"#,
        )
        .unwrap();
        assert_eq!(v["id"], "query-1");
        assert_eq!(v["invoice_payload"], "product_abc#order-123");
    }
}
