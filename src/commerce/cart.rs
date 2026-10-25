use crate::{
    App,
    http::error::{Error, Result},
    stores::StoreId,
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;
#[derive(Clone, Deserialize, Serialize)]
pub struct Line {
    pub sku: String,
    pub title: String,
    pub quantity: u32,
    pub payments: Vec<super::catalog::Payment>,
    /// Units that can be bought now: none once the product is sold out or delisted.
    pub available: i64,
}
impl Line {
    /// Whether the whole quantity can be bought now.
    pub fn in_stock(&self) -> bool {
        i64::from(self.quantity) <= self.available
    }
}
/// The assets every line accepts, each with the total for all lines in it.
pub fn intersection(lines: &[&Line]) -> anyhow::Result<BTreeMap<String, String>> {
    let Some(first) = lines.first() else {
        return Ok(BTreeMap::new());
    };
    let mut totals = BTreeMap::new();
    for option in &first.payments {
        let mut total = BigUint::default();
        let mut common = true;
        for line in lines {
            if let Some(p) = line.payments.iter().find(|p| p.asset == option.asset) {
                total =
                    super::amount::add(total, super::amount::multiply(&p.amount, line.quantity)?)?;
            } else {
                common = false;
                break;
            }
        }
        if common {
            totals.insert(option.asset.clone(), total.to_string());
        }
    }
    anyhow::ensure!(!totals.is_empty(), "incompatible payment options");
    Ok(totals)
}
/// Lines that can be paid together, and what they can be paid with.
pub struct Group<'a> {
    pub lines: Vec<&'a Line>,
    pub totals: BTreeMap<String, String>,
}
/// One order is paid with one asset, so a bag whose lines share none is split into as
/// few payments as a greedy cover finds: each takes the asset most of the remaining
/// lines accept (ties go to the first asset in order), with every line that accepts it.
pub fn groups(lines: &[Line]) -> anyhow::Result<Vec<Group<'_>>> {
    let mut remaining: Vec<&Line> = lines.iter().collect();
    let mut groups = Vec::new();
    while !remaining.is_empty() {
        let mut counts = BTreeMap::<&str, usize>::new();
        for line in &remaining {
            for payment in &line.payments {
                *counts.entry(payment.asset.as_str()).or_default() += 1;
            }
        }
        let mut best: Option<(&str, usize)> = None;
        for (asset, count) in counts {
            if best.is_none_or(|(_, most)| count > most) {
                best = Some((asset, count));
            }
        }
        let (asset, _) = best.ok_or_else(|| anyhow::anyhow!("line without payment options"))?;
        let asset = asset.to_owned();
        let (members, rest): (Vec<&Line>, Vec<&Line>) = remaining
            .into_iter()
            .partition(|line| line.payments.iter().any(|p| p.asset == asset));
        groups.push(Group {
            totals: intersection(&members)?,
            lines: members,
        });
        remaining = rest;
    }
    Ok(groups)
}
pub async fn lines(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    store: &StoreId,
    customer: &[u8],
) -> Result<Vec<Line>> {
    let rows = sqlx::query(
        "SELECT c.sku,c.quantity,p.title,p.payments_json,p.active,p.capacity,p.reserved,p.sold
         FROM carts c JOIN products p ON p.store_id=c.store_id AND p.sku=c.sku
         WHERE c.store_id=? AND c.customer_hash=? ORDER BY c.sku",
    )
    .bind(&store.0)
    .bind(customer)
    .fetch_all(&mut **tx)
    .await?;
    let view: Option<String> = sqlx::query_scalar(
        "SELECT view_json FROM connections WHERE store_id=? AND status='active'",
    )
    .bind(&store.0)
    .fetch_optional(&mut **tx)
    .await?;
    let decimals = match view {
        Some(raw) => crate::payroof::client::decimals(
            &serde_json::from_str(&raw).map_err(|_| Error::internal())?,
        ),
        None => Default::default(),
    };
    let mut lines = Vec::new();
    for r in rows {
        let q: i64 = r.get("quantity");
        let payments = super::catalog::persisted_payments(r.get("payments_json"), &decimals)?;
        if !(1..=100).contains(&q) {
            return Err(Error::internal());
        }
        for p in &payments {
            super::amount::multiply(&p.amount, q as u32)?;
        }
        // A line another customer's order sold out stays in the bag, marked, so the
        // rest of the bag can still be seen and bought.
        let available = if r.get::<bool, _>("active") {
            (r.get::<i64, _>("capacity") - r.get::<i64, _>("reserved") - r.get::<i64, _>("sold"))
                .max(0)
        } else {
            0
        };
        lines.push(Line {
            sku: r.get("sku"),
            title: r.get("title"),
            quantity: q as u32,
            payments,
            available,
        });
    }
    Ok(lines)
}
pub async fn view(app: &App, store: &StoreId, customer: &[u8]) -> Result<Value> {
    let mut tx = app.db.begin().await?;
    let lines = lines(&mut tx, store, customer).await?;
    tx.commit().await?;
    let mut items = Vec::new();
    for line in &lines {
        let mut payments = Vec::new();
        for payment in &line.payments {
            let total = super::amount::multiply(&payment.amount, line.quantity)?;
            payments.push(json!({
                "asset": payment.asset, "unit_amount": payment.amount,
                "line_total": total.to_string()
            }));
        }
        items.push(json!({
            "id": line.sku, "sku": line.sku, "title": line.title,
            "quantity": line.quantity, "available": line.available,
            "payment_options": payments
        }));
    }
    // Only what can be bought now is grouped into payments.
    let buyable: Vec<Line> = lines.iter().filter(|l| l.in_stock()).cloned().collect();
    let groups: Vec<Value> = groups(&buyable)
        .map_err(|_| Error::internal())?
        .iter()
        .map(|group| {
            json!({
                "skus": group.lines.iter().map(|line| &line.sku).collect::<Vec<_>>(),
                "payment_options": group.totals.iter()
                    .map(|(asset, amount)| json!({"asset": asset, "amount": amount}))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    Ok(json!({"items": items, "groups": groups}))
}
pub async fn set(
    app: &App,
    store: &StoreId,
    customer: &[u8],
    sku: &str,
    quantity: Option<u32>,
) -> Result<Value> {
    let mut tx = crate::db::write(&app.db).await?;
    let creating:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orders WHERE store_id=? AND customer_hash=? AND status='creating')").bind(&store.0).bind(customer).fetch_one(&mut *tx).await?;
    if creating {
        return Err(Error::conflict("checkout_in_progress"));
    }
    if let Some(q) = quantity {
        if !(1..=100).contains(&q) {
            return Err(Error::bad("invalid_quantity"));
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM products WHERE store_id=? AND sku=? AND active=1)",
        )
        .bind(&store.0)
        .bind(sku)
        .fetch_one(&mut *tx)
        .await?;
        if !exists {
            return Err(Error::not_found());
        }
        sqlx::query(
            "INSERT INTO carts(store_id,customer_hash,sku,quantity) VALUES(?,?,?,?)
             ON CONFLICT(store_id,customer_hash,sku) DO UPDATE SET quantity=excluded.quantity",
        )
        .bind(&store.0)
        .bind(customer)
        .bind(sku)
        .bind(q)
        .execute(&mut *tx)
        .await?;
    } else if sku.is_empty() {
        sqlx::query("DELETE FROM carts WHERE store_id=? AND customer_hash=?")
            .bind(&store.0)
            .bind(customer)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM carts WHERE store_id=? AND customer_hash=? AND sku=?")
            .bind(&store.0)
            .bind(customer)
            .bind(sku)
            .execute(&mut *tx)
            .await?;
    }
    let lines = lines(&mut tx, store, customer).await?;
    if lines.len() > 50 {
        return Err(Error::conflict("cart_limit"));
    }
    // More than is left can't be put in the bag. Other lines don't matter here.
    if quantity.is_some() && lines.iter().any(|l| l.sku == sku && !l.in_stock()) {
        return Err(Error::conflict("inventory_unavailable"));
    }
    tx.commit().await?;
    view(app, store, customer).await
}
#[cfg(test)]
mod tests {
    use super::*;
    fn line(sku: &str, quantity: u32, assets: &[(&str, &str)]) -> Line {
        Line {
            sku: sku.into(),
            title: sku.into(),
            quantity,
            payments: assets
                .iter()
                .map(|(asset, amount)| crate::commerce::catalog::Payment {
                    asset: (*asset).into(),
                    amount: (*amount).into(),
                })
                .collect(),
            available: 100,
        }
    }
    #[test]
    fn common_assets() {
        let a = line("a", 2, &[("X", "3")]);
        assert_eq!(intersection(&[&a, &a]).unwrap()["X"], "12");
        let (y, x) = (line("a", 1, &[("Y", "3")]), line("b", 1, &[("X", "1")]));
        assert!(intersection(&[&y, &x]).is_err());
    }
    #[test]
    fn a_bag_without_a_shared_asset_splits_into_few_payments() {
        let lines = [
            line("a", 1, &[("X", "1"), ("Y", "5")]),
            line("b", 2, &[("Y", "2")]),
            line("c", 1, &[("Z", "7")]),
            line("d", 1, &[("X", "4"), ("Y", "1")]),
        ];
        let split = groups(&lines).unwrap();
        let skus: Vec<Vec<&str>> = split
            .iter()
            .map(|g| g.lines.iter().map(|l| l.sku.as_str()).collect())
            .collect();
        assert_eq!(skus, [vec!["a", "b", "d"], vec!["c"]]);
        assert_eq!(split[0].totals.len(), 1);
        assert_eq!(split[0].totals["Y"], "10");
        assert_eq!(split[1].totals["Z"], "7");
        // A bag that shares an asset stays one payment with every shared option.
        let shared = groups(&lines[..1]).unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].totals.len(), 2);
        assert!(groups(&[]).unwrap().is_empty());
        assert!(groups(&[line("e", 1, &[])]).is_err());
    }
}
