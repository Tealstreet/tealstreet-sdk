# Hosted exchange server

Read the repository root `AGENTS.md` and this directory's `DESIGN.md` before
changing server behavior. Validate scoped Rust changes with
`cargo test -p safe-cex-exchange-server` and
`cargo clippy -p safe-cex-exchange-server --all-targets` from `sdk-rs/`.

## Public topic diagnostics

`POST /admin/public-topics` accepts an empty body or `{}` (1024-byte limit).
It uses the same on-box peer guard and forwarded-header refusal as
`/admin/router-lease/summary`, returns `Cache-Control: no-store`, and adds no CORS.
The response is `{ "venues": { "<venue>": { "book": B, "candles": C,
"other": N, "total": T, "peakTotal": P, "peakAt": ms } }, "checkedAt": ms }`.

Only public-only engines are observed; private account engines are excluded.
The observer API, state and callbacks compile only on native targets; wasm
retains its original engine layout and code.
Counts aggregate all live public-only engines for a venue, including multiple
engines if present. Managed topics count at final subscription frame handoff,
not when desired or queued; legacy topics count at native transport handoff.
These measure released subscriptions, not exchange acknowledgements. Unsubscribe,
socket retirement and engine teardown remove current counts. Per-venue peaks
and their wall-clock millisecond timestamps survive engine replacement until
process exit. Venues without an engine are omitted.

For Hyperliquid and Hyperliquid spot, `book` counts distinct coin identifiers
across l2Book/bbo/precision tiers; `candles` counts distinct coin/interval pairs.
`total` counts every upstream topic, so it need not equal `book + candles + other`.
Other venues report their actual topics in `other` without inferring categories.
REST requests and polling candles are not upstream socket subscriptions.

One server-owned, self-rescheduling task emits a single INFO journal line every
60 seconds: `public topics over 60s: <venue> book=B candles=C other=N total=T
peak=P peakAt=ms; ...`. It stops on shutdown and does not catch up missed intervals.
Both the route and log are counts-only: never add user IDs, account names, topic
keys, symbol lists, request payloads, credentials, or per-user breakdowns.
