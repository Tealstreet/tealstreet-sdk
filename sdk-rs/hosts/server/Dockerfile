# syntax=docker/dockerfile:1
FROM rust:1.99.0-bookworm AS builder
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY sdk-rs/ ./
RUN cargo build --release --locked -p safe-cex-exchange-server --bin exchange-server \
    && install -D target/release/exchange-server /out/exchange-server

FROM debian:bookworm-slim AS runtime
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
