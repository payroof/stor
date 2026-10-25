use crate::{
    App,
    http::error::{Error, Result},
    stores::StoreId,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use num_bigint::BigUint;
use serde::Deserialize;
use sqlx::Row;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub height: String,
    pub hash: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: String,
    pub event: String,
    pub project: String,
    pub reference: String,
    pub chain: String,
    pub asset: String,
    pub address: String,
    pub transfer: String,
    pub transaction: String,
    pub amount: String,
    pub block: Block,
}
pub async fn receive(
    State(app): State<App>,
    Path(hook): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> Result<StatusCode> {
    let c = sqlx::query(
        "SELECT c.store_id,c.id,c.origin,c.project,c.webhook_ciphertext FROM connections c
         JOIN stores s ON s.id=c.store_id WHERE c.hook_id=? AND s.status='active'",
    )
    .bind(&hook)
    .fetch_optional(&app.db)
    .await?
    .ok_or_else(Error::not_found)?;
    let store = StoreId(c.get("store_id"));
    let connection: String = c.get("id");
    let timestamp: i64 = h
        .get("payroof-timestamp")
        .and_then(|s| s.to_str().ok())
        .ok_or_else(Error::unauthorized)?
        .parse()
        .map_err(|_| Error::unauthorized())?;
    let signature = h
        .get("payroof-signature")
        .and_then(|s| s.to_str().ok())
        .ok_or_else(Error::unauthorized)?;
    let secret = crate::crypto::open(
        &app.master,
        super::client::aad(&store, &connection, "webhook").as_bytes(),
        c.get("webhook_ciphertext"),
    )?;
    let key = super::signature::decode_webhook_secret(
        std::str::from_utf8(&secret).map_err(|_| Error::internal())?,
    )?;
    super::signature::verify_webhook(&key, timestamp, &body, signature, crate::time::now())
        .map_err(|_| Error::unauthorized())?;
    let event: Event = serde_json::from_slice(&body).map_err(|_| Error::bad("invalid_event"))?;
    if !matches!(event.event.as_str(), "seen" | "confirmed")
        || h.get("payroof-id").and_then(|s| s.to_str().ok()) != Some(&event.id)
        || event.project != c.get::<String, _>("project")
    {
        return Err(Error::bad("invalid_event"));
    }
    if event.transfer.is_empty()
        || event.transfer.len() > 128
        || event.transfer.chars().any(char::is_control)
    {
        return Err(Error::bad("invalid_event"));
    }
    let amount =
        crate::commerce::amount::atomic(&event.amount).map_err(|_| Error::bad("invalid_amount"))?;
    let _height: u64 = event
        .block
        .height
        .parse()
        .map_err(|_| Error::bad("invalid_block"))?;
    let mut tx = crate::db::write(&app.db).await?;
    let previous: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT body_hash FROM received_events WHERE store_id=? AND connection_id=? AND id=?",
    )
    .bind(&store.0)
    .bind(&connection)
    .bind(&event.id)
    .fetch_optional(&mut *tx)
    .await?;
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(&body).to_vec();
    if let Some(previous) = previous {
        if previous != digest {
            return Err(Error::conflict("event_identity_collision"));
        }
        return Ok(StatusCode::NO_CONTENT);
    }
    // Any order of this project: a store that changed its key keeps one webhook per
    // project, and older orders settle through the newer one.
    let order = sqlx::query(
        "SELECT o.id,o.status,o.chain,o.asset,o.address,o.expected_amount FROM orders o
         JOIN connections c ON c.store_id=o.store_id AND c.id=o.connection_id
         WHERE o.store_id=? AND o.reference=? AND c.origin=? AND c.project=?",
    )
    .bind(&store.0)
    .bind(&event.reference)
    .bind(c.get::<String, _>("origin"))
    .bind(c.get::<String, _>("project"))
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(r) = order {
        let id: String = r.get("id");
        let status: String = r.get("status");
        if status == "creating" {
            return Err(Error(StatusCode::SERVICE_UNAVAILABLE, "order_initializing"));
        }
        let matches = r.get::<String, _>("chain") == event.chain
            && r.get::<String, _>("asset") == event.asset
            && r.get::<Option<String>, _>("address").as_deref() == Some(&event.address);
        if matches {
            sqlx::query(
                "INSERT INTO payments(store_id,order_id,transfer,amount,confirmed) VALUES(?,?,?,?,?)
                 ON CONFLICT(store_id,order_id,transfer)
                 DO UPDATE SET confirmed=MAX(confirmed,excluded.confirmed)",
            )
            .bind(&store.0)
            .bind(&id)
            .bind(&event.transfer)
            .bind(amount.to_string())
            .bind(event.event == "confirmed")
            .execute(&mut *tx)
            .await?;
        }
        if matches && status != "delivered" {
            let expected = crate::commerce::amount::atomic(r.get("expected_amount"))?;
            let mut received = BigUint::default();
            let mut confirmed = BigUint::default();
            for p in
                sqlx::query("SELECT amount,confirmed FROM payments WHERE store_id=? AND order_id=?")
                    .bind(&store.0)
                    .bind(&id)
                    .fetch_all(&mut *tx)
                    .await?
            {
                let n = crate::commerce::amount::atomic(p.get("amount"))?;
                if p.get::<bool, _>("confirmed") {
                    confirmed = crate::commerce::amount::add(confirmed, n.clone())?;
                }
                received = crate::commerce::amount::add(received, n)?;
            }
            if confirmed >= expected {
                let taken = crate::commerce::inventory::take(&mut tx, &store, &id).await?;
                sqlx::query(
                    "UPDATE orders SET status=?,transaction_id=? WHERE store_id=? AND id=?",
                )
                .bind(if taken { "delivered" } else { "late_payment" })
                .bind(&event.transaction)
                .bind(&store.0)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            } else {
                let next = if received >= expected
                    && matches!(status.as_str(), "awaiting_payment" | "partial")
                {
                    "seen"
                } else if received > BigUint::default() && status == "awaiting_payment" {
                    "partial"
                } else {
                    ""
                };
                if !next.is_empty() {
                    sqlx::query("UPDATE orders SET status=? WHERE store_id=? AND id=?")
                        .bind(next)
                        .bind(&store.0)
                        .bind(&id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
    }
    sqlx::query(
        "INSERT INTO received_events(store_id,connection_id,id,body_hash,received_at)
         VALUES(?,?,?,?,?)",
    )
    .bind(&store.0)
    .bind(&connection)
    .bind(&event.id)
    .bind(digest)
    .bind(crate::time::now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
