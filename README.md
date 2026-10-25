<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/logo-dark.svg">
  <img src="docs/brand/logo-light.svg" alt="Stor" width="128">
</picture>

# Stor

Run an online store from a Google Sheet and get paid in crypto.

[![CI](https://github.com/payroof/stor/actions/workflows/ci.yml/badge.svg)](https://github.com/payroof/stor/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/stortop.svg)](https://crates.io/crates/stortop)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

## Why Stor?

- **Your sheet is your store.** Products, prices and branding live in a Google Sheet.
- **You keep the money.** Customers pay your own address. Stor never holds a blockchain key.
- **Exact prices.** Price in `25 USDC` or `0.01 ETH`, with a QR code and a payment window.
- **Instant delivery.** Codes, links or instructions are shown only after payment confirms.
- **Simple to run.** One small binary with SQLite. No build step for the web pages.

## Install

The command is called `stortop`.

- **Homebrew:** `brew install payroof/tap/stor`
- **Cargo:** `cargo install stortop`
- **Docker:** `docker pull ghcr.io/payroof/stor:0.1.0` (see [`compose.yaml`](compose.yaml))
- **Prebuilt binaries:** download from [GitHub Releases](https://github.com/payroof/stor/releases).

## Quick start

```sh
export STORTOP_MASTER_KEY=$(openssl rand -hex 32)
export STORTOP_ALTCHA_KEY=$(openssl rand -hex 32)
curl -O https://raw.githubusercontent.com/payroof/stor/main/config/stortop.example.toml
stortop stortop.example.toml
```

Set `google_credentials` in the config, then open <http://127.0.0.1:8081> to create a
store. It is served at `http://<store>.stor.localhost:8081`. You also need a running
[Payroof](https://github.com/payroof/payroof) server. For Docker and production
setup, see [Configuration](docs/configuration.md).

## Configuration

One TOML file, passed as the only argument. Every key is explained in
[`config/stortop.example.toml`](config/stortop.example.toml) and
[docs/configuration.md](docs/configuration.md).

## Documentation

- [Configuration](docs/configuration.md): config keys, environment, Docker, running from source
- [Google Sheets](docs/google-sheets.md): the tabs, prices, History and Stats
- [HTTP reference](docs/http-api.md): routes, store creation, store CSS, errors
- [How it works](docs/architecture.md): features, security and operations
- [Development](docs/development.md): checks and releases
- [Changelog](CHANGELOG.md)

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT](LICENSE-MIT), at your option.
