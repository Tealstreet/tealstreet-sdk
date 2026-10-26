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

### Public gate green

- Replaced ticker symbol-key copies with a sorted u32 row index, preserving
  row/frame order and last-duplicate wins. Identity immutability is documented
  and debug-asserted. Independent implementation review has no findings.
- Engine15, BingX78 and Binance tests pass. Public3-run gate PASS:188frames,
  1268markets,1260tickers,2books; TS/Rust CPU76.9/60.4ms, steady29.4/24.9ms,
  retained memory1.8/1.8MB (ratio1.03; wasm1.4MB). Private/Binance gates running.
- Root full suite PASS27/27 tasks with --maxWorkers=2 --testTimeout=30000;
  both safe-cex test tasks pass6489/6489 tests. No test timeout source edits.

### All lane gates green (before landing sync)

- BingX public PASS3runs:188frames,1268markets,1260tickers,2books;
  TS/Rust CPU76.9/60.4ms, steady29.4/24.9ms, memory ratio1.03 (wasm1.4MB).
- BingX private PASS1run:38frames,2markets,2tickers; CPU64.0/43.0ms,
  steady23.1/17.8ms, retained1.0/1.1MB (correctness-only).
- Binance public PASS3runs:871frames,917markets,783tickers,3books;
  CPU208.0/102.6ms, steady122.2/42.8ms, retained2.3/1.9MB.
- Binance private PASS1run:14frames; CPU61.8/40.6ms, retained0.9/1.1MB
  (correctness-only). Bans, fmt, venue/shared-engine clippy and wasm clippy pass.
- Syncing latest landing branch next; preserve Bybit Path/envelope pagination,
  recheck combined code, then push/PR/adversarial review/green CI/merge.

### Landing sync

- Merged current landing structure and Bybit support; resolved crate registration,
  retained the landing test destination and request-ID matching. Shared response
  splitter preserves generic Path cursors and BingX envelope aliases, with strict
  UTF8 and16KiB overflow failure. Engine21 tests and all three venue suites pass.
- Typecheck44/44 and lint28/28 pass after sync. Harness7 tests pass, including
  Bybit binary bodies/connection IDs and native subscription operations.
- Added BingX public/private gates to CI. Combined wasm measured1,849,367bytes;
  explicit combined budget2MiB. All venue gates rerun after final hot-path tuning;
  first merged replay passed output/memory but missed steady CPU, so still WIP.

### Post-sync precision hot path

- Decimal touch prices recover common native power-of-ten scales numerically,
  preserving the original formatting fallback above15digits. A18digit regression
  prevented an unsafe reciprocal-only change; precision0..18 vectors pass.
  Independent review confirms both callers use native power-of-ten tick sizes.
- BingX79 tests pass. Merged public3-run gate PASS: CPU77.5/61.8ms,
  steady29.4/24.5ms, memory ratio1.03. Combined wasm1,849,491bytes (<2MiB).
  Other merged gates and a Node22 CI-toolchain check are next.

### Shared subscription gate exception

- Exact outbound parity exposed landed Bybit's intentional extra execution and
  wallet subscriptions (TS uses cumulative orders and REST wallet reads).
  Declared the native stream improvement in KNOWN_TS_FIXES and Bybit notes.
  Expected extra topics derive only from each TS private connection generation;
  missing Rust execution/wallet or any other mismatch still fails. Independent
  gate-integrity review has no blocker. Bybit private18-frame Node22 gate PASS.
- All merged Node24 venue gates PASS after this declaration: BingX188/38frames,
  Binance871/14frames, Bybit8742/18frames. Workspace clippy --all-targets passes.

### CI runtime replay proof

- Node22.23.3 BingX public3-run gate PASS:188frames,1268markets,1260tickers,
  2books; TS/Rust CPU128.086/93.359ms, steady65.575/45.326ms, median retained
  1,808,648/1,701,880bytes. All required gate numbers now pass post-sync.
- Wasm bridge verification exposed an old local workspace symlink; refreshing
  Yarn fixes its target. Final API audit also found inherited BingX client-ID
  helpers lacked the native rule. Adding the rule and real-wasm contract tests
  before opening the PR; native trading semantics and replay remain green.

### SDK bridge gate green

- Registered BingX's lowercase client-ID rule and attributed minting in the
  Rust SDK bridge; inherited helpers and whole-batch validation now work.
  Honorable shapes also exclude close:true, matching native preflight.
- Independent review found that inherited close mint mismatch; fixed and
  re-reviewed clear. Real wasm tests8/8 PASS on Node22, covering exact wire IDs,
  streamed lookup and no-write rejection for invalid/bracket/close/mixed batches.
  Package typecheck and production-file ESLint pass. Stale local Yarn links were
  refreshed with frozen-lockfile install; no dependency manifest/lock changes.
- All five implementation phases and gates complete. Opening the PR into
  feat/rust-rewrite, then adversarial local reviews and green CI before merge.

### PR1305 adversarial review and concurrent OKX landing

- PR1305 opened against feat/rust-rewrite. OKX landed concurrently; resolving
  its base conflicts while preserving its argument/auth/framing validations,
  every-run correctness checks, passphrases and SDK combined-amend forwarding.
- Adversarial findings posted as PR comments: two trigger-source defects,
  a combined-amend bridge defect (fixed by preserved landing behavior), mixed
  cumulative/no-z execution duplication and stale normalized no-z fill totals.
- Trigger fixes add five regressions (33trading tests pass); account fixes and
  independent repeat reviews are underway. Added real-wasm BingX combined-amend
  no-write regression. Shared engine merge preserves both suites:240 tests pass
  across engine25/Binance29/BingX79/Bybit43/OKX64 before new review fixes.
- Replay helper9 tests pass after unifying command/time staging, typed body
  matching and generated OKX IDs. Rechecking four-venue gates and CI next.

### Shared adversarial findings addressed

- Posted shared review findings on PR1305. Restored merged gate tape loading,
  serialized outbound subscriptions with presence guards, and delivered taped
  closes through TS fake sockets. Enabled BingX leftover-write checks (listen
  lifecycle requests stay repeatable reads). Independent helper/host12 tests
  pass; full replay rerun follows. Every run retains upstream correctness checks.

### Dense public ticker cache

- Retained the seed's market-to-ticker index after premium filtering. Live rows
  now use one native-ID lookup and indexed Store mutation, preserving ordinary
  symbol mutation and identical delta masks. Indexed mutation is range checked;
  identity snapshots allocate only in debug builds.
- Independent plan and implementation audits are clear. Regressions cover early
  frames, reordered daily rows, filtered leading rows, changed reconnect data,
  invalid row indexes and mask equivalence. Engine26/public7 tests pass.
- Four-venue Node22 public gate PASS:188frames,1268markets,1260tickers,2books;
  CPU80.5/58.9ms, steady30.9/23.3ms, retained1.8/1.7MB, wasm1.4MB.

### Trigger review fixes

- Replacements validate the reconciled mark/last trigger source and its finite
  positive reference before cancellation; unknown/index sources fail closed.
  Attached protective legs preserve explicit per-leg/generic trigger sources.
  Unspecified attached SL/TP keep the existing MARK_PRICE venue default.
- These also fix TS always-mark replacement checks and its attached workingType
  omission, declared in KNOWN_TS_FIXES. Five new trading regressions pass
  (33total); standalone take-profit defaults were not conflated with brackets.

### Execution review fixes

- Unified native execution IDs across cumulative/no-z events; positive cumulative
  proof still recognizes distinct fills sharing timestamp/price/size. Normalized
  order quantities remain monotonic, including no-z partials and full terminal q.
- Fresh identifiable late executions can arrive after a no-z terminal event
  without reopening the order. Bounded covered-history/baseline guards still
  suppress historical replay; placement ACK quantities do not hide first fills.
- Independent repeat account audit is clear. Full BingX92 tests and clippy pass:
  5unit,25account,20history,7public,2sign,33trading. Final four-venue gates next.

### Adversarial repeat clear

- Independent repeat reviews of private execution and protective trading fixes
  found no remaining issue in the covered contracts. Final bridge review caught
  a lost BingX registry entry from the OKX merge; restored it. The inherited
  combined price/amount forwarding remains intact with a no-write regression.
- Pushing review fixes now for PR CI. Final rebuilt four-venue replay/SDK gates
  and repository checks run before merge; no CI-green/landing claim yet.


### Concurrent Crypto.com landing integration

- Merged the new landing base while retaining nested venue paths and all five
  registries. Removed duplicate harness script key introduced by the auto-merge.
  Independent SDK/Cargo/engine audits found no remaining registry regression.
- Preserved landing crypto dead-code cleanup, signed Crypto.com protocol checks,
  real TS worker egress, and isolated retained-memory measurement. Combined these
  with typed request matching, staged BingX snapshots, reconnect delivery and
  subscription/write coverage. Independent shared harness review and rebuilt
  five-venue verification run before pushing this integration.

- Shared review found a lost BingX write classification in the new response
  helper. Restored lifecycle-aware once-only mutations and leftover enforcement;
  independent repeat audit is clear. All22 harness tests pass. Rebuilt aggregate
  wasm2,114,189bytes (CI allowance2.125MiB), SDK18/host5 tests pass, workspace
  clippy passes. Full native tests and fresh replay gates remain in progress.

### Stable replay memory baseline

- New landing checks exposed112bytes of late TS loader buffer release during
  private replay. Import tracing confirms cleanup can continue across15/33/57ms;
  immediate-turn equality was insufficient. Both implementations/endpoints now
  require100ms unchanged backing-buffer counts with1s fail-closed deadline.
  Exact subtraction and negative-component checks remain; CPU is sampled before
  final settling. Three injected-timing checks and independent repeat review pass.
- BingX private38frames passes after this correction. Final five-venue public
  gates so far pass for BingX/Binance/Bybit; CI exposed asynchronous Bybit
  subscription batches finishing after accelerated replay, so adding explicit
  subscription readiness before delivering its frames. No subscription mismatch
  is normalized away. Native326tests and wasm clippy pass; SDK18/host5 are green.

### CI replay startup and gate scope

- Fast CI finished Bybit replay before its10ms subscription batches drained.
  Both implementations now wait for all seed-derived outgoing ticker/book topics
  under the existing15s deadline. Exact per-generation comparison stays intact.
  Expected-topic Map storage survives both memory samples. Independent helpers23
  pass; Bybit8742frame public gate passes with914matching subscriptions.
- Per spec, public fixtures alone gate performance. Private paced tapes still
  enforce all state/request/write/protocol checks; their raw CPU/memory figures
  are informational (long-lived code GC can make tiny-state heap deltas negative).
  Classification derives from tape sockets identically to replay; missing/extra
  account output fails parity. Public negative-memory/performance checks unchanged.
- Root validation passes:44typecheck tasks,28lint tasks,27test tasks, including
  both6494-test safe-cex runs. Native workspace326tests, all-target clippy,
  wasmclippy, bans and fmt pass. Latest public BingX gate:CPU95.1/60.4ms TS/Rust,
  steady36.1/22.9ms, retained3.50/1.97MB; Binance871,Bybit8742,Crypto.com2177,
  OKX8902-frame public gates pass. Final private variants/CI continue before merge.

### Concurrent WEEX landing integration

- Synced latest landing6cd219ce4c after WEEX landed during final PR CI startup.
  Retained its header-authenticated socket/HTTP transport, disposal behavior,
  ticker-symbol command, generated acknowledgement templates and effectiveAt
  snapshots alongside BingX's decompression,16KiB chunks, typed/staged matching,
  reconnect delivery, paced-subscription barrier and memory measurement rules.
- Independent host and harness merge reviews are clear; header/gzip/deflate probe
  and10host tests pass. Engine now follows landing's bounded1MiB emptied metadata
  envelopes for every selector; preserves BingX aliases, UTF8/overflow safeguards,
  compact interner and indexed ticker API. Engine33+BingX92+WEEX76tests pass,
  as do workspace clippy and refreshed SDK typecheck. Full rebuilt six-venue
  gates/aggregate size and final-head CI remain required before landing.

- Six-venue release artifact measures2,576,287bytes under the2.5MiB allowance.
  Full workspace408tests, native/wasm clippy, bans and fmt pass; rebuilt SDK19,
  host10 and harness23tests pass. Refreshed package typecheck/lint pass.
  Combined public/private replays now run on this artifact before landing.

### Port-review lows plan (2026-10-21)

1. Reverify hedge leverage/bracket working-type findings and preserve existing
   correct behavior. Keep risky amend, trigger-source and financial-history
   contract changes documented rather than guessing.
2. Add explicit per-symbol leverage/margin metadata reads with strict native
   decoding, independent request correlation and fencing against newer writes.
   Keep the shared capability contract and caller metadata flow aligned.
3. Isolate malformed ACCOUNT_UPDATE position rows, retain valid peers, emit a
   coded row-skipped warning and schedule authoritative REST repair. Preserve
   strict order/fill/config handling where identity cannot be proven.
4. Add regressions reproducing current failures, run focused BingX/Binance tests,
   fmt/strict Clippy/bans and relevant host/TS checks with CARGO_BUILD_JOBS=4.
   Independently review, fix and push for CI; merge only after overseer hold lifts.

- Reproduced both failures before implementation: flat metadata produced no native
  request and a malformed WS position row blocked its valid peer. Metadata now
  signs the documented GET leverage/marginType endpoints, validates positive
  native rails and exact mode, and stages both for a current atomic commit.
  Dispatch/ACK/config/authoritative settings snapshots and newer same-symbol
  queries prevent stale publication. Superseded reads and reads issued during a
  pending write return current cached rails, with validated read values used
  independently for missing rails; they do not fail merely because of overlap.
  Empty position snapshots do not invalidate flat metadata. A nonempty settings
  snapshot conservatively invalidates the read even if values are unchanged.
- Stream position arrays now retain raw rows until independent strict decoding;
  source precedence P/B/p stays intact, balance-only B records stay quiet, and
  malformed-only position packets still warn and schedule REST repair. Order,
  execution and config envelopes keep their strict contracts.
- Primary endpoint/field reference:
  https://github.com/BingX-API/api-ai-skills/blob/main/skills/swap-trade/api-reference.md
  (Query Margin Mode and Query Leverage and Available Position).
- Independent design/adversarial review found snapshot/pending-write races;
  separate settings revision, per-symbol query generations and native pending
  write tracking address them. Final read-only review found no blockers.
  Risky partial-fill cancel-replace, unknown trigger-source and strict financial
  history contracts are recorded rather than relaxed.
- Final focused validation passes: BingX143 + Binance246 native tests, 14
  BingX-enabled host tests, 13 TS capability/metadata tests, safe-cex typecheck,
  touched ESLint including ignored tests, fmt, strict all-target BingX Clippy
  and banned patterns. Additional regressions prove independent-symbol reads
  both succeed and empty position polls do not invalidate flat metadata.
- Integrated pinned34adfdb71a6 and its success-only command completion API.
  Metadata failures use Cx.fail with NotSent (no trading effect), retaining raw
  message classification; success returns plain JSON. Added one-failure/no-I/O
  preflight outcome coverage. Independent API-integration review clean;
  144BingX+246Binance,14host,13TS tests and typecheck/fmt/Clippy/bans pass.

- Integrated landing90875b0968c, including native account identities and the
  explicit fresh leverage-read contract from #1522. New combined query policy
  keeps explicit reads fresh while selected metadata updates fetch the same
  atomic leverage/margin pair. Other OnRead venues retain their policy. The
  original fresh-read regressions finish both stages when current. The race
  follow-up preserves newer cached rails in both store and response rather than
  failing; no stale or partial metadata is published.
