use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value, json};
use std::sync::Arc;
use stortop::{App, State as StorState, stores::StoreId};
use zeroize::Zeroizing;

#[tokio::test]
async fn storefront_styles_are_host_isolated_and_keep_the_csp() {
    use tower::ServiceExt;
    let app = fixture().await;
    let css = ".product-card { border-radius: 4px; }";
    sqlx::query("UPDATE stores SET metadata_json=? WHERE id='a'")
        .bind(
            json!({"title":"Styled store","description":"","website":null,"style":css}).to_string(),
        )
        .execute(&app.db)
        .await
        .unwrap();
    for (host, expected) in [
        ("alpha.stor.localhost:8081", StatusCode::OK),
        ("stor:8081", StatusCode::NOT_FOUND),
        ("unknown.stor.localhost:8081", StatusCode::NOT_FOUND),
    ] {
        let response = stortop::http::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/store.css")
                    .header("host", host)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let csp = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(csp.contains("style-src 'self'") && !csp.contains("unsafe-inline"));
        if expected == StatusCode::OK {
            assert_eq!(
                response.headers()["content-type"],
                "text/css; charset=utf-8"
            );
            let body = axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap();
            assert_eq!(body.as_ref(), css.as_bytes());
        }
    }
}

#[tokio::test]
async fn the_merchants_theme_locks_the_page_before_it_paints() {
    use tower::ServiceExt;
    let app = fixture().await;
    for (theme, expected) in [
        (
            "dark",
            "document.documentElement.dataset.lock = \"dark\";\n",
        ),
        (
            "light",
            "document.documentElement.dataset.lock = \"light\";\n",
        ),
        ("auto", ""),
    ] {
        sqlx::query("UPDATE stores SET metadata_json=? WHERE id='a'")
            .bind(
                json!({"title":"Themed","description":"","website":null,"style":"","theme":theme})
                    .to_string(),
            )
            .execute(&app.db)
            .await
            .unwrap();
        for (host, status) in [
            ("alpha.stor.localhost:8081", StatusCode::OK),
            ("stor:8081", StatusCode::NOT_FOUND),
        ] {
            let response = stortop::http::router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/store-theme.js")
                        .header("host", host)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            if status == StatusCode::OK {
                assert_eq!(
                    response.headers()["content-type"],
                    "text/javascript; charset=utf-8"
                );
                let body = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                assert_eq!(body.as_ref(), expected.as_bytes());
            }
        }
    }
    // A store saved before themes existed reads as auto.
    let old: stortop::commerce::catalog::Metadata =
        serde_json::from_str(r#"{"title":"T","description":"","website":null,"style":""}"#)
            .unwrap();
    assert_eq!(old.theme, "auto");
    let mut metadata = stortop::commerce::catalog::Metadata::new("shop");
    metadata.theme = "sepia".into();
    assert!(stortop::commerce::catalog::validate_metadata(&metadata).is_err());
}

#[tokio::test]
async fn icons_are_served_on_every_host_with_their_types() {
    use tower::ServiceExt;
    let app = fixture().await;
    for host in ["stor:8081", "alpha.stor.localhost:8081"] {
        for (uri, kind, file) in [
            (
                "/favicon.svg",
                "image/svg+xml",
                include_bytes!("../web/favicon.svg").as_slice(),
            ),
            (
                "/favicon.ico",
                "image/x-icon",
                include_bytes!("../web/favicon.ico").as_slice(),
            ),
        ] {
            let response = stortop::http::router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri(uri)
                        .header("host", host)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{host}{uri}");
            assert_eq!(response.headers()["content-type"], kind);
            let body = axum::body::to_bytes(response.into_body(), 1 << 20)
                .await
                .unwrap();
            assert_eq!(body.as_ref(), file);
        }
    }
    // An ICO directory: reserved 0, type 1 (icon), then its image count.
    let ico = include_bytes!("../web/favicon.ico");
    assert_eq!(&ico[..4], &[0, 0, 1, 0]);
    assert!(u16::from_le_bytes([ico[4], ico[5]]) >= 3);
}

#[tokio::test]
async fn product_pages_serve_the_storefront_with_the_same_csp_and_host_rules() {
    use tower::ServiceExt;
    let app = fixture().await;
    let get = |host: &'static str, uri: &'static str| {
        let app = app.clone();
        async move {
            stortop::http::router(app)
                .oneshot(
                    axum::http::Request::builder()
                        .uri(uri)
                        .header("host", host)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    };
    let store = "alpha.stor.localhost:8081";
    let index = get(store, "/").await;
    assert_eq!(index.status(), StatusCode::OK);
    let index_csp = index.headers()["content-security-policy"].clone();
    let index_body = axum::body::to_bytes(index.into_body(), 1 << 20)
        .await
        .unwrap();
    for sku in ["guide", "a.b_c-d~e", &"x".repeat(64)] {
        let uri: &'static str = Box::leak(format!("/p/{sku}").into_boxed_str());
        let page = get(store, uri).await;
        assert_eq!(page.status(), StatusCode::OK, "{sku}");
        assert_eq!(page.headers()["content-security-policy"], index_csp);
        assert_eq!(page.headers()["cache-control"], "no-store");
        assert!(
            page.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let body = axum::body::to_bytes(page.into_body(), 1 << 20)
            .await
            .unwrap();
        assert_eq!(body, index_body);
    }
    let too_long: &'static str = Box::leak(format!("/p/{}", "x".repeat(65)).into_boxed_str());
    for (host, uri) in [
        (store, "/p/bad%20sku"),
        (store, "/p/bad%2Fsku"),
        (store, "/p/%3Cscript%3E"),
        (store, too_long),
        (store, "/p/"),
        ("stor:8081", "/p/guide"),
        ("unknown.stor.localhost:8081", "/p/guide"),
    ] {
        let response = get(host, uri).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{host}{uri}");
        assert!(
            response.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("script-src 'self'")
        );
    }
}

#[test]
fn metadata_is_exactly_five_bounded_fields_titled_by_address() {
    use stortop::commerce::catalog::{Metadata, validate_metadata};
    let mut metadata = Metadata::new("shop");
    assert_eq!(
        serde_json::to_value(&metadata).unwrap(),
        json!({"title":"shop","description":"","website":null,"style":"","theme":"auto"})
    );
    assert!(serde_json::from_str::<Metadata>(r#"{"title":"Store"}"#).is_err());
    metadata.style = "a".repeat(65536);
    assert!(validate_metadata(&metadata).is_ok());
    metadata.style.push('a');
    assert!(validate_metadata(&metadata).is_err());
    metadata.style = "é".repeat(32769);
    assert!(validate_metadata(&metadata).is_err());
    metadata.style.clear();
    metadata.title = "  ".into();
    assert!(validate_metadata(&metadata).is_err());
}

#[test]
fn product_images_are_up_to_five_https_urls() {
    use stortop::commerce::catalog::images;
    assert!(images("").unwrap().is_empty());
    let five = "https://a.example/1.png https://a.example/2.png\nhttps://a.example/3.png https://a.example/4.png https://a.example/5.png";
    assert_eq!(images(five).unwrap().len(), 5);
    for invalid in [
        "http://a.example/1.png",
        "https://user:pass@a.example/1.png",
        "data:image/png;base64,AAAA",
        "javascript:alert(1)",
        "not-a-url",
    ] {
        assert!(images(invalid).is_err(), "{invalid}");
    }
    assert!(images(&format!("{five} https://a.example/6.png")).is_err());
    assert_eq!(
        images("https://a.example/1.png, https://a.example/2.png").unwrap(),
        ["https://a.example/1.png", "https://a.example/2.png"]
    );
}

#[tokio::test]
async fn order_display_metadata_uses_its_retired_connection_not_a_new_provider() {
    let app = fixture().await;
    let old = json!({"chains":[{"id":"eip155:1","assets":[{"id":"eip155:1/slip44:60","symbol":"ETH","decimals":18}]}]});
    sqlx::query("UPDATE connections SET view_json=?,status='retired' WHERE store_id='a'")
        .bind(old.to_string())
        .execute(&app.db)
        .await
        .unwrap();
    let new = json!({"chains":[{"id":"eip155:1","assets":[{"id":"eip155:1/slip44:60","symbol":"OTHER","decimals":6}]}]});
    sqlx::query("INSERT INTO connections(store_id,id,origin,project,key_hash,access_ciphertext,webhook_id,webhook_ciphertext,hook_id,status,view_json,created_at) VALUES('a','new','http://api:8080','other',x'00','unused','wh','unused','new-hook','active',?,1)")
        .bind(new.to_string()).execute(&app.db).await.unwrap();
    let view = stortop::commerce::order::view(
        &app,
        &StoreId("a".into()),
        &stortop::crypto::hash("customer"),
        "order",
    )
    .await
    .unwrap();
    assert_eq!(view["asset_display"]["symbol"], "ETH");
    assert_eq!(view["asset_display"]["decimals"], 18);
    assert!(view.get("delivery").is_none());
}

async fn fixture() -> App {
    let config = toml::from_str(
        r#"bind="0.0.0.0:8081"
origin="http://stor:8081"
store_suffix="stor.localhost:8081"
database="sqlite::memory:"
payroof_origins=["http://api:8080"]
development=true
"#,
    )
    .unwrap();
    let db = stortop::db::connect("sqlite::memory:").await.unwrap();
    let app = StorState::new(
        config,
        db,
        Zeroizing::new([7; 32]),
        Zeroizing::new("test".into()),
        reqwest::Client::new(),
        Vec::new(),
    );
    for (store, slug, key) in [("a", "alpha", [1; 32]), ("b", "beta", [2; 32])] {
        sqlx::query(
            "INSERT INTO stores(id,slug,source_kind,source_id,metadata_json,created_at) VALUES(?,?,'google',?,?,0)",
        )
        .bind(store)
        .bind(slug)
        .bind(format!("sheet-{store}"))
        .bind(
            serde_json::to_string(&stortop::commerce::catalog::Metadata::new(slug))
                .unwrap(),
        )
        .execute(&app.db)
        .await
        .unwrap();
        let secret = stortop::payroof::signature::encode_webhook_secret(&key);
        let cipher = stortop::payroof::client::encrypt(
            &app,
            &StoreId(store.into()),
            "conn",
            "webhook",
            &secret,
        )
        .unwrap();
        let access = stortop::payroof::client::encrypt(
            &app,
            &StoreId(store.into()),
            "conn",
            "access",
            "prf_key",
        )
        .unwrap();
        sqlx::query("INSERT INTO connections(store_id,id,origin,project,key_hash,access_ciphertext,webhook_id,webhook_ciphertext,hook_id,status,view_json,created_at) VALUES(?,'conn','http://api:8080','project',?,?,'wh',?,?,'active','{\"chains\":[]}',0)")
            .bind(store)
            .bind(stortop::crypto::hash("prf_key"))
            .bind(access)
            .bind(cipher)
            .bind(format!("hook-{store}"))
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO customers(store_id,id_hash,created_at) VALUES(?,?,0)")
            .bind(store)
            .bind(stortop::crypto::hash("customer"))
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO orders(store_id,id,customer_hash,connection_id,reference,status,chain,asset,expected_amount,address,items_json,delivery_ciphertext,created_at,expires_at) VALUES(?,'order',?,'conn','ref','awaiting_payment','eip155:1','eip155:1/slip44:60','10','0xabc','[]','unused',0,?)").bind(store).bind(stortop::crypto::hash("customer")).bind(stortop::time::now()+3600).execute(&app.db).await.unwrap();
    }
    app
}
fn event() -> Value {
    json!({"id":"evt-1","event":"confirmed","project":"project","reference":"ref","chain":"eip155:1","asset":"eip155:1/slip44:60","address":"0xabc","transfer":"t1","transaction":"tx","amount":"10","block":{"height":"1","hash":"hash"}})
}
async fn send(
    app: &App,
    hook: &str,
    value: Value,
    key: &[u8],
    timestamp: i64,
    header_id: &str,
) -> Result<StatusCode, stortop::http::error::Error> {
    let body = serde_json::to_vec(&value).unwrap();
    let mut h = HeaderMap::new();
    h.insert("payroof-timestamp", timestamp.to_string().parse().unwrap());
    h.insert("payroof-id", header_id.parse().unwrap());
    h.insert(
        "payroof-signature",
        stortop::payroof::signature::sign_webhook(key, timestamp, &body)
            .parse()
            .unwrap(),
    );
    stortop::payroof::webhook::receive(State(app.clone()), Path(hook.into()), h, Bytes::from(body))
        .await
}
async fn status(app: &App, store: &str) -> String {
    sqlx::query_scalar("SELECT status FROM orders WHERE store_id=? AND id='order'")
        .bind(store)
        .fetch_one(&app.db)
        .await
        .unwrap()
}
#[tokio::test]
async fn webhook_negatives_and_tenant_collision() {
    let app = fixture().await;
    let now = stortop::time::now();
    for (key, timestamp, id, expected) in [
        ([2; 32], now, "evt-1", 401),
        ([1; 32], now - 301, "evt-1", 401),
        ([1; 32], now, "different", 400),
    ] {
        let error = send(&app, "hook-a", event(), &key, timestamp, id)
            .await
            .err()
            .unwrap();
        assert_eq!(error.0.as_u16(), expected);
    }
    let mut wrong_project = event();
    wrong_project["project"] = json!("other");
    assert_eq!(
        send(&app, "hook-a", wrong_project, &[1; 32], now, "evt-1")
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::BAD_REQUEST
    );
    for (field, value) in [
        ("chain", "eip155:2"),
        ("asset", "eip155:1/slip44:1"),
        ("address", "0xdef"),
        ("reference", "unknown"),
    ] {
        let mut e = event();
        e["id"] = json!(format!("evt-{field}"));
        e[field] = json!(value);
        assert!(
            send(&app, "hook-a", e, &[1; 32], now, &format!("evt-{field}"))
                .await
                .is_ok()
        );
        assert_eq!(status(&app, "a").await, "awaiting_payment");
    }
    let mut seen = event();
    seen["event"] = json!("seen");
    assert!(
        send(&app, "hook-a", seen.clone(), &[1; 32], now, "evt-1")
            .await
            .is_ok()
    );
    assert_eq!(status(&app, "a").await, "seen");
    assert_eq!(status(&app, "b").await, "awaiting_payment");
    assert!(
        send(&app, "hook-a", seen, &[1; 32], now, "evt-1")
            .await
            .is_ok()
    );
    assert_eq!(
        send(&app, "hook-a", event(), &[1; 32], now, "evt-1")
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::CONFLICT
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM received_events WHERE store_id='b'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
fn payment(id: &str, kind: &str, transfer: &str, amount: &str) -> Value {
    let mut e = event();
    e["id"] = json!(id);
    e["event"] = json!(kind);
    e["transfer"] = json!(transfer);
    e["transaction"] = json!(format!("tx-{transfer}"));
    e["amount"] = json!(amount);
    e
}
async fn deliver(app: &App, e: Value) {
    let id = e["id"].as_str().unwrap().to_string();
    send(app, "hook-a", e, &[1; 32], stortop::time::now(), &id)
        .await
        .unwrap();
}
#[tokio::test]
async fn several_payments_settle_one_order() {
    let app = fixture().await;
    deliver(&app, payment("e1", "seen", "t1", "4")).await;
    let hash = stortop::crypto::hash("customer");
    let view = stortop::commerce::order::view(&app, &StoreId("a".into()), &hash, "order")
        .await
        .unwrap();
    assert_eq!(
        (view["paid"].as_str(), view["remaining"].as_str()),
        (Some("4"), Some("6"))
    );
    assert_eq!(status(&app, "a").await, "partial");
    let mut other = payment("e-other", "confirmed", "t9", "100");
    other["asset"] = json!("eip155:1/slip44:1");
    deliver(&app, other).await;
    assert_eq!(status(&app, "a").await, "partial");
    deliver(&app, payment("e2", "seen", "t2", "6")).await;
    assert_eq!(status(&app, "a").await, "seen");
    deliver(&app, payment("e1c", "confirmed", "t1", "4")).await;
    deliver(&app, payment("e1c", "confirmed", "t1", "4")).await;
    assert_eq!(status(&app, "a").await, "seen");
    deliver(&app, payment("e2c", "confirmed", "t2", "6")).await;
    assert_eq!(status(&app, "a").await, "delivered");
    let tx: String =
        sqlx::query_scalar("SELECT transaction_id FROM orders WHERE store_id='a' AND id='order'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(tx, "tx-t2");
    let paid: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE store_id='a'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(paid, 2);
}
#[tokio::test]
async fn expiry_with_a_partial_payment_needs_review() {
    let app = fixture().await;
    deliver(&app, payment("e1", "seen", "t1", "4")).await;
    sqlx::query("UPDATE orders SET expires_at=0")
        .execute(&app.db)
        .await
        .unwrap();
    stortop::commerce::inventory::expire(&app).await.unwrap();
    assert_eq!(status(&app, "a").await, "manual_review");
    assert_eq!(status(&app, "b").await, "expired");
}
async fn expired_order_with_stock(app: &App, sold: i64) {
    sqlx::query("INSERT INTO products(store_id,sku,title,description,payments_json,capacity,sold,delivery_ciphertext,source_revision) VALUES('a','sku','Title','','[]',1,?,'unused',1)")
        .bind(sold)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO allocations(store_id,order_id,sku,quantity,state) VALUES('a','order','sku',1,'released')")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET status='expired',expires_at=0 WHERE store_id='a'")
        .execute(&app.db)
        .await
        .unwrap();
}
async fn stock(app: &App) -> (i64, String) {
    let sold = sqlx::query_scalar("SELECT sold FROM products WHERE store_id='a'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let state = sqlx::query_scalar("SELECT state FROM allocations WHERE store_id='a'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    (sold, state)
}
#[tokio::test]
async fn a_late_full_payment_is_delivered_while_the_stock_remains() {
    let app = fixture().await;
    expired_order_with_stock(&app, 0).await;
    deliver(&app, payment("late", "confirmed", "t1", "10")).await;
    assert_eq!(status(&app, "a").await, "delivered");
    assert_eq!(stock(&app).await, (1, "sold".into()));
}
#[tokio::test]
async fn a_late_full_payment_without_stock_is_left_for_the_merchant() {
    let app = fixture().await;
    expired_order_with_stock(&app, 1).await;
    deliver(&app, payment("late", "confirmed", "t1", "10")).await;
    assert_eq!(status(&app, "a").await, "late_payment");
    assert_eq!(stock(&app).await, (1, "released".into()));
}
#[tokio::test]
async fn tenant_customer_and_host_boundaries() {
    let app = fixture().await;
    let mut h = HeaderMap::new();
    h.insert(
        "host",
        "alpha.stor.localhost:8081.attacker".parse().unwrap(),
    );
    assert!(stortop::stores::from_host(&app, &h).await.is_err());
    h.insert("host", "alpha.stor.localhost:8081".parse().unwrap());
    assert_eq!(stortop::stores::from_host(&app, &h).await.unwrap().0, "a");
    let hash = stortop::crypto::hash("a-only");
    sqlx::query("INSERT INTO customers(store_id,id_hash,created_at) VALUES('a',?,0)")
        .bind(&hash)
        .execute(&app.db)
        .await
        .unwrap();
    h.insert("cookie", "stor_customer=a-only".parse().unwrap());
    assert!(
        stortop::auth::customer(&app, &StoreId("a".into()), &h)
            .await
            .is_ok()
    );
    assert!(
        stortop::auth::customer(&app, &StoreId("b".into()), &h)
            .await
            .is_err()
    );
    assert!(
        stortop::commerce::order::view(&app, &StoreId("b".into()), &hash, "order")
            .await
            .is_err()
    );
}
#[tokio::test]
async fn corrupted_stored_secret_is_internal_not_client_error() {
    let app = fixture().await;
    sqlx::query("UPDATE connections SET webhook_ciphertext='corrupt' WHERE store_id='a'")
        .execute(&app.db)
        .await
        .unwrap();
    let error = send(
        &app,
        "hook-a",
        event(),
        &[1; 32],
        stortop::time::now(),
        "evt-1",
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.1, "internal");
}

#[tokio::test]
async fn corrupt_catalog_and_cart_fail_closed_over_http() {
    use tower::ServiceExt;
    let app = fixture().await;
    let hash = stortop::crypto::hash("customer");
    sqlx::query("INSERT INTO products(store_id,sku,title,description,payments_json,capacity,delivery_ciphertext,source_revision) VALUES('a','sku','Title','','corrupt',10,'unused',1)").execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO carts(store_id,customer_hash,sku,quantity) VALUES('a',?,'sku',1)")
        .bind(hash)
        .execute(&app.db)
        .await
        .unwrap();
    for corrupt in [
        "corrupt",
        r#"[{"asset":"eip155:1/slip44:60","amount":"01"}]"#,
        r#"[{"asset":"eip155:1/slip44:60","amount":"0"}]"#,
        "null",
        "[]",
    ] {
        sqlx::query("UPDATE products SET payments_json=? WHERE store_id='a'")
            .bind(corrupt)
            .execute(&app.db)
            .await
            .unwrap();
        for path in ["/v1/catalog", "/v1/cart"] {
            let response = stortop::http::router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri(path)
                        .header("host", "alpha.stor.localhost:8081")
                        .header("cookie", "stor_customer=customer")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{path} accepted {corrupt}"
            );
            let id = response.headers()["x-request-id"]
                .to_str()
                .unwrap()
                .to_string();
            let body = axum::body::to_bytes(response.into_body(), 32768)
                .await
                .unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["error"], "internal");
            assert_eq!(value["request_id"], id);
            assert!(!String::from_utf8_lossy(&body).contains(corrupt));
        }
    }
}
#[tokio::test]
async fn canonical_proof_route_works_on_store_host_without_alias() {
    use tower::ServiceExt;
    let app = fixture().await;
    for (path, status) in [
        ("/v1/altcha?purpose=checkout", StatusCode::OK),
        ("/api/v1/altcha?purpose=checkout", StatusCode::NOT_FOUND),
        ("/api/altcha?purpose=checkout", StatusCode::NOT_FOUND),
    ] {
        let response = stortop::http::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .header("host", "alpha.stor.localhost:8081")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
}
#[tokio::test]
async fn active_rate_storage_is_bounded_and_existing_keys_still_count() {
    let mut app = fixture().await;
    Arc::get_mut(&mut app)
        .unwrap()
        .config
        .limits
        .rate_limit_entries = 2;
    assert!(
        stortop::limits::allow(&app, "one", "test", 3600, 2)
            .await
            .is_ok()
    );
    assert!(
        stortop::limits::allow(&app, "two", "test", 3600, 2)
            .await
            .is_ok()
    );
    assert!(
        stortop::limits::allow(&app, "three", "test", 3600, 2)
            .await
            .is_err()
    );
    assert!(
        stortop::limits::allow(&app, "one", "test", 3600, 2)
            .await
            .is_ok()
    );
    assert!(
        stortop::limits::allow(&app, "one", "test", 3600, 2)
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rate_limits")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

/// Store a's payment connection offers ETH on Ethereum and Base with these decimals.
async fn eth_with_decimals(app: &App, decimals: u32) {
    let asset =
        |chain: &str| json!({"id":format!("{chain}/slip44:60"),"symbol":"ETH","decimals":decimals});
    let view = json!({"chains":[
        {"id":"eip155:1","assets":[asset("eip155:1")]},
        {"id":"eip155:8453","assets":[asset("eip155:8453")]}
    ]});
    sqlx::query("UPDATE connections SET view_json=? WHERE store_id='a'")
        .bind(view.to_string())
        .execute(&app.db)
        .await
        .unwrap();
}

#[tokio::test]
async fn final_cart_put_sets_quantity_and_preserves_exact_amounts() {
    use tower::ServiceExt;
    let app = fixture().await;
    // The catalog's readable decimal becomes the exact integer, with no float rounding.
    eth_with_decimals(&app, 15).await;
    sqlx::query("INSERT INTO products(store_id,sku,title,description,payments_json,capacity,delivery_ciphertext,source_revision) VALUES('a','sku','Title','',?,100,'unused',1)")
        .bind(r#"[{"asset":"eip155:1/slip44:60","amount":"9.007199254740993"}]"#).execute(&app.db).await.unwrap();
    for (method, path, body, quantity) in [
        ("PUT", "/v1/cart/sku", r#"{"quantity":2}"#, 2),
        ("PUT", "/v1/cart/sku", r#"{"quantity":1}"#, 1),
        ("DELETE", "/v1/cart/sku", "", 0),
        ("PUT", "/v1/cart/sku", r#"{"quantity":2}"#, 2),
        ("DELETE", "/v1/cart", "", 0),
    ] {
        let response = stortop::http::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", "alpha.stor.localhost:8081")
                    .header("origin", "http://alpha.stor.localhost:8081")
                    .header("cookie", "stor_customer=customer")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{method} {path}");
        let bytes = axum::body::to_bytes(response.into_body(), 32768)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        if quantity == 0 {
            assert_eq!(value["items"], json!([]));
        } else {
            assert_eq!(value["items"][0]["quantity"], quantity);
            assert_eq!(
                value["groups"][0]["payment_options"][0]["amount"],
                if quantity == 2 {
                    "18014398509481986"
                } else {
                    "9007199254740993"
                }
            );
        }
    }
}

#[tokio::test]
async fn a_bag_without_a_shared_asset_is_kept_and_split_into_payments() {
    use tower::ServiceExt;
    let app = fixture().await;
    eth_with_decimals(&app, 1).await;
    for (sku, payments) in [
        ("eth", r#"[{"asset":"eip155:1/slip44:60","amount":"0.5"}]"#),
        (
            "both",
            r#"[{"asset":"eip155:1/slip44:60","amount":"0.2"},{"asset":"eip155:8453/slip44:60","amount":"0.3"}]"#,
        ),
        (
            "base",
            r#"[{"asset":"eip155:8453/slip44:60","amount":"0.7"}]"#,
        ),
    ] {
        sqlx::query("INSERT INTO products(store_id,sku,title,description,payments_json,capacity,delivery_ciphertext,source_revision) VALUES('a',?,?,'',?,100,'unused',1)")
            .bind(sku).bind(sku).bind(payments).execute(&app.db).await.unwrap();
    }
    let mut value = Value::Null;
    for sku in ["eth", "base", "both"] {
        let response = stortop::http::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("PUT")
                    .uri(format!("/v1/cart/{sku}"))
                    .header("host", "alpha.stor.localhost:8081")
                    .header("origin", "http://alpha.stor.localhost:8081")
                    .header("cookie", "stor_customer=customer")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"quantity":1}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{sku}");
        let bytes = axum::body::to_bytes(response.into_body(), 32768)
            .await
            .unwrap();
        value = serde_json::from_slice(&bytes).unwrap();
    }
    assert_eq!(value["items"].as_array().unwrap().len(), 3);
    // Both assets cover two items. The tie goes to the first in order (Ethereum), and
    // the Base-only item pays on its own.
    assert_eq!(
        value["groups"],
        json!([
            {"skus":["both","eth"],"payment_options":[{"asset":"eip155:1/slip44:60","amount":"7"}]},
            {"skus":["base"],"payment_options":[{"asset":"eip155:8453/slip44:60","amount":"7"}]}
        ])
    );
}

#[tokio::test]
async fn a_paid_group_leaves_the_rest_of_the_bag() {
    use axum::{Json, Router, routing::put};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let api = Router::new().route(
        "/v1/projects/{project}/references/{reference}",
        put(|| async {
            Json(json!({"destinations":[{"chain":"eip155:1","address":format!("0x{}", "ab".repeat(20))}]}))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, api).await.unwrap() });
    let mut app = fixture().await;
    Arc::get_mut(&mut app).unwrap().config.payroof_origins = vec![origin.clone()];
    let customer = stortop::crypto::hash("customer");
    for sku in ["paid", "also-paid", "other"] {
        sqlx::query("INSERT INTO products(store_id,sku,title,description,payments_json,capacity,delivery_ciphertext,source_revision) VALUES('a',?,?,'','[]',100,'unused',1)")
            .bind(sku).bind(sku).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO carts(store_id,customer_hash,sku,quantity) VALUES('a',?,?,1)")
            .bind(&customer)
            .bind(sku)
            .execute(&app.db)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO orders(store_id,id,customer_hash,connection_id,reference,status,chain,asset,expected_amount,items_json,delivery_ciphertext,created_at,expires_at) VALUES('a','ord',?,'conn','ref-split','creating','eip155:1','eip155:1/slip44:60','10',?,'unused',0,?)")
        .bind(&customer)
        .bind(r#"[{"sku":"paid","title":"Paid","quantity":1,"unit_amount":"5"},{"sku":"also-paid","title":"Also","quantity":1,"unit_amount":"5"}]"#)
        .bind(stortop::time::now() + 600)
        .execute(&app.db).await.unwrap();
    let connection = stortop::payroof::client::Connection {
        id: "conn".into(),
        origin,
        project: "project".into(),
        token: Zeroizing::new("token".into()),
        view: Value::Null,
    };
    stortop::commerce::order::map(&app, &StoreId("a".into()), "ord", &connection)
        .await
        .unwrap();
    let left: Vec<String> =
        sqlx::query_scalar("SELECT sku FROM carts WHERE store_id='a' AND customer_hash=?")
            .bind(&customer)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(left, ["other"]);
}

#[tokio::test]
async fn sample_tables_download_as_csv_that_stor_recognises_as_its_tabs() {
    use stortop::source::sheet;
    use tower::ServiceExt;
    let app = fixture().await;
    let get = |host: &'static str, uri: String| {
        let app = app.clone();
        async move {
            stortop::http::router(app)
                .oneshot(
                    axum::http::Request::builder()
                        .uri(uri)
                        .header("host", host)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    };
    let mut tabs = Vec::new();
    for (index, name) in sheet::SAMPLE_TABLES.iter().enumerate() {
        let response = get("stor:8081", format!("/v1/templates/{name}.csv")).await;
        assert_eq!(response.status(), StatusCode::OK, "{name}");
        assert_eq!(
            response.headers()["content-type"],
            "text/csv; charset=utf-8"
        );
        assert_eq!(
            response.headers()["content-disposition"],
            format!("attachment; filename=\"{name}.csv\"").as_str()
        );
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        // Read back as Google Sheets would hold the imported file.
        let rows = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(body.as_ref())
            .records()
            .map(|r| r.unwrap().iter().map(str::to_owned).collect())
            .collect();
        tabs.push(sheet::Tab {
            id: index as i64,
            title: name.to_string(),
            rows,
        });
    }
    let layout = sheet::classify(&tabs).unwrap();
    assert_eq!(
        [
            layout.catalog,
            layout.tokens,
            layout.branding,
            layout.payments,
            layout.history
        ],
        [Some(0), Some(1), Some(2), Some(3), Some(4)]
    );
    assert_eq!(sheet::catalog(&tabs[0]).unwrap().len(), 32);
    // Only what exists, and only on Stor's own host.
    for (host, uri) in [
        ("stor:8081", "/v1/templates/Stats.csv"),
        ("stor:8081", "/v1/templates/Catalog"),
        ("stor:8081", "/v1/templates/..%2FCatalog.csv"),
        ("alpha.stor.localhost:8081", "/v1/templates/Catalog.csv"),
    ] {
        assert_eq!(
            get(host, uri.into()).await.status(),
            StatusCode::NOT_FOUND,
            "{host}{uri}"
        );
    }
}
