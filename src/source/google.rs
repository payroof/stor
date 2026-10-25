//! Google Sheets, through a service account the merchant shares a spreadsheet with as
//! an editor. Only fixed Google origins are called, and only with IDs, never URLs from
//! a merchant.
use super::{
    Change, Failure, Fut, Source,
    sheet::{self, Cell, Tab},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;
use zeroize::Zeroizing;

const SHEETS: &str = "https://sheets.googleapis.com/v4/spreadsheets";
/// Most tabs a source may have.
const TABS: usize = 30;

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
}
pub struct Google {
    email: String,
    key: jsonwebtoken::EncodingKey,
    http: reqwest::Client,
    /// The current access token and when it expires.
    token: Mutex<Option<(Zeroizing<String>, i64)>>,
}
#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}
impl Google {
    pub fn load(path: &str, http: reqwest::Client) -> anyhow::Result<Self> {
        let raw = Zeroizing::new(std::fs::read_to_string(path)?);
        let mut account: ServiceAccount = serde_json::from_str(&raw)?;
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key.as_bytes())?;
        use zeroize::Zeroize;
        account.private_key.zeroize();
        Ok(Self {
            email: account.client_email,
            key,
            http,
            token: Mutex::new(None),
        })
    }
    /// An access token, reused until a minute before it expires.
    async fn token(&self) -> anyhow::Result<Zeroizing<String>> {
        let now = crate::time::now();
        if let Some((token, expires)) = &*self.token.lock().map_err(|_| Failure::Unavailable)?
            && *expires > now + 60
        {
            return Ok(token.clone());
        }
        let claims = Claims {
            iss: &self.email,
            scope: "https://www.googleapis.com/auth/spreadsheets",
            aud: "https://oauth2.googleapis.com/token",
            iat: now,
            exp: now + 3600,
        };
        let assertion = Zeroizing::new(jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &claims,
            &self.key,
        )?);
        let v = crate::fetch::json(
            self.http
                .post("https://oauth2.googleapis.com/token")
                .form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                    ("assertion", &*assertion),
                ])
                .send()
                .await?
                .error_for_status()?,
        )
        .await?;
        let token = Zeroizing::new(
            v["access_token"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Google authorization failed"))?
                .to_string(),
        );
        let expires = now + v["expires_in"].as_i64().unwrap_or(3600).min(3600);
        *self.token.lock().map_err(|_| Failure::Unavailable)? = Some((token.clone(), expires));
        Ok(token)
    }
    /// A Sheets API call. Access errors become [`Failure`]s.
    async fn call(&self, request: reqwest::RequestBuilder, write: bool) -> anyhow::Result<Value> {
        let token = self.token().await.map_err(|_| Failure::Unavailable)?;
        let response = request
            .bearer_auth(&*token)
            .send()
            .await
            .map_err(|_| Failure::Unavailable)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return crate::fetch::json(response)
                .await
                .map_err(|_| Failure::Unavailable.into());
        }
        // Google's own explanation, for the operator. It never holds credentials.
        let reason = crate::fetch::json(response)
            .await
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
            .unwrap_or_default();
        tracing::warn!(status, reason = %reason.chars().take(300).collect::<String>(), "Google Sheets request failed");
        Err(match status {
            403 if write => Failure::ReadOnly,
            403 | 404 => Failure::Unreachable,
            _ => Failure::Unavailable,
        }
        .into())
    }
    async fn tabs(&self, id: &str) -> anyhow::Result<Vec<Tab>> {
        let meta = self
            .call(
                self.http
                    .get(format!("{SHEETS}/{id}"))
                    .query(&[("fields", "sheets.properties(sheetId,title)")]),
                false,
            )
            .await?;
        let mut tabs: Vec<Tab> = meta["sheets"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing tabs"))?
            .iter()
            .map(|t| {
                let p = &t["properties"];
                Some(Tab {
                    // Google leaves out a zero ID.
                    id: p["sheetId"].as_i64().unwrap_or(0),
                    title: p["title"].as_str()?.to_string(),
                    rows: Vec::new(),
                })
            })
            .collect::<Option<_>>()
            .ok_or_else(|| anyhow::anyhow!("missing tab identity"))?;
        anyhow::ensure!(tabs.len() <= TABS, "more than {TABS} tabs");
        let filters: Vec<Value> = tabs
            .iter()
            .map(|t| {
                json!({"gridRange": {
                    "sheetId": t.id, "startRowIndex": 0, "endRowIndex": sheet::ROWS,
                    "startColumnIndex": 0, "endColumnIndex": sheet::COLUMNS
                }})
            })
            .collect();
        let values = self
            .call(
                self.http
                    .post(format!("{SHEETS}/{id}/values:batchGetByDataFilter"))
                    .json(&json!({
                        "dataFilters": filters, "valueRenderOption": "UNFORMATTED_VALUE",
                        "dateTimeRenderOption": "FORMATTED_STRING"
                    })),
                false,
            )
            .await?;
        for matched in values["valueRanges"].as_array().into_iter().flatten() {
            let tab = matched["dataFilters"][0]["gridRange"]["sheetId"]
                .as_i64()
                .unwrap_or(0);
            if let Some(t) = tabs.iter_mut().find(|t| t.id == tab) {
                t.rows = grid(&matched["valueRange"]["values"]);
            }
        }
        Ok(tabs)
    }
}
impl Source for Google {
    fn kind(&self) -> &'static str {
        "google"
    }
    /// A spreadsheet link (`…/spreadsheets/d/<ID>/…`) or the bare ID.
    fn id(&self, input: &str) -> Option<String> {
        let input = input.trim();
        let id = match input.split_once("/spreadsheets/d/") {
            Some((prefix, rest)) => {
                let u = url::Url::parse(input).ok()?;
                (u.scheme() == "https"
                    && u.host_str() == Some("docs.google.com")
                    && prefix == "https://docs.google.com")
                    .then_some(())?;
                rest.split(['/', '?', '#']).next()?
            }
            None => input,
        };
        ((20..=100).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
        .then(|| id.to_string())
    }
    fn account(&self) -> &str {
        &self.email
    }
    fn read<'a>(&'a self, id: &'a str) -> Fut<'a, Vec<Tab>> {
        Box::pin(self.tabs(id))
    }
    fn apply<'a>(&'a self, id: &'a str, changes: &'a [Change]) -> Fut<'a, ()> {
        Box::pin(async move {
            let requests: Vec<Value> = changes.iter().flat_map(requests).collect();
            self.call(
                self.http
                    .post(format!("{SHEETS}/{id}:batchUpdate"))
                    .json(&json!({ "requests": requests })),
                true,
            )
            .await?;
            Ok(())
        })
    }
}

fn grid(values: &Value) -> Vec<Vec<String>> {
    values
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            r.as_array()
                .into_iter()
                .flatten()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => String::new(),
                })
                .collect()
        })
        .collect()
}

/// A cell as Sheets takes it. Text is never read as a formula.
fn cell(cell: &Cell) -> Value {
    match cell {
        Cell::Header(s) => json!({
            "userEnteredValue": {"stringValue": s},
            "userEnteredFormat": {"textFormat": {"bold": true}}
        }),
        Cell::Text(s) => json!({"userEnteredValue": {"stringValue": s}}),
        Cell::Number(n) => json!({"userEnteredValue": {"numberValue": n}}),
        // Sheets counts days from 1899-12-30. Unix time starts 25569 days later.
        Cell::Date(t) => json!({
            "userEnteredValue": {"numberValue": *t as f64 / 86400.0 + 25569.0},
            "userEnteredFormat": {"numberFormat": {"type": "DATE_TIME", "pattern": "yyyy-mm-dd hh:mm"}}
        }),
        Cell::Formula(f) => json!({"userEnteredValue": {"formulaValue": f}}),
    }
}
fn rows(rows: &[Vec<Cell>]) -> Value {
    rows.iter()
        .map(|r| json!({"values": r.iter().map(cell).collect::<Vec<_>>()}))
        .collect()
}
const FIELDS: &str = "userEnteredValue,userEnteredFormat(numberFormat,textFormat)";

fn requests(change: &Change) -> Vec<Value> {
    match change {
        Change::Add { tab, title } => vec![json!({"addSheet": {"properties": {
            "sheetId": tab, "title": title, "gridProperties": {"frozenRowCount": 1}
        }}})],
        Change::Rename { tab, title } => vec![json!({"updateSheetProperties": {
            "properties": {"sheetId": tab, "title": title, "gridProperties": {"frozenRowCount": 1}},
            "fields": "title,gridProperties.frozenRowCount"
        }})],
        Change::Write {
            tab,
            row,
            column,
            rows: values,
        } => vec![json!({"updateCells": {
            "start": {"sheetId": tab, "rowIndex": row, "columnIndex": column},
            "rows": rows(values), "fields": FIELDS
        }})],
        Change::Append { tab, rows: values } => vec![json!({"appendCells": {
            "sheetId": tab, "rows": rows(values), "fields": FIELDS
        }})],
        Change::Stats {
            tab,
            history,
            catalog,
        } => stats(*tab, history, catalog.as_deref()),
        Change::Mark { store } => vec![json!({"createDeveloperMetadata": {"developerMetadata": {
            "metadataKey": "stortop", "metadataValue": store,
            "location": {"spreadsheet": true}, "visibility": "PROJECT"
        }}})],
    }
}

/// A tab title quoted for a formula: `'It''s'`.
fn quoted(title: &str) -> String {
    format!("'{}'", title.replace('\'', "''"))
}

/// What is left of each Catalog product: its Stock (everything it may ever sell) less
/// the units History lists as delivered. Stor never writes the merchant's Stock cell;
/// this table follows it and every sale.
fn stock_left(history: &str, catalog: &str) -> String {
    let (h, c) = (quoted(history), quoted(catalog));
    format!(
        "=LET(sku,FILTER({c}!A2:A,{c}!A2:A<>\"\"),stock,FILTER({c}!E2:E,{c}!A2:A<>\"\"),\
         sold,MAP(sku,LAMBDA(s,SUMIFS({h}!E2:E,{h}!C2:C,s,{h}!I2:I,\"delivered\"))),\
         VSTACK({{\"SKU\",\"Stock\",\"Sold\",\"Left\"}},HSTACK(sku,stock,sold,ARRAYFORMULA(stock-sold))))"
    )
}

/// Four summaries of the History tab, as formulas (they follow every new sale), each
/// with its chart beside the tables: sales per day, by token, by network, and the
/// products sold most. With the Catalog tab's title, a fifth table: the stock left.
fn stats(tab: i64, history: &str, catalog: Option<&str>) -> Vec<Value> {
    let source = format!("{}!A2:J", quoted(history));
    let query = |select: &str| Cell::Formula(format!("=QUERY({source},\"{select}\",0)"));
    let tables = [
        (
            0,
            "Sales per day",
            "select toDate(A), count(B) where A is not null group by toDate(A) label toDate(A) 'Day', count(B) 'Sales'",
        ),
        (
            3,
            "Sales by token",
            "select G, count(B), sum(F) where G is not null group by G label G 'Token', count(B) 'Sales', sum(F) 'Amount'",
        ),
        (
            7,
            "Sales by network",
            "select H, count(B) where H is not null group by H label H 'Network', count(B) 'Sales'",
        ),
        (
            10,
            "Top products",
            "select D, sum(E) where D is not null group by D order by sum(E) desc limit 10 label D 'Product', sum(E) 'Sold'",
        ),
    ];
    let mut out = Vec::new();
    for (column, title, select) in tables {
        out.extend(requests(&Change::Write {
            tab,
            row: 0,
            column,
            rows: vec![vec![Cell::Header(title.into())], vec![query(select)]],
        }));
    }
    // The charts go right of the tables.
    let mut charts_at = 13;
    if let Some(catalog) = catalog {
        out.extend(requests(&Change::Write {
            tab,
            row: 0,
            column: 13,
            rows: vec![
                vec![Cell::Header("Stock left".into())],
                vec![Cell::Formula(stock_left(history, catalog))],
            ],
        }));
        charts_at = 18;
    }
    let range = |column: usize| {
        json!({"sources": [{
            "sheetId": tab, "startRowIndex": 2, "endRowIndex": 1000,
            "startColumnIndex": column, "endColumnIndex": column + 1
        }]})
    };
    let position = |index: usize| {
        json!({"overlayPosition": {
            "anchorCell": {"sheetId": tab, "rowIndex": index * 18, "columnIndex": charts_at},
            "widthPixels": 520, "heightPixels": 320
        }})
    };
    let basic = |kind: &str, axis: &str, column: usize| {
        json!({"chartType": kind, "legendPosition": "NO_LEGEND", "headerCount": 0,
            "domains": [{"domain": {"sourceRange": range(column)}}],
            "series": [{"series": {"sourceRange": range(column + 1)}, "targetAxis": axis}]})
    };
    let pie = |column: usize| {
        json!({"legendPosition": "RIGHT_LEGEND",
            "domain": {"sourceRange": range(column)}, "series": {"sourceRange": range(column + 1)}})
    };
    let charts = [
        (
            "Sales per day",
            json!({"basicChart": basic("COLUMN", "LEFT_AXIS", 0)}),
        ),
        ("Sales by token", json!({"pieChart": pie(3)})),
        ("Sales by network", json!({"pieChart": pie(7)})),
        (
            "Top products",
            json!({"basicChart": basic("BAR", "BOTTOM_AXIS", 10)}),
        ),
    ];
    for (index, (title, spec)) in charts.into_iter().enumerate() {
        let mut spec = spec;
        spec["title"] = json!(title);
        out.push(json!({"addChart": {"chart": {"spec": spec, "position": position(index)}}}));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_come_from_links_or_bare_ids_only() {
        let google = Google {
            email: String::new(),
            key: jsonwebtoken::EncodingKey::from_secret(b"unused"),
            http: reqwest::Client::new(),
            token: Mutex::new(None),
        };
        let id = "1bEHabcdefghijklmnopqrstuvwxyzB88g";
        for input in [
            format!("https://docs.google.com/spreadsheets/d/{id}/edit?gid=0#gid=0"),
            format!("https://docs.google.com/spreadsheets/d/{id}"),
            format!(" {id} "),
        ] {
            assert_eq!(google.id(&input).as_deref(), Some(id), "{input}");
        }
        for input in [
            format!("http://docs.google.com/spreadsheets/d/{id}/edit"),
            format!("https://evil.example/spreadsheets/d/{id}/edit"),
            format!("https://docs.google.com.evil.example/spreadsheets/d/{id}"),
            "short".into(),
            format!("{id}/../x"),
        ] {
            assert_eq!(google.id(&input), None, "{input}");
        }
    }
    #[test]
    fn written_text_is_never_a_formula_and_dates_are_serial_days() {
        assert_eq!(
            cell(&Cell::Text("=IMPORTXML(\"x\")".into()))["userEnteredValue"],
            json!({"stringValue": "=IMPORTXML(\"x\")"})
        );
        assert_eq!(
            cell(&Cell::Date(0))["userEnteredValue"]["numberValue"],
            json!(25569.0)
        );
    }
    #[test]
    fn stats_quote_the_history_title_and_add_four_charts() {
        let requests = stats(7, "Sales 'log'", None);
        let charts = requests
            .iter()
            .filter(|r| r.get("addChart").is_some())
            .count();
        assert_eq!(charts, 4);
        let text = serde_json::to_string(&requests).unwrap();
        assert!(text.contains("'Sales ''log'''!A2:J"));
        assert!(!text.contains("Stock left"));
    }
    #[test]
    fn stats_list_the_stock_left_beside_the_sales_when_the_catalog_is_known() {
        let requests = stats(7, "History", Some("Shop's catalog"));
        let text = serde_json::to_string(&requests).unwrap();
        assert!(text.contains("Stock left"));
        let formula = stock_left("History", "Shop's catalog");
        assert!(formula.starts_with("=LET(sku,FILTER('Shop''s catalog'!A2:A,"));
        assert!(
            formula
                .contains("SUMIFS('History'!E2:E,'History'!C2:C,s,'History'!I2:I,\"delivered\")")
        );
        assert!(formula.ends_with("HSTACK(sku,stock,sold,ARRAYFORMULA(stock-sold))))"));
        // The charts move right of the fifth table.
        let anchors: Vec<i64> = requests
            .iter()
            .filter_map(|r| {
                r["addChart"]["chart"]["position"]["overlayPosition"]["anchorCell"]["columnIndex"]
                    .as_i64()
            })
            .collect();
        assert_eq!(anchors, vec![18; 4]);
    }
}
