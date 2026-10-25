use serde::Deserialize;
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bind: std::net::SocketAddr,
    pub origin: String,
    /// Other origins of the same server for its own page and store creation, e.g. its
    /// loopback address next to a LAN one. Webhooks always use `origin`.
    #[serde(default)]
    pub origin_aliases: Vec<String>,
    pub store_suffix: String,
    /// Other host suffixes that reach the same stores, e.g. `stor.localhost:8081` on
    /// the machine itself next to a LAN one. Store links use `store_suffix`.
    #[serde(default)]
    pub store_suffix_aliases: Vec<String>,
    pub database: String,
    /// The Payroof servers (origins) stores may use for payments: at least one.
    pub payroof_origins: Vec<String>,
    /// Allows HTTP origins and non-`Secure` cookies. Never in production.
    #[serde(default)]
    pub development: bool,
    /// A service-account key file. Without one, no store can be created.
    pub google_credentials: Option<String>,
    /// How often each store's source is read again.
    #[serde(default = "sync_seconds")]
    pub sync_seconds: i64,
    /// A store whose source can't be read for this long is deactivated for good.
    #[serde(default = "deactivate_after_days")]
    pub deactivate_after_days: i64,
    #[serde(default)]
    pub trusted_proxies: Vec<crate::trusted_proxy::Cidr>,
    /// Store names refused whole (impersonation, phishing bait). Given, it replaces the
    /// built-in list. Stor's technical names stay reserved either way. See `reserved`.
    #[serde(default = "crate::reserved::default_names")]
    pub reserved_names: Vec<String>,
    /// Words refused anywhere in a store name, as one of its hyphen-separated parts.
    /// Given, it replaces the built-in list.
    #[serde(default = "crate::reserved::default_words")]
    pub blocked_words: Vec<String>,
    #[serde(default)]
    pub limits: Limits,
}
fn sync_seconds() -> i64 {
    120
}
fn deactivate_after_days() -> i64 {
    7
}
#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub json_body: usize,
    pub webhook_body: usize,
    pub request_timeout_seconds: u64,
    pub concurrency: usize,
    pub customers_per_store: i64,
    pub ordinary_per_minute: i64,
    pub proof_per_minute: i64,
    pub create_per_minute: i64,
    /// Stores one client address may create per day.
    pub stores_per_day: i64,
    pub rate_limit_entries: i64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            json_body: 32768,
            webhook_body: 16384,
            request_timeout_seconds: 30,
            concurrency: 64,
            customers_per_store: 10000,
            ordinary_per_minute: 120,
            proof_per_minute: 10,
            create_per_minute: 3,
            stores_per_day: 10,
            rate_limit_entries: 100000,
        }
    }
}
impl Limits {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=1024 * 1024).contains(&self.json_body)
                && (1..=1024 * 1024).contains(&self.webhook_body),
            "invalid body limits"
        );
        anyhow::ensure!(
            (1..=300).contains(&self.request_timeout_seconds)
                && (1..=1024).contains(&self.concurrency),
            "invalid execution limits"
        );
        for n in [
            self.customers_per_store,
            self.ordinary_per_minute,
            self.proof_per_minute,
            self.create_per_minute,
            self.stores_per_day,
            self.rate_limit_entries,
        ] {
            anyhow::ensure!((1..=1000000).contains(&n), "invalid quota");
        }
        Ok(())
    }
}
impl Config {
    /// Reads and validates the TOML file at `path`.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let c: Self = toml::from_str(&std::fs::read_to_string(path)?)?;
        for origin in std::iter::once(&c.origin).chain(&c.origin_aliases) {
            let u = url::Url::parse(origin)?;
            anyhow::ensure!(
                u.path() == "/"
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.query().is_none()
                    && u.fragment().is_none(),
                "invalid origin"
            );
            anyhow::ensure!(
                u.host_str().is_some()
                    && (u.scheme() == "https" || (c.development && u.scheme() == "http")),
                "HTTPS required"
            );
        }
        anyhow::ensure!(
            std::iter::once(&c.store_suffix)
                .chain(&c.store_suffix_aliases)
                .all(|s| !s.is_empty() && !s.contains('/')),
            "invalid store suffix"
        );
        // Entries are names as stores have them: lowercase letters, digits and hyphens.
        for entry in c.reserved_names.iter().chain(&c.blocked_words) {
            anyhow::ensure!(
                (1..=40).contains(&entry.len())
                    && entry
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && !entry.starts_with('-')
                    && !entry.ends_with('-'),
                "invalid reserved_names or blocked_words entry: {entry:?} (lowercase letters, digits, inner hyphens)"
            );
        }
        anyhow::ensure!(
            !c.payroof_origins.is_empty(),
            "payroof_origins must list at least one Payroof server"
        );
        for origin in &c.payroof_origins {
            let u = url::Url::parse(origin)?;
            anyhow::ensure!(
                u.host_str().is_some()
                    && u.path() == "/"
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.query().is_none()
                    && u.fragment().is_none()
                    && (u.scheme() == "https" || (c.development && u.scheme() == "http")),
                "invalid payroof_origins entry"
            );
        }
        anyhow::ensure!(
            (30..=3600).contains(&c.sync_seconds) && (1..=365).contains(&c.deactivate_after_days),
            "invalid sync schedule"
        );
        c.limits.validate()?;
        Ok(c)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn useful_limits_are_bounded() {
        let mut limits = Limits::default();
        assert!(limits.validate().is_ok());
        limits.concurrency = 0;
        assert!(limits.validate().is_err());
        limits = Limits::default();
        limits.json_body = usize::MAX;
        assert!(limits.validate().is_err());
        limits = Limits::default();
        limits.rate_limit_entries = 0;
        assert!(limits.validate().is_err());
        assert!(toml::from_str::<Limits>("unknown_limit=1").is_err());
    }
    #[test]
    fn the_example_file_is_valid_and_lists_the_default_limits() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/config/stortop.example.toml");
        let config = Config::load(path).unwrap();
        let defaults = Limits::default();
        assert_eq!(config.limits.json_body, defaults.json_body);
        assert_eq!(config.limits.webhook_body, defaults.webhook_body);
        assert_eq!(
            config.limits.request_timeout_seconds,
            defaults.request_timeout_seconds
        );
        assert_eq!(config.limits.concurrency, defaults.concurrency);
        assert_eq!(
            config.limits.customers_per_store,
            defaults.customers_per_store
        );
        assert_eq!(
            config.limits.ordinary_per_minute,
            defaults.ordinary_per_minute
        );
        assert_eq!(config.limits.proof_per_minute, defaults.proof_per_minute);
        assert_eq!(config.limits.create_per_minute, defaults.create_per_minute);
        assert_eq!(config.limits.stores_per_day, defaults.stores_per_day);
        assert_eq!(
            config.limits.rate_limit_entries,
            defaults.rate_limit_entries
        );
        assert_eq!(
            (config.sync_seconds, config.deactivate_after_days),
            (120, 7)
        );
    }
}
