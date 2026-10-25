//! Store creation and source sync, with an in-memory source and a mock payment server.
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use stortop::{
    App, State as StorState,
    source::{
        Change, Failure, Fut, Source,
        sheet::{Cell, Tab},
    },
    stores::StoreId,
};
use tower::ServiceExt;
use zeroize::Zeroizing;

/// Spreadsheets in memory, by ID. `mem:<id>` names one.
#[derive(Default)]
struct Memory {
    sheets: Mutex<std::collections::HashMap<String, Vec<Tab>>>,
    read_only: Mutex<Vec<String>>,
    marks: Mutex<Vec<String>>,
}
impl Memory {
    fn add(&self, id: &str, tabs: Vec<Tab>) {
        self.sheets.lock().unwrap().insert(id.into(), tabs);
    }
    fn tab(&self, id: &str, title: &str) -> Tab {
        self.sheets.lock().unwrap()[id]
            .iter()
            .find(|t| t.title == title)
            .cloned()
            .unwrap_or_else(|| panic!("no tab {title}"))
    }
    fn titles(&self, id: &str) -> Vec<String> {
        self.sheets.lock().unwrap()[id]
            .iter()
            .map(|t| t.title.clone())
            .collect()
    }
    fn set(&self, id: &str, title: &str, row: usize, column: usize, value: &str) {
        let mut sheets = self.sheets.lock().unwrap();
        let tab = sheets
            .get_mut(id)
            .unwrap()
            .iter_mut()
            .find(|t| t.title == title)
            .unwrap();
        write(&mut tab.rows, row, column, value.into());
    }
}
fn write(rows: &mut Vec<Vec<String>>, row: usize, column: usize, value: String) {
    while rows.len() <= row {
        rows.push(Vec::new());
    }
    while rows[row].len() <= column {
        rows[row].push(String::new());
    }
    rows[row][column] = value;
}
fn render(cell: &Cell) -> String {
    match cell {
        Cell::Header(s) | Cell::Text(s) | Cell::Formula(s) => s.clone(),
        Cell::Number(n) => n.to_string(),
        Cell::Date(d) => d.to_string(),
    }
}
impl Source for Memory {
    fn kind(&self) -> &'static str {
        "google"
    }
    fn id(&self, input: &str) -> Option<String> {
        input.strip_prefix("mem:").map(str::to_string)
    }
    fn account(&self) -> &str {
        "stor@example.iam.gserviceaccount.com"
    }
    fn read<'a>(&'a self, id: &'a str) -> Fut<'a, Vec<Tab>> {
        Box::pin(async move {
            self.sheets
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| Failure::Unreachable.into())
        })
    }
    fn apply<'a>(&'a self, id: &'a str, changes: &'a [Change]) -> Fut<'a, ()> {
        Box::pin(async move {
            if self.read_only.lock().unwrap().iter().any(|r| r == id) {
                return Err(Failure::ReadOnly.into());
            }
            let mut sheets = self.sheets.lock().unwrap();
            let tabs = sheets.get_mut(id).ok_or(Failure::Unreachable)?;
            for change in changes {
                match change {
                    Change::Add { tab, title } => tabs.push(Tab {
                        id: *tab,
                        title: title.clone(),
                        rows: Vec::new(),
                    }),
                    Change::Rename { tab, title } => {
                        tabs.iter_mut().find(|t| t.id == *tab).unwrap().title = title.clone()
                    }
                    Change::Write {
                        tab,
                        row,
                        column,
                        rows,
                    } => {
                        let t = tabs.iter_mut().find(|t| t.id == *tab).unwrap();
                        for (r, cells) in rows.iter().enumerate() {
                            for (c, cell) in cells.iter().enumerate() {
                                write(&mut t.rows, row + r, column + c, render(cell));
                            }
                        }
                    }
                    Change::Append { tab, rows } => {
                        let t = tabs.iter_mut().find(|t| t.id == *tab).unwrap();
                        t.rows
                            .extend(rows.iter().map(|r| r.iter().map(render).collect()));
                    }
                    Change::Stats { tab, .. } => {
                        let t = tabs.iter_mut().find(|t| t.id == *tab).unwrap();
                        t.rows = vec![vec!["Sales per day".into()]];
                    }
                    Change::Mark { store } => self.marks.lock().unwrap().push(store.clone()),
                }
            }
            Ok(())
        })
    }
}

const USDC: &str = "eip155:1/erc20:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

/// A payment server for project `shop` that accepts the key `prf_key`.
async fn payment_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (hooks, views) = (calls.clone(), calls.clone());
    let authorized = |h: &HeaderMap| h["authorization"] == "Bearer prf_key";
    let server = Router::new()
        .route(
            "/v1/projects/shop",
            get(move |h: HeaderMap| {
                let calls = views.clone();
                async move {
                    if !authorized(&h) {
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                    calls.lock().unwrap().push("view".into());
                    Ok(Json(
                        json!({"project":"shop","chains":[{"id":"eip155:1","name":"Ethereum",
                        "assets":[{"id":USDC,"symbol":"USDC","decimals":6},
                                  {"id":"eip155:1/slip44:60","symbol":"ETH","decimals":18}]}]}),
                    ))
                }
            }),
        )
        .route(
            "/v1/projects/shop/webhooks",
            post(move |h: HeaderMap, Json(body): Json<Value>| {
                let calls = hooks.clone();
                async move {
                    if !authorized(&h) {
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                    assert!(
                        body["url"]
                            .as_str()
                            .unwrap()
                            .starts_with("http://stor:8081/v1/webhooks/hook_")
                    );
                    assert_eq!(body["events"], json!(["seen", "confirmed"]));
                    calls.lock().unwrap().push("webhook".into());
                    Ok(Json(json!({"id":"wh_1","secret":
                        stortop::payroof::signature::encode_webhook_secret(&[3; 32])})))
                }
            }),
        );
    tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
    (origin, calls)
}

async fn stor(api: &str) -> (App, Arc<Memory>) {
    let mut config: stortop::config::Config = toml::from_str(
        r#"bind="0.0.0.0:8081"
origin="http://stor:8081"
store_suffix="stor.localhost:8081"
database="sqlite::memory:"
payroof_origins=["http://placeholder"]
development=true
"#,
    )
    .unwrap();
    config.payroof_origins = vec![api.into()];
    config.limits.create_per_minute = 100;
    config.limits.stores_per_day = 100;
    let memory = Arc::new(Memory::default());
    let app = StorState::new(
        config,
        stortop::db::connect("sqlite::memory:").await.unwrap(),
        Zeroizing::new([7; 32]),
        Zeroizing::new("test".into()),
        reqwest::Client::new(),
        vec![memory.clone()],
    );
    (app, memory)
}

/// A store-creation proof whose answer is counter zero, so solving it is instant.
fn proof() -> Value {
    let challenge = altcha::create_challenge(altcha::CreateChallengeOptions {
        algorithm: "PBKDF2/SHA-256".into(),
        cost: 5000,
        counter: Some(0),
        key_prefix_length: Some(4),
        hmac_signature_secret: Some("test".into()),
        expires_at: Some(stortop::time::now() + 120),
        data: Some([("purpose".to_string(), json!("store"))].into()),
        ..Default::default()
    })
    .unwrap();
    let solution = altcha::solve_challenge(altcha::SolveChallengeOptions::new(&challenge))
        .unwrap()
        .unwrap();
    json!({"challenge": challenge, "solution": solution})
}

async fn create(app: &App, body: Value, headers: &[(&str, &str)]) -> (StatusCode, Value) {
    let mut request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/stores")
        .header("host", "stor:8081")
        .header("content-type", "application/json");
    for (k, v) in headers {
        request = request.header(*k, *v);
    }
    let mut body = body;
    body["altcha"] = proof();
    let response = stortop::http::router(app.clone())
        .oneshot(
            request
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    9,
                ))))
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn store(app: &App, slug: &str) -> StoreId {
    StoreId(
        sqlx::query_scalar("SELECT id FROM stores WHERE slug=?")
            .bind(slug)
            .fetch_one(&app.db)
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn one_request_maps_a_subdomain_to_a_prepared_sheet_for_good() {
    let (api, calls) = payment_server().await;
    let (app, memory) = stor(&api).await;
    memory.add(
        "fresh",
        vec![Tab {
            id: 0,
            title: "Sheet1".into(),
            rows: Vec::new(),
        }],
    );
    let payments = json!({"url": format!("{api}/v1/projects/shop"), "key": "prf_key"});
    let (status, body) = create(
        &app,
        json!({"subdomain":"shop","source":"mem:fresh","payments":payments}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["url"], "http://shop.stor.localhost:8081");
    assert_eq!(body["aliases"], json!([]));
    // The empty tab became the Catalog. The others were added, History with its Stats.
    assert_eq!(
        memory.titles("fresh"),
        [
            "Catalog", "Tokens", "Branding", "Payments", "History", "Stats"
        ]
    );
    assert_eq!(*memory.marks.lock().unwrap(), ["shop"]);
    let catalog = memory.tab("fresh", "Catalog");
    assert_eq!(catalog.rows.len(), 33);
    assert_eq!(catalog.rows[1][3], "12 USD");
    let tokens = memory.tab("fresh", "Tokens");
    assert_eq!(tokens.rows[1][..3], ["USDC", "Ethereum", USDC]);
    assert_eq!(tokens.rows[1][4], "1");
    let payments_tab = memory.tab("fresh", "Payments");
    assert_eq!(payments_tab.rows[1][1], format!("{api}/v1/projects/shop"));
    assert_eq!(payments_tab.rows[2][1], "prf_key");
    // Nothing about the key is kept until the source is read.
    assert_eq!(*calls.lock().unwrap(), ["view"]);
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(kept, 0);

    memory.add("other", vec![]);
    memory.add("viewer", vec![]);
    memory.read_only.lock().unwrap().push("viewer".into());
    for (subdomain, source, status, code) in [
        ("shop", "mem:other", StatusCode::CONFLICT, "subdomain_taken"),
        ("shop-2", "mem:fresh", StatusCode::CONFLICT, "source_in_use"),
        (
            "Shop",
            "mem:other",
            StatusCode::BAD_REQUEST,
            "invalid_subdomain",
        ),
        (
            "admin",
            "mem:other",
            StatusCode::BAD_REQUEST,
            "subdomain_reserved",
        ),
        (
            "free-porn",
            "mem:other",
            StatusCode::BAD_REQUEST,
            "subdomain_reserved",
        ),
        (
            "pay-pal",
            "mem:other",
            StatusCode::BAD_REQUEST,
            "subdomain_reserved",
        ),
        (
            "viewer",
            "mem:viewer",
            StatusCode::UNPROCESSABLE_ENTITY,
            "source_read_only",
        ),
        (
            "missing",
            "mem:missing",
            StatusCode::UNPROCESSABLE_ENTITY,
            "source_unreachable",
        ),
        (
            "link",
            "https://example.com/x",
            StatusCode::BAD_REQUEST,
            "invalid_source",
        ),
    ] {
        let (got, body) = create(&app, json!({"subdomain":subdomain,"source":source}), &[]).await;
        assert_eq!(
            (got, body["error"].as_str()),
            (status, Some(code)),
            "{subdomain}"
        );
    }
    let (got, body) = create(
        &app,
        json!({"subdomain":"elsewhere","source":"mem:other",
               "payments":{"url":"https://evil.example/v1/projects/shop","key":"prf_key"}}),
        &[],
    )
    .await;
    assert_eq!(
        (got, body["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_payments"))
    );
    // A browser request rides no cookie without its Origin.
    let (got, _) = create(
        &app,
        json!({"subdomain":"cookie","source":"mem:other"}),
        &[("cookie", "a=b")],
    )
    .await;
    assert_eq!(got, StatusCode::FORBIDDEN);
    let (got, _) = create(
        &app,
        json!({"subdomain":"cross","source":"mem:other"}),
        &[("origin", "http://evil.example")],
    )
    .await;
    assert_eq!(got, StatusCode::FORBIDDEN);
    // Fair use: a client creates only so many stores a day.
    let mut strict = app.config.clone();
    strict.limits.stores_per_day = 1;
    let limited = StorState::new(
        strict,
        app.db.clone(),
        Zeroizing::new([7; 32]),
        Zeroizing::new("test".into()),
        reqwest::Client::new(),
        vec![memory.clone()],
    );
    let (got, _) = create(
        &limited,
        json!({"subdomain":"quota","source":"mem:other"}),
        &[],
    )
    .await;
    assert_eq!(got, StatusCode::TOO_MANY_REQUESTS);
    // The mapping can't be changed afterwards.
    assert!(
        sqlx::query("UPDATE stores SET source_id='other' WHERE slug='shop'")
            .execute(&app.db)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_sync_publishes_the_sheet_and_reports_in_it() {
    let (api, calls) = payment_server().await;
    let (app, memory) = stor(&api).await;
    memory.add("sheet", vec![]);
    let payments = json!({"url": format!("{api}/v1/projects/shop"), "key": "prf_key"});
    let (status, _) = create(
        &app,
        json!({"subdomain":"shop","source":"mem:sheet","payments":payments}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let shop = store(&app, "shop").await;
    stortop::source::sync(&app, &shop).await.unwrap();
    assert_eq!(
        memory.tab("sheet", "Payments").rows[3][1],
        "Live · 32 products · no USD rate for ETH: click Allow access on Google's external data warning"
    );
    assert_eq!(*calls.lock().unwrap(), ["view", "view", "webhook"]);
    let catalog = stortop::http::router(app.clone())
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/catalog")
                .header("host", "shop.stor.localhost:8081")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(catalog.status(), StatusCode::OK);
    let catalog: Value = serde_json::from_slice(
        &axum::body::to_bytes(catalog.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(catalog["status"]["status"], "active");
    assert_eq!(catalog["products"].as_array().unwrap().len(), 32);
    // `api-credits`, first by SKU: $30, in USDC only while ETH has no rate.
    assert_eq!(catalog["products"][0]["price"], "$30");
    assert_eq!(
        catalog["products"][0]["payment_options"],
        json!([{"asset": USDC, "amount": "30000000"}])
    );
    // Typed over, ETH's rate makes it a way to pay dollar prices.
    memory.set("sheet", "Tokens", 2, 4, "3000");
    stortop::source::sync(&app, &shop).await.unwrap();
    let options: String = sqlx::query_scalar(
        "SELECT payments_json FROM products WHERE store_id=? AND sku='api-credits'",
    )
    .bind(&shop.0)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&options).unwrap(),
        json!([{"asset":USDC,"amount":"30"},{"asset":"eip155:1/slip44:60","amount":"0.01"}])
    );

    // A mistake is reported in the sheet. The last good catalog stays live.
    memory.set("sheet", "Catalog", 2, 3, "25 DAI");
    stortop::source::sync(&app, &shop).await.unwrap();
    assert_eq!(
        memory.tab("sheet", "Payments").rows[3][1],
        "Error: Catalog ui-kit: DAI is not in the Tokens tab"
    );
    let live: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE store_id=? AND active=1")
            .bind(&shop.0)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(live, 32);
    // An unchanged key is not connected again.
    memory.set("sheet", "Catalog", 2, 3, "25 usdc\n0.01 ETH");
    stortop::source::sync(&app, &shop).await.unwrap();
    assert_eq!(
        memory.tab("sheet", "Payments").rows[3][1],
        "Live · 32 products"
    );
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| *c == "webhook")
            .count(),
        1
    );
    // A CoinGecko rate older than two hours isn't used. One over 45 minutes old makes
    // Stor write the fetching cell again, at most once per 45 minutes.
    let now = stortop::time::now();
    memory.set("sheet", "Tokens", 2, 5, &(now - 3 * 3600).to_string());
    memory.set("sheet", "Tokens", 0, 13, "");
    stortop::source::sync(&app, &shop).await.unwrap();
    assert_eq!(
        memory.tab("sheet", "Payments").rows[3][1],
        "Live · 32 products · USD rate out of date for ETH"
    );
    let tokens = memory.tab("sheet", "Tokens");
    assert!(tokens.rows[0][13].starts_with("=IMPORTDATA("));
    let options: String = sqlx::query_scalar(
        "SELECT payments_json FROM products WHERE store_id=? AND sku='api-credits'",
    )
    .bind(&shop.0)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(options, format!(r#"[{{"asset":"{USDC}","amount":"30"}}]"#));
    memory.set("sheet", "Tokens", 0, 13, "");
    stortop::source::sync(&app, &shop).await.unwrap();
    assert_eq!(memory.tab("sheet", "Tokens").rows[0][13], "");
    memory.set("sheet", "Tokens", 2, 5, &(now - 60).to_string());
    stortop::source::sync(&app, &shop).await.unwrap();
    assert_eq!(
        memory.tab("sheet", "Payments").rows[3][1],
        "Live · 32 products"
    );
    // A deleted History tab comes back.
    memory
        .sheets
        .lock()
        .unwrap()
        .get_mut("sheet")
        .unwrap()
        .retain(|t| t.title != "History");
    stortop::source::sync(&app, &shop).await.unwrap();
    assert!(memory.titles("sheet").contains(&"History".to_string()));
}

#[tokio::test]
async fn an_unreachable_sheet_ends_its_store_after_the_grace_period() {
    let (api, _) = payment_server().await;
    let (app, memory) = stor(&api).await;
    memory.add("gone", vec![]);
    let (status, _) = create(&app, json!({"subdomain":"gone","source":"mem:gone"}), &[]).await;
    assert_eq!(status, StatusCode::CREATED);
    let gone = store(&app, "gone").await;
    memory.sheets.lock().unwrap().remove("gone");
    assert!(stortop::source::sync(&app, &gone).await.is_err());
    let state: (String, Option<i64>) =
        sqlx::query_as("SELECT status,failing_since FROM stores WHERE id=?")
            .bind(&gone.0)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(state.0, "active");
    assert!(state.1.is_some());
    sqlx::query("UPDATE stores SET failing_since=? WHERE id=?")
        .bind(stortop::time::now() - 7 * 86400)
        .bind(&gone.0)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(stortop::source::sync(&app, &gone).await.is_err());
    let status: String = sqlx::query_scalar("SELECT status FROM stores WHERE id=?")
        .bind(&gone.0)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "inactive");
    let mut h = HeaderMap::new();
    h.insert("host", "gone.stor.localhost:8081".parse().unwrap());
    assert!(stortop::stores::from_host(&app, &h).await.is_err());
    // The address stays taken. The sheet may start a new store.
    memory.add("gone", vec![]);
    let (status, body) = create(&app, json!({"subdomain":"gone","source":"mem:gone"}), &[]).await;
    assert_eq!(body["error"], "subdomain_taken", "{status}");
    let (status, _) = create(&app, json!({"subdomain":"again","source":"mem:gone"}), &[]).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn paid_orders_are_appended_to_history_once() {
    let (api, _) = payment_server().await;
    let (app, memory) = stor(&api).await;
    memory.add("sheet", vec![]);
    let payments = json!({"url": format!("{api}/v1/projects/shop"), "key": "prf_key"});
    create(
        &app,
        json!({"subdomain":"shop","source":"mem:sheet","payments":payments}),
        &[],
    )
    .await;
    let shop = store(&app, "shop").await;
    stortop::source::sync(&app, &shop).await.unwrap();
    let connection: String = sqlx::query_scalar("SELECT id FROM connections WHERE store_id=?")
        .bind(&shop.0)
        .fetch_one(&app.db)
        .await
        .unwrap();
    let customer = stortop::crypto::hash("customer");
    sqlx::query("INSERT INTO customers(store_id,id_hash,created_at) VALUES(?,?,0)")
        .bind(&shop.0)
        .bind(&customer)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO orders(store_id,id,customer_hash,connection_id,reference,status,chain,asset,expected_amount,address,items_json,delivery_ciphertext,created_at,expires_at) VALUES(?,'ord_1',?,?,'ord_1','awaiting_payment','eip155:1',?,'50000000','0xabc',?,'unused',86400,?)")
        .bind(&shop.0)
        .bind(&customer)
        .bind(&connection)
        .bind(USDC)
        .bind(r#"[{"sku":"license","title":"Pro license","quantity":2,"unit_amount":"25000000"}]"#)
        .bind(stortop::time::now() + 600)
        .execute(&app.db)
        .await
        .unwrap();
    for status in ["delivered", "delivered"] {
        sqlx::query("UPDATE orders SET status=?,transaction_id='0xtx' WHERE id='ord_1'")
            .bind(status)
            .execute(&app.db)
            .await
            .unwrap();
    }
    stortop::source::history::tick(&app).await.unwrap();
    stortop::source::history::tick(&app).await.unwrap();
    let history = memory.tab("sheet", "History");
    assert_eq!(history.rows.len(), 2);
    assert_eq!(
        history.rows[1],
        [
            "86400",
            "ord_1",
            "license",
            "Pro license",
            "2",
            "50",
            "USDC",
            "Ethereum",
            "delivered",
            "0xtx"
        ]
    );
}

#[tokio::test]
async fn stor_answers_on_its_origin_and_aliases_only() {
    let (api, _) = payment_server().await;
    let (app, _) = stor(&api).await;
    let mut config = app.config.clone();
    config.origin_aliases = vec!["http://127.0.0.1:8081".into()];
    config.store_suffix_aliases = vec!["stor.home:8081".into()];
    let app = StorState::new(
        config,
        app.db.clone(),
        Zeroizing::new([7; 32]),
        Zeroizing::new("test".into()),
        reqwest::Client::new(),
        app.sources.clone(),
    );
    for (host, status) in [
        ("stor:8081", StatusCode::OK),
        ("127.0.0.1:8081", StatusCode::OK),
        ("elsewhere:8081", StatusCode::NOT_FOUND),
        ("shop.stor.localhost:8081", StatusCode::NOT_FOUND),
    ] {
        let response = stortop::http::router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/v1/info")
                    .header("host", host)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{host}");
        if status == StatusCode::OK {
            let body: Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                body,
                json!({"store_suffix":"stor.localhost:8081",
                       "share_with":"stor@example.iam.gserviceaccount.com"})
            );
        }
    }
    // A store answers under every suffix. Nothing else does.
    sqlx::query("INSERT INTO stores(id,slug,source_kind,source_id,metadata_json,created_at) VALUES('s','shop','google','x','{}',0)")
        .execute(&app.db)
        .await
        .unwrap();
    for (host, found) in [
        ("shop.stor.localhost:8081", true),
        ("shop.stor.home:8081", true),
        ("shop.elsewhere:8081", false),
        ("stor.home:8081", false),
    ] {
        let mut h = HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert_eq!(
            stortop::stores::from_host(&app, &h).await.is_ok(),
            found,
            "{host}"
        );
    }
}
