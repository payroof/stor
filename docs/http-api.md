# Stor · HTTP reference

Routes, store creation, the storefront, store CSS, requests, responses and
errors. For running Stor, see the [README](../README.md).

## HTTP routes

Store creation goes to the configured Stor origin (or one of its `origin_aliases`).
Storefront requests go to `{store}.{store_suffix}`. Host identity is part of tenant
isolation.

| Auth | Method · route | Body / result |
|---|---|---|
| Public | `GET /v1/info` | `share_with` (the account to share a sheet with), `store_suffix` |
| Public | `GET /v1/templates/{Tab}.csv` | A new store's tab as Stor would add it, for making it by hand: `Catalog`, `Tokens`, `Branding`, `Payments` or `History` (Stor's own host only. Else `404`) |
| Public | `GET /v1/altcha?purpose=store` | Store-creation proof challenge |
| Proof | `POST /v1/stores` | `subdomain`, `source`, optional `payments`, `altcha` → `201` |
| Public | `GET /v1/altcha?purpose=checkout` | Store-bound checkout challenge |
| Public | `GET /` · `GET /p/{sku}` | Storefront page. `/p/{sku}` is a product's own URL (same page, same headers, `sku` follows the catalog rules, else `404`) |
| Cookie | `GET /v1/catalog` · `GET /v1/cart` | Catalog / current cart |
| Cookie | `PUT /v1/cart/{sku}` | `{"quantity":2}` → **set**, not increment |
| Cookie | `DELETE /v1/cart/{sku}` · `DELETE /v1/cart` | Remove item / clear cart |
| Cookie + proof | `POST /v1/checkout` | `asset`, `skus`, `altcha` → order for one payment group |
| Cookie | `GET /v1/orders/{id}` | Customer-scoped order and confirmed delivery |
| HMAC | `POST /v1/webhooks/{hook}` | Raw signed payment event |

There is nothing else: no store keys, sessions, admin pages or edit routes. A store is
edited only in its Google Sheet. `/healthz` and `/readyz` are process and readiness
probes. Stor's origin serves a one-form creation page at `/`.

### Create: one request

```json
{"subdomain":"my-store","source":"https://docs.google.com/spreadsheets/d/1bEH…B88g/edit?gid=0#gid=0",
 "payments":{"url":"https://api.payroof.top/v1/projects/my-store","key":"prf_…"},
 "altcha":{"challenge":{},"solution":{}}}
```

→ `201 {"store":"my-store","url":"https://my-store.stor.top","aliases":[]}`, `aliases` lists the store's addresses under `store_suffix_aliases`.

`source` is the sheet's link or bare ID. Share the sheet with `share_with` as
**Editor** first: Stor proves edit access while it prepares the sheet, and a store
exists only once that succeeded. The mapping `subdomain = sheet` is permanent. A
subdomain is never reused, and a sheet has one active store.

`payments` is optional. Payroof Wallet sends it: the project's URL and a key that can
only create payment addresses (`references`, `paths`, `webhooks`). Stor writes both
into the sheet's Payments tab and keeps nothing else from the request. On the web page
the merchant fills in Payments later.

Preparing the sheet adds what it lacks, by content, whatever the tabs are named (an
empty tab, such as a new sheet's `Sheet1`, is reused): **Catalog** with 32 sample
products, **Tokens** (the payment server's tokens with USD rates), **Branding**,
**Payments**, and **History** with a **Stats** tab charting it.
Tabs already there and filled in are kept. See [Google Sheets](google-sheets.md).

| Code | Meaning |
|---|---|
| `400 invalid_subdomain` | 3–40 lowercase letters, digits or inner hyphens (not first or last) |
| `400 subdomain_reserved` | A name kept back for technical or ethical reasons (`www`, `login`, brands, `reserved_names`, `blocked_words`) |
| `400 invalid_source` | Not a Google Sheets link or ID |
| `400 invalid_payments` | URL not `<allowed server>/v1/projects/<project>`, or an invalid key |
| `409 subdomain_taken` · `409 source_in_use` | Address taken (ever) · sheet already has a store |
| `422 source_unreachable` | Sheet missing or not shared with `share_with` |
| `422 source_read_only` | Shared without edit access |
| `422 source_invalid` | Two tabs look like the same role, or more than 30 tabs |
| `502 source_unavailable` | Google failed. Try again |
| `503 source_unavailable` | Stor has no Google credentials configured |
| `429 rate_limited` | Per-minute and per-day creation limits per client |

### Stores after creation

Every `sync_seconds` Stor reads the sheet: Branding (title, description, website,
style), Payments (URL, KEY), Tokens and the Catalog. A changed URL or KEY registers Stor's
webhook with the payment server, which proves the key works. Catalog prices resolve
against the payment server's tokens. All of it is published in one transaction. A sheet
with a mistake keeps the last good catalog live, and Stor writes the outcome on the
Payments tab's STATUS line (`Live · 12 products`, or what to fix). Paid orders are
appended to History. A sheet that can't be read (deleted, or no longer shared) for
`deactivate_after_days` deactivates the store for good.

### Storefront

Storefronts show a product grid, each card opens that product's page at
`/p/{sku}` (History API navigation: Back/Forward, reloads and shared links work, an
unknown sku shows a not-found state). The product page has a gallery of up to 5
photos, payment options, quantity and add to bag, a sold-out state and the
description with its line breaks preserved. Checkout offers shared-asset payment
choices, then a reserved order with address QR/copy, payment window, progress and
confirmed-only delivery. Recent public order IDs may be retained in sessionStorage.
Customer cookies still authorize every lookup.

`/ui.js` provides dependency-free, tested exact amount, request, toast, empty-state
and safe DOM helpers. Merchant content never becomes `innerHTML`. CSP is self-only except
`img-src https:` for product photos (loaded without a referrer): no external fonts,
scripts, connections, injected styles, or widened script permissions.
Public catalog `assets` contains only symbol/decimal/identity display metadata.
Orders use their own connection's `asset_display`, not a newer provider's metadata.
Unknown decimals are explicitly shown as atomic units, never guessed.

### Store theme and CSS · v1

The first word of `Style` on the sheet's Branding tab is the theme (any case):

| Style begins with | Behaviour |
|---|---|
| `light` or `dark` | The theme is fixed for every visitor and the visitor's switch is hidden |
| `auto`, or anything else | The default. The page follows the visitor's system, and the switch in the header lets them choose (kept on their device) |

Whatever follows the theme word is the store's CSS. A `Style` that does not begin
with one of the three words is all CSS and the theme is `auto`, as it always was.
The theme is `store.theme` in `GET /v1/catalog` (`auto`, `light` or `dark`). A fixed
theme is applied before the first paint by `/store-theme.js`, a same-origin script
that the CSP allows and that is empty for `auto`. It sets `data-lock` on `<html>`,
and `/theme.js` then sets `data-theme` (`light` or `dark`). To style one theme, scope
a rule with `:root[data-theme="dark"] .store { … }`.

The rest of `Style` is served as `/store.css` **after** the
standard stylesheet, only on that store's public host. CSS is not HTML. CSP blocks
external scripts, fonts and imports. Merchant CSS never reaches Stor's own page or
the Wallet.

| Semantic variable | Purpose |
|---|---|
| `--store-accent` | Primary actions and links |
| `--store-background` | Page canvas |
| `--store-surface` | Cards and sheets |
| `--store-text` | Main text |
| `--store-muted` | Secondary text |
| `--store-border` | Dividers and card outlines |
| `--store-radius` | Cards and sheets |
| `--store-font` | Local/system font stack |

Stable classes: `.store`, `.store-header`, `.store-brand`, `.store-hero` (title intro),
`.product-grid`, `.product-card`, `.product-title`, `.product-description`,
`.product-price`, `.product-action`, `.cart`, `.cart-item`, `.cart-summary`,
`.checkout`, `.payment`, `.delivery`. Other DOM details are internal.

```css
.store {
  --store-accent: #8062ce;
  --store-radius: 12px;
}
.product-title { letter-spacing: -.02em; }
:root[data-theme="dark"] .store { --store-accent: #f5a15c; }
```

Written in the cell as `dark` followed by the CSS, the first rule fixes the dark theme.

## Requests and responses

Health endpoints accept no body: `/healthz` returns `{"alive":true}`.
`/readyz` returns `{"ready":true}`, or an error when the database can't answer. Static browser resources are `/`,
`/app.js`, `/proof.js`, `/ui.js`, `/qr.js`, `/theme.js`, `/style.css` and `/favicon.svg`. `/store.css` and `/store-theme.js` are a store's own style and theme. They contain no credentials.
ALTCHA returns the challenge object. Solve it and submit
`{"challenge":{…},"solution":{…}}` with the purpose-bound operation.

On the tenant host, GET catalog establishes a host-only customer cookie:

```json
{"store":{"title":"My store","description":"Digital goods","website":null},"status":{"status":"active"},"products":[{"sku":"guide","title":"Guide","description":"Download","price":"$12","images":["https://cdn.example/guide.jpg"],"payment_options":[{"asset":"eip155:1/slip44:60","amount":"1000"}],"available":10}],"assets":[{"id":"eip155:1/slip44:60","chain":"eip155:1","network":"Ethereum","symbol":"ETH","decimals":18}]}
```

GET cart, PUT `cart/guide` with `{"quantity":2}`, DELETE `cart/guide`, and
DELETE cart all return the resulting cart (200):

```json
{"items":[{"id":"guide","sku":"guide","title":"Guide","quantity":2,"payment_options":[{"asset":"eip155:1/slip44:60","unit_amount":"1000","line_total":"2000"}]}],"groups":[{"skus":["guide"],"payment_options":[{"asset":"eip155:1/slip44:60","amount":"2000"}]}]}
```

An empty cart is `{"items":[],"groups":[]}`. Quantities are 1–100. Any products can
share a cart. An order is paid with one asset, so `groups` splits the cart into as
few payments as a greedy cover finds: each group takes the asset most remaining
items accept (ties: first asset in order) and lists every asset its items share,
with totals. A cart whose items share an asset is one group.
DELETE removes, while PUT sets rather than increments. Browser mutations require
the exact tenant Origin and customer cookie.

POST checkout input:

```json
{"asset":"eip155:1/slip44:60","skus":["guide"],"altcha":{"challenge":{},"solution":{}}}
```

The proof placeholders must be replaced by real solved objects. `skus` names the
cart items the order pays for (normally one group). They must all be in the cart
(`409 cart_changed`) and accept `asset` (`409 incompatible_payment_options`). Once
the order has its address, only those items leave the cart. Checkout and
GET `/v1/orders/{id}` return the customer-owned immutable order:

```json
{"id":"ord_...","status":"awaiting_payment","chain":"eip155:1","asset":"eip155:1/slip44:60","amount":"2000","paid":"0","remaining":"2000","address":"0x...","asset_display":{"id":"eip155:1/slip44:60","chain":"eip155:1","network":"Ethereum","symbol":"ETH","decimals":18},"items":[{"sku":"guide","title":"Guide","quantity":2,"unit_amount":"1000","image":null}],"expires_at":"2026-10-08T12:30:00Z","transaction":null}
```

An order accepts several payments to its address. They count together once they sum to `amount`. `partial` means some but not all arrived. An order that expires with any payment goes to `manual_review`. `seen` never delivers. Matching `confirmed` changes the status to `delivered`,
sets `transaction`, and adds `"delivery":["Secret instructions","Secret instructions"]`.
There is no cross-customer or cross-tenant order lookup.

The Payroof server sends POST `/v1/webhooks/{hook}` with `Payroof-Id`,
`Payroof-Timestamp`, and `Payroof-Signature`:

```json
{"id":"evt_...","event":"confirmed","project":"my-store","reference":"ord_...","chain":"eip155:1","asset":"eip155:1/slip44:60","address":"0x...","transaction":"0x...","amount":"2000","block":{"height":"123","hash":"0x..."}}
```

Signature is HMAC-SHA256 over `timestamp + "." + exact_raw_body` using the
subscription secret. Valid accepted/idempotent callbacks return 204.

### Errors

Errors are `{"error":"invalid_quantity","message":"invalid quantity","request_id":"req_..."}`.
Use codes/status, not message wording. No error exposes credentials or SQL.

| HTTP | Meaning |
|---|---|
| 400 | Invalid input or proof |
| 401 | Invalid webhook signature or customer cookie |
| 403 | Origin or permission violation |
| 404 | Unknown tenant or order |
| 405 | Method mismatch |
| 502 | Source or payment server failed (`source_unavailable`, `mapping_unavailable`) |
| 408 | Request timeout |
| 409 | Taken subdomain or sheet, inventory or payment conflict |
| 422 | Sheet unreachable, read-only or ambiguous |
| 413 | Body too large |
| 429 | Rate limited |
| 500 | Unexpected or corrupt persisted state |
| 503 | Local database unavailable, no source configured, or the order's payment address is still being created (`order_initializing`) |
