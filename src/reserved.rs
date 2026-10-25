//! Store names no one may take. Three lists:
//!
//! - [`TECHNICAL`]: always reserved, whatever the configuration says. Taken, they
//!   would pose as the service (`www`, `api`, `login`), its infrastructure (`mail`,
//!   `ns1`) or the projects around it (`payroof`, `wallet`).
//! - `reserved_names` (configured, [`NAMES`] by default): whole names refused as
//!   impersonation or phishing bait, e.g. well-known brands, banks and authorities.
//! - `blocked_words` (configured, [`WORDS`] by default): words no name may contain
//!   as one of its hyphen-separated parts, for ethical reasons (sexual content, abuse,
//!   hate, violence, illegal trade, fraud). `free-porn` is refused, `sussex` is not:
//!   words are matched whole, so a word inside another one is never a false match.
//!   Compounds without hyphens (`pornshop`) are refused only when listed themselves.
//!
//! Every list is also matched with the name's hyphens removed, so `pay-pal` is
//! `paypal` and `p-o-r-n` is `porn`. Only creation is checked: a store keeps working
//! when its name is listed later (the operator can deactivate it).

use crate::config::Config;

pub const TECHNICAL: &[&str] = &[
    // The service and its neighbours.
    "stor",
    "stortop",
    "payroof",
    "wallet",
    "roofer",
    "official",
    "team",
    "staff",
    // Web and API entry points.
    "www",
    "www1",
    "www2",
    "web",
    "api",
    "app",
    "apps",
    "admin",
    "administrator",
    "dashboard",
    "console",
    "panel",
    "portal",
    "manage",
    "management",
    "static",
    "assets",
    "cdn",
    "media",
    "img",
    "images",
    "files",
    "download",
    "downloads",
    "upload",
    "uploads",
    "docs",
    "doc",
    "help",
    "support",
    "status",
    "health",
    "healthz",
    "readyz",
    "metrics",
    "monitor",
    "blog",
    "news",
    "about",
    "legal",
    "terms",
    "tos",
    "privacy",
    "security",
    "abuse",
    "contact",
    "jobs",
    "careers",
    "press",
    // Mail, names and other services on a domain.
    "mail",
    "email",
    "smtp",
    "imap",
    "pop",
    "pop3",
    "mx",
    "webmail",
    "autodiscover",
    "autoconfig",
    "postmaster",
    "hostmaster",
    "webmaster",
    "noc",
    "ns",
    "ns1",
    "ns2",
    "ns3",
    "dns",
    "ftp",
    "sftp",
    "ssh",
    "vpn",
    "proxy",
    "gateway",
    "git",
    "svn",
    // Environments and placeholders.
    "localhost",
    "local",
    "host",
    "server",
    "dev",
    "test",
    "testing",
    "staging",
    "stage",
    "beta",
    "alpha",
    "demo",
    "sandbox",
    "preview",
    "internal",
    "root",
    "system",
    "null",
    "undefined",
    "default",
    "example",
    // Accounts and money: what a phishing page would be called.
    "account",
    "accounts",
    "auth",
    "oauth",
    "sso",
    "login",
    "logout",
    "signin",
    "signup",
    "register",
    "password",
    "reset",
    "verify",
    "verification",
    "secure",
    "billing",
    "pay",
    "payment",
    "payments",
    "checkout",
    "invoice",
    "invoices",
    "webhook",
    "webhooks",
    "recovery",
    "refund",
    "refunds",
];

/// Whole names reserved by default: brands, exchanges, wallets, banks, networks and
/// authorities a store could pose as, and scam bait.
pub const NAMES: &[&str] = &[
    // Companies and payment brands.
    "google",
    "gmail",
    "youtube",
    "apple",
    "icloud",
    "microsoft",
    "outlook",
    "amazon",
    "meta",
    "facebook",
    "instagram",
    "whatsapp",
    "telegram",
    "twitter",
    "discord",
    "tiktok",
    "paypal",
    "stripe",
    "visa",
    "mastercard",
    "amex",
    "wise",
    "revolut",
    "venmo",
    "cashapp",
    "western-union",
    "moneygram",
    // Exchanges, wallets and networks.
    "binance",
    "coinbase",
    "kraken",
    "bybit",
    "okx",
    "kucoin",
    "bitfinex",
    "gemini",
    "crypto-com",
    "metamask",
    "trustwallet",
    "phantom",
    "ledger",
    "trezor",
    "exodus",
    "uniswap",
    "opensea",
    "etherscan",
    "ethereum",
    "bitcoin",
    "solana",
    "polygon",
    "arbitrum",
    "optimism",
    "base",
    "tether",
    "usdt",
    "usdc",
    "circle",
    // Banks and authorities.
    "bank",
    "banking",
    "irs",
    "hmrc",
    "fbi",
    "cia",
    "police",
    "interpol",
    "gov",
    "government",
    "court",
    "tax",
    "taxes",
    "un",
    "unicef",
    "who",
    "redcross",
    // Scam bait.
    "airdrop",
    "airdrops",
    "giveaway",
    "giveaways",
    "claim",
    "claims",
    "free-crypto",
    "double-your-crypto",
    "investment",
    "invest",
    "profit",
    "lottery",
    "prize",
    "winner",
];

/// Words refused anywhere in a name by default (as a hyphen-separated part).
pub const WORDS: &[&str] = &[
    // Sexual content.
    "porn",
    "porno",
    "pornography",
    "p0rn",
    "pr0n",
    "xxx",
    "sex",
    "s3x",
    "sexy",
    "sexshop",
    "nsfw",
    "nude",
    "nudes",
    "naked",
    "erotic",
    "erotica",
    "adult",
    "escort",
    "escorts",
    "hooker",
    "hookers",
    "prostitute",
    "prostitution",
    "camgirl",
    "camgirls",
    "webcam-sex",
    "onlyfans",
    "pornhub",
    "xvideos",
    "xhamster",
    "chaturbate",
    "fetish",
    "bdsm",
    "hentai",
    "milf",
    "incest",
    "orgy",
    "cum",
    "dick",
    "cock",
    "pussy",
    "tits",
    "boobs",
    // Child abuse: never.
    "pedo",
    "pedophile",
    "paedo",
    "paedophile",
    "loli",
    "lolita",
    "shota",
    "childporn",
    "cp",
    "underage",
    "jailbait",
    // Violence, hate and self-harm.
    "rape",
    "rapist",
    "bestiality",
    "zoophilia",
    "gore",
    "snuff",
    "murder",
    "kill",
    "killer",
    "suicide",
    "selfharm",
    "self-harm",
    "terror",
    "terrorist",
    "terrorism",
    "isis",
    "jihad",
    "nazi",
    "nazis",
    "hitler",
    "kkk",
    "genocide",
    "nigger",
    "nigga",
    "faggot",
    "retard",
    "kike",
    "chink",
    "spic",
    "tranny",
    // Illegal trade.
    "drugs",
    "cocaine",
    "heroin",
    "meth",
    "fentanyl",
    "mdma",
    "lsd",
    "darknet",
    "darkweb",
    "weapons",
    "firearms",
    "guns",
    "ammo",
    "explosives",
    "counterfeit",
    "fake-id",
    "fakeid",
    "stolen",
    "carding",
    "cvv",
    "fullz",
    "dumps",
    "ransomware",
    "malware",
    "botnet",
    "ddos",
    "phishing",
    "hacked",
    "exploit-kit",
    // Fraud and gambling.
    "scam",
    "scams",
    "scammer",
    "fraud",
    "ponzi",
    "pyramid-scheme",
    "casino",
    "gambling",
    "betting",
    "slots",
];

/// The built-in `reserved_names`.
pub fn default_names() -> Vec<String> {
    NAMES.iter().map(|s| s.to_string()).collect()
}
/// The built-in `blocked_words`.
pub fn default_words() -> Vec<String> {
    WORDS.iter().map(|s| s.to_string()).collect()
}

/// Whether a well-formed store name (see `stores::valid_slug`) is refused.
pub fn is_reserved(config: &Config, slug: &str) -> bool {
    let joined = slug.replace('-', "");
    named(TECHNICAL.iter().copied(), slug, &joined)
        || named(
            config.reserved_names.iter().map(String::as_str),
            slug,
            &joined,
        )
        || config
            .blocked_words
            .iter()
            .any(|word| word.replace('-', "") == joined || contains_parts(slug, word))
}
/// Whether a list names `slug` whole, with or without its hyphens.
fn named<'a>(mut list: impl Iterator<Item = &'a str>, slug: &str, joined: &str) -> bool {
    list.any(|n| n == slug || n.replace('-', "") == joined)
}
/// Whether `word` (itself one part or several joined by hyphens) appears in `slug` as
/// whole hyphen-separated parts.
fn contains_parts(slug: &str, word: &str) -> bool {
    let parts: Vec<&str> = slug.split('-').collect();
    let wanted: Vec<&str> = word.split('-').collect();
    parts.windows(wanted.len()).any(|w| w == wanted.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(names: &[&str], words: &[&str]) -> Config {
        let mut c: Config = toml::from_str(
            r#"bind="127.0.0.1:1"
origin="http://stor"
store_suffix="stor.localhost"
database="sqlite::memory:"
payroof_origins=["http://api"]
development=true
"#,
        )
        .unwrap();
        c.reserved_names = names.iter().map(|s| s.to_string()).collect();
        c.blocked_words = words.iter().map(|s| s.to_string()).collect();
        c
    }
    #[test]
    fn defaults_refuse_service_names_brands_and_unethical_words() {
        let c = config(NAMES, WORDS);
        for slug in [
            "www",
            "api",
            "login",
            "payroof",
            "paypal",
            "pay-pal",
            "porn",
            "free-porn",
            "porn-shop",
            "best-sex-toys",
            "p-o-r-n",
            "xxx-videos",
            "fake-id-shop",
            "cp",
        ] {
            assert!(is_reserved(&c, slug), "{slug}");
        }
        // Whole words only: no false match inside another word.
        for slug in [
            "sussex-tea",
            "essex",
            "therapist",
            "analytics",
            "cockpit-books",
            "scunthorpe",
            "class-notes",
            "my-store",
            "shop-cpu",
            "basement",
        ] {
            assert!(!is_reserved(&c, slug), "{slug}");
        }
    }
    #[test]
    fn the_example_configuration_lists_exactly_the_defaults() {
        let example: toml::Table =
            toml::from_str(include_str!("../config/stortop.example.toml")).unwrap();
        let list = |key: &str| -> Vec<String> {
            example[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(list("reserved_names"), default_names());
        assert_eq!(list("blocked_words"), default_words());
    }
    #[test]
    fn configuration_replaces_the_lists_but_never_the_technical_names() {
        let c = config(&["acme"], &["spam"]);
        assert!(is_reserved(&c, "acme"));
        assert!(is_reserved(&c, "cheap-spam-here"));
        assert!(!is_reserved(&c, "paypal"), "the default list was replaced");
        assert!(is_reserved(&c, "www") && is_reserved(&c, "admin"));
        let none = config(&[], &[]);
        assert!(is_reserved(&none, "api"));
        assert!(!is_reserved(&none, "porn"));
    }
}
