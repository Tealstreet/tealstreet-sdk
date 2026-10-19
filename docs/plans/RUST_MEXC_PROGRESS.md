# MEXC futures port

Branch: `feat/rust-mexc`. Scope: USDT/USDC perpetual futures. The venue
brief and `RUST_REWRITE.md` define acceptance. Caller client order IDs are
rejected across the whole batch before any write.

## Phases

1. Public data: borrowed wire structs, streamed market/ticker lists, full
   incremental depth (uncompressed versions, bounded buffering, gap resync),
   candles, watchdog/reconnect. Register workspace/wasm/harness venue.
2. Private data: exact REST/WS signatures, login/filter, streamed account
   reads, periodic reconciliation, snapshot epochs, terminal order shields,
   native position IDs and per-side settings.
3. Trading: sequenced jobs, all-batch client-ID preflight, paced placement,
   batch entitlement fallback, plan/position stops, cancel/replace, checked
   cancel results, leverage/margin/mode changes. Never retry ambiguous writes.
4. Queries: typed paged fills/history/income/public trades and candle reads,
   Binance-compatible result JSON. Unsupported behavior must fail explicitly.
5. Gates: documented private tape, live 20-second two-book recording,
   committed compressed public fixture, correctness/CPU/memory gates,
   Binance regressions, workspace checks, adversarial PR review.

Each phase gets plan audit, verification, sub-agent implementation review,
fixes, and a stable commit. Once every gate passes, merge the target branch,
rerun affected checks, push, open/review/merge a PR into `feat/rust-rewrite`,
and cut a follow-up branch. Never touch master or force-push.

## Venue-specific audit

- Placement uses native `vol`; store sizes use `vol * contractSize`.
- Separate regular and trigger lifecycle enums and cancellation endpoints.
- WS terminal events can precede placement HTTP responses.
- Partial/failed snapshots cannot erase last-known-good state.
- Separate long/short leverage rails exist; the current frame supports one
  leverage per market, so retain both internally for placement/settings writes.
- TS public depth currently takes only 20 levels and treats increments as
  replacement snapshots. Track any deliberate correction in `KNOWN_TS_FIXES`.

## Evidence

- Read root/package/exchanges/MEXC guidance and both Rust plan documents.
- Read revised Process section from `origin/feat/rust-rewrite` after fetch.
- Official depth protocol:
  https://mexcdevelop.github.io/apidocs/contract_v1_en/#how-is-depth-information-maintained
  (uncompressed versions are contiguous; discard old updates and resync gaps).
- Public phase implemented and reviewed by `public_review`; all four findings
  fixed and re-reviewed clean: stale re-subscription snapshots, startup read
  retries, omitted candle limits, malformed streamed lists.
- Public verification: bans pass, crate clippy passes, 8 MEXC integration tests
  and 11 engine unit tests pass. Root lint: 28/28 tasks pass. Root typecheck
  and tests still running; final replay/performance gates remain phase 5.
- Engine adds opt-in `EnvelopeField` streaming to retain and validate the
  surrounding response without retaining the large list. Existing Binance
  streaming behavior is unchanged.
- Private-phase plan audit: keep native position IDs and separate long/short
  leverage internally; stream snapshot rows into temporary typed collections;
  compare snapshot-start revision with order/position/setting updates before
  commit; preserve last-known-good data on failed envelopes or pages.
- Merged landing layout at `d9e5d3f507`: venue is now `sdk-rs/venues/mexc`,
  engine `sdk-rs/engine`, host `sdk-rs/hosts/wasm`, crypto `sdk-rs/crypto`.
- Private phase implemented. Review `private_review` found and re-reviewed
  fixes for private rate-limit pauses, flat authenticated leverage metadata,
  canonical position IDs, zero equity, and a hedge-rail snapshot race.
- Private verification: crate clippy and all 19 Rust tests pass; crypto's 28
  unit tests pass. safe-cex typecheck/lint pass. Root typecheck: 44/44 tasks.
  Runtime-boundary test passes after moving the existing wasm exchange test
  from production code into audit-regressions (full root tests had exposed it).
- Removed unused first-port `crypto::wallet_utils`; a repository-wide symbol
  search found only its definitions and export, no callers.
- Active-symbol metadata now has a `FetchLeverage` command through engine,
  wasm and the Rust exchange shell; both internal leverage rails and the
  authenticated max cap are retained even when flat.
- Trading plan audit: preflight the entire placement batch before enqueue;
  use typed sequenced jobs and exact serialized bodies; never retry a write
  after transport failure; check per-row cancel results; retain stop routing
  and native position IDs; reconcile settings only after accepted mutations.
