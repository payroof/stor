//! The History tab: one row per product line of every paid order, appended once the
//! payment confirms. Stor never rewrites a row there.
use super::{
    Change,
    sheet::{Cell, text},
};
use crate::{App, stores::StoreId, time::now};
use serde_json::Value;
use sqlx::Row;

/// Appends the paid orders waiting for each store's History tab, a batch per store.
pub async fn tick(app: &App) -> anyhow::Result<()> {
    let stores = sqlx::query(
        "SELECT DISTINCT s.id,s.source_kind,s.source_id,s.history_tab FROM history_outbox h
         JOIN stores s ON s.id=h.store_id
         WHERE h.next_attempt<=? AND s.status='active' AND s.history_tab IS NOT NULL LIMIT 20",
    )
    .bind(now())
    .fetch_all(&app.db)
    .await?;
    for r in stores {
        let store = StoreId(r.get("id"));
        let Some(source) = app.source(r.get("source_kind")) else {
            continue;
        };
        let orders: Vec<String> = sqlx::query_scalar(
            "SELECT h.order_id FROM history_outbox h
             JOIN orders o ON o.store_id=h.store_id AND o.id=h.order_id
             WHERE h.store_id=? AND h.next_attempt<=? ORDER BY o.created_at LIMIT 100",
        )
        .bind(&store.0)
        .bind(now())
        .fetch_all(&app.db)
        .await?;
        let mut rows = Vec::new();
        for order in &orders {
            rows.extend(lines(app, &store, order).await?);
        }
        let change = Change::Append {
            tab: r.get("history_tab"),
            rows,
        };
        let id: String = r.get("source_id");
        let appended = source
            .apply(&id, std::slice::from_ref(&change))
            .await
            .is_ok();
        for order in &orders {
            let query = if appended {
                sqlx::query("DELETE FROM history_outbox WHERE store_id=? AND order_id=?")
            } else {
                sqlx::query(
                    "UPDATE history_outbox SET next_attempt=? WHERE store_id=? AND order_id=?",
                )
                .bind(now() + 300)
            };
            query.bind(&store.0).bind(order).execute(&app.db).await?;
        }
        if !appended {
            tracing::warn!(
                worker = "sync",
                operation = "history",
                "history append pending"
            );
        }
    }
    Ok(())
}

/// The order's rows: `Date | Order | SKU | Product | Quantity | Amount | Token | Network |
/// Status | Transaction`. No delivery contents and nothing about the buyer. Amounts are
/// in the token's unit, as numbers the Stats formulas can add up.
pub async fn lines(app: &App, store: &StoreId, order: &str) -> anyhow::Result<Vec<Vec<Cell>>> {
    let o = sqlx::query(
        "SELECT o.created_at,o.status,o.items_json,o.asset,o.transaction_id,c.view_json
         FROM orders o JOIN connections c ON c.store_id=o.store_id AND c.id=o.connection_id
         WHERE o.store_id=? AND o.id=?",
    )
    .bind(&store.0)
    .bind(order)
    .fetch_one(&app.db)
    .await?;
    let asset: String = o.get("asset");
    let view: Value = serde_json::from_str(o.get("view_json"))?;
    let known = crate::payroof::client::display_assets(&view)
        .into_iter()
        .find(|a| a["id"] == asset.as_str());
    let field = |key: &str| {
        known
            .as_ref()
            .and_then(|a| a[key].as_str())
            .unwrap_or(&asset)
            .to_string()
    };
    let decimals = known
        .as_ref()
        .and_then(|a| a["decimals"].as_u64())
        .unwrap_or(0) as usize;
    let items: Vec<Value> = serde_json::from_str(o.get("items_json"))?;
    let mut rows = Vec::new();
    for item in &items {
        let quantity = item["quantity"].as_u64().unwrap_or(0);
        let total = match item["unit_amount"].as_str() {
            Some(unit) => crate::commerce::amount::multiply(unit, quantity as u32)?.to_string(),
            None => "0".into(),
        };
        rows.push(vec![
            Cell::Date(o.get("created_at")),
            text(order),
            text(item["sku"].as_str().unwrap_or_default()),
            text(item["title"].as_str().unwrap_or_default()),
            Cell::Number(quantity as f64),
            Cell::Number(decimal(&total, decimals).parse().unwrap_or(0.0)),
            text(field("symbol")),
            text(field("network")),
            text(o.get::<String, _>("status").replace('_', " ")),
            text(
                o.get::<Option<String>, _>("transaction_id")
                    .unwrap_or_default(),
            ),
        ]);
    }
    Ok(rows)
}

/// An exact decimal from a smallest-unit integer: ("1500000", 6) is "1.5".
fn decimal(atomic: &str, decimals: usize) -> String {
    if decimals == 0 {
        return atomic.to_string();
    }
    let padded = format!("{atomic:0>width$}", width = decimals + 1);
    let (whole, fraction) = padded.split_at(padded.len() - decimals);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{fraction}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn amounts_are_exact_decimals() {
        assert_eq!(decimal("1500000", 6), "1.5");
        assert_eq!(decimal("1", 18), "0.000000000000000001");
        assert_eq!(decimal("2000", 3), "2");
        assert_eq!(decimal("0", 6), "0");
        assert_eq!(decimal("123", 0), "123");
    }
}
