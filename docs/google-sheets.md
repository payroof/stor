# Google Sheets

A store is one Google Sheet. Stor finds each tab by what is in it, never by its
name, and adds the ones a sheet lacks when the store is created.

| Tab | Who edits it | Content |
|---|---|---|
| Catalog | Merchant | Header `SKU · Title · Description · Price · Stock · Images · Delivery`, one product per row |
| Tokens | Merchant | Header `Token · Network · Asset · CoinGecko · USD rate · Rate time`, one row per token and network |
| Branding | Merchant | Header `Setting · Value`, rows `Title`, `Description`, `Website`, `Style`. A third column is free notes |
| Payments | Wallet, then merchant | Header `Setting · Value`, rows `URL`, `KEY`, and `STATUS`, which Stor writes |
| History | Stor | One row per product line of each paid order |
| Stats | Formulas | Sales per day, by token, by network, top products (each with a chart), and the stock left per product |

To make the tabs yourself instead, download them from the creation page ("Make the tabs
yourself", or `GET /v1/templates/{Tab}.csv`) and import each in Google Sheets: File →
Import → Upload → Insert new sheet. Each file is what Stor would add: Catalog with its
sample products, Tokens with its header only (Stor lists the payment server's tokens
in it), Branding, Payments with an empty URL and KEY, and History's header. Stats is
not offered: Stor adds it, with its charts, when it adds History.

## Branding

Rows are `Title`, `Description`, `Website` (an HTTPS link) and `Style`. Stor writes
`auto` into `Style` with a note.

**Style** sets the look of the storefront, starting with its theme:

| Value | Result |
|---|---|
| `auto` | The default. Light or dark follows each visitor's system, and a switch in the header lets them choose |
| `light` | Always light. The switch is hidden |
| `dark` | Always dark. The switch is hidden |

CSS may follow the theme word, for example `dark .store { --store-accent: #c2410c }`.
A cell that holds only CSS keeps working and counts as `auto`. The theme is applied
before the page paints, so there is no flash. Colours, classes and per-theme rules are
listed under Store theme and CSS in [http-api.md](http-api.md).

## Catalog

- **SKU**: up to 64 letters, digits, `-` `.` `_` `~`. Unique, at most 1000 products. It names the product's page, `/p/<SKU>`.
- **Price**: an amount and a unit, one per line (or separated by commas).
  - `25 USD`: worth $25 in every token of the Tokens tab that has a USD rate, rounded
    up to six significant digits (`0.00623053 ETH`).
  - `25 USDC`: exactly 25 of every Tokens row named `USDC`, one per network. A token
    priced this way keeps this price even when the product also has a dollar price.
  The storefront shows the first price as written (`$25`). The product page lists each
  token and the network to send it on.
- **Stock**: how many can be sold in total. Stor never changes this cell: paid and
  reserved units count against it, so the number stays as you typed it after a sale.
  Add to it to restock. The storefront shows what is left, and so does the Stats tab's
  *Stock left* table (Stock − units delivered, per SKU).
- **Images**: up to five HTTPS links, separated by spaces, commas or new lines.
- **Delivery**: shown to the buyer once the payment confirms (a code, a link,
  instructions). Stor keeps it encrypted.

## Tokens

The tokens the store accepts, and their dollar rates. Created from the payment server's
tokens. Delete a row to stop accepting that token.

- **Token**: the name prices use (`USDC`). Several rows may share it, one per network.
- **Network**: for you. Stor shows the payment server's own name.
- **Asset**: what identifies the token exactly, contract included:
  `eip155:<chain>/erc20:<contract>` or `eip155:<chain>/slip44:<coin>` for a network's
  coin. It must be one the payment server offers.
- **CoinGecko**: the token's CoinGecko ID, used by the rate formula.
- **USD rate**: dollars per token. Dollar stablecoins start at `1`. Others have a formula
  that reads CoinGecko's price through the cell in column N (Google refreshes it about
  hourly). Type a number over it to set a rate by hand. A token without a rate takes
  only prices in itself. Stor never calls CoinGecko. It reads the number in the cell.
  Google fetches again only while the sheet is open, or when the fetching cell is
  written, so Stor rewrites that cell once a rate is 45 minutes old (at most every 45
  minutes per store) and converts dollar prices only with rates under two hours old.
  Older ones are named on the STATUS line. Google asks once per sheet to **Allow access**
  to external data before the formula can fetch (the warning reads "Some formulas are
  trying to send and receive data from external parties". The creation page says so
  too). Until an editor clicks it, those rates stay empty and STATUS names the tokens
  waiting. Google offers no way to grant it from outside (the Sheets API can't).
  A Google Workspace admin can change the default for an organization.
- **Rate time**: when CoinGecko priced the token (Unix seconds), filled by formula. Empty
  for a rate typed in.

## Payments

`URL` is `<payment server>/v1/projects/<project>`, `KEY` a key that can only create
payment addresses. Payroof Wallet fills both in when it creates the store. For a store
made on Stor's page (or from another device), Payroof Wallet → Project → Stores →
**Connect store** issues the key and shows both, to paste into the Value column. Anyone who
can read the sheet can read the key: it creates addresses and payment notifications for
the project, and cannot move funds. A new URL or KEY takes effect at the next sync.

`STATUS` is Stor's report after each sync: `Live · 12 products`, or the first thing
to fix (`Error: Catalog row 7: Stock must be a whole number`). While something is
wrong, the last good catalog stays live.

## History and Stats

Stor appends a row when an order is paid: `Date · Order · SKU · Product · Quantity ·
Amount · Token · Network · Status · Transaction`. It never edits or removes rows. A
deleted History tab is created again (empty). Values are written as data, never as
formulas.

The Stats tab is formulas over History (`QUERY`) with a chart beside each table, so it
follows every new sale without Stor touching it. Its *Stock left* table lists each
Catalog SKU with its Stock, the units History shows as delivered, and what is left. It is a separate tab so History
stays a plain table to sort, filter and export. Edit or delete Stats freely.

## Access

Share the sheet with Stor's service account (`GET /v1/info` → `share_with`) as
**Editor**. Stor reads every tab's first 1001 rows and 12 columns, at most 30 tabs,
every `sync_seconds`. A sheet Stor can't read (deleted, or no longer shared) for
`deactivate_after_days` ends its store for good. Its address is never reused.
