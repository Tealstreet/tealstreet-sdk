# syntax=docker/dockerfile:1
FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6 AS builder
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY sdk-rs/ sdk-rs/
COPY packages/safe-cex/src/exchanges/rust/construction-rules.json packages/safe-cex/src/exchanges/rust/construction-rules.json
WORKDIR /build/sdk-rs
RUN cargo build --release --locked -p safe-cex-candle-server --bin candle-server \
    && install -D target/release/candle-server /out/candle-server
# Keep the cache owner's JSON features out of the exchange engine build.
RUN cargo build --release --locked -p safe-cex-candle-cache --bin candle-cache-owner \
    && install -D target/release/candle-cache-owner /out/candle-cache-owner

FROM debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3 AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /out/candle-server /usr/local/bin/candle-server
COPY --from=builder /out/candle-cache-owner /usr/local/bin/candle-cache-owner
USER 65532:65532
ENV TEALSTREET_CANDLE_PORT=23457
EXPOSE 23457
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD curl --fail --silent --show-error --max-time 2 "http://127.0.0.1:${TEALSTREET_CANDLE_PORT}/health" || exit 1
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/candle-server"]
