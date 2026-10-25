# How Stor works

Customers pay the merchant's own address directly. Stor never holds a blockchain key,
only a Payroof key that creates payment addresses.

```text
POST /v1/stores {subdomain, source} --> sheet prepared (Catalog, Branding, Payments, History, Stats)
                                                        |
Google Sheet --> validated catalog --> cart --> reserved order --> Payroof payment address
                                                        |
             signed "confirmed" webhook --> order paid --> delivery shown, row added to History
```

## Features in detail

- **One request**: a store is `subdomain = Google Sheet`, for good. Stor adds the tabs
  the sheet lacks, with sample products. Everything after that is edited in the sheet.
  Stor issues no keys and has no admin pages. Payroof Wallet creates a store in one
  click and fills in its Payments tab.
- **Exact checkout**: prices like `25 USDC, 0.01 ETH`, converted exactly to each token's
  smallest unit on every network the [Payroof](https://github.com/payroof/payroof)
  server offers it on, a QR code and a payment window. Partial payments add up across
  transfers.
- **Light and dark**: the Branding tab's `Style` cell fixes a theme (`light` or `dark`)
  or leaves it to each visitor (`auto`), with optional CSS after it.
- **Sales in the sheet**: paid orders are appended to History. A Stats tab charts sales
  per day, token, network and product.
- **Delivery on confirmation**: each product's encrypted delivery text (a code, a
  link, instructions) is revealed to the buyer only after the payment confirms.
- **Inventory**: stock is reserved at checkout and released when a window expires.
- **No build step on the web side**: dependency-free HTML, CSS and JavaScript,
  embedded into one static binary with SQLite.

## Security

- There are no merchant credentials: creation is proof-of-work plus per-client limits,
  and a store is changed only through its sheet. Customer host-only, HttpOnly,
  SameSite cookies isolate stores and customers. Browser mutations require same-origin
  headers.
- Payment servers are allowlisted (`payroof_origins`). Redirects/proxies are disabled.
  Google is called only at fixed origins with validated sheet IDs. Values Stor writes
  into a sheet are data, never formulas. Keep production HTTPS and narrowly configured
  trusted proxies. Never enable development in production.
- Payment keys, webhook secrets and delivery snapshots use tenant/connection-bound
  encryption. Back up SQLite **and** the master key securely. Do not log either.
- Webhooks verify HMAC over the exact raw body with timestamp/replay checks,
  integration/Project/order matching and idempotent event storage. Exact integer
  payment totals determine fulfillment, not display values or token symbols.
- Inventory reservation, expiration and delivery transitions are transactional.
  Remote calls run outside SQLite write transactions. Pending payment addresses and
  History rows persist across restarts and retry without exposing secrets.

## Operations

- `GET /healthz` reports liveness and `GET /readyz` readiness. The binary's
  `healthcheck [URL]` command probes readiness and the image uses it.
- `SIGTERM` drains HTTP requests, then stops the background workers.
- Pending payment addresses and History rows persist and retry across restarts.
- The schema is one file in [`migrations/`](../migrations), applied at startup.
