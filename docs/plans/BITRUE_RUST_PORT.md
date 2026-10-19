# Bitrue futures Rust port

Branch: `feat/rust-bitrue`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.

## Plan and audit

1. Public data: typed streamed contract metadata, ticker seed/stubs and channel
   lookup, full snapshot depth (simple-depth then timed fallback), candles and
   websocket history requests, server-ping responses and reconnect watchdog.
2. Private data: exact timestamp/method/path/body HMAC, real futures URLs,
   listen-key create/refresh, bounded safe read retry, account/order reconciliation,
   one-way settings and stale-snapshot guards. Running totals yield individual
   executions with duplicate/regressive suppression and exact large IDs.
3. Trading: whole-batch preflight, caller IDs verbatim on one regular order,
   integral quantity splitting, quote-sized opening market orders, atomic attached
   TP/SL, standalone conditional protection, cancel/replace and settings commands.
   Only an explicit signature rejection (-1022) authorizes one re-signed resend;
   ambiguous writes never retry. Margin-mode changes and hedged mode fail closed.
4. Queries: documented per-execution account trades with exact IDs and window
   filtering; enumerate official endpoints before declaring a query unsupported.
   Avoid synthetic fills, PnL or ledgers inferred from unrelated fields.
5. Harness gates: WASM/SDK registration, generic replay extension, documented
   private tape with signatures and mutation matching, live 20-second/two-book
   public fixture, Binance regression, native/wasm quality gates and CI.

Each phase ends with a sub-agent audit, fixes, verification and a commit.
Keep transport routing host-side; this venue emits actual Bitrue HTTPS/WSS URLs.
Use borrowed serde types and typed continuations; no JSON value trees or JS facades.
Check crypto leftovers for provably unused code. Preserve existing venue gates.

## Landing

After all gates: merge latest landing, push, PR into `feat/rust-rewrite`, post
local adversarial sub-agent findings, fix/repeat, wait for green CI, merge PR,
create a clean follow-up branch. Never stash, rebase, force-push or touch master.

## Progress

- Plan committed; source/API audit in progress. Official documentation:
  https://www.bitrue.com/api_docs_includes_file/futures/index.html and
  https://github.com/Bitrue-exchange/USDT-M-Future-open-api-docs/tree/main/v2 .

### Public checkpoint

- Implemented typed public state and registered the venue with the WASM/SDK and
  replay harness; private/trading/query module groundwork compiles alongside it.
- Sub-agent public review PASS after fixing candle routing on reconnect,
  per-channel request serialization/correlation, timeout isolation, error replies,
  and symbol aliases. Five independent review regressions pass.
- Live 20-second tape: 776 markets/tickers, two full books, 3,287 replay frames.
  Gate PASS: TS/Rust CPU 243.8/151.7 ms; steady 131.6/89.3 ms;
  retained 2.9/2.4 MB. Fixture committed.
- Native public tests (8) and review tests (5) pass; private/trading/query phase
  audits and the documented private gate are still in progress.
- TS correction discovered from official payload units: apply contract size to
  public depth/ticker/candle volumes and retain contractSize metadata. Private
  units, lifecycle and cancellation corrections are being validated separately.

### Private, trading and query checkpoints

- Independent private review PASS: exact large IDs; stable native-contract fill
  conversion with individual L prices and cumulative-average fallback; bounded
  high-water/terminal guards; real futures listen key, 30-minute renewal and
  watchdog; atomic account/settings and paced per-symbol order snapshots.
- Independent trading review PASS after fixing protection erasure through an
  empty childOrders list. Batch preflight, quantity conservation, quote-market
  sizing, atomic brackets, safe cancel/detail/replace, caller-ID isolation and
  no uncertain write retry have direct native tests. Protective commands return
  the TS refresh result; the actual native state comes from reconciliation.
- Independent query review PASS after fixing side-specific fill ownership and
  rejecting incomplete filtered full pages. Official v2 has a forward fromId
  cursor with a most-recent default, no documented backward cursor, complete
  order-history list or complete PnL/funding ledger. These cases fail explicitly.
- Paired TS fixes also cover conditional/attached REST children, exact native
  cancellation identity, SL Mark / TP Last defaults, and malformed execution
  IDs / failed query propagation. KNOWN_TS_FIXES documents every divergence.
- Native venue tests pass (52); SDK WASM boundary test passes, proving real
  registration, worker/main frame identities/units, actual read-only leverage,
  recvWindow forwarding and exactly one newly signed -1022 resend.
- Harness adversarial fixes preserve create-ID field presence and caller IDs,
  require exact cancellation identity and signed private requests. 77 harness
  tests pass. Crypto exports remain used by this and existing venue crates;
  no further first-port leftover became unused.
