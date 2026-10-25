//! A store's source: where its merchant keeps the catalog, branding and payment
//! settings, and where Stor writes the store's status and its sales. Google Sheets is
//! the one kind today. Another kind implements [`Source`] and nothing else changes.
pub mod google;
pub mod history;
pub mod sheet;
use crate::{
    App,
    commerce::catalog::{Metadata, Product},
    crypto::{open, seal},
    stores::StoreId,
};
use sheet::{Cell, Tab};
use sqlx::Row;
use std::{future::Future, pin::Pin};

/// A boxed future, so [`Source`] can be used as a trait object.
pub type Fut<'a, T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send + 'a>>;

/// One change to a source. A list of them is applied together.
#[derive(Clone, Debug)]
pub enum Change {
    /// A new tab with this ID and title.
    Add { tab: i64, title: String },
    /// An existing tab gets a new title.
    Rename { tab: i64, title: String },
    /// Rows written from a cell on, over what is there.
    Write {
        tab: i64,
        row: usize,
        column: usize,
        rows: Vec<Vec<Cell>>,
    },
    /// Rows added after the last one.
    Append { tab: i64, rows: Vec<Vec<Cell>> },
    /// A tab of statistics over the History tab (`history` is its title) and, when the
    /// Catalog tab is known (`catalog` is its title), what is left of each product.
    Stats {
        tab: i64,
        history: String,
        catalog: Option<String>,
    },
    /// Marks the source as this store's.
    Mark { store: String },
}

/// Why a source can't be read or changed.
#[derive(Debug, PartialEq)]
pub enum Failure {
    /// Missing, or not shared with Stor.
    Unreachable,
    /// Shared without edit access.
    ReadOnly,
    /// Reachable, but the request failed for another reason (a temporary error).
    Unavailable,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unreachable => "source unreachable",
            Self::ReadOnly => "source read-only",
            Self::Unavailable => "source unavailable",
        })
    }
}
impl std::error::Error for Failure {}

/// Where a store keeps its catalog, branding and payment settings. Implement it to
/// serve stores from something other than Google Sheets (or from memory, in tests) and
/// pass it in [`crate::State::sources`].
pub trait Source: Send + Sync {
    /// The kind stored with each store, e.g. `google`.
    fn kind(&self) -> &'static str;
    /// The source ID in what a merchant pasted (a link or a bare ID), if it is one of
    /// this kind.
    fn id(&self, input: &str) -> Option<String>;
    /// The account a merchant shares the source with, as an editor.
    fn account(&self) -> &str;
    /// Every tab with its first [`sheet::ROWS`] rows and [`sheet::COLUMNS`] columns.
    /// Errors are [`Failure`]s.
    fn read<'a>(&'a self, id: &'a str) -> Fut<'a, Vec<Tab>>;
    /// Applies the changes in one step. Errors are [`Failure`]s.
    fn apply<'a>(&'a self, id: &'a str, changes: &'a [Change]) -> Fut<'a, ()>;
}

/// Why `error` happened. Anything that is not a [`Failure`] counts as `Unavailable`.
pub fn failure(error: &anyhow::Error) -> &Failure {
    error.downcast_ref().unwrap_or(&Failure::Unavailable)
}

/// The payment server's URL and KEY, when a store is created from the Wallet.
pub struct Payments {
    pub url: String,
    pub key: zeroize::Zeroizing<String>,
}

/// Prepares a source for a new store: proves edit access and adds the tabs it lacks
/// (Catalog with sample products, Branding, Payments, History with Stats). Tabs that
/// are there and filled in are kept as they are. The Payments the Wallet sends are
/// written over the tab's URL and KEY.
pub async fn setup(
    app: &App,
    source: &dyn Source,
    id: &str,
    store: &str,
    payments: Option<&Payments>,
) -> anyhow::Result<()> {
    let tabs = source.read(id).await?;
    let layout = sheet::classify(&tabs)?;
    // The payment server's tokens fill the Tokens tab and price the sample products.
    let view = match payments {
        Some(p) => crate::payroof::client::server_view(app, &p.url, &p.key)
            .await
            .ok()
            .map(|(_, view)| view),
        None => None,
    };
    let symbol = view.as_ref().and_then(preferred_symbol);
    let mut plan = Plan::new(&tabs, &layout);
    let catalog = if sheet::wants_content(&tabs, layout.catalog) {
        let tab = plan.place(
            layout.catalog,
            "Catalog",
            sheet::catalog_template(symbol.as_deref()),
        );
        plan.named(tab)
    } else {
        layout.catalog.map(|i| tabs[i].title.clone())
    };
    if sheet::wants_content(&tabs, layout.tokens) {
        let assets = view
            .as_ref()
            .map(crate::payroof::client::display_assets)
            .unwrap_or_default();
        plan.tokens(layout.tokens, &assets);
    }
    if sheet::wants_content(&tabs, layout.branding) {
        plan.place(layout.branding, "Branding", sheet::branding_template(store));
    }
    if let Some(p) = payments {
        plan.place(
            layout.payments,
            "Payments",
            sheet::payments_template(&p.url, &p.key),
        );
    } else if sheet::wants_content(&tabs, layout.payments) {
        plan.place(
            layout.payments,
            "Payments",
            sheet::payments_template("", ""),
        );
    }
    if layout.history.is_none() {
        plan.history(catalog);
    }
    plan.changes.push(Change::Mark {
        store: store.into(),
    });
    source.apply(id, &plan.changes).await
}

/// The starting content for missing tabs: an empty tab is reused (a new spreadsheet's
/// `Sheet1`), else a tab is added under a title no other tab has.
struct Plan<'a> {
    tabs: &'a [Tab],
    empty: std::vec::IntoIter<usize>,
    titles: Vec<String>,
    ids: Vec<i64>,
    changes: Vec<Change>,
}
impl<'a> Plan<'a> {
    fn new(tabs: &'a [Tab], layout: &sheet::Layout) -> Self {
        Self {
            tabs,
            empty: layout.empty.clone().into_iter(),
            titles: tabs.iter().map(|t| t.title.to_lowercase()).collect(),
            ids: tabs.iter().map(|t| t.id).collect(),
            changes: Vec::new(),
        }
    }
    fn title(&mut self, name: &str) -> String {
        let title = (1..)
            .map(|n| {
                if n == 1 {
                    name.to_string()
                } else {
                    format!("{name} {n}")
                }
            })
            .find(|t| !self.titles.contains(&t.to_lowercase()))
            .unwrap_or_else(|| name.into());
        self.titles.push(title.to_lowercase());
        title
    }
    /// A new tab ID, never one in use.
    fn id(&mut self) -> i64 {
        let mut id = 0;
        while id == 0 || self.ids.contains(&id) {
            id = crate::crypto::random::<4>()
                .map(|b| i64::from(u32::from_le_bytes(b) >> 1))
                .unwrap_or(1);
        }
        self.ids.push(id);
        id
    }
    /// A tab for `name` with `rows`, the role's own tab when it has one. Returns its ID.
    fn place(&mut self, role: Option<usize>, name: &str, rows: Vec<Vec<Cell>>) -> i64 {
        let tab = match role.or_else(|| self.empty.next()) {
            Some(i) => {
                let tab = self.tabs[i].id;
                if role.is_none() {
                    let title = self.title(name);
                    self.changes.push(Change::Rename { tab, title });
                }
                tab
            }
            None => {
                let (tab, title) = (self.id(), self.title(name));
                self.changes.push(Change::Add { tab, title });
                tab
            }
        };
        self.changes.push(Change::Write {
            tab,
            row: 0,
            column: 0,
            rows,
        });
        tab
    }
    /// The title `tab` has once the planned changes are made.
    fn named(&self, tab: i64) -> Option<String> {
        self.changes
            .iter()
            .rev()
            .find_map(|c| match c {
                Change::Add { tab: t, title } | Change::Rename { tab: t, title } if *t == tab => {
                    Some(title.clone())
                }
                _ => None,
            })
            .or_else(|| {
                self.tabs
                    .iter()
                    .find(|t| t.id == tab)
                    .map(|t| t.title.clone())
            })
    }
    /// A Tokens tab listing `assets`, with its rate source.
    fn tokens(&mut self, role: Option<usize>, assets: &[serde_json::Value]) {
        let tab = self.place(role, "Tokens", sheet::tokens_template(assets));
        let (column, rows) = sheet::rates_source();
        self.changes.push(Change::Write {
            tab,
            row: 0,
            column,
            rows,
        });
    }
    /// A History tab, and a Stats tab charting it (and the stock left in `catalog`).
    fn history(&mut self, catalog: Option<String>) {
        let history = self.title("History");
        let tab = self.id();
        self.changes.push(Change::Add {
            tab,
            title: history.clone(),
        });
        self.changes.push(Change::Write {
            tab,
            row: 0,
            column: 0,
            rows: sheet::history_template(),
        });
        let (stats, title) = (self.id(), self.title("Stats"));
        self.changes.push(Change::Add { tab: stats, title });
        self.changes.push(Change::Stats {
            tab: stats,
            history,
            catalog,
        });
    }
}

/// A stablecoin when the server offers one, else its first token.
fn preferred_symbol(view: &serde_json::Value) -> Option<String> {
    let symbols: Vec<String> = crate::payroof::client::display_assets(view)
        .iter()
        .filter_map(|a| a["symbol"].as_str().map(str::to_string))
        .collect();
    ["USDC", "USDT", "DAI", "ETH"]
        .iter()
        .find_map(|s| symbols.iter().find(|x| x.eq_ignore_ascii_case(s)).cloned())
        .or_else(|| symbols.into_iter().next())
}

/// Reads a store's source and takes in what changed: Branding, Payments and the Catalog.
/// What it finds wrong is written to the Payments tab's STATUS line, and the last good
/// catalog stays live. A source that can't be read for `deactivate_after_days` ends the
/// store for good.
pub async fn sync(app: &App, store: &StoreId) -> anyhow::Result<()> {
    let r = sqlx::query(
        "SELECT source_kind,source_id,metadata_json,status_text,failing_since FROM stores
         WHERE id=? AND status='active'",
    )
    .bind(&store.0)
    .fetch_one(&app.db)
    .await?;
    let source = app
        .source(r.get("source_kind"))
        .ok_or_else(|| anyhow::anyhow!("source kind unavailable"))?;
    let id: String = r.get("source_id");
    let now = crate::time::now();
    let tabs = match source.read(&id).await {
        Ok(tabs) => tabs,
        Err(error) => {
            let since: i64 = r.get::<Option<i64>, _>("failing_since").unwrap_or(now);
            let ended = *failure(&error) == Failure::Unreachable
                && now - since >= app.config.deactivate_after_days * 86400;
            sqlx::query(
                "UPDATE stores SET failing_since=?,status=CASE WHEN ? THEN 'inactive' ELSE status END
                 WHERE id=?",
            )
            .bind(since)
            .bind(ended)
            .bind(&store.0)
            .execute(&app.db)
            .await?;
            if ended {
                tracing::warn!(worker = "sync", "store deactivated: source unreachable");
            }
            return Err(error);
        }
    };
    let metadata: Metadata = serde_json::from_str(r.get("metadata_json"))?;
    let layout = sheet::classify(&tabs);
    let status = match &layout {
        Ok(layout) => refresh(app, store, (source, &id), &tabs, layout, metadata)
            .await
            .unwrap_or_else(|e| format!("Error: {e}")),
        Err(e) => format!("Error: {e}"),
    };
    let layout = layout.unwrap_or_default();
    let payments = layout.payments.map(|i| &tabs[i]);
    let status_row = payments.and_then(|t| sheet::payments(t).ok()?.status_row);
    // Stor's own History tab comes back when it is deleted.
    let history = match layout.history {
        Some(i) => Some(tabs[i].id),
        None => {
            let mut plan = Plan::new(&tabs, &layout);
            plan.history(layout.catalog.map(|i| tabs[i].title.clone()));
            source.apply(&id, &plan.changes).await.ok().and_then(|_| {
                plan.changes.iter().find_map(|c| match c {
                    Change::Add { tab, title } if title.starts_with("History") => Some(*tab),
                    _ => None,
                })
            })
        }
    };
    let mut written = r.get::<Option<String>, _>("status_text");
    if let (Some(tab), Some(row)) = (payments, status_row)
        && written.as_deref() != Some(status.as_str())
    {
        let change = Change::Write {
            tab: tab.id,
            row,
            column: 1,
            rows: vec![vec![sheet::text(&status)]],
        };
        if source
            .apply(&id, std::slice::from_ref(&change))
            .await
            .is_ok()
        {
            written = Some(status);
        }
    }
    sqlx::query(
        "UPDATE stores SET failing_since=NULL,status_tab=?,status_row=?,history_tab=?,status_text=?
         WHERE id=?",
    )
    .bind(payments.map(|t| t.id))
    .bind(status_row.map(|r| r as i64))
    .bind(history)
    .bind(written)
    .bind(&store.0)
    .execute(&app.db)
    .await?;
    Ok(())
}

/// Takes in the source's settings and catalog. Returns the status to show.
async fn refresh(
    app: &App,
    store: &StoreId,
    (source, id): (&dyn Source, &str),
    tabs: &[Tab],
    layout: &sheet::Layout,
    metadata: Metadata,
) -> anyhow::Result<String> {
    let tab = |role: Option<usize>, name: &str| {
        role.map(|i| &tabs[i])
            .ok_or_else(|| anyhow::anyhow!("no {name} tab. Restore its header row"))
    };
    let metadata = sheet::branding(tab(layout.branding, "Branding")?, metadata)?;
    sqlx::query("UPDATE stores SET metadata_json=? WHERE id=?")
        .bind(serde_json::to_string(&metadata)?)
        .bind(&store.0)
        .execute(&app.db)
        .await?;
    let payments = sheet::payments(tab(layout.payments, "Payments")?)?;
    let Some((url, key)) = payments.server else {
        return Ok("Add the URL and KEY: Payroof Wallet → Project → Stores → Connect store".into());
    };
    let view = crate::payroof::connect::ensure(app, store, &url, &key).await?;
    let offered = crate::payroof::client::display_assets(&view);
    // A missing or empty Tokens tab is filled from the payment server. Its rates are
    // read at the next sync.
    if layout.tokens.is_none_or(|i| sheet::tokens_empty(&tabs[i])) {
        let mut plan = Plan::new(tabs, layout);
        plan.tokens(layout.tokens, &offered);
        source.apply(id, &plan.changes).await?;
        return Ok("Tokens added; prices follow in a moment".into());
    }
    let mut tokens = Vec::new();
    // Rates read from CoinGecko stay empty until someone allows external data in the
    // sheet (Google asks once per sheet), and Google fetches them again only while the
    // sheet is open, or when the fetching cell is written. So Stor rewrites that cell
    // before rates age, and a rate older than RATE_AGE is not used: the status names
    // both kinds of token instead of failing.
    let now = crate::time::now();
    let tokens_tab = tab(layout.tokens, "Tokens")?;
    let rows = sheet::tokens(tokens_tab)?;
    let aging = rows
        .iter()
        .any(|r| r.updated.is_some_and(|t| now - t > RATE_REFRESH));
    // At most once per RATE_REFRESH: each rewrite is one call to CoinGecko.
    let due = aging
        && app
            .rates_refreshed
            .lock()
            .map(|mut asked| {
                let due = asked.get(&store.0).is_none_or(|t| now - t > RATE_REFRESH);
                if due {
                    asked.insert(store.0.clone(), now);
                }
                due
            })
            .unwrap_or(false);
    if due {
        let (column, cells) = sheet::rates_source();
        let change = Change::Write {
            tab: tokens_tab.id,
            row: 0,
            column,
            rows: cells,
        };
        source.apply(id, std::slice::from_ref(&change)).await?;
    }
    let (mut waiting, mut stale): (Vec<String>, Vec<String>) = Default::default();
    for mut row in rows {
        let old = row.updated.is_some_and(|t| now - t > RATE_AGE);
        let list = if old {
            Some(&mut stale)
        } else if row.fetched && row.rate.is_none() {
            Some(&mut waiting)
        } else {
            None
        };
        if let Some(list) = list
            && !list.contains(&row.symbol)
        {
            list.push(row.symbol.clone());
        }
        if old {
            row.rate = None;
        }
        let decimals = offered
            .iter()
            .find(|a| a["id"] == row.asset.as_str())
            .and_then(|a| a["decimals"].as_u64())
            .ok_or_else(|| {
                anyhow::anyhow!("Tokens: {} is not offered by the payment server", row.asset)
            })?;
        tokens.push(crate::commerce::catalog::Token {
            symbol: row.symbol,
            asset: row.asset,
            decimals: decimals as u32,
            rate: row.rate,
        });
    }
    let rows = sheet::catalog(tab(layout.catalog, "Catalog")?)?;
    let mut products = Vec::with_capacity(rows.len());
    for row in rows {
        let payments = crate::commerce::catalog::resolve(&row.prices, &tokens)
            .map_err(|e| anyhow::anyhow!("Catalog {}: {e}", row.sku))?;
        products.push(Product {
            price: row.prices[0].label(),
            sku: row.sku,
            title: row.title,
            description: row.description,
            payments,
            quantity: row.quantity,
            images: row.images,
            delivery: row.delivery,
        });
    }
    publish(app, store, &products).await?;
    let mut status = format!(
        "Live · {} product{}",
        products.len(),
        if products.len() == 1 { "" } else { "s" }
    );
    if !waiting.is_empty() {
        status += &format!(
            " · no USD rate for {}: click Allow access on Google's external data warning",
            waiting.join(", ")
        );
    }
    if !stale.is_empty() {
        status += &format!(" · USD rate out of date for {}", stale.join(", "));
    }
    Ok(status)
}
/// When Stor makes Google fetch rates again (seconds since CoinGecko priced them).
const RATE_REFRESH: i64 = 45 * 60;
/// The oldest rate a dollar price is converted with.
const RATE_AGE: i64 = 2 * 60 * 60;

pub fn delivery_aad(store: &StoreId, sku: &str, revision: i64) -> String {
    format!("stortop:v2:{}:product:{sku}:{revision}:delivery", store.0)
}

/// Replaces the live catalog with `products` in one transaction. Products no longer
/// listed stay for their orders, hidden. Stock can't drop below what is sold or held.
pub async fn publish(app: &App, store: &StoreId, products: &[Product]) -> anyhow::Result<()> {
    crate::commerce::catalog::validate_products(products)?;
    let mut tx = crate::db::write(&app.db).await?;
    let revision: i64 = sqlx::query_scalar("SELECT revision+1 FROM stores WHERE id=?")
        .bind(&store.0)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE products SET active=0 WHERE store_id=?")
        .bind(&store.0)
        .execute(&mut *tx)
        .await?;
    for p in products {
        let committed: Option<i64> =
            sqlx::query_scalar("SELECT reserved+sold FROM products WHERE store_id=? AND sku=?")
                .bind(&store.0)
                .bind(&p.sku)
                .fetch_optional(&mut *tx)
                .await?;
        anyhow::ensure!(
            committed.is_none_or(|n| n <= p.quantity),
            "Catalog {}: Stock is below the {} already sold or in checkout",
            p.sku,
            committed.unwrap_or(0)
        );
        let sealed = seal(
            &app.master,
            delivery_aad(store, &p.sku, revision).as_bytes(),
            p.delivery.as_bytes(),
        )?;
        sqlx::query(
            "INSERT INTO products
             (store_id,sku,title,description,price,payments_json,images_json,capacity,
              delivery_ciphertext,source_revision)
             VALUES(?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(store_id,sku) DO UPDATE SET title=excluded.title,price=excluded.price,
                 description=excluded.description,payments_json=excluded.payments_json,
                 images_json=excluded.images_json,capacity=excluded.capacity,
                 delivery_ciphertext=excluded.delivery_ciphertext,
                 source_revision=excluded.source_revision,active=1",
        )
        .bind(&store.0)
        .bind(&p.sku)
        .bind(&p.title)
        .bind(&p.description)
        .bind(&p.price)
        .bind(serde_json::to_string(&p.payments)?)
        .bind(serde_json::to_string(&p.images)?)
        .bind(p.quantity)
        .bind(sealed)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE stores SET revision=? WHERE id=?")
        .bind(revision)
        .bind(&store.0)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
pub fn delivery(
    app: &App,
    store: &StoreId,
    r: &sqlx::sqlite::SqliteRow,
) -> anyhow::Result<zeroize::Zeroizing<Vec<u8>>> {
    open(
        &app.master,
        delivery_aad(store, r.get("sku"), r.get("source_revision")).as_bytes(),
        r.get("delivery_ciphertext"),
    )
}
