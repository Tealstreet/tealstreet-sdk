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
