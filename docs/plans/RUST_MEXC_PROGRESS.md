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
- Trading phase implemented and reviewed by `trading_review` through fixes
  for queued settings intent, per-side mutation revisions, delayed amend fills,
  closing stop leverage, terminal cancel races, and snapped optimistic volume.
  Regular amend now reads definitive order state after cancel and retries only
  inconclusive reads (bounded); it never replaces uncertain/filled quantities.
- Trading verification: 36 MEXC Rust tests + 11 engine tests pass; crate clippy,
  safe-cex typecheck/lint, 5 wasm host tests and 3 frame bridge tests pass.
  Bans pass. A first wasm clippy and release build passed; final gates must
  rebuild/rerun after subsequent changes.
- Added optional buy/sell SETTINGS columns, preserving older frame decoding
  and unchanged Binance payloads. Both native placement extras and hedge
  leverage survive worker/main frame application. Host command failures retain
  partial-placement metadata and release failed command parsing completions.
- Intentional TS corrections declared in `KNOWN_TS_FIXES.mexc`: full depth,
  closing stop rail, attached brackets, reject unsupported opening triggers,
  cancel all order surfaces, amend final quantity/Closed status, failed-read
  preservation. The TS reference still needs corresponding repairs in phase 5
  so gates compare independent implementations without hiding differences.
- Public live recording started (`mexc-smoke.jsonl`, BTCUSDT/ETHUSDT, 20s).
- Queries plan: typed JSON results in Binance's shapes; stream and page fills,
  orders and funding ledger rows; verify official response schemas before
  choosing pagination; reject unsupported income kinds explicitly. Queries
  must not mutate account state. Then build private tape, generic socket-role
  and request-body matching, fix TS reference bugs, and run every replay gate.
- Query phase implemented: streamed 100-row pages for fills/history, nested
  funding resultList streaming, 90-day disjoint windows, exact snowflake IDs,
  native/base quantities and Binance JSON timestamp conventions. Queries never
  write account state; errors reject the command instead of returning partial
  results. Public prints lack venue execution IDs, so deterministic print keys
  include the response ordinal to preserve otherwise identical executions.
- Income supports explicit FUNDING_FEE only: MEXC documents a funding ledger;
  generic/all-kind income requests reject rather than inventing accounting rows.
- Query independent review by trading_review found no concrete issue. All 43
  MEXC Rust tests and 12 engine tests pass, including 7 query regressions and
  chunk-boundary nested streaming tests. Crate clippy and bans pass.
- Merged latest landing branch through f036684d26; retained its relocated wasm
  test path and corrected relative imports in e2d8e7c43b.
- Phase 5 reference repairs are implemented and independently reviewed:
  full incremental depth and snapshot lifecycle; native brackets; unsupported
  conditional/trailing preflight; closing stop leverage; all cancel-all scopes;
  definitive amend reads; failed regular snapshots; monotonic cumulative fills,
  Closed precedence, and once-per-execution fill delivery. These are documented
  in KNOWN_TS_FIXES.mexc and corresponding TS regression tests.
- Adversarial phase review caught stale partial amend reads and completed-stop
  replacement. Both implementations now withhold unsafe replacements. Delayed
  Open fill frames after cancel acceptance retain higher cumulative fills while
  preserving the terminal status; lower cumulative rows cannot erase fills.
- Verification: 44 MEXC Rust tests, 12 engine tests, 91 focused TS MEXC tests,
  14 public depth regressions, package typecheck/lint, wasm clippy and bindings
  build pass. Root checks passed (44 typecheck, 28 lint, 27 test tasks; safe-cex
  6523 tests); final targeted tests cover the subsequent race fixes.
- Public recording succeeded: 20s BTCUSDT/ETHUSDT, 14706 WS frames. Harness
  review fixed same-URL socket ordinals, mutation response reuse and reconnect
  lifecycle ordering. Public virtual timers now follow recorded timestamps.
  Gates remain pending: the trimmer dropped resync responses after the third
  identical URL; preserve complete recorded input before the final comparison.
- Final raw snapshot audit fixed a TS normalization trap: validate the common
  envelope, version and every raw numeric level before normalization can drop
  invalid rows. Decimal strings are checked consistently. Rust requires both
  REST sides and refuses malformed/negative levels. Public regression coverage
  now includes malformed raw REST anchors.
- Final replay results (three public runs, one paced private run, no output
  exemptions): MEXC public 14701 frames / 1135 markets / 1135 tickers / 2 books,
  CPU TS 369.2 -> Rust 154.5ms, steady 231.7 -> 81.3ms, retained 5.2 -> 2.7MB;
  MEXC private 21 frames, PASS, zero unmatched requests. Binance public 873
  frames / 3 books, CPU 147.3 -> 56.8ms, steady 79.7 -> 18.7ms, retained
  2.3 -> 1.9MB; Binance private 14 frames, PASS. Private gates assert correctness.
- All venue/engine/Binance Rust tests and fmt/clippy/bans pass; wasm clippy,
  release bindings build, 8 tape tests, 5 host tests and 4 wasm exchange tests
  pass. Final TS-focused run passes 110 tests. Full root checks rerun after
  final edits before pushing. Browser build strips debug names only, retaining
  the existing code optimization and the unchanged 1258291-byte CI budget.
- Landing branch advanced through Bybit linear PR #1298 while final checks ran.
  Merged 20dd66dddb, preserving all three registrations, Bybit requestId/socket
  matching and CI gates. Consolidated MEXC streaming onto landing's generic
  ArrayAt::Field/Path+outside envelope API, retaining malformed-array, UTF-8 and
  bounded-envelope failure checks. Added MarketSetting defaults to Bybit's
  existing initializer. All affected replays must run again after this merge.
- The updated Process explicitly requires green PR CI before merging; wait for
  those checks after local adversarial PR reviews. The landing branch's wasm
  budget is now 1468006 bytes and is retained unchanged.
