use crate::{App, stores::StoreId};
use sqlx::Row;

/// Every second: expired payment windows, orders whose payment address is still being
/// created, and expired anti-abuse records.
async fn commerce_tick(app: &App) -> anyhow::Result<()> {
    crate::commerce::inventory::expire(app).await?;
    let rows = sqlx::query(
        "SELECT store_id,id,connection_id FROM orders WHERE status='creating' AND created_at<? LIMIT 10",
    )
    .bind(crate::time::now() - 30)
    .fetch_all(&app.db)
    .await?;
    for r in rows {
        let store = StoreId(r.get("store_id"));
        let result = async {
            let c = crate::payroof::client::by_id(app, &store, r.get("connection_id")).await?;
            crate::commerce::order::map(app, &store, r.get("id"), &c).await
        }
        .await;
        if result.is_err() {
            tracing::warn!(
                worker = "commerce",
                operation = "map",
                "order mapping pending"
            );
        }
    }
    sqlx::query("DELETE FROM altcha_replay WHERE expires_at<?")
        .bind(crate::time::now() - 300)
        .execute(&app.db)
        .await?;
    sqlx::query("DELETE FROM rate_limits WHERE expires_at<?")
        .bind(crate::time::now() - 300)
        .execute(&app.db)
        .await?;
    Ok(())
}
pub fn start(app: App) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if commerce_tick(&app).await.is_err() {
                tracing::error!(worker = "commerce", "worker tick unavailable");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    })
}

/// Stores due for a sync, a few at a time and never one store twice at once. Then the
/// History rows waiting to be appended.
async fn sync_tick(app: &App) -> anyhow::Result<()> {
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM stores WHERE status='active' AND next_sync<=? ORDER BY next_sync LIMIT 32",
    )
    .bind(crate::time::now())
    .fetch_all(&app.db)
    .await?;
    let mut tasks = tokio::task::JoinSet::new();
    for id in due {
        sqlx::query("UPDATE stores SET next_sync=? WHERE id=?")
            .bind(crate::time::now() + app.config.sync_seconds)
            .bind(&id)
            .execute(&app.db)
            .await?;
        let app = app.clone();
        tasks.spawn(async move {
            if crate::source::sync(&app, &StoreId(id)).await.is_err() {
                tracing::warn!(worker = "sync", "source unavailable, last catalog kept");
            }
        });
        while tasks.len() >= 8 {
            tasks.join_next().await;
        }
    }
    tasks.join_all().await;
    crate::source::history::tick(app).await
}
pub fn start_sync(app: App) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if sync_tick(&app).await.is_err() {
                tracing::error!(worker = "sync", "worker tick unavailable");
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    })
}
