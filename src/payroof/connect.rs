//! The store's connection to its payment server, from the source's Payments URL and KEY.
//! A new URL or KEY registers a webhook with the server and becomes the active
//! connection. The previous one is retired but keeps its webhook secret, so its open
//! orders still settle.
use super::client;
use crate::{App, stores::StoreId};
use serde_json::{Value, json};
use sqlx::Row;

/// The server's current view of its tokens, connecting first when the URL or KEY changed.
pub async fn ensure(app: &App, store: &StoreId, url: &str, key: &str) -> anyhow::Result<Value> {
    let (origin, view) = client::server_view(app, url, key).await?;
    let (_, project) = client::server(app, url)?;
    let key_hash = crate::crypto::hash(key);
    let current = sqlx::query(
        "SELECT id,origin,project,key_hash FROM connections WHERE store_id=? AND status='active'",
    )
    .bind(&store.0)
    .fetch_optional(&app.db)
    .await?;
    if let Some(c) = &current
        && c.get::<String, _>("origin") == origin
        && c.get::<String, _>("project") == project
        && c.get::<Vec<u8>, _>("key_hash") == key_hash
    {
        sqlx::query("UPDATE connections SET view_json=? WHERE store_id=? AND id=?")
            .bind(view.to_string())
            .bind(&store.0)
            .bind(c.get::<String, _>("id"))
            .execute(&app.db)
            .await?;
        return Ok(view);
    }
    let id = crate::crypto::id("con_")?;
    let hook = crate::crypto::id("hook_")?;
    let webhook = client::call(
        app,
        &origin,
        key,
        reqwest::Method::POST,
        &format!("/v1/projects/{project}/webhooks"),
        Some(json!({
            "url": format!("{}/v1/webhooks/{hook}", app.config.origin.trim_end_matches('/')),
            "events": ["seen", "confirmed"]
        })),
    )
    .await
    .map_err(|_| anyhow::anyhow!("the KEY can't register payment notifications"))?;
    let webhook_id = webhook["id"]
        .as_str()
        .filter(|w| !w.is_empty() && w.len() <= 128)
        .ok_or_else(|| anyhow::anyhow!("invalid webhook"))?;
    let secret = webhook["secret"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("invalid webhook"))?;
    super::signature::decode_webhook_secret(secret)?;
    let mut tx = crate::db::write(&app.db).await?;
    // Same project, new key: the new webhook reports every order of the project, so
    // the previous one is removed instead of piling up on the server.
    let replaced = current
        .filter(|c| {
            c.get::<String, _>("origin") == origin && c.get::<String, _>("project") == project
        })
        .map(|c| c.get::<String, _>("id"));
    sqlx::query("UPDATE connections SET status='retired' WHERE store_id=? AND status='active'")
        .bind(&store.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO connections
         (store_id,id,origin,project,key_hash,access_ciphertext,webhook_id,webhook_ciphertext,
          hook_id,status,view_json,created_at)
         VALUES(?,?,?,?,?,?,?,?,?,'active',?,?)",
    )
    .bind(&store.0)
    .bind(&id)
    .bind(&origin)
    .bind(&project)
    .bind(key_hash)
    .bind(client::encrypt(app, store, &id, "access", key)?)
    .bind(webhook_id)
    .bind(client::encrypt(app, store, &id, "webhook", secret)?)
    .bind(&hook)
    .bind(view.to_string())
    .bind(crate::time::now())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if let Some(previous) = replaced {
        let old: Option<String> =
            sqlx::query_scalar("SELECT webhook_id FROM connections WHERE store_id=? AND id=?")
                .bind(&store.0)
                .bind(previous)
                .fetch_optional(&app.db)
                .await?;
        if let Some(old) = old {
            let _ = client::call(
                app,
                &origin,
                key,
                reqwest::Method::DELETE,
                &format!("/v1/projects/{project}/webhooks/{old}"),
                None,
            )
            .await;
        }
    }
    Ok(view)
}
