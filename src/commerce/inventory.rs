use crate::{App, stores::StoreId};
use sqlx::Row;
pub async fn transition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    store: &StoreId,
    order: &str,
    sold: bool,
) -> anyhow::Result<()> {
    let rows = sqlx::query(
        "SELECT a.sku,a.quantity FROM allocations a
         JOIN products p ON p.store_id=a.store_id AND p.sku=a.sku
         WHERE a.store_id=? AND a.order_id=? AND a.state='reserved'",
    )
    .bind(&store.0)
    .bind(order)
    .fetch_all(&mut **tx)
    .await?;
    for r in rows {
        sqlx::query(
            "UPDATE products SET reserved=reserved-?,sold=sold+? WHERE store_id=? AND sku=?",
        )
        .bind(r.get::<i64, _>("quantity"))
        .bind(if sold { r.get::<i64, _>("quantity") } else { 0 })
        .bind(&store.0)
        .bind(r.get::<String, _>("sku"))
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE allocations SET state=?
             WHERE store_id=? AND order_id=? AND sku=? AND state='reserved'",
        )
        .bind(if sold { "sold" } else { "released" })
        .bind(&store.0)
        .bind(order)
        .bind(r.get::<String, _>("sku"))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
/// Sells the order's stock: the units still reserved for it or, once its window ended
/// and they were released, the same units again if they are all still in stock.
/// Returns whether the order now holds its stock.
pub async fn take(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    store: &StoreId,
    order: &str,
) -> anyhow::Result<bool> {
    let held: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM allocations WHERE store_id=? AND order_id=? AND state='reserved')",
    )
    .bind(&store.0)
    .bind(order)
    .fetch_one(&mut **tx)
    .await?;
    if held {
        transition(tx, store, order, true).await?;
        return Ok(true);
    }
    let short: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM allocations a
         LEFT JOIN products p ON p.store_id=a.store_id AND p.sku=a.sku
         WHERE a.store_id=? AND a.order_id=? AND a.state='released'
           AND (p.sku IS NULL OR p.active=0 OR p.capacity-p.reserved-p.sold<a.quantity))",
    )
    .bind(&store.0)
    .bind(order)
    .fetch_one(&mut **tx)
    .await?;
    if short {
        return Ok(false);
    }
    let rows = sqlx::query(
        "SELECT a.sku,a.quantity FROM allocations a
         JOIN products p ON p.store_id=a.store_id AND p.sku=a.sku
         WHERE a.store_id=? AND a.order_id=? AND a.state='released'",
    )
    .bind(&store.0)
    .bind(order)
    .fetch_all(&mut **tx)
    .await?;
    for r in &rows {
        sqlx::query("UPDATE products SET sold=sold+? WHERE store_id=? AND sku=?")
            .bind(r.get::<i64, _>("quantity"))
            .bind(&store.0)
            .bind(r.get::<String, _>("sku"))
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "UPDATE allocations SET state='sold' WHERE store_id=? AND order_id=? AND sku=?",
        )
        .bind(&store.0)
        .bind(order)
        .bind(r.get::<String, _>("sku"))
        .execute(&mut **tx)
        .await?;
    }
    Ok(true)
}
pub async fn expire(app: &App) -> anyhow::Result<()> {
    let mut tx = crate::db::write(&app.db).await?;
    let orders=sqlx::query("SELECT store_id,id FROM orders WHERE status IN ('creating','awaiting_payment','partial','seen') AND expires_at<=? LIMIT 100").bind(crate::time::now()).fetch_all(&mut *tx).await?;
    for r in orders {
        let store = StoreId(r.get("store_id"));
        let id: String = r.get("id");
        transition(&mut tx, &store, &id, false).await?;
        sqlx::query(
            "UPDATE orders SET status=CASE WHEN EXISTS(
                 SELECT 1 FROM payments WHERE store_id=orders.store_id AND order_id=orders.id)
             THEN 'manual_review' ELSE 'expired' END WHERE store_id=? AND id=?",
        )
        .bind(&store.0)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
