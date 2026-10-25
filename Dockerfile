# syntax=docker/dockerfile:1
# Build: a static, size-optimised musl binary (see [profile.release] in Cargo.toml).
FROM rust:1.99.1-alpine3.24@sha256:b1d5dd5a22a09404d3e798c0cf051b689077360374eb505fa5bb93c479693d87 AS builder
RUN apk add --no-cache build-base ca-certificates
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
# The image already contains the pinned toolchain. Rustfmt/clippy aren't needed here.
ENV RUSTUP_TOOLCHAIN=1.99.1
COPY Cargo.toml Cargo.lock ./
COPY migrations migrations
COPY src src
COPY web web
RUN --mount=type=cache,id=stortop-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=stortop-musl,target=/build/target \
    cargo build --locked --release \
    && mkdir -p /out/data /out/tmp && chmod 1777 /out/tmp \
    && cp target/release/stortop /out/stortop \
    && ! readelf -l /out/stortop | grep -q INTERP

# Runtime: nothing but the binary, CA certificates and two writable directories.
FROM scratch
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /out/stortop /usr/local/bin/stortop
COPY --from=builder --chown=10001:10001 /out/data /data
COPY --from=builder /out/tmp /tmp
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
USER 10001:10001
WORKDIR /data
EXPOSE 8081
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=6 \
    CMD ["/usr/local/bin/stortop", "healthcheck", "http://127.0.0.1:8081/readyz"]
ENTRYPOINT ["/usr/local/bin/stortop"]
CMD ["/etc/stortop/stortop.toml"]
