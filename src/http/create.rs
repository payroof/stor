//! The one request a merchant makes: claim a subdomain for a source.
use super::{
    CheckedJson,
    error::{Error, Result},
};
use crate::{
    App,
    source::{Payments, sheet},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    url: String,
    key: Zeroizing<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Create {
    subdomain: String,
    /// A Google Sheets link or ID.
    source: String,
    /// The payment server's URL and KEY, written into the source's Payments tab. The
    /// Wallet sends them. A merchant on the web page fills them in later.
    payments: Option<Server>,
    altcha: altcha::Payload,
}

/// What the creation page needs: where stores live and who to share a source with.
pub async fn info(State(app): State<App>) -> Json<Value> {
    Json(json!({
        "store_suffix": app.config.store_suffix,
        "share_with": app.sources.first().map(|s| s.account()),
    }))
}

/// One tab of a new store as a CSV file, for a merchant who makes the tabs by hand:
/// `GET /v1/templates/Catalog.csv`.
pub async fn sample_table(
    State(app): State<App>,
    Path(file): Path<String>,
    h: HeaderMap,
) -> Result<impl IntoResponse> {
    crate::stores::management_host(&app, &h)?;
    let name = file.strip_suffix(".csv").ok_or_else(Error::not_found)?;
    let rows = sheet::sample_table(name).ok_or_else(Error::not_found)?;
    let body = sheet::csv(&rows)?;
    Ok((
        [
            ("content-type", "text/csv; charset=utf-8".to_owned()),
            (
                "content-disposition",
                format!("attachment; filename=\"{name}.csv\""),
            ),
        ],
        body,
    ))
}

pub async fn create(
    State(app): State<App>,
    Extension(client): Extension<super::Client>,
    CheckedJson(b): CheckedJson<Create>,
) -> Result<(StatusCode, Json<Value>)> {
    crate::altcha::consume(&app, &b.altcha, "store").await?;
    crate::limits::allow(
        &app,
        &client.0,
        "stores",
        86400,
        app.config.limits.stores_per_day,
    )
    .await?;
    let payments = b.payments.map(|p| Payments {
        url: p.url,
        key: p.key,
    });
    let url = crate::stores::create(&app, &b.subdomain, &b.source, payments.as_ref()).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "store": b.subdomain, "url": url,
            "aliases": crate::stores::alias_urls(&app, &b.subdomain)
        })),
    ))
}
