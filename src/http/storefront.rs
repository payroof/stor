use crate::{App, stores};

use super::{CheckedJson, error::Result};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    quantity: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkout {
    asset: String,
    skus: Vec<String>,
    altcha: altcha::Payload,
}
// Merchant styles are served only on that merchant's storefront origin.
// The shared CSP disallows third-party fonts, imports and connections. Images may
// come from any HTTPS origin, as product photos do.
pub async fn stylesheet(
    State(app): State<App>,
    h: HeaderMap,
) -> Result<impl axum::response::IntoResponse> {
    let store = stores::from_host(&app, &h).await?;
    let raw: String = sqlx::query_scalar("SELECT metadata_json FROM stores WHERE id=?")
        .bind(&store.0)
        .fetch_one(&app.db)
        .await?;
    let metadata: crate::commerce::catalog::Metadata = serde_json::from_str(&raw)?;
    Ok((
        [("content-type", "text/css; charset=utf-8")],
        metadata.style,
    ))
}
// Locks the page's theme before first paint when the merchant chose `light` or `dark`.
// It is empty for `auto`. Loaded in the head, ahead of /theme.js.
pub async fn theme_script(
    State(app): State<App>,
    h: HeaderMap,
) -> Result<impl axum::response::IntoResponse> {
    let store = stores::from_host(&app, &h).await?;
    let raw: String = sqlx::query_scalar("SELECT metadata_json FROM stores WHERE id=?")
        .bind(&store.0)
        .fetch_one(&app.db)
        .await?;
    let metadata: crate::commerce::catalog::Metadata = serde_json::from_str(&raw)?;
    let script = match metadata.theme.as_str() {
        theme @ ("light" | "dark") => {
            format!("document.documentElement.dataset.lock = \"{theme}\";\n")
        }
        _ => String::new(),
    };
    Ok(([("content-type", "text/javascript; charset=utf-8")], script))
}
pub async fn catalog(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let store = stores::from_host(&app, &h).await?;
    let raw: String = sqlx::query_scalar("SELECT metadata_json FROM stores WHERE id=?")
        .bind(&store.0)
        .fetch_one(&app.db)
        .await?;
    let metadata: crate::commerce::catalog::Metadata = serde_json::from_str(&raw)?;
    let rows = sqlx::query(
        "SELECT sku,title,description,price,payments_json,images_json,
                capacity-reserved-sold AS available
         FROM products WHERE store_id=? AND active=1 ORDER BY sku",
    )
    .bind(&store.0)
    .fetch_all(&app.db)
    .await?;
    let decimals = crate::payroof::client::store_decimals(&app, &store).await?;
    let mut products = Vec::new();
    for row in rows {
        let payments =
            crate::commerce::catalog::persisted_payments(row.get("payments_json"), &decimals)?;
        products.push(json!({
            "sku": row.get::<String,_>("sku"),
            "title": row.get::<String,_>("title"),
            "description": row.get::<String,_>("description"),
            "price": row.get::<String,_>("price"),
            "images": serde_json::from_str::<Value>(row.get("images_json"))?,
            "payment_options": payments, "available": row.get::<i64,_>("available")
        }));
    }
    Ok(Json(
        json!({"store":{"title":metadata.title,"description":metadata.description,"website":metadata.website,"theme":metadata.theme},"status":stores::status(&app,&store).await?,"products":products,
            "assets":crate::payroof::client::store_display_assets(&app,&store).await?}),
    ))
}
pub async fn cart(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let s = stores::from_host(&app, &h).await?;
    let c = crate::auth::customer(&app, &s, &h).await?;
    Ok(Json(crate::commerce::cart::view(&app, &s, &c).await?))
}
pub async fn quantity(
    State(app): State<App>,
    Path(sku): Path<String>,
    h: HeaderMap,
    CheckedJson(b): CheckedJson<Quantity>,
) -> Result<Json<Value>> {
    let s = stores::from_host(&app, &h).await?;
    let c = crate::auth::customer(&app, &s, &h).await?;
    Ok(Json(
        crate::commerce::cart::set(&app, &s, &c, &sku, Some(b.quantity)).await?,
    ))
}
pub async fn remove(
    State(app): State<App>,
    Path(sku): Path<String>,
    h: HeaderMap,
) -> Result<Json<Value>> {
    let s = stores::from_host(&app, &h).await?;
    let c = crate::auth::customer(&app, &s, &h).await?;
    Ok(Json(
        crate::commerce::cart::set(&app, &s, &c, &sku, None).await?,
    ))
}
pub async fn clear(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let s = stores::from_host(&app, &h).await?;
    let c = crate::auth::customer(&app, &s, &h).await?;
    Ok(Json(
        crate::commerce::cart::set(&app, &s, &c, "", None).await?,
    ))
}
pub async fn checkout(
    State(app): State<App>,
    h: HeaderMap,
    CheckedJson(b): CheckedJson<Checkout>,
) -> Result<Json<Value>> {
    let s = stores::from_host(&app, &h).await?;
    let c = crate::auth::customer(&app, &s, &h).await?;
    Ok(Json(
        crate::commerce::order::checkout(&app, &s, &c, &b.asset, &b.skus, &b.altcha).await?,
    ))
}
pub async fn order(
    State(app): State<App>,
    Path(id): Path<String>,
    h: HeaderMap,
) -> Result<Json<Value>> {
    let s = stores::from_host(&app, &h).await?;
    let c = crate::auth::customer(&app, &s, &h).await?;
    Ok(Json(crate::commerce::order::view(&app, &s, &c, &id).await?))
}
