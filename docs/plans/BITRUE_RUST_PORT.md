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

### Harness review and final verification

- Independent harness review PASS after retaining validation failures in the
  signature counter even if a later signed GET retry succeeds; regression added.
  78 harness tests pass. The private tape requires all writes to be consumed,
  exact caller/native IDs and request bodies, signed private reads/writes,
  subscriptions and server ping replies, and all individual execution prices.
- Four required replay gates PASS after landing integration: Bitrue public
  (three runs), Bitrue private (20 frames), Binance public and Binance private.
  Bitrue three-run medians: TS/Rust CPU 293.0/224.0 ms; steady 187.2/141.2 ms;
  retained 3.7/2.4 MB. Binance CPU 284.6/149.5 ms, memory 4.2/2.3 MB.
- Final private price recovery also ties cumulative value to its exact filled
  quantity across REST advancement, including an earlier L-only fill. A new
  native and paired TS regression cover the interleaving.
- Typecheck (44 tasks) and lint (28 tasks) pass. Workspace native clippy passes;
  full workspace tests and final WASM rebuild are running. The default root
  test command runs both test-ci and test-unit concurrently; local unrelated
  5-second import timeouts are being rechecked after compilation settles.
- No budget increase: inherited landing budget is 3,145,728 bytes. Final head
  must pass every CI check before merge; no outstanding review waiver.

### Final artifact checkpoint

- Final required gates PASS: Bitrue public CPU 252.5/196.1 ms, steady
  164.5/121.2 ms, retained 3.7/2.4 MB; 3,287 frames/776 markets/two books.
  Private 20 frames PASS. Binance public CPU 258.5/130.0 ms, retained 4.2/2.3
  MB; public and 14-frame private gates PASS.
- Final WASM 3,138,885 bytes, below inherited 3,145,728-byte budget.
- Workspace tests PASS (641 including doctests), Bitrue tests PASS (53),
  complete SDK WASM tests PASS (22), host tests PASS (25), harness PASS (78),
  targeted Bitrue TS PASS (56). Native/wasm clippy, fmt and bans PASS.
- Final package typecheck and lint PASS. Updated older history tests to use
  documented native trade IDs and to expect rejected reads to propagate; no
  production exceptions were added for outdated test fixtures.
- Opening a draft PR for required local adversarial review and CI while the
  complete TS suite's local 30-second import-timeout recheck finishes. It must
  be ready and every CI check green before merging.
