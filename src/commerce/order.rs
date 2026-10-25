use crate::{
    App,
    http::error::{Error, Result},
    payroof::client,
    stores::StoreId,
};
use serde_json::{Value, json};
use sqlx::Row;
use zeroize::Zeroizing;
pub fn aad(store: &StoreId, order: &str) -> String {
    format!("stortop:v1:{}:order:{order}:delivery", store.0)
}
pub async fn checkout(
    app: &App,
    store: &StoreId,
    customer: &[u8],
    asset: &str,
    skus: &[String],
    proof: &altcha::Payload,
) -> Result<Value> {
    // The bag lines this order pays for: one payment group, named by its SKUs.
    let mut wanted: Vec<&str> = skus.iter().map(String::as_str).collect();
    wanted.sort_unstable();
    wanted.dedup();
    if wanted.is_empty() || wanted.len() != skus.len() || wanted.len() > 50 {
        return Err(Error::bad("invalid_items"));
    }
    crate::altcha::consume(app, proof, &format!("checkout:{}", store.0)).await?;
    let connection = client::active(app, store)
        .await
        .map_err(|_| Error::conflict("payment_unavailable"))?;
    if !client::supports(&connection.view, asset) {
        return Err(Error::bad("unsupported_asset"));
    }
    let chain = super::catalog::asset(asset)?;
    let mut tx = crate::db::write(&app.db).await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM connections WHERE store_id=? AND id=? AND status='active')",
    )
    .bind(&store.0)
    .bind(&connection.id)
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        return Err(Error::conflict("connection_changed"));
    }
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orders WHERE store_id=? AND customer_hash=? AND status='creating')").bind(&store.0).bind(customer).fetch_one(&mut *tx).await?;
    if pending {
        return Err(Error::conflict("checkout_in_progress"));
    }
    let bag = super::cart::lines(&mut tx, store, customer).await?;
    if bag.is_empty() {
        return Err(Error::conflict("empty_cart"));
    }
    let lines: Vec<&super::cart::Line> = bag
        .iter()
        .filter(|line| wanted.binary_search(&line.sku.as_str()).is_ok())
        .collect();
    if lines.len() != wanted.len() {
        return Err(Error::conflict("cart_changed"));
    }
    if lines.iter().any(|line| !line.in_stock()) {
        return Err(Error::conflict("inventory_unavailable"));
    }
    let amount = super::cart::intersection(&lines)
        .ok()
        .and_then(|totals| totals.get(asset).cloned())
        .ok_or_else(|| Error::conflict("incompatible_payment_options"))?;
    let id = crate::crypto::id("ord_")?;
    let reference = id.clone();
    let mut deliveries = Zeroizing::new(Vec::<String>::new());
    let mut items = Vec::new();
    for line in &lines {
        let p = sqlx::query(
            "SELECT sku,source_revision,delivery_ciphertext FROM products
             WHERE store_id=? AND sku=? AND active=1",
        )
        .bind(&store.0)
        .bind(&line.sku)
        .fetch_one(&mut *tx)
        .await?;
        let secret = crate::source::delivery(app, store, &p)?;
        let secret = Zeroizing::new(
            String::from_utf8(secret.to_vec()).map_err(|_| Error::bad("invalid_delivery"))?,
        );
        if deliveries.iter().map(String::len).sum::<usize>() + secret.len() * line.quantity as usize
            > 2 * 1024 * 1024
        {
            return Err(Error::conflict("fulfillment_size_limit"));
        }
        for _ in 0..line.quantity {
            deliveries.push(secret.to_string());
        }
        items.push(json!({"sku":line.sku,"title":line.title,"quantity":line.quantity,"unit_amount":line.payments.iter().find(|p|p.asset==asset).map(|p|&p.amount)}));
    }
    let plaintext = Zeroizing::new(serde_json::to_string(&*deliveries)?);
    let sealed = crate::crypto::seal(
        &app.master,
        aad(store, &id).as_bytes(),
        plaintext.as_bytes(),
    )?;
    let now = crate::time::now();
    sqlx::query(
        "INSERT INTO orders
         (store_id,id,customer_hash,connection_id,reference,status,chain,asset,expected_amount,
          items_json,delivery_ciphertext,created_at,expires_at)
         VALUES(?,?,?,?,?,'creating',?,?,?,?,?,?,?)",
    )
    .bind(&store.0)
    .bind(&id)
    .bind(customer)
    .bind(&connection.id)
    .bind(&reference)
    .bind(&chain)
    .bind(asset)
    .bind(&amount)
    .bind(serde_json::to_string(&items)?)
    .bind(sealed)
    .bind(now)
    .bind(now + 1800)
    .execute(&mut *tx)
    .await?;
    for line in &lines {
        let n = sqlx::query(
            "UPDATE products SET reserved=reserved+?
             WHERE store_id=? AND sku=? AND active=1 AND capacity-reserved-sold>=?",
        )
        .bind(line.quantity)
        .bind(&store.0)
        .bind(&line.sku)
        .bind(line.quantity)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n != 1 {
            return Err(Error::conflict("inventory_unavailable"));
        }
        sqlx::query("INSERT INTO allocations(store_id,order_id,sku,quantity,state) VALUES(?,?,?,?,'reserved')").bind(&store.0).bind(&id).bind(&line.sku).bind(line.quantity).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    // Idempotent PUT happens outside the writer. A crash leaves a durable creating order for the worker.
    if map(app, store, &id, &connection).await.is_err() {
        let mut tx = crate::db::write(&app.db).await?;
        super::inventory::transition(&mut tx, store, &id, false).await?;
        sqlx::query(
            "UPDATE orders SET status='failed' WHERE store_id=? AND id=? AND status='creating'",
        )
        .bind(&store.0)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Err(Error(
            axum::http::StatusCode::BAD_GATEWAY,
            "mapping_unavailable",
        ));
    }
    view(app, store, customer, &id).await
}
pub async fn map(
    app: &App,
    store: &StoreId,
    id: &str,
    connection: &client::Connection,
) -> anyhow::Result<()> {
    let r=sqlx::query("SELECT reference,chain,customer_hash,items_json FROM orders WHERE store_id=? AND id=? AND status='creating'").bind(&store.0).bind(id).fetch_one(&app.db).await?;
    let mapping = client::call(
        app,
        &connection.origin,
        &connection.token,
        reqwest::Method::PUT,
        &format!(
            "/v1/projects/{}/references/{}",
            connection.project,
            r.get::<String, _>("reference")
        ),
        None,
    )
    .await?;
    let chain: String = r.get("chain");
    let destinations = mapping["destinations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing destinations"))?;
    let matches: Vec<_> = destinations
        .iter()
        .filter(|d| d["chain"] == chain)
        .collect();
    anyhow::ensure!(matches.len() == 1, "destination chain mismatch");
    let address = matches[0]["address"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing address"))?;
    anyhow::ensure!(
        address.len() == 42
            && address.starts_with("0x")
            && address[2..].bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid receive address"
    );
    let mut tx = crate::db::write(&app.db).await?;
    let n = sqlx::query(
        "UPDATE orders SET address=?,status='awaiting_payment'
         WHERE store_id=? AND id=? AND status='creating' AND expires_at>?",
    )
    .bind(address.to_ascii_lowercase())
    .bind(&store.0)
    .bind(id)
    .bind(crate::time::now())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 1 {
        // Only this order's lines leave the bag. Other payment groups stay in it.
        sqlx::query(
            "DELETE FROM carts WHERE store_id=? AND customer_hash=?
             AND sku IN (SELECT json_extract(value,'$.sku') FROM json_each(?))",
        )
        .bind(&store.0)
        .bind(r.get::<Vec<u8>, _>("customer_hash"))
        .bind(r.get::<String, _>("items_json"))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    anyhow::ensure!(n == 1, "order no longer creating");
    Ok(())
}
pub async fn view(app: &App, store: &StoreId, customer: &[u8], id: &str) -> Result<Value> {
    let r = sqlx::query("SELECT * FROM orders WHERE store_id=? AND customer_hash=? AND id=?")
        .bind(&store.0)
        .bind(customer)
        .bind(id)
        .fetch_optional(&app.db)
        .await?
        .ok_or_else(Error::not_found)?;
    let mut paid = num_bigint::BigUint::default();
    for a in sqlx::query_scalar::<_, String>(
        "SELECT amount FROM payments WHERE store_id=? AND order_id=?",
    )
    .bind(&store.0)
    .bind(id)
    .fetch_all(&app.db)
    .await?
    {
        paid = crate::commerce::amount::add(paid, crate::commerce::amount::atomic(&a)?)?;
    }
    let expected = crate::commerce::amount::atomic(r.get("expected_amount"))?;
    let remaining = if expected > paid {
        &expected - &paid
    } else {
        Default::default()
    };
    let mut value = json!({
        "paid": paid.to_string(), "remaining": remaining.to_string(),
        "id": id, "status": r.get::<String,_>("status"),
        "chain": r.get::<String,_>("chain"),
        "asset": r.get::<String,_>("asset"),
        "amount": r.get::<String,_>("expected_amount"),
        "address": r.get::<Option<String>,_>("address"),
        "items": serde_json::from_str::<Value>(r.get("items_json"))?,
        "expires_at": crate::time::rfc(r.get("expires_at")),
        "transaction": r.get::<Option<String>,_>("transaction_id")
    });
    // An order keeps the display metadata of its own (possibly retired)
    // connection, not a newer integration or a guessed token decimal count.
    let view: String =
        sqlx::query_scalar("SELECT view_json FROM connections WHERE store_id=? AND id=?")
            .bind(&store.0)
            .bind(r.get::<String, _>("connection_id"))
            .fetch_one(&app.db)
            .await?;
    let assets = crate::payroof::client::display_assets(&serde_json::from_str::<Value>(&view)?);
    value["asset_display"] = assets
        .into_iter()
        .find(|asset| asset["id"] == value["asset"])
        .unwrap_or(Value::Null);
    // Each line shows its product's current first photo, when the product has one.
    let photos = sqlx::query("SELECT sku,images_json FROM products WHERE store_id=?")
        .bind(&store.0)
        .fetch_all(&app.db)
        .await?;
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            let image = photos
                .iter()
                .find(|p| item["sku"] == p.get::<String, _>("sku"))
                .and_then(|p| serde_json::from_str::<Vec<String>>(p.get("images_json")).ok())
                .and_then(|images| images.into_iter().next());
            item["image"] = image.map(Value::String).unwrap_or(Value::Null);
        }
    }
    if r.get::<String, _>("status") == "delivered" {
        let plain = crate::crypto::open(
            &app.master,
            aad(store, id).as_bytes(),
            r.get("delivery_ciphertext"),
        )?;
        value["delivery"] = serde_json::from_slice(&plain)?;
    }
    Ok(value)
}
