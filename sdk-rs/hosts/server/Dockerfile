# syntax=docker/dockerfile:1
FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6 AS builder
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY sdk-rs/ sdk-rs/
COPY packages/safe-cex/src/exchanges/rust/construction-rules.json packages/safe-cex/src/exchanges/rust/construction-rules.json
WORKDIR /build/sdk-rs
RUN cargo build --release --locked -p safe-cex-exchange-server --bin exchange-server \
    && install -D target/release/exchange-server /out/exchange-server

FROM debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3 AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /out/exchange-server /usr/local/bin/exchange-server
USER 65532:65532
ENV TEALSTREET_PORT=23456
EXPOSE 23456
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD curl --fail --silent --show-error --max-time 2 "http://127.0.0.1:${TEALSTREET_PORT}/health" || exit 1
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/exchange-server"]
