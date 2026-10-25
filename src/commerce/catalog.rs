use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
/// A price option. In catalogs (CSV, Google Sheets) and storage the amount is a decimal
/// in the token's unit, as the merchant wrote it ("0.001"). What `persisted_payments`
/// returns, and everything after it, holds the exact smallest-unit integer.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    pub asset: String,
    pub amount: String,
}
/// Token decimals by asset ID, as the store's payment connection reports them.
pub type Decimals = BTreeMap<String, u32>;
/// A catalog amount: a positive plain decimal, like "25", "0.5" or "0.001".
pub fn valid_amount(s: &str) -> bool {
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    s.len() <= 80
        && !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && s.contains('.') != fraction.is_empty()
        && fraction.len() <= 36
        && fraction.bytes().all(|b| b.is_ascii_digit())
        && s.bytes().any(|b| (b'1'..=b'9').contains(&b))
}
/// The exact smallest-unit integer of a catalog amount: ("0.001", 18) is
/// "1000000000000000". Never rounds: more decimal places than the token has fails.
pub fn atomic_amount(s: &str, decimals: u32) -> anyhow::Result<String> {
    anyhow::ensure!(valid_amount(s), "invalid amount");
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    let fraction = fraction.trim_end_matches('0');
    anyhow::ensure!(fraction.len() <= decimals as usize, "amount too precise");
    let digits = format!("{whole}{fraction:0<width$}", width = decimals as usize);
    let digits = digits.trim_start_matches('0');
    let n = super::amount::atomic(if digits.is_empty() { "0" } else { digits })?;
    anyhow::ensure!(n > num_bigint::BigUint::default(), "invalid amount");
    Ok(n.to_string())
}
/// Most payment options one product resolves to (one per token and network).
pub const MAX_OPTIONS: usize = 32;
/// A price as the merchant writes it: an amount and a unit, `25 USD` or `25 USDC`.
#[derive(Clone, Debug, PartialEq)]
pub struct Price {
    pub amount: String,
    /// `USD`, or a token as the Tokens tab names it.
    pub unit: String,
}
impl Price {
    fn usd(&self) -> bool {
        self.unit.eq_ignore_ascii_case("USD")
    }
    /// How a buyer reads it: `$25`, `25 USDC`.
    pub fn label(&self) -> String {
        if self.usd() {
            format!("${}", self.amount)
        } else {
            format!("{} {}", self.amount, self.unit)
        }
    }
}
/// A token the store accepts: a row of the Tokens tab, joined with what the payment
/// server says about its asset.
#[derive(Clone, Debug)]
pub struct Token {
    pub symbol: String,
    pub asset: String,
    pub decimals: u32,
    /// US dollars per whole token. Without one, the token takes only prices in itself.
    pub rate: Option<f64>,
}
/// A Price cell: one price per line (or separated by commas), like `25 USD` or
/// `25 USDC` and `0.01 ETH` on two lines.
pub fn prices(cell: &str) -> anyhow::Result<Vec<Price>> {
    let mut prices = Vec::new();
    for part in cell
        .split([',', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (amount, unit) = part
            .split_once(char::is_whitespace)
            .ok_or_else(|| anyhow::anyhow!("write a price as amount and unit, like 25 USD"))?;
        let unit = unit.trim();
        anyhow::ensure!(
            valid_amount(amount),
            "{amount} is not an amount. Write it like 25 or 0.01"
        );
        anyhow::ensure!(
            (1..=16).contains(&unit.len())
                && unit
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "{unit} is not USD or a token"
        );
        anyhow::ensure!(
            !prices
                .iter()
                .any(|p: &Price| p.unit.eq_ignore_ascii_case(unit)),
            "{unit} is priced twice"
        );
        prices.push(Price {
            amount: amount.into(),
            unit: unit.into(),
        });
    }
    anyhow::ensure!(!prices.is_empty(), "add a price, like 25 USD");
    Ok(prices)
}
/// The payment options of a product's prices. A token price (`25 USDC`) is that amount
/// of every Tokens row with that name, one per network. A dollar price (`25 USD`) is
/// worth that much in every other token with a rate. A token priced in itself keeps
/// that price.
pub fn resolve(prices: &[Price], tokens: &[Token]) -> anyhow::Result<Vec<Payment>> {
    let mut options = Vec::new();
    let (dollars, fixed): (Vec<&Price>, Vec<&Price>) = prices.iter().partition(|p| p.usd());
    for price in fixed {
        let mut found = false;
        for token in tokens
            .iter()
            .filter(|t| t.symbol.eq_ignore_ascii_case(&price.unit))
        {
            atomic_amount(&price.amount, token.decimals).map_err(|_| {
                anyhow::anyhow!(
                    "{} has more decimals than {} has",
                    price.label(),
                    token.symbol
                )
            })?;
            options.push(Payment {
                asset: token.asset.clone(),
                amount: price.amount.clone(),
            });
            found = true;
        }
        anyhow::ensure!(found, "{} is not in the Tokens tab", price.unit);
    }
    if let Some(price) = dollars.first() {
        let usd: f64 = price.amount.parse()?;
        let mut found = false;
        for token in tokens {
            let Some(rate) = token.rate else { continue };
            found = true;
            if !options.iter().any(|o: &Payment| o.asset == token.asset) {
                options.push(Payment {
                    asset: token.asset.clone(),
                    amount: in_token(usd, rate, token.decimals),
                });
            }
        }
        anyhow::ensure!(found, "no token in the Tokens tab has a USD rate");
    }
    anyhow::ensure!(
        options.len() <= MAX_OPTIONS,
        "more than {MAX_OPTIONS} payment options"
    );
    Ok(options)
}
/// Dollars in a token at `rate`, rounded up to six significant digits (never more
/// decimals than the token has): $25 at 4012.5 is 0.00623053 ETH.
pub fn in_token(usd: f64, rate: f64, decimals: u32) -> String {
    let value = usd / rate;
    let places = (5 - value.log10().floor() as i32).clamp(0, decimals as i32);
    let scale = 10f64.powi(places);
    // A hair under the exact product, so 6.25 stays 6.25 instead of 6.250001.
    let units = (value * scale * (1.0 - 1e-12)).ceil();
    let text = format!("{:.*}", places as usize, units / scale);
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    if valid_amount(&text) {
        text
    } else {
        // Smaller than the token's smallest unit: one unit.
        format!("0.{}1", "0".repeat(decimals.saturating_sub(1) as usize))
    }
}
/// Stored options with exact integer amounts. An option whose token decimals aren't
/// known (no payment connection yet) or can't hold its amount exactly is left out.
pub fn persisted_payments(raw: &str, decimals: &Decimals) -> anyhow::Result<Vec<Payment>> {
    let payments: Vec<Payment> = serde_json::from_str(raw)?;
    anyhow::ensure!(
        (1..=MAX_OPTIONS).contains(&payments.len()),
        "invalid persisted payment count"
    );
    let mut assets = BTreeSet::new();
    let mut exact = Vec::new();
    for p in &payments {
        asset(&p.asset)?;
        anyhow::ensure!(
            assets.insert(&p.asset) && valid_amount(&p.amount),
            "invalid persisted payment"
        );
        if let Some(amount) = decimals
            .get(&p.asset)
            .and_then(|d| atomic_amount(&p.amount, *d).ok())
        {
            exact.push(Payment {
                asset: p.asset.clone(),
                amount,
            });
        }
    }
    Ok(exact)
}
/// Store presentation, from the source's Branding tab.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub title: String,
    pub description: String,
    pub website: Option<String>,
    /// Custom CSS, from the Style cell without its leading theme word.
    pub style: String,
    /// `auto` (the visitor's system and toggle), or `light` or `dark` (forced).
    #[serde(default = "auto_theme")]
    pub theme: String,
}
fn auto_theme() -> String {
    "auto".into()
}
/// Splits a Style cell into its theme and CSS. A first word of `light`, `dark` or
/// `auto` (any case) is the theme and the rest is CSS. Otherwise the whole cell is CSS
/// and the theme is `auto`, which is how Style always read.
pub fn split_style(cell: &str) -> (&'static str, String) {
    let cell = cell.trim();
    let (word, rest) = cell.split_once(char::is_whitespace).unwrap_or((cell, ""));
    for theme in ["light", "dark", "auto"] {
        if word.eq_ignore_ascii_case(theme) {
            return (theme, rest.trim().to_string());
        }
    }
    ("auto", cell.to_string())
}
impl Metadata {
    /// A new store is titled with its address until its Branding is read.
    pub fn new(slug: &str) -> Self {
        Self {
            title: slug.into(),
            description: String::new(),
            website: None,
            style: String::new(),
            theme: auto_theme(),
        }
    }
}
/// One Catalog row, its payment options resolved against the payment server's tokens.
#[derive(Clone)]
pub struct Product {
    pub sku: String,
    pub title: String,
    pub description: String,
    /// The first price as written, for display: `$25`, `25 USDC`.
    pub price: String,
    pub payments: Vec<Payment>,
    pub quantity: i64,
    pub images: Vec<String>,
    pub delivery: zeroize::Zeroizing<String>,
}
pub const MAX_IMAGES: usize = 5;
/// Product images: up to five HTTPS URLs separated by spaces, commas or new lines.
pub fn images(cell: &str) -> anyhow::Result<Vec<String>> {
    let images: Vec<String> = cell
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    anyhow::ensure!(images.len() <= MAX_IMAGES, "too many images");
    for image in &images {
        let u = url::Url::parse(image)?;
        anyhow::ensure!(
            image.len() <= 2048
                && u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none(),
            "invalid image URL"
        );
    }
    Ok(images)
}
pub fn asset(s: &str) -> anyhow::Result<String> {
    let (chain, asset) = s
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("invalid asset"))?;
    let id: u64 = chain
        .strip_prefix("eip155:")
        .ok_or_else(|| anyhow::anyhow!("unsupported family"))?
        .parse()?;
    anyhow::ensure!(id > 0 && chain == format!("eip155:{id}"), "invalid chain");
    if let Some(contract) = asset.strip_prefix("erc20:") {
        anyhow::ensure!(
            contract.len() == 42
                && contract.starts_with("0x")
                && contract[2..].bytes().all(|b| b.is_ascii_hexdigit())
                && contract == contract.to_ascii_lowercase(),
            "invalid contract"
        );
    } else {
        let n: u32 = asset
            .strip_prefix("slip44:")
            .ok_or_else(|| anyhow::anyhow!("unsupported asset"))?
            .parse()?;
        anyhow::ensure!(asset == format!("slip44:{n}"), "invalid native asset");
    }
    Ok(chain.into())
}
/// Store presentation: a title, and bounded description, website and style.
pub fn validate_metadata(metadata: &Metadata) -> anyhow::Result<()> {
    anyhow::ensure!(
        !metadata.title.trim().is_empty()
            && metadata.title.len() <= 200
            && metadata.description.len() <= 4000
            && metadata.style.len() <= 65536
            && ["auto", "light", "dark"].contains(&metadata.theme.as_str()),
        "invalid metadata"
    );
    if let Some(w) = &metadata.website {
        let u = url::Url::parse(w)?;
        anyhow::ensure!(
            u.scheme() == "https" && u.username().is_empty() && u.password().is_none(),
            "invalid website URL"
        );
    }
    Ok(())
}
pub fn valid_sku(sku: &str) -> bool {
    !sku.is_empty()
        && sku.len() <= 64
        && sku
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}
/// A complete catalog snapshot: at most 1000 products with unique SKUs.
pub fn validate_products(products: &[Product]) -> anyhow::Result<()> {
    anyhow::ensure!(
        products.len() <= 1000,
        "the Catalog has more than 1000 products"
    );
    let mut skus = BTreeSet::new();
    for p in products {
        anyhow::ensure!(
            valid_sku(&p.sku)
                && skus.insert(&p.sku)
                && !p.title.is_empty()
                && p.title.len() <= 200
                && p.description.len() <= 4000
                && p.delivery.len() <= 65536
                && (0..=1000000).contains(&p.quantity)
                && p.images.len() <= MAX_IMAGES,
            "invalid product {}",
            p.sku
        );
        let mut assets = BTreeSet::new();
        anyhow::ensure!(
            (1..=MAX_OPTIONS).contains(&p.payments.len()),
            "invalid payment count"
        );
        for option in &p.payments {
            asset(&option.asset)?;
            anyhow::ensure!(
                assets.insert(&option.asset) && valid_amount(&option.amount),
                "invalid payment"
            );
        }
    }
    Ok(())
}
#[cfg(test)]
mod amount_tests {
    use super::*;
    #[test]
    fn catalog_amounts_are_exact_decimals() {
        assert_eq!(atomic_amount("0.001", 18).unwrap(), "1000000000000000");
        assert_eq!(atomic_amount("25", 6).unwrap(), "25000000");
        assert_eq!(atomic_amount("1.50", 2).unwrap(), "150");
        assert_eq!(atomic_amount("0.000001", 6).unwrap(), "1");
        assert_eq!(atomic_amount("7", 0).unwrap(), "7");
        // Never rounded: more places than the token has is refused.
        assert!(atomic_amount("0.0000001", 6).is_err());
        assert!(atomic_amount("1.5", 0).is_err());
        for bad in [
            "", "0", "0.0", "01", "1.", ".5", "-1", "1e3", " 1", "1,5", "0x10",
        ] {
            assert!(!valid_amount(bad), "{bad:?}");
            assert!(atomic_amount(bad, 18).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn stored_options_become_integers_only_with_known_decimals() {
        let raw = r#"[{"asset":"eip155:1/slip44:60","amount":"0.5"},{"asset":"eip155:8453/slip44:60","amount":"1"}]"#;
        let decimals = Decimals::from([("eip155:1/slip44:60".to_string(), 18)]);
        let exact = persisted_payments(raw, &decimals).unwrap();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].amount, "500000000000000000");
        assert!(
            persisted_payments(raw, &Decimals::new())
                .unwrap()
                .is_empty()
        );
        assert!(
            persisted_payments(
                r#"[{"asset":"eip155:1/slip44:60","amount":"01"}]"#,
                &decimals
            )
            .is_err()
        );
    }
}
#[cfg(test)]
mod price_tests {
    use super::*;
    fn token(symbol: &str, asset: &str, decimals: u32, rate: Option<f64>) -> Token {
        Token {
            symbol: symbol.into(),
            asset: asset.into(),
            decimals,
            rate,
        }
    }
    #[test]
    fn prices_are_amounts_with_units() {
        assert_eq!(
            prices("25 USD\n0.01 ETH, 3 dai").unwrap(),
            vec![
                Price {
                    amount: "25".into(),
                    unit: "USD".into()
                },
                Price {
                    amount: "0.01".into(),
                    unit: "ETH".into()
                },
                Price {
                    amount: "3".into(),
                    unit: "dai".into()
                },
            ]
        );
        assert_eq!(prices("25 usd").unwrap()[0].label(), "$25");
        assert_eq!(prices("25 USDC").unwrap()[0].label(), "25 USDC");
        for bad in ["", "25", "USDC 25", "25 USDC, 3 usdc", "-1 ETH", "1 E TH"] {
            assert!(prices(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn dollars_become_every_rated_token_and_token_prices_stay_exact() {
        let tokens = [
            token("USDC", "eip155:1/erc20:0xa", 6, Some(1.0)),
            token("USDC", "eip155:56/erc20:0xb", 18, Some(1.0)),
            token("ETH", "eip155:1/slip44:60", 18, Some(4012.5)),
            token("TEST", "eip155:31337/erc20:0xc", 6, None),
        ];
        let options = resolve(&prices("25 USD").unwrap(), &tokens).unwrap();
        let amounts: Vec<(&str, &str)> = options
            .iter()
            .map(|o| (o.asset.as_str(), o.amount.as_str()))
            .collect();
        assert_eq!(
            amounts,
            [
                ("eip155:1/erc20:0xa", "25"),
                ("eip155:56/erc20:0xb", "25"),
                ("eip155:1/slip44:60", "0.00623053")
            ]
        );
        // A token's own price wins over its dollar price.
        let mixed = resolve(&prices("25 USD\n20 usdc").unwrap(), &tokens).unwrap();
        assert_eq!(mixed[0].amount, "20");
        assert_eq!(mixed.len(), 3);
        assert_eq!(
            resolve(&prices("5 TEST").unwrap(), &tokens).unwrap().len(),
            1
        );
        assert!(resolve(&prices("1 DAI").unwrap(), &tokens).is_err());
        assert!(resolve(&prices("0.0000001 USDC").unwrap(), &tokens).is_err());
        assert!(resolve(&prices("1 USD").unwrap(), &tokens[3..]).is_err());
    }
    #[test]
    fn dollar_amounts_round_up_to_six_significant_digits() {
        assert_eq!(in_token(25.0, 1.0, 6), "25");
        assert_eq!(in_token(25.0, 0.9998, 6), "25.0051");
        assert_eq!(in_token(10.0, 3.0, 18), "3.33334");
        assert_eq!(in_token(1_000_000.0, 0.5, 6), "2000000");
        assert_eq!(in_token(25.0, 1e9, 6), "0.000001");
        assert_eq!(in_token(25.0, 4000.0, 18), "0.00625");
    }
}
