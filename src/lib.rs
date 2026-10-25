//! Stor: multi-tenant storefronts with exact cryptocurrency checkout and confirmed
//! delivery. Each store is a subdomain mapped to one source (a Google Sheet), and
//! payments go through a Payroof server.
//!
//! The `stortop` binary is [`startup`] plus [`http::router`] and the two [`workers`].
//! To embed Stor with a custom [`source::Source`] (an in-memory one in tests, say),
//! build a [`State`] with [`State::new`] and serve it the same way:
//!
//! ```ignore
//! let app = stortop::State::new(config, db, master, altcha_key, http, vec![source]);
//! let workers = [stortop::workers::start(app.clone()), stortop::workers::start_sync(app.clone())];
//! let router = stortop::http::router(app.clone())
//!     .into_make_service_with_connect_info::<std::net::SocketAddr>();
//! axum::serve(listener, router).await?;
//! ```
//!
//! `db` comes from [`db::connect`], which also runs the migrations. The server reads the
//! client address from the connection info, so serve it with
//! `into_make_service_with_connect_info`. When calling the router directly, add a
//! `ConnectInfo<SocketAddr>` request extension.
pub mod altcha;
pub mod auth;
pub mod commerce;
pub mod config;
pub mod crypto;
pub mod db;
pub mod fetch;
pub mod http;
pub mod limits;
pub mod payroof;
pub mod reserved;
pub mod source;
pub mod stores;
pub mod time;
pub mod trusted_proxy;
pub mod workers;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Everything a request or worker needs. Share it as an [`App`].
pub struct State {
    pub db: sqlx::SqlitePool,
    pub config: config::Config,
    /// Encrypts secrets at rest (payment keys, webhook secrets, delivery content).
    pub master: Zeroizing<[u8; 32]>,
    /// The HMAC secret that signs the anti-abuse challenges. [`startup`] sets it to the
    /// hex of `STORTOP_ALTCHA_KEY`. A test that makes its own challenges signs them with
    /// the same value.
    pub altcha_key: Zeroizing<String>,
    /// The client for every outgoing call (Payroof servers, Google). Build it without
    /// redirects and proxies, as [`startup`] does.
    pub http: reqwest::Client,
    /// The kinds of source stores can use: none without credentials.
    pub sources: Vec<Arc<dyn source::Source>>,
    /// When Stor last made each store's sheet fetch its rates again (Unix seconds).
    rates_refreshed: std::sync::Mutex<std::collections::HashMap<String, i64>>,
}
impl State {
    pub fn new(
        config: config::Config,
        db: sqlx::SqlitePool,
        master: Zeroizing<[u8; 32]>,
        altcha_key: Zeroizing<String>,
        http: reqwest::Client,
        sources: Vec<Arc<dyn source::Source>>,
    ) -> App {
        Arc::new(Self {
            db,
            config,
            master,
            altcha_key,
            http,
            sources,
            rates_refreshed: Default::default(),
        })
    }
    /// The source of this kind (see [`source::Source::kind`]), if Stor has one.
    pub fn source(&self, kind: &str) -> Option<&dyn source::Source> {
        self.sources
            .iter()
            .find(|s| s.kind() == kind)
            .map(|s| s.as_ref())
    }
}
pub type App = Arc<State>;

/// Opens the database and reads the keys (`STORTOP_MASTER_KEY`, `STORTOP_ALTCHA_KEY`,
/// each 32 random bytes as hex, and different) from the environment. Google is a
/// source only when the config names credentials.
pub async fn startup(config: config::Config) -> anyhow::Result<App> {
    let db = db::connect(&config.database).await?;
    let master = crypto::env_key("STORTOP_MASTER_KEY")?;
    let key = crypto::env_key("STORTOP_ALTCHA_KEY")?;
    anyhow::ensure!(
        *master != *key,
        "STORTOP_MASTER_KEY and STORTOP_ALTCHA_KEY must differ"
    );
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut sources: Vec<Arc<dyn source::Source>> = Vec::new();
    if let Some(path) = &config.google_credentials {
        sources.push(Arc::new(source::google::Google::load(path, http.clone())?));
    }
    Ok(State::new(
        config,
        db,
        master,
        Zeroizing::new(hex::encode(*key)),
        http,
        sources,
    ))
}
