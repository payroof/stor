//! Stores: each one a subdomain mapped for good to one source.
use crate::{
    App,
    http::error::{Error, Result},
    source::{Failure, Payments},
};
use axum::http::{HeaderMap, StatusCode};
use sqlx::Row;
#[derive(Clone)]
pub struct StoreId(pub String);
/// A well-formed store name: 3–40 lowercase letters, digits or inner hyphens. Which
/// names may be taken is `reserved::is_reserved`, checked on creation only.
pub fn valid_slug(s: &str) -> bool {
    (3..=40).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && s.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && s.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
}
pub fn url(app: &App, slug: &str) -> String {
    let scheme = if app.config.development {
        "http"
    } else {
        "https"
    };
    format!("{scheme}://{slug}.{}", app.config.store_suffix)
}
/// The store's other addresses, under `store_suffix_aliases` (e.g. on this machine).
pub fn alias_urls(app: &App, slug: &str) -> Vec<String> {
    let scheme = if app.config.development {
        "http"
    } else {
        "https"
    };
    app.config
        .store_suffix_aliases
        .iter()
        .map(|suffix| format!("{scheme}://{slug}.{suffix}"))
        .collect()
}

/// Creates a store: `subdomain = source`, for good. The source is prepared first
/// (edit access proven, missing tabs added), so a store exists only for a source Stor
/// can use. Nothing is issued: everything after this happens in the source.
pub async fn create(
    app: &App,
    slug: &str,
    input: &str,
    payments: Option<&Payments>,
) -> Result<String> {
    if !valid_slug(slug) {
        return Err(Error::bad("invalid_subdomain"));
    }
    if crate::reserved::is_reserved(&app.config, slug) {
        return Err(Error::bad("subdomain_reserved"));
    }
    let (source, id) = app
        .sources
        .iter()
        .find_map(|s| s.id(input).map(|id| (s.as_ref(), id)))
        .ok_or_else(|| {
            if app.sources.is_empty() {
                Error(StatusCode::SERVICE_UNAVAILABLE, "source_unavailable")
            } else {
                Error::bad("invalid_source")
            }
        })?;
    if let Some(p) = payments {
        crate::payroof::client::server(app, &p.url).map_err(|_| Error::bad("invalid_payments"))?;
        if !(4..=256).contains(&p.key.len()) || p.key.bytes().any(|b| !b.is_ascii_graphic()) {
            return Err(Error::bad("invalid_payments"));
        }
    }
    let (taken, used): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM stores WHERE slug=?),
                EXISTS(SELECT 1 FROM stores WHERE source_kind=? AND source_id=? AND status='active')",
    )
    .bind(slug)
    .bind(source.kind())
    .bind(&id)
    .fetch_one(&app.db)
    .await?;
    if taken {
        return Err(Error::conflict("subdomain_taken"));
    }
    if used {
        return Err(Error::conflict("source_in_use"));
    }
    crate::source::setup(app, source, &id, slug, payments)
        .await
        .map_err(|error| match crate::source::failure(&error) {
            Failure::Unreachable => Error(StatusCode::UNPROCESSABLE_ENTITY, "source_unreachable"),
            Failure::ReadOnly => Error(StatusCode::UNPROCESSABLE_ENTITY, "source_read_only"),
            Failure::Unavailable if error.downcast_ref::<Failure>().is_some() => {
                Error(StatusCode::BAD_GATEWAY, "source_unavailable")
            }
            // Read, but its tabs can't be told apart (two Catalogs, …).
            Failure::Unavailable => Error(StatusCode::UNPROCESSABLE_ENTITY, "source_invalid"),
        })?;
    let store = crate::crypto::id("sto_")?;
    let metadata = crate::commerce::catalog::Metadata::new(slug);
    let created = sqlx::query(
        "INSERT INTO stores(id,slug,source_kind,source_id,metadata_json,created_at)
         VALUES(?,?,?,?,?,?)",
    )
    .bind(&store)
    .bind(slug)
    .bind(source.kind())
    .bind(&id)
    .bind(serde_json::to_string(&metadata).map_err(anyhow::Error::from)?)
    .bind(crate::time::now())
    .execute(&app.db)
    .await;
    match created {
        Ok(_) => Ok(url(app, slug)),
        // Another request took the subdomain or the source meanwhile.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(Error::conflict("subdomain_taken"))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn from_host(app: &App, h: &HeaderMap) -> Result<StoreId> {
    let host = h
        .get("host")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(Error::not_found)?;
    let slug = std::iter::once(&app.config.store_suffix)
        .chain(&app.config.store_suffix_aliases)
        .find_map(|suffix| host.strip_suffix(&format!(".{suffix}")))
        .filter(|s| valid_slug(s))
        .ok_or_else(Error::not_found)?;
    let id: Option<String> =
        sqlx::query_scalar("SELECT id FROM stores WHERE slug=? AND status='active'")
            .bind(slug)
            .fetch_optional(&app.db)
            .await?;
    Ok(StoreId(id.ok_or_else(Error::not_found)?))
}
/// Stor's own page and store creation answer only on its origin and aliases.
pub fn management_host(app: &App, h: &HeaderMap) -> Result<()> {
    let host = h.get("host").and_then(|h| h.to_str().ok());
    let ours = std::iter::once(&app.config.origin)
        .chain(&app.config.origin_aliases)
        .filter_map(|o| url::Url::parse(o).ok())
        .any(|u| {
            let expected = match u.port() {
                Some(p) => format!("{}:{p}", u.host_str().unwrap_or("")),
                None => u.host_str().unwrap_or("").into(),
            };
            host == Some(expected.as_str())
        });
    if !ours {
        return Err(Error::not_found());
    }
    Ok(())
}
/// What the storefront shows: `active` once the catalog is published and payments are
/// connected, `draft` before.
pub async fn status(app: &App, store: &StoreId) -> Result<serde_json::Value> {
    let r = sqlx::query(
        "SELECT revision,EXISTS(SELECT 1 FROM connections WHERE store_id=stores.id AND status='active') AS connected
         FROM stores WHERE id=?",
    )
    .bind(&store.0)
    .fetch_one(&app.db)
    .await?;
    let live = r.get::<i64, _>("revision") > 0 && r.get::<bool, _>("connected");
    Ok(serde_json::json!({"status": if live { "active" } else { "draft" }}))
}
