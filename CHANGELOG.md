# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-25

First public release.

### Added
- Storefronts kept in Google Sheets: one request maps a subdomain to a sheet, and Stor prepares the Catalog, Branding, Payments, History and Stats tabs.
- Exact cryptocurrency checkout through Payroof, with QR codes, payment windows and partial payments that add up.
- Delivery of encrypted product text only after the payment is confirmed.
- Stock reservation at checkout, released when a payment window expires.
- Paid orders appended to the History tab, with a Stats tab for sales charts.
- Light, dark and automatic themes with optional custom CSS.
- Single static binary with embedded web UI and SQLite, a hardened Docker image, health endpoints and graceful shutdown.
