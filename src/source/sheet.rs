//! What Stor expects in a source's tabs, whatever the tabs are named: their roles are
//! told by their content. Also the starting content Stor writes into a new source.
use crate::commerce::catalog::{self, Metadata, Price};
use zeroize::Zeroizing;

pub const CATALOG: [&str; 7] = [
    "SKU",
    "Title",
    "Description",
    "Price",
    "Stock",
    "Images",
    "Delivery",
];
pub const HISTORY: [&str; 10] = [
    "Date",
    "Order",
    "SKU",
    "Product",
    "Quantity",
    "Amount",
    "Token",
    "Network",
    "Status",
    "Transaction",
];
pub const TOKENS: [&str; 6] = [
    "Token",
    "Network",
    "Asset",
    "CoinGecko",
    "USD rate",
    "Rate time",
];
/// Where the Tokens tab fetches its rates, right of what Stor reads (column N).
const RATES_COLUMN: usize = 13;
/// The header of the Branding and Payments tabs: `Setting | Value`, then free notes.
const SETTING: [&str; 2] = ["Setting", "Value"];
const BRANDING_KEYS: [&str; 4] = ["title", "description", "website", "style"];
const PAYMENTS_KEYS: [&str; 3] = ["url", "key", "status"];
/// Most rows Stor reads from one tab: a header and 1000 products.
pub const ROWS: usize = 1001;
/// Most columns Stor reads from one tab.
pub const COLUMNS: usize = 12;

/// One tab of a source: its stable ID, its title and its first rows as text.
#[derive(Clone, Debug)]
pub struct Tab {
    pub id: i64,
    pub title: String,
    pub rows: Vec<Vec<String>>,
}
impl Tab {
    fn cell(&self, row: usize, column: usize) -> &str {
        self.rows
            .get(row)
            .and_then(|r| r.get(column))
            .map_or("", |c| c.trim())
    }
    fn is_empty(&self) -> bool {
        self.rows.iter().flatten().all(|c| c.trim().is_empty())
    }
    /// A header and nothing below it.
    fn is_bare(&self) -> bool {
        self.rows
            .iter()
            .skip(1)
            .flatten()
            .all(|c| c.trim().is_empty())
    }
    fn header_is(&self, header: &[&str]) -> bool {
        let first: Vec<&str> = (0..header.len()).map(|c| self.cell(0, c)).collect();
        first
            .iter()
            .zip(header)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }
    /// The lowercase keys of a `Setting | Value` tab.
    fn keys(&self) -> Vec<String> {
        (1..self.rows.len())
            .map(|r| self.cell(r, 0).to_ascii_lowercase())
            .filter(|k| !k.is_empty())
            .collect()
    }
}

/// A value Stor writes into a source.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Header(String),
    Text(String),
    Number(f64),
    /// Unix seconds, shown as a date and time.
    Date(i64),
    Formula(String),
}
pub fn text(s: impl Into<String>) -> Cell {
    Cell::Text(s.into())
}
fn header(columns: &[&str]) -> Vec<Cell> {
    columns.iter().map(|c| Cell::Header((*c).into())).collect()
}

/// Which tab plays which role, by index into the tabs. Tabs with no content are spare.
#[derive(Debug, Default)]
pub struct Layout {
    pub catalog: Option<usize>,
    pub tokens: Option<usize>,
    pub branding: Option<usize>,
    pub payments: Option<usize>,
    pub history: Option<usize>,
    pub empty: Vec<usize>,
}
pub fn classify(tabs: &[Tab]) -> anyhow::Result<Layout> {
    let mut layout = Layout::default();
    let take = |slot: &mut Option<usize>, index: usize, name: &str| -> anyhow::Result<()> {
        anyhow::ensure!(slot.is_none(), "two tabs look like {name}. Keep one");
        *slot = Some(index);
        Ok(())
    };
    for (index, tab) in tabs.iter().enumerate() {
        if tab.is_empty() {
            layout.empty.push(index);
        } else if tab.header_is(&CATALOG) {
            take(&mut layout.catalog, index, "the Catalog")?;
        } else if tab.header_is(&TOKENS) {
            take(&mut layout.tokens, index, "Tokens")?;
        } else if tab.header_is(&HISTORY) {
            take(&mut layout.history, index, "the History")?;
        } else if tab.header_is(&SETTING) {
            let keys = tab.keys();
            if keys.iter().any(|k| k == "url" || k == "key") {
                take(&mut layout.payments, index, "Payments")?;
            } else if keys.iter().any(|k| BRANDING_KEYS.contains(&k.as_str())) {
                take(&mut layout.branding, index, "Branding")?;
            }
        }
    }
    Ok(layout)
}
/// Whether the role's tab is missing or has only its header.
pub fn wants_content(tabs: &[Tab], role: Option<usize>) -> bool {
    role.is_none_or(|i| tabs[i].is_bare())
}

/// A Tokens row: the name prices use, the exact asset, and its USD rate if it has one.
pub struct TokenRow {
    pub symbol: String,
    pub asset: String,
    pub rate: Option<f64>,
    /// When CoinGecko last priced it (Unix seconds). None for a rate typed in.
    pub updated: Option<i64>,
    /// It should get its rate from CoinGecko (a CoinGecko ID is set).
    pub fetched: bool,
}
/// The Tokens tab's rows. A rate that isn't a positive number (empty, or a formula that
/// failed) leaves the token out of dollar prices only.
pub fn tokens(tab: &Tab) -> anyhow::Result<Vec<TokenRow>> {
    let mut rows = Vec::new();
    for r in 1..tab.rows.len() {
        let (symbol, asset) = (tab.cell(r, 0), tab.cell(r, 2));
        if symbol.is_empty() && asset.is_empty() {
            continue;
        }
        let row = (|| {
            anyhow::ensure!(
                (1..=16).contains(&symbol.len())
                    && symbol
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    && !symbol.eq_ignore_ascii_case("USD"),
                "Token must be a short name like USDC"
            );
            catalog::asset(asset).map_err(|_| {
                anyhow::anyhow!(
                    "Asset must be eip155:<chain>/erc20:<contract> or eip155:<chain>/slip44:<coin>"
                )
            })?;
            anyhow::ensure!(
                !rows.iter().any(|t: &TokenRow| t.asset == asset),
                "{asset} is listed twice"
            );
            Ok(TokenRow {
                symbol: symbol.into(),
                asset: asset.into(),
                rate: tab
                    .cell(r, 4)
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.0 && *v < 1e12),
                updated: tab
                    .cell(r, 5)
                    .parse::<f64>()
                    .ok()
                    .filter(|t| *t > 0.0 && *t < 1e11)
                    .map(|t| t as i64),
                fetched: !tab.cell(r, 3).is_empty(),
            })
        })()
        .map_err(|e: anyhow::Error| anyhow::anyhow!("Tokens row {}: {e}", r + 1))?;
        rows.push(row);
    }
    Ok(rows)
}
/// Whether the Tokens tab lists no token yet.
pub fn tokens_empty(tab: &Tab) -> bool {
    (1..tab.rows.len()).all(|r| tab.cell(r, 0).is_empty() && tab.cell(r, 2).is_empty())
}
/// A product as written in the Catalog, before its prices are resolved.
pub struct Row {
    pub sku: String,
    pub title: String,
    pub description: String,
    pub prices: Vec<Price>,
    pub quantity: i64,
    pub images: Vec<String>,
    pub delivery: Zeroizing<String>,
}
/// The Catalog's products. An error names the row to fix.
pub fn catalog(tab: &Tab) -> anyhow::Result<Vec<Row>> {
    let mut products = Vec::new();
    for r in 1..tab.rows.len() {
        if (0..CATALOG.len()).all(|c| tab.cell(r, c).is_empty()) {
            continue;
        }
        let row = (|| {
            let sku = tab.cell(r, 0).to_string();
            anyhow::ensure!(
                catalog::valid_sku(&sku),
                "SKU must be letters, digits, - . _ or ~"
            );
            let quantity = tab.cell(r, 4);
            Ok(Row {
                title: tab.cell(r, 1).into(),
                description: tab.cell(r, 2).into(),
                prices: catalog::prices(tab.cell(r, 3))?,
                quantity: quantity
                    .parse::<f64>()
                    .ok()
                    .filter(|q| q.fract() == 0.0 && (0.0..=1e6).contains(q))
                    .map(|q| q as i64)
                    .ok_or_else(|| anyhow::anyhow!("Stock must be a whole number"))?,
                images: catalog::images(tab.cell(r, 5))?,
                delivery: Zeroizing::new(tab.cell(r, 6).into()),
                sku,
            })
        })()
        .map_err(|e: anyhow::Error| anyhow::anyhow!("Catalog row {}: {e}", r + 1))?;
        products.push(row);
    }
    Ok(products)
}
/// The Branding tab over the current presentation: keys it leaves out keep their value.
pub fn branding(tab: &Tab, mut metadata: Metadata) -> anyhow::Result<Metadata> {
    for r in 1..tab.rows.len() {
        let value = tab.cell(r, 1).to_string();
        match tab.cell(r, 0).to_ascii_lowercase().as_str() {
            "" => {}
            "title" => metadata.title = value,
            "description" => metadata.description = value,
            "website" => metadata.website = Some(value).filter(|w| !w.is_empty()),
            "style" => {
                let (theme, css) = catalog::split_style(&value);
                metadata.theme = theme.into();
                metadata.style = css;
            }
            other => anyhow::bail!("Branding row {}: unknown setting {other}", r + 1),
        }
    }
    catalog::validate_metadata(&metadata)
        .map_err(|_| anyhow::anyhow!("Branding: Title is required; Website must be HTTPS"))?;
    Ok(metadata)
}
/// The Payments tab: its URL and KEY when both are filled in, and the row of its STATUS.
pub struct Payments {
    pub server: Option<(String, Zeroizing<String>)>,
    pub status_row: Option<usize>,
}
pub fn payments(tab: &Tab) -> anyhow::Result<Payments> {
    let mut url = String::new();
    let mut key = Zeroizing::new(String::new());
    let mut status_row = None;
    for r in 1..tab.rows.len() {
        match tab.cell(r, 0).to_ascii_lowercase().as_str() {
            "url" => url = tab.cell(r, 1).into(),
            "key" => key = Zeroizing::new(tab.cell(r, 1).into()),
            "status" => status_row = Some(r),
            "" => {}
            other => anyhow::ensure!(
                PAYMENTS_KEYS.contains(&other),
                "Payments row {}: unknown setting {other}",
                r + 1
            ),
        }
    }
    Ok(Payments {
        server: (!url.is_empty() && !key.is_empty()).then_some((url, key)),
        status_row,
    })
}

/// A sample product: SKU, title, description, price, stock, photos (picsum seeds) and
/// delivery.
type Sample = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    f64,
    &'static [&'static str],
    &'static str,
);
/// A new Catalog: sample products priced in dollars, to show what a store looks like.
/// `symbol` is a token the payment server offers, for the few priced in a token. Without
/// one (a store made before its payment server is known), those are priced in dollars
/// only, which every token with a USD rate can pay.
pub fn catalog_template(symbol: Option<&str>) -> Vec<Vec<Cell>> {
    let products: [Sample; 32] = [
        (
            "starter-guide",
            "Crypto payments starter guide",
            "A 60-page guide to wallets, networks, fees and confirmations.\nPDF and EPUB, updated twice a year.",
            "12 USD",
            500.0,
            &["guide-1", "guide-2", "guide-3"],
            "Download: https://example.com/d/starter-guide (sample)",
        ),
        (
            "ui-kit",
            "Minimal UI kit",
            "120 components for Figma and Sketch: buttons, forms, cards, navigation.\nLight and dark themes included.",
            "49 USD",
            200.0,
            &["uikit-1", "uikit-2"],
            "Figma link: https://example.com/d/ui-kit (sample)",
        ),
        (
            "icon-pack",
            "Line icon pack",
            "1,400 icons in SVG, PNG and an icon font. 24 px grid, 1.5 px stroke.",
            "19 USD",
            1000.0,
            &["icons-1"],
            "Download: https://example.com/d/icons (sample)",
        ),
        (
            "photo-presets",
            "Film photo presets",
            "18 Lightroom presets inspired by classic film stock.\nWorks on desktop and mobile.",
            "15 USD",
            800.0,
            &["presets-1", "presets-2", "presets-3"],
            "Download: https://example.com/d/presets (sample)",
        ),
        (
            "font-family",
            "Grotesk font family",
            "Eight weights with italics, variable font included. Desktop and web license.",
            "39 USD",
            300.0,
            &["font-1", "font-2"],
            "Download: https://example.com/d/grotesk (sample)",
        ),
        (
            "notion-planner",
            "Notion life planner",
            "Goals, habits, reading list and weekly reviews in one Notion workspace.",
            "9 USD",
            1000.0,
            &["planner-1"],
            "Template link: https://example.com/d/planner (sample)",
        ),
        (
            "resume-kit",
            "Resume template kit",
            "Six resume layouts for Word, Pages and Google Docs, with cover letters.",
            "14 USD",
            600.0,
            &["resume-1", "resume-2"],
            "Download: https://example.com/d/resume (sample)",
        ),
        (
            "course-solidity",
            "Solidity from zero",
            "Video course, 9 hours across 42 lessons, with exercises and a final project.\nLifetime access.",
            "79 USD",
            150.0,
            &["solidity-1", "solidity-2", "solidity-3"],
            "Course access code: SAMPLE-SOL-0001",
        ),
        (
            "course-photo",
            "Mobile photography course",
            "Shoot and edit better photos with your phone: 3 hours, 20 lessons.",
            "29 USD",
            300.0,
            &["mobilephoto-1"],
            "Course access code: SAMPLE-PHO-0001",
        ),
        (
            "ebook-trading",
            "Market structure handbook",
            "An introduction to order books, liquidity and price discovery. 140 pages.",
            "18 USD",
            400.0,
            &["handbook-1", "handbook-2"],
            "Download: https://example.com/d/handbook (sample)",
        ),
        (
            "music-loops",
            "Lo-fi loop pack",
            "250 royalty-free loops: drums, keys, bass and textures at 70–90 BPM.",
            "24 USD",
            500.0,
            &["loops-1", "loops-2"],
            "Download: https://example.com/d/loops (sample)",
        ),
        (
            "sound-effects",
            "Interface sound effects",
            "120 short sounds for apps and games, WAV and OGG.",
            "11 USD",
            700.0,
            &["sfx-1"],
            "Download: https://example.com/d/sfx (sample)",
        ),
        (
            "stock-photos",
            "City at night photo set",
            "40 high-resolution photos, 6000 × 4000 px, commercial license.",
            "35 USD",
            250.0,
            &["city-1", "city-2", "city-3"],
            "Download: https://example.com/d/city (sample)",
        ),
        (
            "wallpapers",
            "Abstract wallpapers",
            "30 wallpapers for desktop and phone, up to 6K.",
            "5 USD",
            2000.0,
            &["wallpaper-1", "wallpaper-2"],
            "Download: https://example.com/d/wallpapers (sample)",
        ),
        (
            "mockups",
            "Device mockups",
            "Photoreal phone, laptop and tablet mockups as layered PSD files.",
            "22 USD",
            400.0,
            &["mockup-1"],
            "Download: https://example.com/d/mockups (sample)",
        ),
        (
            "plugin-pro",
            "Pro plugin license",
            "One-year license for one site, with updates and email support.",
            "59 USD",
            100.0,
            &["plugin-1"],
            "License key: SAMPLE-PLG-0000-0000",
        ),
        (
            "app-license",
            "Desktop app lifetime license",
            "Lifetime license for two computers. Works offline.",
            "45 USD",
            300.0,
            &["app-1", "app-2"],
            "License key: SAMPLE-APP-0000-0000",
        ),
        (
            "api-credits",
            "API credits, 100,000 calls",
            "Prepaid credits for the sample API. They never expire.",
            "30 USD",
            1000.0,
            &["api-1"],
            "Credit code: SAMPLE-API-100K",
        ),
        (
            "membership-month",
            "Community membership, 1 month",
            "Access to the private forum, monthly live sessions and the archive.",
            "10 USD",
            500.0,
            &["community-1"],
            "Invite link: https://example.com/join (sample)",
        ),
        (
            "membership-year",
            "Community membership, 1 year",
            "Twelve months for the price of ten.",
            "100 USD",
            200.0,
            &["community-2"],
            "Invite link: https://example.com/join-year (sample)",
        ),
        (
            "consulting-hour",
            "One-hour consulting call",
            "A video call about payments, wallets or your product.\nBook a time after paying.",
            "150 USD",
            10.0,
            &["call-1"],
            "Booking link: https://example.com/book (sample)",
        ),
        (
            "code-review",
            "Smart contract review",
            "A written review of one contract up to 500 lines, delivered within 5 days.",
            "400 USD",
            3.0,
            &["review-1", "review-2"],
            "Send your contract to review@example.com with this code: SAMPLE-REV-01",
        ),
        (
            "gift-card-25",
            "Gift card, $25",
            "A store gift card worth $25. Code by email after payment.",
            "25 USD",
            100.0,
            &["gift-1"],
            "Gift card code: SAMPLE-GIFT-0025",
        ),
        (
            "gift-card-100",
            "Gift card, $100",
            "A store gift card worth $100.",
            "100 USD",
            50.0,
            &["gift-2"],
            "Gift card code: SAMPLE-GIFT-0100",
        ),
        (
            "ticket-meetup",
            "Meetup ticket",
            "Entry to the monthly meetup, drinks included. Show the code at the door.",
            "20 USD",
            80.0,
            &["meetup-1", "meetup-2"],
            "Ticket: SAMPLE-MEET-0001",
        ),
        (
            "ticket-workshop",
            "Workshop ticket",
            "A full-day hands-on workshop, lunch included.",
            "120 USD",
            25.0,
            &["workshop-1"],
            "Ticket: SAMPLE-WORK-0001",
        ),
        (
            "recipe-book",
            "Plant-based recipe book",
            "85 recipes with photos and shopping lists. PDF.",
            "16 USD",
            600.0,
            &["recipes-1", "recipes-2", "recipes-3"],
            "Download: https://example.com/d/recipes (sample)",
        ),
        (
            "workout-plan",
            "12-week workout plan",
            "A progressive home plan with video demos for every exercise.",
            "27 USD",
            500.0,
            &["workout-1"],
            "Plan link: https://example.com/d/workout (sample)",
        ),
        (
            "language-pack",
            "Spanish phrase pack",
            "1,000 phrases with audio by native speakers, for Anki and Quizlet.",
            "13 USD",
            700.0,
            &["spanish-1"],
            "Download: https://example.com/d/spanish (sample)",
        ),
        (
            "game-skin",
            "Game skin bundle",
            "Five character skins and an emote for the sample game.",
            "8 USD",
            0.0,
            &["skins-1", "skins-2"],
            "Redeem code: SAMPLE-SKIN-0001",
        ),
        (
            "token-sticker",
            "Sticker pack, paid in token",
            "Twelve digital stickers, priced in a token instead of dollars.",
            "3 TOKEN",
            1000.0,
            &["stickers-1"],
            "Download: https://example.com/d/stickers (sample)",
        ),
        (
            "donation",
            "Support the project",
            "Pay what you like, as several fixed amounts in a token or in dollars.",
            "5 USD\n10 TOKEN",
            10000.0,
            &["support-1"],
            "Thank you for your support!",
        ),
    ];
    let mut rows = vec![header(&CATALOG)];
    for (sku, title, description, price, stock, photos, delivery) in products {
        let images: Vec<String> = photos
            .iter()
            .map(|seed| format!("https://picsum.photos/seed/payroof-{seed}/800/600"))
            .collect();
        rows.push(vec![
            text(sku),
            text(title),
            text(description),
            text(sample_price(price, symbol)),
            Cell::Number(stock),
            text(images.join("\n")),
            text(delivery),
        ]);
    }
    rows
}
/// A sample price with `TOKEN` as the server's token. Without one, its dollar lines
/// (or the token's amount in dollars, when it has none).
fn sample_price(price: &str, symbol: Option<&str>) -> String {
    if let Some(symbol) = symbol {
        return price.replace("TOKEN", symbol);
    }
    let dollars: Vec<&str> = price.lines().filter(|l| !l.contains("TOKEN")).collect();
    if dollars.is_empty() {
        price.replace("TOKEN", "USD")
    } else {
        dollars.join("\n")
    }
}
/// A new Tokens tab: every token the payment server offers, one row per network. Dollar
/// stablecoins start at a rate of 1. Other tokens with a known CoinGecko ID get formulas
/// for their rate and for when CoinGecko priced it, both read from the cell in column N
/// (Stor rewrites that cell before rates age, which makes Google fetch them again).
/// Any rate can be typed over. A typed rate has no time.
pub fn tokens_template(assets: &[serde_json::Value]) -> Vec<Vec<Cell>> {
    let mut rows = vec![header(&TOKENS)];
    for asset in assets {
        let symbol = asset["symbol"].as_str().unwrap_or_default();
        let gecko = coingecko(symbol);
        let row = rows.len() + 1;
        let fixed = ["USDC", "USDT", "DAI"].contains(&symbol.to_ascii_uppercase().as_str());
        // CoinGecko's answer, `{"ethereum":{"usd":4012.5,"last_updated_at":1760000000}}`,
        // lands in row 1 from column N split at its commas. Joined again, a pattern finds
        // this row's ID and reads its price or its time.
        let read = |value: &str| {
            Cell::Formula(format!(
                "=IFERROR(VALUE(SUBSTITUTE(REGEXEXTRACT(\",\"&TEXTJOIN(\",\",TRUE,$N$1:$AZ$3),\"\\W\"&D{row}&\"\\W+{value}\"),\".\",MID(1/2,2,1))),\"\")"
            ))
        };
        let (rate, time) = if fixed {
            (Cell::Number(1.0), text(""))
        } else if gecko.is_empty() {
            (text(""), text(""))
        } else {
            (
                read("usd\\W+([0-9.eE+-]+)"),
                read("usd\\W+[0-9.eE+-]+\\W+last_updated_at\\W+([0-9]+)"),
            )
        };
        rows.push(vec![
            text(symbol),
            text(asset["network"].as_str().unwrap_or_default()),
            text(asset["id"].as_str().unwrap_or_default()),
            text(gecko),
            rate,
            time,
        ]);
    }
    rows
}
/// The cell the Tokens tab's rate formulas read: CoinGecko's prices for every ID in
/// column D.
pub fn rates_source() -> (usize, Vec<Vec<Cell>>) {
    (
        RATES_COLUMN,
        vec![vec![Cell::Formula(
            "=IMPORTDATA(\"https://api.coingecko.com/api/v3/simple/price?vs_currencies=usd&include_last_updated_at=true&ids=\"&TEXTJOIN(\",\",TRUE,UNIQUE(D2:D)))".into(),
        )]],
    )
}
/// CoinGecko's ID of common tokens. Others are left for the merchant.
fn coingecko(symbol: &str) -> &'static str {
    match symbol.to_ascii_uppercase().as_str() {
        "ETH" => "ethereum",
        "WETH" => "weth",
        "USDC" => "usd-coin",
        "USDT" => "tether",
        "DAI" => "dai",
        "POL" | "MATIC" => "polygon-ecosystem-token",
        "BNB" => "binancecoin",
        "WBTC" => "wrapped-bitcoin",
        "ARB" => "arbitrum",
        "OP" => "optimism",
        "EURC" => "euro-coin",
        _ => "",
    }
}
/// A new Branding tab: every setting with a sample value and what it does.
pub fn branding_template(store: &str) -> Vec<Vec<Cell>> {
    vec![
        header(&["Setting", "Value", "Notes"]),
        vec![
            text("Title"),
            text(store),
            text("The store's name. Required."),
        ],
        vec![
            text("Description"),
            text("Digital goods, delivered the moment your payment confirms."),
            text("Shown under the name."),
        ],
        vec![
            text("Website"),
            text(""),
            text("Optional HTTPS link to your site."),
        ],
        vec![
            text("Style"),
            text("auto"),
            text(
                "The theme, then optional CSS after it. light or dark fixes the theme for everyone. auto follows each visitor's system and lets them switch. E.g. dark .store { --store-accent: #c2410c; }",
            ),
        ],
    ]
}
/// A new Payments tab: the payment server's URL and KEY (from the Wallet), and the line
/// where Stor reports how the store is doing.
pub fn payments_template(url: &str, key: &str) -> Vec<Vec<Cell>> {
    vec![
        header(&["Setting", "Value", "Notes"]),
        vec![
            text("URL"),
            text(url),
            text(
                "Where payments are made: your Payroof server and project. Payroof Wallet → Project → Stores → Connect store gives it.",
            ),
        ],
        vec![
            text("KEY"),
            text(key),
            text(
                "Lets Stor create a payment address for each order. It can't move funds. From the same place as URL.",
            ),
        ],
        vec![text("STATUS"), text("Starting…"), text("Written by Stor.")],
    ]
}
pub fn history_template() -> Vec<Vec<Cell>> {
    vec![header(&HISTORY)]
}
/// The tabs a merchant may make by hand instead, as Stor would fill them: named by
/// file, since an imported CSV becomes a tab of that name. Tokens has its header only.
/// Stor lists the payment server's tokens in it. Payments' URL and KEY are left empty.
pub const SAMPLE_TABLES: [&str; 5] = ["Catalog", "Tokens", "Branding", "Payments", "History"];
pub fn sample_table(name: &str) -> Option<Vec<Vec<Cell>>> {
    Some(match name {
        "Catalog" => catalog_template(None),
        "Tokens" => vec![header(&TOKENS)],
        "Branding" => branding_template("My store"),
        "Payments" => payments_template("", ""),
        "History" => history_template(),
        _ => return None,
    })
}
/// Rows as CSV, which Google Sheets imports with formulas as formulas.
pub fn csv(rows: &[Vec<Cell>]) -> anyhow::Result<String> {
    let mut out = csv::Writer::from_writer(Vec::new());
    for row in rows {
        out.write_record(row.iter().map(|cell| match cell {
            Cell::Header(s) | Cell::Text(s) | Cell::Formula(s) => s.clone(),
            Cell::Number(n) => n.to_string(),
            Cell::Date(t) => t.to_string(),
        }))?;
    }
    Ok(String::from_utf8(out.into_inner()?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_without_a_known_token_are_priced_in_dollars_only() {
        assert_eq!(sample_price("3 TOKEN", Some("ETH")), "3 ETH");
        assert_eq!(
            sample_price("5 USD\n10 TOKEN", Some("DAI")),
            "5 USD\n10 DAI"
        );
        assert_eq!(sample_price("3 TOKEN", None), "3 USD");
        assert_eq!(sample_price("5 USD\n10 TOKEN", None), "5 USD");
        // Every price of the sample catalog then names dollars, which any token with
        // a USD rate pays, whatever the payment server offers.
        for row in &catalog_template(None)[1..] {
            let Cell::Text(price) = &row[3] else {
                panic!("price cell")
            };
            assert!(price.lines().all(|l| l.ends_with(" USD")), "{price:?}");
        }
    }
    fn tab(id: i64, title: &str, rows: &[&[&str]]) -> Tab {
        Tab {
            id,
            title: title.into(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|c| c.to_string()).collect())
                .collect(),
        }
    }
    #[test]
    fn roles_come_from_content_not_names() {
        let tabs = [
            tab(1, "Sheet1", &[]),
            tab(
                2,
                "Shop",
                &[&[
                    "sku",
                    "TITLE",
                    "Description",
                    "Price",
                    "Stock",
                    "Images",
                    "Delivery",
                ]],
            ),
            tab(3, "Look", &[&["Setting", "Value"], &["Title", "Mine"]]),
            tab(
                4,
                "Money",
                &[&["Setting", "Value"], &["URL", "x"], &["KEY", "y"]],
            ),
            tab(5, "Notes", &[&["anything"]]),
        ];
        let layout = classify(&tabs).unwrap();
        assert_eq!(
            (
                layout.catalog,
                layout.branding,
                layout.payments,
                layout.history
            ),
            (Some(1), Some(2), Some(3), None)
        );
        assert_eq!(layout.empty, vec![0]);
        assert!(wants_content(&tabs, layout.catalog));
        assert!(!wants_content(&tabs, layout.branding));
        let twice = [tabs[1].clone(), tabs[1].clone()];
        assert!(classify(&twice).is_err());
    }
    #[test]
    fn rows_parse_with_their_row_named_in_errors() {
        let good = tab(
            1,
            "C",
            &[
                &CATALOG,
                &[
                    "a-1",
                    "A",
                    "",
                    "5 USDC, 0.01 ETH",
                    "3",
                    "https://x.example/a.png",
                    "code",
                ],
                &["", "", "", "", "", "", ""],
            ],
        );
        let rows = catalog(&good).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].quantity, rows[0].prices.len()), (3, 2));
        let bad = tab(1, "C", &[&CATALOG, &["a", "A", "", "5", "1", "", ""]]);
        let error = catalog(&bad).err().unwrap().to_string();
        assert!(error.starts_with("Catalog row 2:"), "{error}");
        let stock = tab(
            1,
            "C",
            &[&CATALOG, &["a", "A", "", "5 USDC", "1.5", "", ""]],
        );
        assert!(catalog(&stock).is_err());
    }
    #[test]
    fn tokens_rows_name_exact_assets() {
        let t = tab(
            1,
            "T",
            &[
                &TOKENS,
                &[
                    "USDC",
                    "Base",
                    "eip155:8453/erc20:0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
                    "usd-coin",
                    "0.9998",
                ],
                &["ETH", "Base", "eip155:8453/slip44:60", "ethereum", "#N/A"],
            ],
        );
        let rows = tokens(&t).unwrap();
        assert_eq!((rows[0].rate, rows[1].rate), (Some(0.9998), None));
        assert_eq!(rows[0].updated, None);
        let timed = tab(
            1,
            "T",
            &[
                &TOKENS,
                &[
                    "ETH",
                    "Base",
                    "eip155:8453/slip44:60",
                    "ethereum",
                    "4012.5",
                    "1760000000",
                ],
            ],
        );
        assert_eq!(tokens(&timed).unwrap()[0].updated, Some(1_760_000_000));
        for bad in [
            ["USD", "x", "eip155:1/slip44:60", "", ""],
            ["ETH", "x", "ethereum", "", ""],
            ["ETH", "x", "eip155:1/erc20:0xABC", "", ""],
        ] {
            assert!(tokens(&tab(1, "T", &[&TOKENS, &bad])).is_err(), "{bad:?}");
        }
        let twice = tab(
            1,
            "T",
            &[
                &TOKENS,
                &["A", "", "eip155:1/slip44:60", "", ""],
                &["B", "", "eip155:1/slip44:60", "", ""],
            ],
        );
        assert!(
            tokens(&twice)
                .err()
                .unwrap()
                .to_string()
                .starts_with("Tokens row 3:")
        );
    }
    #[test]
    fn settings_tabs_read_their_keys() {
        let p = tab(
            1,
            "P",
            &[
                &["Setting", "Value"],
                &["url", "u"],
                &["Key", "k"],
                &["STATUS", "Live"],
            ],
        );
        let parsed = payments(&p).unwrap();
        assert_eq!(parsed.server.as_ref().map(|s| s.0.as_str()), Some("u"));
        assert_eq!(parsed.status_row, Some(3));
        let b = tab(
            1,
            "B",
            &[&["Setting", "Value"], &["Title", "New"], &["Website", ""]],
        );
        let metadata = branding(&b, Metadata::new("old")).unwrap();
        assert_eq!((metadata.title.as_str(), metadata.website), ("New", None));
        assert_eq!(
            (metadata.theme.as_str(), metadata.style.as_str()),
            ("auto", "")
        );
        // Style: a leading light, dark or auto is the theme, and the rest is CSS.
        for (cell, theme, css) in [
            ("dark", "dark", ""),
            ("  Light ", "light", ""),
            ("AUTO", "auto", ""),
            (
                "dark\n:root { --store-accent: red; }",
                "dark",
                ":root { --store-accent: red; }",
            ),
            (
                ".store { --store-accent: red; }",
                "auto",
                ".store { --store-accent: red; }",
            ),
            ("darkness { color: red }", "auto", "darkness { color: red }"),
            ("", "auto", ""),
        ] {
            let b = tab(
                1,
                "B",
                &[&["Setting", "Value"], &["Title", "T"], &["Style", cell]],
            );
            let metadata = branding(&b, Metadata::new("old")).unwrap();
            assert_eq!(
                (metadata.theme.as_str(), metadata.style.as_str()),
                (theme, css)
            );
        }
        let unknown = tab(1, "B", &[&["Setting", "Value"], &["Colour", "red"]]);
        assert!(branding(&unknown, Metadata::new("x")).is_err());
    }
    #[test]
    fn templates_are_what_classify_recognizes() {
        let render = |rows: Vec<Vec<Cell>>| -> Vec<Vec<String>> {
            rows.into_iter()
                .map(|r| {
                    r.into_iter()
                        .map(|c| match c {
                            Cell::Header(s) | Cell::Text(s) | Cell::Formula(s) => s,
                            Cell::Number(n) => n.to_string(),
                            Cell::Date(d) => d.to_string(),
                        })
                        .collect()
                })
                .collect()
        };
        let tabs = [
            Tab {
                id: 1,
                title: "Catalog".into(),
                rows: render(catalog_template(Some("USDC"))),
            },
            Tab {
                id: 2,
                title: "Branding".into(),
                rows: render(branding_template("shop")),
            },
            Tab {
                id: 3,
                title: "Payments".into(),
                rows: render(payments_template("", "")),
            },
            Tab {
                id: 4,
                title: "History".into(),
                rows: render(history_template()),
            },
            Tab {
                id: 5,
                title: "Tokens".into(),
                rows: render(tokens_template(&[
                    serde_json::json!({"id":"eip155:1/erc20:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48","symbol":"USDC","network":"Ethereum"}),
                    serde_json::json!({"id":"eip155:1/slip44:60","symbol":"ETH","network":"Ethereum"}),
                    serde_json::json!({"id":"eip155:31337/erc20:0x5fbdb2315678afecb367f032d93f642f64180aa3","symbol":"TEST","network":"Anvil"}),
                ])),
            },
        ];
        let layout = classify(&tabs).unwrap();
        assert_eq!(
            (
                layout.catalog,
                layout.branding,
                layout.payments,
                layout.history,
                layout.tokens
            ),
            (Some(0), Some(1), Some(2), Some(3), Some(4))
        );
        let products = catalog(&tabs[0]).unwrap();
        assert!(products.len() >= 30);
        assert!(products.iter().any(|p| p.images.len() == 3));
        assert!(products.iter().any(|p| p.quantity == 0));
        // Stablecoins start at 1, ETH reads CoinGecko, an unknown token waits for a rate.
        let tokens = tokens(&tabs[4]).unwrap();
        assert_eq!(
            tokens.iter().map(|t| t.rate).collect::<Vec<_>>(),
            [Some(1.0), None, None]
        );
        assert_eq!(tabs[4].rows[2][3], "ethereum");
        assert!(tabs[4].rows[2][4].contains("D3"));
        assert!(tabs[4].rows[2][5].contains("last_updated_at"));
        assert_eq!(tabs[4].rows[1][5], "");
        assert!(
            render(vec![rates_source().1[0].clone()])[0][0]
                .contains("include_last_updated_at=true")
        );
        assert_eq!(
            branding(&tabs[1], Metadata::new("x")).unwrap().title,
            "shop"
        );
        assert!(payments(&tabs[2]).unwrap().server.is_none());
    }
}
