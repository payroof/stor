use crate::{
    App,
    http::error::{Error, Result},
    stores::StoreId,
};
use axum::http::HeaderMap;
/// The value of the cookie `name` in the request, if present.
pub fn cookie(h: &HeaderMap, name: &str) -> Option<String> {
    h.get("cookie")?.to_str().ok()?.split(';').find_map(|s| {
        let (k, v) = s.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}
/// The customer cookie's name. Outside development the `__Host-` prefix makes browsers
/// accept it only as a secure, host-only cookie.
pub fn customer_cookie(app: &App) -> &'static str {
    if app.config.development {
        "stor_customer"
    } else {
        "__Host-stor_customer"
    }
}
/// The storefront's customer: a random ID in a host-only cookie, stored hashed.
pub async fn customer(app: &App, store: &StoreId, h: &HeaderMap) -> Result<Vec<u8>> {
    let c = cookie(h, customer_cookie(app)).ok_or_else(Error::unauthorized)?;
    let hash = crate::crypto::hash(&c);
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customers WHERE store_id=? AND id_hash=?)")
            .bind(&store.0)
            .bind(&hash)
            .fetch_one(&app.db)
            .await?;
    if !exists {
        return Err(Error::unauthorized());
    }
    Ok(hash)
}
