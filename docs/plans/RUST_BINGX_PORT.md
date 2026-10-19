# BingX perpetual Rust port

Branch: `feat/rust-bingx`. Spec: `RUST_REWRITE.md`; process: `VENUE_PORT_BRIEF.md`.

## Phases

1. Public data: borrowed wire records, streamed markets/ticker seeds, internal
   full-snapshot public feed, candles, socket watchdogs; workspace/wasm/harness
   registration. Verify native tests, clippy and public recording. Sub-agent audit.
2. Private data: sorted HMAC signing; independent listen-key creation, keepalive,
   expiry and recovery; private watchdog; mode, balance, orders, positions and
   settings with generation guards and polling. Verify scripted recovery and
   stale-snapshot tests. Sub-agent audit.
3. Trading: preflight entire batches; sequential jobs; ordinary/conditional/
   bracket/trailing orders, stable-ID quantity amend and safe price replacement,
   cancellations and account settings. Verify writes are never retried, caller
   IDs fail closed, and failures preserve state. Sub-agent audit.
4. Queries: typed fills/history/income/trades in shared JSON shapes; candles;
   pagination and documented response variants. Verify deterministic fixtures.
   Sub-agent audit.
5. Gates: documented private tape and recorded two-book public fixture; generic
   harness extensions; BingX and Binance parity/performance; workspace checks.
   Sub-agent audit, fix, commit final evidence. Merge latest integration branch,
   reverify affected gates, push PR to `feat/rust-rewrite`, adversarial reviews
   posted on PR, fixes, merge PR and create follow-up branch.

## Plan audit

- Public TS uses an internal `qq-os.com` protocol, while private uses documented
  BingX streams. Keep the public protocol for replay parity and disclose host
  dependency. Hosts must inflate compressed binary websocket frames.
- Listen-key HTTP requests are unsigned on production and have their own single
  owner recovery loop; disconnects must invalidate old responses/timers.
- REST errors never mean an empty successful snapshot. In-flight snapshots must
  not resurrect terminal orders or overwrite newer private position events.
- TS emits terminal cumulative fills and drops dedupe state; Rust must emit each
  execution once, including partial fills, with a declared known TS fix.
- Caller IDs are lowercase, 1–40 chars, on one ordinary native order only.
- No credentials or live private writes are needed for deterministic gates.

## Progress

- Read spec, brief, root and TS package/exchange guidance; inspected engine,
  Binance reference and TS BingX behavior. Fetched integration process update.
- Official payload references: BingX-API/api-ai-skills `swap-market`,
  `swap-account`, `swap-trade`, `swap-ws-account` API references.

### Public checkpoint

- Public data implementation registered in engine/harness/SDK. Six public
  regressions pass; engine's 11 tests pass. Public clippy passed before audit
  follow-ups; final rerun pending. Binary host decoding tests pass (2); full
  host suite awaits wasm build.
- Independent reviewer found malformed depth defaults, Unicode interval panic,
  missing startup retries, handshake timeout and envelope truncation issues.
  Fixed all, including keeping heartbeat sends off connecting sockets.
- In-progress private/trading/query modules are separate work, not yet wired.
- Overseer requested integration of the nested workspace immediately; relocating
  the crate before resuming account/trading/query verification.

### Nested workspace

- Merged `origin/feat/rust-rewrite` restructure into this lane (`8aa90f0406`).
  BingX now lives at `sdk-rs/venues/bingx`; workspace membership is automatic.
- Independent crypto audit found `wallet_utils` has no callers beyond its own
  definitions/export; `ethereum::is_address` only has predicate-specific tests.
  Removed those first-port facades; kept address validation and signing
  primitives with live callers. Updated licence notice paths after nesting.

### Private checkpoint

- Account module integrated: unsigned production listen keys, independent
  keepalive/expiry/recovery, socket watchdog, REST position mode and account
  reconciliation, settings, and stale-read guards. Both open-order envelopes
  stream row by row via the engine's new nested-array selector.
- Independent review found historical fill replay, liquidation drops, unbounded
  dedupe and placement-ACK suppression. Fixed with REST-only historical fill
  baselines, per-execution quantities, bounded terminal tombstones, and
  ACK-before-WS regressions. 18 account integration tests and 2 unit tests pass;
  public regressions (6) also pass. Account clippy passes with warnings denied.
- Live public tape recorded for 20s, two books. First gate exposed TS precision
  zero mapped incorrectly to 0.1, and excessive Rust ticker allocation. Rust
  uses native integer precision and borrowed streamed ticker rows. Known TS
  fixes declared in gate: precision zero, terminal cumulative/repeated fills,
  and private close-position `cp` alias. Public gate recheck pending rebuild.

### Trading checkpoint

- Sequenced preflighted placement/brackets/conditionals/trailing, cancellation,
  stable-ID quantity amendment, atomic LIMIT price replacement and settings
  commands integrated. Account observations preserve native status for safe
  `ONLY_PENDING` replacements. Writes invalidate in-flight account snapshots.
- Independent review found missing full-stop quantity, stale replacement ACK
  overwrites, contradictory/rejected ACK acceptance, oversized bracket legs and
  PENDING restrictions. All corrected with dedicated regressions; 26 trading
  tests pass. Public 6 + account 18 + account unit 2 remain green.
- Verified current official docs bundle explicitly requires full-stop quantity;
  updated nearby guidance and gate declarations. Live public feed also sends
  descending asks and '-' placeholders; Rust sorts snapshots and keeps ticker
  fields finite. Repeat public gate pending final wasm build.

### Queries checkpoint

- Typed fills, order history, income and public trade reads integrated, with
  streamed arrays and JSON command results matching the shared shapes.
- Independent review found equal-time truncation, missing native fill paging,
  empty identifier collapse and oversized ISO-year arithmetic. Fixed with
  pageIndex walks, explicit incomplete errors and validated IDs/timestamps.
- 18 query tests pass; independent repeat audit has no remaining query blocker.
  Crate clippy --all-targets -D warnings passes. Full repository checks and
  post-optimization replay gates running; no public performance claim yet.

### Gate checkpoint (work in progress)

- BingX documented private gate PASS: 37 frames, 2 markets, 2 tickers; TS/Rust
  main CPU 64.1/46.0ms, retained memory 0.9/0.8MB (correctness-only private gate).
- Public output matches after declared fixes. Latest completed 3-run gate:
  CPU112.6/96.6ms, steady53.4/47.9ms; memory1.8/1.9MB. Memory still FAILS;
  reducing startup table reservations/seed allocations, no ready claim.
- Harness review fixed Rust-oracle precision normalization, wire-type masking,
  reusable write responses, future staged snapshots and unsafe numeric WS IDs.
  Four request-matcher regression tests pass. Added staged account mutation to
  the private tape; final gate rerun pending latest build.
- Streamed all fill-history envelope variants; 20 history tests pass. Shared
  splitter alias tests and table reservations preserve the frame layout.
- Root typecheck44/44 and lint28/28 pass. Full tests found existing misplaced
  wasm tests violating exchange runtime boundary; moved them to audit tests.
  Boundary regression +4 wasm tests pass; full root test rerun underway.

### Final race and memory corrections

- Independent trading re-review is clean after strict shared ACK validation,
  applying placement ACK only after validation, preserving private partials and
  terminal tombstones, and reconciling Amend/Replace immediately. All28 trading
  tests +3 account unit race tests pass; targeted clippy passes.
- Added independent sorted HMAC/POST-type vector and unsigned listen-key tests
  (2 pass). Extended tape with a numeric execution ID beyond JS safe integers.
- Reduced shared index key size and frame interner metadata. Interner retains
  full-string collision checks; forced-collision test passes (engine14 tests).
  Rebuild and all four public/private venue gates underway.

### Gate allocation and recovery audit

- Wasm32 frame interner now uses compact u32 hashes with full string equality
  and wraparound collision probing; engine14 tests pass. String cache allocates
  only on first decode. HTTP streaming bounds both transport body forms to16KiB.
- Public prices/sizes decode through typed visitors, avoiding untagged-enum
  intermediates; books sort native levels in place. Numeric placeholder tests
  cover strict versus optional fields. BingX78 tests (4unit,18account,20history,
  6public,2sign,28trading) and clippy pass. Host5 and harness6 tests pass.
- Independent final-phase review found subscription parity missed reconnects.
  Fixed by comparing native topic/operation per connection generation; repeat
  review is clear, including interner collisions and decoder behavior.
- Gates rerunning with final build. Latest pre-visitor public run passed memory
  but missed steady CPU; no all-gates claim. Root test rerun still in progress,
  with two unrelated5s Hyro dynamic-import timeouts under shared machine load.
