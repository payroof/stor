# Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
for f in web/*.js web/*/*.js; do node --check "$f"; done
node --test tests/*.test.mjs          # browser code: UI, proof of work, QR codes
cargo build --locked --release
docker build -t stor:dev .
```

## Release

Versions follow [SemVer](https://semver.org). Changes are listed in
[CHANGELOG.md](../CHANGELOG.md). Pushing a tag `vX.Y.Z` that matches `Cargo.toml` runs
the release workflow: checks, then crates.io
[`stortop`](https://crates.io/crates/stortop), then `ghcr.io/payroof/stor:X.Y.Z`, then
a GitHub release with the changelog section and prebuilt binaries. Nothing is deployed
automatically.
