pub mod create;
pub mod error;
pub mod storefront;
use crate::App;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, FromRequest, Path, Query, Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
/// The client's address, as the trusted proxies report it.
#[derive(Clone)]
pub struct Client(pub String);
pub struct CheckedJson<T>(pub T);
impl<S, T> FromRequest<S> for CheckedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = error::Error;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(v) = Json::<T>::from_request(req, state)
            .await
            .map_err(|_| error::Error::bad("invalid_json"))?;
        Ok(Self(v))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Purpose {
    purpose: String,
}
async fn challenge(
    State(app): State<App>,
    Query(q): Query<Purpose>,
    h: axum::http::HeaderMap,
) -> error::Result<Json<altcha::Challenge>> {
    let purpose = if q.purpose == "store" {
        crate::stores::management_host(&app, &h)?;
        q.purpose
    } else if q.purpose == "checkout" {
        let store = crate::stores::from_host(&app, &h).await?;
        format!("checkout:{}", store.0)
    } else {
        return Err(error::Error::bad("invalid_purpose"));
    };
    Ok(Json(crate::altcha::challenge(&app.altcha_key, &purpose)?))
}
async fn product(
    State(app): State<App>,
    Path(sku): Path<String>,
    h: axum::http::HeaderMap,
) -> error::Result<Html<String>> {
    if !crate::commerce::catalog::valid_sku(&sku) {
        return Err(error::Error::not_found());
    }
    crate::stores::from_host(&app, &h).await?;
    Ok(Html(
        include_str!("../../web/storefront/index.html").to_owned(),
    ))
}
async fn index(State(app): State<App>, h: axum::http::HeaderMap) -> error::Result<Html<String>> {
    if crate::stores::management_host(&app, &h).is_ok() {
        Ok(Html(include_str!("../../web/create/index.html").to_owned()))
    } else {
        crate::stores::from_host(&app, &h).await?;
        Ok(Html(
            include_str!("../../web/storefront/index.html").to_owned(),
        ))
    }
}
async fn script(
    State(app): State<App>,
    h: axum::http::HeaderMap,
) -> error::Result<impl IntoResponse> {
    let js = if crate::stores::management_host(&app, &h).is_ok() {
        include_str!("../../web/create/app.js")
    } else {
        crate::stores::from_host(&app, &h).await?;
        include_str!("../../web/storefront/app.js")
    };
    Ok(([("content-type", "text/javascript; charset=utf-8")], js))
}
async fn proof_script() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../../web/proof.js"),
    )
}
async fn ui_script() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../../web/ui.js"),
    )
}
async fn ready(State(app): State<App>) -> error::Result<Json<serde_json::Value>> {
    let _: i64 = sqlx::query_scalar("SELECT 1").fetch_one(&app.db).await?;
    Ok(Json(json!({"ready":true})))
}
async fn guard(State(app): State<App>, mut req: Request, next: Next) -> Response {
    let request_id = crate::crypto::id("req_").unwrap_or_else(|_| "req_unavailable".into());
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let mut new_cookie = None;
    let mut response = async {
        let peer = req
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|p| {
                crate::trusted_proxy::client_ip(
                    p.0.ip(),
                    req.headers(),
                    &app.config.trusted_proxies,
                )
                .to_string()
            })
            .unwrap_or_else(|| "unknown_peer".into());
        let is_hook = path.starts_with("/v1/webhooks/");
        let is_management = path == "/v1/stores" || path == "/v1/info";
        if is_hook || is_management {
            crate::stores::management_host(&app, req.headers())?;
        }
        let limit = if path.ends_with("altcha") {
            app.config.limits.proof_per_minute
        } else if path == "/v1/stores" || path == "/v1/checkout" {
            app.config.limits.create_per_minute
        } else {
            app.config.limits.ordinary_per_minute
        };
        crate::limits::allow(
            &app,
            &peer,
            if is_hook {
                "webhook"
            } else if is_management {
                "management"
            } else if path.ends_with("altcha") {
                "proof"
            } else if path == "/v1/checkout" {
                "checkout"
            } else {
                "storefront"
            },
            60,
            limit,
        )
        .await?;
        if method != Method::GET && method != Method::HEAD && !is_hook {
            // Cookies require same-origin requests. CLI bearer clients may omit Origin.
            // Browsers send Origin. The Wallet, an API client, sends none and no cookies.
            if let Some(origin) = req.headers().get("origin").and_then(|v| v.to_str().ok()) {
                let host = req
                    .headers()
                    .get("host")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let scheme = if app.config.development {
                    "http"
                } else {
                    "https"
                };
                if origin != format!("{scheme}://{host}") {
                    return Err(error::Error(StatusCode::FORBIDDEN, "origin_mismatch"));
                }
            } else if !is_management || req.headers().contains_key("cookie") {
                return Err(error::Error(StatusCode::FORBIDDEN, "origin_required"));
            }
        }
        if (path == "/" || path.starts_with("/v1/") && !is_management && !is_hook)
            && let Ok(store) = crate::stores::from_host(&app, req.headers()).await
        {
            let name = crate::auth::customer_cookie(&app);
            if crate::auth::customer(&app, &store, req.headers())
                .await
                .is_err()
            {
                let count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE store_id=?")
                        .bind(&store.0)
                        .fetch_one(&app.db)
                        .await?;
                if count >= app.config.limits.customers_per_store {
                    return Err(error::Error::conflict("customer_quota"));
                }
                let id = crate::crypto::id("cus_")?;
                sqlx::query("INSERT INTO customers(store_id,id_hash,created_at) VALUES(?,?,?)")
                    .bind(&store.0)
                    .bind(crate::crypto::hash(&id))
                    .bind(crate::time::now())
                    .execute(&app.db)
                    .await?;
                let cookie = format!("{name}={id}");
                let existing = req
                    .headers()
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let existing = existing
                    .split(';')
                    .filter(|s| s.trim().split_once('=').is_some_and(|(k, _)| k != name))
                    .collect::<Vec<_>>()
                    .join(";");
                let combined = format!("{existing}; {cookie}");
                req.headers_mut().insert(
                    "cookie",
                    combined
                        .parse()
                        .map_err(|_| error::Error::bad("invalid_cookie"))?,
                );
                new_cookie = Some(format!(
                    "{cookie}; HttpOnly; SameSite=Strict; Path=/{}",
                    if app.config.development {
                        ""
                    } else {
                        "; Secure"
                    }
                ));
            }
        }
        req.extensions_mut().insert(Client(peer.clone()));
        let max = if is_hook {
            app.config.limits.webhook_body
        } else {
            app.config.limits.json_body
        };
        let (parts, body) = req.into_parts();
        // The deadline covers reading the body too, so a slow upload can't hold a slot.
        tokio::time::timeout(
            std::time::Duration::from_secs(app.config.limits.request_timeout_seconds),
            async {
                let bytes = to_bytes(body, max)
                    .await
                    .map_err(|_| error::Error(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"))?;
                Ok::<_, error::Error>(
                    next.run(Request::from_parts(parts, Body::from(bytes)))
                        .await,
                )
            },
        )
        .await
        .map_err(|_| error::Error(StatusCode::REQUEST_TIMEOUT, "request_timeout"))?
    }
    .await
    .unwrap_or_else(IntoResponse::into_response);
    if response.status().is_client_error() || response.status().is_server_error() {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 32768)
            .await
            .unwrap_or_default();
        let mut v: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"error":"invalid_request","message":"request rejected"}));
        v["request_id"] = json!(request_id);
        let code = v["error"]
            .as_str()
            .filter(|s| s.len() <= 64 && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
            .unwrap_or("internal");
        if status.is_server_error() {
            tracing::error!(request_id=%request_id,status=status.as_u16(),code,"HTTP request rejected");
        } else {
            tracing::warn!(request_id=%request_id,status=status.as_u16(),code,"HTTP request rejected");
        }
        response = (status, Json(v)).into_response();
    }
    for (k, v) in [
        ("x-request-id", request_id.as_str()),
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' https:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(k),
            v.parse().expect("safe header"),
        );
    }
    if let Some(cookie) = new_cookie {
        response
            .headers_mut()
            .append("set-cookie", cookie.parse().expect("generated cookie"));
    }
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        response
            .headers_mut()
            .insert("retry-after", HeaderValue::from_static("5"));
    }
    response
}
pub fn router(app: App) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/p/{sku}", get(product))
        .route("/app.js", get(script))
        .route("/proof.js", get(proof_script))
        .route("/ui.js", get(ui_script))
        .route("/store.css", get(storefront::stylesheet))
        .route("/store-theme.js", get(storefront::theme_script))
        .route(
            "/favicon.svg",
            get(|| async {
                (
                    [("content-type", "image/svg+xml")],
                    include_str!("../../web/favicon.svg"),
                )
            }),
        )
        // For browsers and tools that ask for /favicon.ico or can't use SVG icons.
        .route(
            "/favicon.ico",
            get(|| async {
                (
                    [("content-type", "image/x-icon")],
                    include_bytes!("../../web/favicon.ico").as_slice(),
                )
            }),
        )
        .route(
            "/theme.js",
            get(|| async {
                (
                    [("content-type", "text/javascript; charset=utf-8")],
                    include_str!("../../web/theme.js"),
                )
            }),
        )
        .route(
            "/qr.js",
            get(|| async {
                (
                    [("content-type", "text/javascript; charset=utf-8")],
                    include_str!("../../web/qr.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [("content-type", "text/css")],
                    include_str!("../../web/style.css"),
                )
            }),
        )
        .route("/healthz", get(|| async { Json(json!({"alive":true})) }))
        .route("/readyz", get(ready))
        .route("/v1/altcha", get(challenge))
        .route("/v1/info", get(create::info))
        .route("/v1/templates/{file}", get(create::sample_table))
        .route("/v1/stores", post(create::create))
        .route("/v1/catalog", get(storefront::catalog))
        .route("/v1/cart", get(storefront::cart).delete(storefront::clear))
        .route(
            "/v1/cart/{sku}",
            put(storefront::quantity).delete(storefront::remove),
        )
        .route("/v1/checkout", post(storefront::checkout))
        .route("/v1/orders/{id}", get(storefront::order))
        .route(
            "/v1/webhooks/{hook}",
            post(crate::payroof::webhook::receive),
        )
        .fallback(|| async { error::Error::not_found() })
        .layer(axum::extract::DefaultBodyLimit::max(
            app.config
                .limits
                .json_body
                .max(app.config.limits.webhook_body),
        ))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .layer(tower::limit::ConcurrencyLimitLayer::new(
            app.config.limits.concurrency,
        ))
        .with_state(app)
}
