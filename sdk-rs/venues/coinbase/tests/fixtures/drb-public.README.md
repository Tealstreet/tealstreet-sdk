# Coinbase drb public replay

`drb-public.jsonl` was recorded from the live production gateway on
2026-11-07. Response bodies and WebSocket payloads are verbatim. The recorder
does not read credentials, authenticate, or retain HTTP headers. Capture URLs
and parameters, capture times, and the chart range are included in the tape.

Re-record from the repository root:

```sh
node sdk-rs/harness/record-coinbase-drb-public.mjs
```

Run the offline acceptance gate:

```sh
cd sdk-rs
cargo test -j 2 -p safe-cex-venue-coinbase --test drb_public_replay
```

The WASM/EngineHost gate exercises the recorded data through the JavaScript
host and binary frames. From the repository root:

```sh
CARGO_BUILD_JOBS=2 bash sdk-rs/harness/build.sh --venue coinbase
node sdk-rs/harness/coinbase-drb-public.integration.mjs
```

The gate uses the real engine, its request callbacks, subscription commands,
store, and candle frames. It requires markets and tickers within the harness's
30-second startup budget. HTTP replay changes only POST response IDs to match
outbound requests; the GET chart response retains its missing ID. The gate
checks HTTP methods, discovery parameters and the chart range,
rejects legacy hosts and authorization headers, and reconstructs every recorded
book change independently. It also checks funding, candle and trade units,
duplicate suppression, and immediate book invalidation on sequence gaps.

The HTTP capture includes funding history for protocol reference; the SDK's
public funding surface is the ticker's projected `funding_8h` rate and next
eight-hour settlement time. No account income ledger is inferred from public
funding history.

Public subscriptions use `wss://streams.drb.coinbase.com/ws/api/v2`.
The main `wss://drb.coinbase.com/ws/api/v2` rejected `public/subscribe` with
`-32601` (method not in allowlist) during live capture. This separation is also
documented in the [API host reference](https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/introduction).

Chart history uses the documented GET URL and query parameters. Both POST
`/api/v2/public/get_tradingview_chart_data` and POST `/api/v2` returned HTTP 400
with `-32601` during capture, while the GET response succeeded and omitted `id`.
The transport allows a missing ID only for that engine-correlated HTTP read.
