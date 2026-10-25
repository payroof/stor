use crate::{
    App,
    crypto::{open, seal},
    stores::StoreId,
};
use serde_json::Value;
use sqlx::Row;
use zeroize::Zeroizing;
pub fn aad(store: &StoreId, connection: &str, field: &str) -> String {
    format!("stortop:v1:{}:{connection}:{field}", store.0)
}
pub fn allowed(app: &App, origin: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        app.config
            .payroof_origins
            .iter()
            .any(|o| o.trim_end_matches('/') == origin.trim_end_matches('/')),
        "Payroof origin not allowlisted"
    );
    Ok(())
}
pub struct Connection {
    pub id: String,
    pub origin: String,
    pub project: String,
    pub token: Zeroizing<String>,
    pub view: Value,
}
pub async fn active(app: &App, store: &StoreId) -> anyhow::Result<Connection> {
    let id: String =
        sqlx::query_scalar("SELECT id FROM connections WHERE store_id=? AND status='active'")
            .bind(&store.0)
            .fetch_one(&app.db)
            .await?;
    by_id(app, store, &id).await
}
pub async fn by_id(app: &App, store: &StoreId, connection: &str) -> anyhow::Result<Connection> {
    let r = sqlx::query(
        "SELECT id,origin,project,access_ciphertext,view_json FROM connections
         WHERE store_id=? AND id=? AND status='active'",
    )
    .bind(&store.0)
    .bind(connection)
    .fetch_one(&app.db)
    .await?;
    let id: String = r.get("id");
    let origin: String = r.get("origin");
    allowed(app, &origin)?;
    let b = open(
        &app.master,
        aad(store, &id, "access").as_bytes(),
        r.get("access_ciphertext"),
    )?;
    Ok(Connection {
        id,
        origin,
        project: r.get("project"),
        token: Zeroizing::new(String::from_utf8(b.to_vec())?),
        view: serde_json::from_str(r.get("view_json"))?,
    })
}
/// The project a Payments URL names, `<origin>/v1/projects/<project>`, on an allowed
/// payment server.
pub fn server(app: &App, url: &str) -> anyhow::Result<(String, String)> {
    let u = url::Url::parse(url.trim())?;
    let project = u
        .path()
        .strip_prefix("/v1/projects/")
        .map(|p| p.trim_end_matches('/'))
        .filter(|p| {
            (1..=63).contains(&p.len())
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        .ok_or_else(|| anyhow::anyhow!("URL must end in /v1/projects/<project>"))?;
    anyhow::ensure!(
        u.username().is_empty() && u.password().is_none() && u.query().is_none(),
        "URL must be the plain project address"
    );
    let origin = u.origin().ascii_serialization();
    allowed(app, &origin).map_err(|_| anyhow::anyhow!("URL is not an allowed payment server"))?;
    Ok((origin, project.to_string()))
}
/// The project's public view (its networks and tokens), read with the key: proves the
/// key works for this project.
pub async fn server_view(app: &App, url: &str, key: &str) -> anyhow::Result<(String, Value)> {
    let (origin, project) = server(app, url)?;
    let view = call(
        app,
        &origin,
        key,
        reqwest::Method::GET,
        &format!("/v1/projects/{project}"),
        None,
    )
    .await
    .map_err(|_| anyhow::anyhow!("the payment server did not accept the URL and KEY"))?;
    anyhow::ensure!(view["chains"].is_array(), "invalid project view");
    Ok((origin, view))
}
pub async fn call(
    app: &App,
    origin: &str,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> anyhow::Result<Value> {
    allowed(app, origin)?;
    let mut request = app
        .http
        .request(method, format!("{}{path}", origin.trim_end_matches('/')))
        .bearer_auth(token);
    if let Some(b) = body {
        request = request.json(&b);
    }
    crate::fetch::json(request.send().await?.error_for_status()?).await
}
pub fn encrypt(
    app: &App,
    store: &StoreId,
    id: &str,
    field: &str,
    value: &str,
) -> anyhow::Result<String> {
    seal(
        &app.master,
        aad(store, id, field).as_bytes(),
        value.as_bytes(),
    )
}
pub fn supports(view: &Value, asset: &str) -> bool {
    view["chains"].as_array().is_some_and(|cs| {
        cs.iter().any(|c| {
            c["assets"]
                .as_array()
                .is_some_and(|aa| aa.iter().any(|a| a["id"] == asset))
        })
    })
}

/// Public display metadata only. Never expose RPCs, access tokens, or arbitrary
/// operator fields to storefronts. Atomic strings remain payment authority.
pub fn display_assets(view: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    if let Some(chains) = view["chains"].as_array() {
        for chain in chains {
            let network = chain["name"].as_str().filter(|name| {
                (1..=40).contains(&name.chars().count())
                    && !name.trim().is_empty()
                    && !name.chars().any(char::is_control)
            });
            if let (Some(id), Some(assets)) = (chain["id"].as_str(), chain["assets"].as_array()) {
                for asset in assets {
                    if let (Some(asset_id), Some(symbol), Some(decimals)) = (
                        asset["id"].as_str(),
                        asset["symbol"].as_str(),
                        asset["decimals"].as_u64(),
                    ) && decimals <= 36
                        && !symbol.is_empty()
                        && crate::commerce::catalog::asset(asset_id).is_ok_and(|chain| chain == id)
                    {
                        result.push(serde_json::json!({
                            "id":asset_id,"chain":id,"network":network,"symbol":symbol,"decimals":decimals
                        }));
                    }
                }
            }
        }
    }
    result
}

/// Token decimals by asset ID from a project view.
pub fn decimals(view: &Value) -> crate::commerce::catalog::Decimals {
    display_assets(view)
        .iter()
        .filter_map(|a| {
            Some((
                a["id"].as_str()?.to_string(),
                a["decimals"].as_u64()? as u32,
            ))
        })
        .collect()
}
/// The store's token decimals. Empty without a payment connection.
pub async fn store_decimals(
    app: &App,
    store: &StoreId,
) -> anyhow::Result<crate::commerce::catalog::Decimals> {
    let view: Option<String> = sqlx::query_scalar(
        "SELECT view_json FROM connections WHERE store_id=? AND status='active'",
    )
    .bind(&store.0)
    .fetch_optional(&app.db)
    .await?;
    Ok(match view {
        Some(raw) => decimals(&serde_json::from_str(&raw)?),
        None => Default::default(),
    })
}
pub async fn store_display_assets(app: &App, store: &StoreId) -> anyhow::Result<Vec<Value>> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT view_json FROM connections WHERE store_id=? AND status='active'",
    )
    .bind(&store.0)
    .fetch_optional(&app.db)
    .await?;
    raw.map(|raw| serde_json::from_str::<Value>(&raw).map(|view| display_assets(&view)))
        .transpose()
        .map(|assets| assets.unwrap_or_default())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    #[test]
    fn display_metadata_is_exact_and_whitelisted() {
        let view = serde_json::json!({"token":"secret","chains":[{"id":"eip155:31337","name":"Anvil","rpc":"private", "assets":[
            {"id":"eip155:31337/slip44:60","symbol":"ETH","decimals":18,"private":"secret"},
            {"id":"eip155:1/slip44:60","symbol":"Wrong chain","decimals":18},
            {"id":"eip155:31337/erc20:bad","symbol":"Bad decimals","decimals":255}
        ]}]});
        assert_eq!(
            super::display_assets(&view),
            vec![serde_json::json!({
                "id":"eip155:31337/slip44:60","chain":"eip155:31337","network":"Anvil",
                "symbol":"ETH","decimals":18
            })]
        );
        for name in [
            serde_json::json!(""),
            serde_json::json!("x".repeat(41)),
            serde_json::json!("A\u{7}B"),
        ] {
            let view = serde_json::json!({"chains":[{"id":"eip155:1","name":name,"assets":[
                {"id":"eip155:1/slip44:60","symbol":"ETH","decimals":18}
            ]}]});
            assert!(super::display_assets(&view)[0]["network"].is_null());
        }
    }
    #[test]
    fn support_comes_from_operator_catalog_without_project_switches() {
        let view = serde_json::json!({"chains":[{"id":"eip155:1","assets":[
            {"id":"eip155:1/slip44:60","symbol":"ETH","decimals":18}
        ]}]});
        assert!(super::supports(&view, "eip155:1/slip44:60"));
        assert!(!super::supports(&view, "eip155:8453/slip44:60"));
    }
}
