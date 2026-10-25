# Configuration

Stor reads one TOML file, passed as the only argument (`stortop <config.toml>`). Start
from [`config/stortop.example.toml`](../config/stortop.example.toml), which documents
every key and its default.

## Config file

| Key | Purpose |
|---|---|
| `bind`, `origin` | Listen address and Stor's own origin (creation page, webhooks) |
| `origin_aliases` | Other origins of the same server, e.g. loopback next to a LAN address |
| `store_suffix` | Host suffix of stores: `<store>.<store_suffix>` |
| `store_suffix_aliases` | Other suffixes that reach the same stores. Store links use `store_suffix` |
| `database` | SQLite URL. Its directory must be writable |
| `payroof_origins` | Origins of the Payroof servers stores may connect to (at least one) |
| `google_credentials` | Service-account key file. Merchants share sheets with its account |
| `sync_seconds` | How often each sheet is read (30-3600, default 120) |
| `deactivate_after_days` | A sheet unreadable this long ends its store (default 7) |
| `development` | Allows HTTP origins. **Never in production** |
| `trusted_proxies` | CIDRs whose forwarding headers set the client IP |
| `reserved_names`, `blocked_words` | Store names refused whole (brands, phishing bait) and words refused in any name (sexual content, abuse, hate, illegal trade, fraud). The example file lists the defaults. Stor's technical names (`www`, `api`, `mail`, `login`, ...) are always reserved |
| `[limits]` | Body, rate, concurrency, stores per client per day, customers per store. See the example file for every key and default |

## Environment

| Variable | Purpose |
|---|---|
| `STORTOP_MASTER_KEY` | Encrypts payment keys, webhook secrets and delivery data |
| `STORTOP_ALTCHA_KEY` | Signs proof-of-work challenges |
| `RUST_LOG` | Log filter, e.g. `info` (JSON logs on stdout) |

Each key is 32 random bytes as 64 hex characters, and the two must differ:

```sh
export STORTOP_MASTER_KEY=$(openssl rand -hex 32) \
       STORTOP_ALTCHA_KEY=$(openssl rand -hex 32)
```

Back up the database **and** the master key.

## Docker

[`compose.yaml`](../compose.yaml) runs `ghcr.io/payroof/stor:<version>` read-only,
without capabilities or privilege escalation, as UID 10001, with the database on a
named volume. Mount your config at `/etc/stortop/stortop.toml` and set
`bind = "0.0.0.0:8081"` and `database = "sqlite:///data/stortop.sqlite"` in it.

```sh
cp config/stortop.example.toml stortop.toml   # edit as above, plus origin,
                                              # store_suffix and payroof_origins
docker compose up -d                          # or add --build to build locally
curl -fsS http://127.0.0.1:8081/readyz
```

Stor needs a reachable Payroof server (`payroof_origins`) and wildcard DNS and TLS for
`*.<store_suffix>`.

## From source

Requires the Rust toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml).

```sh
RUST_LOG=info cargo run --locked --release -- config/stortop.example.toml
```

With `google_credentials` set, open `http://127.0.0.1:8081` to create a store (or use
Payroof Wallet). It is served at `http://<store>.stor.localhost:8081`.
