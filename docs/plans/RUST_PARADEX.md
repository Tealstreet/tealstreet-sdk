# Paradex perpetuals Rust port

Branch: `feat/rust-paradex`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and mandatory gates: `VENUE_PORT_BRIEF.md`.
Full automatic execution; no stash, rebase, force push, or master changes.

## Phases and review gates

1. Public data: typed streamed markets and ticker seeds; BBO/trade/summary
   sockets; on-demand snapshot books, reconnect/watchdog and subscription
   recovery; REST candle reads and trade/poll candle subscriptions.
2. Private data/auth: StarkNet signatures, configuration, single-flight JWT
   acquisition/refresh, private socket auth/subscriptions, account reconciliation,
   settings and stale-snapshot protection. Review signing/auth separately.
3. Trading: sequenced jobs with complete preflight, exact caller identities,
   native amendments, conditional/bracket placement, conservative cancel chunks,
   leverage and explicit cross/one-way mode constraints. No uncertain-write replay.
4. Queries: cursor-bounded fills/order history/income/public trades and client-ID
   lookup in the engine's established JSON shapes; no store mutation from reads.
5. Harness gates/integration: wasm and SDK registration, independent signing
   verification, documented private tape, recorded/trimmed public fixture, CI
   matrix and size budget. Re-run Binance public and private gates.

Every phase: audit plan, implement, run focused verification, independent
sub-agent bug review, fix, verify and commit. Keep progress here. After all
mandatory gates pass: merge the latest landing branch, push, PR into
`feat/rust-rewrite`, post adversarial reviews, fix/repeat, wait for green CI,
merge and create a fresh follow-up branch.

## Signing/JWT design and audit checklist

- Real prod/testnet URLs only inside the venue. Reuse host transport routing:
  `createEngineTransport`/serialized native routing plan. Explicit public/private
  socket roles even though the upstream URL is shared. Paradex metadata enables saved-account tunnel defaults; the current routing
  table declares no Paradex WS override and resolves its real socket hosts.
  Preserve that policy; do not invent a socket proxy in Rust.
- Private mode uses key + main Paradex address + subkey public address, never
  secret. Public-only mode must perform no signing/account I/O.
- Fetch system configuration for the environment. Validate chain ID, credentials
  and felt bounds before auth. Reuse audited `safe-cex-crypto::starknet` primitives;
  typed domain/message structs or direct typed hashes, never a dynamic JSON tree.
- Match revision-0 SNIP-12 domain ordering: name, chainId, version. Authentication
  Request fields: method, path, body, timestamp, expiration. Sign POST `/v1/auth`
  with empty body, epoch seconds, expiration +86400 seconds; send to
  `/v1/auth/{subkeyAddress}` with account/signature/timestamp/expiration headers.
  Signature is a JSON string containing decimal r/s strings.
- Orders use epoch milliseconds; integer size/price quantum = truncate decimal
  string *1e8, without float multiplication. Order fields: timestamp, market,
  side(1/2), orderType, size, price. Native ModifyOrder appends the stable id;
  amendments cannot reuse placement signatures. Market signatures use price 0.
- Preserve the TS subkey path and audit Ethereum-derived account support against
  the SDK (key-length ambiguity must be documented rather than silently guessed).
  Remote signing must use the engine's host signing contract if supported; an
  unsupported mode must fail before any account/write I/O, never interpret a
  public key as a private key.
- JWT cache refreshes on demand after 180s (300s validity minus 120s buffer).
  Queue typed continuations behind one in-flight auth. Failed/malformed refresh
  fails pending operations without losing last-good account state. Bound queues,
  recover sockets with fresh auth, and never resend an ambiguous write. Validate
  the correlated WS auth response before subscribing; timeout and reconnect on
  missing auth/subscription acknowledgments. No credential/signature/JWT logs.
- REST failures and HTTP200 business errors never become empty account snapshots.
  REST starts capture revision counters; private events and accepted writes fence
  old snapshots. Emit one fill per execution, not cumulative order fill amounts.
- Independent oracles: current Paradex official docs, upstream paradex-py,
  vendored official JS SDK, existing StarkNet JS fixtures; deterministic synthetic
  keys only. Private tape validates real emitted signatures and outbound requests.

## Progress

- Initial checkout clean; fetched landing and created fresh requested branch.
- Read port brief/spec and Paradex runtime guidance; initial auth and harness
  plan audits running in independent sub-agents.
- SDK/wasm registration and credential/routing integration implemented. Focused
  integration verification: 199 TS tests, package typecheck/lint, wasm single-feature
  check PASS. These are provisional until independent phase audit completes.
- Public phase independent audit found malformed subscription acknowledgments,
  stale book snapshots and trade-versus-poll candle races; fixes in progress.
- Official Paradex asyncapi confirms snapshot feed depth15 only; full depth uses
  `order_book.{symbol}.deltas` with initial snapshot then deltas. Rust and TS
  reference will both use the full-depth protocol; declare the TS depth correction.
- StarkNet generic TypedData parser rejects the venue's legacy domain field
  ordering. Fixed-structure SNIP-12 hashing is being checked against independent
  StarkNet JS and upstream SDK vectors. No old dead request-signing facade ported.
- Trading and query continuations drafted separately, not wired into the public
  phase commit yet. No mandatory replay/performance gates claimed yet.

### First implementation checkpoint

- Corrected the TS reference full-depth book reducer and Paradex factory routing.
  22 depth/followup regression tests, 199 wrapper/routing tests, package typecheck
  and scoped lint pass. Public Rust review found/fixed ACK and stale-state bugs;
  final review is pending the binary-wire extension.
- Live probes now prove every prod/testnet JSON handshake fails with code40301,
  `SBE binary encoding required`. Explicit SBE versions1/2/3 negotiate; use v2,
  which has a published official XML schema. This is a real venue protocol change.
  Extend the engine with optional borrowed binary ingress and native/JS host
  forwarding; decode SBE in the typed venue state machine. The TS oracle gets an
  independent schema decoder. Record raw binary traffic, not fabricated JSON.
- Private/auth, trading and query drafts compile together; four independent
  signing corpus tests and the remaining Stark signing primitive test pass.
- Removed unused first-port generic Stark typed-data/felt parsing helpers and
  their obsolete fixture; crypto now depends only on starknet-crypto for these
  primitives. Venue-specific fixed hashing preserves Paradex domain field order.

### Reviewed data/auth checkpoint

- Phase1 public and phase2 private/signing independent reviews PASS after fixes.
  Native verification: 23 public +16 account +4 signing tests, plus6 unit tests
  (SBE/price-band/tradingcorrelation). Phase3 trading currently12 tests pass;
  phase4 queries7 tests pass; their independent reviews are still underway.
- SBE schema1:2 is decoded directly into typed Rust state; JS/native hosts expose
  explicit binary mode and borrowed ingress. The JS host suite passed36 tests;
  the review also caught an older-engine physicalsocket leak and native fallback
  health omission, nowfixed with additional regressions awaiting final checks.
- Independent TS codec uses official Python-generated vectors across8 templates;
  public/private null, enum, root/group/string bounds and rapidcrosssymbol updates
  reviewed. Paired TS corrections retain full depth, every market's BBO/summary,
  candle generation fences and final CLOSED statuses.
- Privateauth preserves environmentconfiguration, exact SNIP12 domains and
  deterministic decimalquantums. JWT queue singleflights and fails pending typed
  operations explicitly; missingauth/subscription acks recover; serverstamps and
  terminalposition/order fences block old REST/event resurrection. One fill emits
  per executionID, retaining distinct economicallyidentical partials.
- Shared maxSlippagePct input now survives deserialization. Paradex cappedmarket
  uses a freshdepth5 read then constrained signed LIMIT/IOC; other venue builders
  receive only a mechanical None default. No other venue behavior changed.
- Proxy JWT formerly overwrote Paradex's venue Authorization when proxied. The
  shared TS policy now uses Tealstreet-Authorization for Paradex, preserving both
  credentials identically for the native resolvedplan and JS host.
- No replay, performance, finalCI or PR/merge gate has been claimed yet. Harness
  registration and live20second publicrecording are underway.

### Trading/query phase completion

- Phase3 independent review PASS after invalidating/locking native amendment
  continuations and fencing reads again at write completion; empty/unknown
  cancel-all markets reject before I/O. All quote reads, shapes, IDs and signatures
  preflight before the first write. Fresh capped-market quotes pin fair price,
  apply venue bands and side-aware tick rounding; LIMIT chunks use1..10 orders.
  Fourteen trading regressions pass; fixed-mode commands return the established
  empty-array completion, with unsupported isolated/hedged operations rejecting.
- Phase4 two independent reviews PASS. Open client-ID pages are exhausted before
  closed history; only complete coverage makes a miss definitive. Cursor loops,
  incomplete capped lookups, malformed financial fields, invalid ranges and unknown
  symbols have explicit outcomes. Fills retain execution size and identity, walk
  empty cursor pages, stop at the requested bound and use the bounded TS order
  fallback only after primary coverage. Thirteen query regressions pass.
- Private phase now17 tests, including canonical TIF/client identities and
  filtering unregistered option/spot rows before typed perp decoding. Balance
  remains venue aggregate; rare ledger reads retain delisted/raw market IDs.
- The first live record exposed two startup bugs, repaired and independently
  reviewed: irrelevant option summaries with empty numerics poisoned the perp
  seed; early book/candle subscriptions were discarded before markets arrived.
  Twenty-five public tests now pass, including queued reference counts/cancel.
- First scripted private replay verifies real signatures, all writes and every
  query. Remaining account formatting/parity differences are being paired with
  TS fixes (canonical TIF, preserved client IDs, balance uPnL and fixed modes),
  never normalized away. Harness170 tests pass, including17 Paradex protocol
  and signing negatives; public recording/performance gate is in progress.
- This is the spec's phase8 opt-in engine lane. Production realm loader adoption
  remains phase6 and default TS retirement remains phase9; no default-runtime
  cutover or generated-WASM publishing is claimed. The TS implementation remains
  the independently executing oracle alongside the other landing-branch venues.

### Harness audit checkpoint

- Independent phase5 audit found replay replaced recorded control errors with
  synthetic success ACKs. Recorded errors now fail closed, successful ACKs bind
  to actual request IDs, and shared-URL open/close/reopen controls follow the
  subscribed role rather than its connection ordinal. Harness172 tests pass.
- The stricter check exposed a genuine live protocol defect in both runtimes:
  legacy JSON ping messages receive invalid-request errors. Official protocol
  control pings are answered by hosts; both venues now retain watchdogs without
  sending unsupported application pings. A fresh20second recording is required;
  the first tape is rejected, never trimmed to hide the error.
- First single-venue WASM size was1,490,030 bytes, above the unchanged1,258,291
  budget. Fixed EIP712 hashing replaces dynamic ABI parsing and narrowly scoped
  Pedersen/Cairo encodings replace the generic StarkNet SDK dependency. Four
  existing signature tests plus two independently vectored formula tests pass;
  reduced artifact size is awaiting measurement and independent review.
- TS replay preloads only the SDK code before constructor/baseline measurement,
  matching the existing Rust module-load boundary. Account/signing I/O remains
  measured; the memory algorithm and thresholds are unchanged.
- Repository typecheck44 tasks and lint28 tasks pass. First full test run passed
  6,913 safe-cex cases but timed out on two unrelated HyroTrader lazy imports;
  rerun with serialized tasks is in progress. The full JS host suite needs the
  Binance-enabled artifact; the Paradex-only build cannot run its Binance test.
- No public/private replay, performance, artifact-size, finalCI or merge gate is
  claimed at this checkpoint.

### First passing replay checkpoint

- Merged landing WOO/Bitget parity, preserving both venue registrations and the
  binary host path. Removed duplicate shared slippage fields introduced by the
  automatic merge. Full workspace formatting is clean; final Clippy found a WOO
  test's exhaustive socket pattern, now repaired for the new optional field.
- The fresh 20-second public tape contains 12,261 replayed frames, 66 markets and
  tickers and two full-depth books (BTC 113/71 bid/ask levels, ETH 107/34). Three-run
  public gate PASS: main-thread CPU TS 428.6ms / Rust 410.6ms; steady CPU
  191.0ms / 125.5ms; retained memory 1,547,816 / 1,281,104 bytes.
- Private gate PASS: 15 commands and 32 replayed frames, with independent exact
  signatures, write ordering, caller identities and account/query comparisons.
  Informational CPU TS 477.0ms / Rust 330.9ms; retained memory 10.0MB / 7.0MB.
  Configured leverage is read from the real TS account cache; effective position
  leverage remains separate and is still compared exactly.
- Artifact size PASS: 1,214,403 bytes against the unchanged 1,258,291-byte budget.
  Removed unused production dynamic EIP712 helpers/dependencies, kept independent
  ABI oracles in test dependencies, and used the existing per-venue size profile.
  Independent signing/formula and harness reviews pass; harness179 tests pass.
- Final native venue run passes 85 tests: 8 unit, 18 account, 13 history, 28 public,
  4 signing and 14 trading. The last public audit extended identity filtering to
  markets before financial decode; irrelevant malformed options cannot poison
  the perpetual seed, while selected malformed perps still reject atomically.
  This final public source change requires the scheduled artifact/replay rerun.
- Binance regressions, combined-host tests, final workspace checks and CI/PR
  reviews remain in progress. All 6,917 safe-cex tests passed with one test worker;
  repository checks are being repeated after merging the landing branch.

### Final local gates (landing merged through Lighter #1319)

All mandatory brief gates PASS on the final venue source:

| Gate | Result |
| --- | --- |
| Bans; workspace format/Clippy; WASM Clippy | PASS |
| Paradex native tests | 85 PASS |
| Merged replay harness | 195 PASS |
| Combined WASM build; host tests | PASS; 38 PASS |
| Single Paradex WASM | 1,216,038 bytes; budget 1,258,291 |
| Private replay | PASS: 15 commands, 32 frames, exact signatures/writes/account/queries |
| Public replay, three runs | PASS: 12,261 frames, 66 markets/tickers, two full-depth books |
| Public main-thread CPU, TS / Rust | 915.837ms / 711.566ms (1.29x) |
| Public steady CPU, TS / Rust | 511.9ms / 374.4ms (1.37x) |
| Public retained memory, TS / Rust | 1,565,800 / 1,304,200 bytes (1.20x) |
| Binance public replay | PASS: 873 frames, three books; CPU 238.6ms / 121.0ms; retained 4.2MB / 2.6MB |
| Binance private replay | PASS |

The final private run's performance is informational: CPU TS 568.4ms / Rust
360.4ms, retained memory 6.7MB / 7.1MB. Private replay gates correctness, while
the public replay gates both CPU and memory using unchanged thresholds.
Configured account leverage is compared independently from effective position
leverage; neither runtime's output is rewritten for Paradex.

Landing integration preserves both Lighter and Paradex registrations, normalization
hooks and host roles. Five focused wrapper/routing files pass55 tests after that
merge. PR adversarial reviews and green CI are the remaining landing steps.

PR: https://github.com/Tealstreet/tealstreet-sdk/pull/1320, targeting
`feat/rust-rewrite`. Two adversarial reviewers are examining the actual PR diff;
findings will be posted as PR comments and addressed before merging on green CI.

### Adversarial PR review, first round

- Posted both independent reports on PR #1320. One confirmed P1 stale-order
  repair and three P2 history/book/candle findings were reproduced. Repairs retain
  per-order timestamp/exact-fill watermarks and valid amendment data; walk empty
  cursor pages in Rust and TS; apply all JSON book rows; and prevent candle polls
  from rewinding buckets or known volume in either runtime.
- A review of the repair rejected permanent terminal inference from REST absence:
  a lagging replica can omit a live order. Actual terminal-event tombstones remain;
  later valid REST evidence can restore an unchanged order after an omission.
  Duplicate order IDs invalidate an entire snapshot before state mutation.
- Regression proof: native and TS empty-page tests fail before the change and
  pass after; the independent stale-order reproduction now passes. Latest suites:
  23 account, 15 history, 32 public, 14 trading, 8 unit and 4 signing tests.
  Final independent re-review reports PASS with no outstanding blocker.
- CI's secret scan flagged two deterministic derived keys in the synthetic signing
  corpus. The exception names only that exact fixture file and documents its public
  repeated-0x11 seed; the complete new-commit scan now passes without disabling any
  detector. CI also exposed missing single-feature WOO/Lighter credential cfgs
  after merging those ports; the shared options now include both features.
- Repository full test rerun passes all27 tasks, in addition to typecheck/lint.
  Fresh artifact/replay gates and green CI are required after these review fixes.

### Final amendment review and Orderly integration

- A fresh adversarial reproduction found an additional P1: a repair GET started
  after an amendment's id-only ACK could restore the old price at the same venue
  timestamp. The next amount-only amend could then silently undo the accepted
  price. Typed per-order target guards now retain exact price/size/trigger
  expectations after accepted or ambiguous writes, including after matching
  same-timestamp evidence. Pending fills remain visible without becoming signing
  evidence. Definite rejection restores the previous settled expectation; an
  unsent JWT-queue failure releases its target. No uncertain write is replayed.
- Matching full Modify responses reconcile immediately. Independently newer
  coherent venue updates may retarget the guard; actual terminal events release
  it. Bounded reconciliation capacity fails before a new write rather than
  discarding an unresolved expectation. Eight additional native regressions cover
  post-ACK reads, repeated old evidence, size/trigger changes, pending fills,
  external edits, rejection/transport ambiguity, full ACKs and unsent failures.
- The independent reproduction now rejects the unsafe second write, accepts the
  matching same-timestamp row, ignores another old row and preserves the accepted
  price on the next signed amendment. Final review found no remaining blocker.
- Merged landing through Orderly #1318, preserving its dynamic socket feeds,
  default realm loader and disposal/recovery behavior alongside binary ingress.
  The venue remains opt-in; shared default loader adoption is now implemented,
  while default TS runtime retirement remains separate. Updated Orderly handler
  assertions retain the new binary callback. Engine and single-Orderly WASM
  Clippy pass; merged replay harness passes224 checks.
- Final artifact/replay gates and exact-head green CI remain required before
  landing. Earlier checkpoint measurements above are historical, not a waiver.
- Landing then advanced through Bybit variants #1307. Integration preserves its
  socket/action allocation changes, typed request matching, independently selected
  fixture writes and stricter memory baselines. Paradex rejects the newly exposed
  Bybit-only order controls before any quote or write. Independent merge review
  caught the optional TS signing-module import inside the CPU timer; it now runs
  before CPU sampling, like WASM initialization, while remaining inside the early
  retained-memory baseline. Both binary ingress and all signature checks remain.

### Explicit observation horizon and final merged checkpoint

The Bybit landing merge exposed a private-tape timing dependency: terminal UI
rows remain until the existing debounced REST reconciliation runs. Earlier GC
sampling happened to provide that delay. A generic `wait` record now advances
paced/virtual time to an explicit final observation horizon without making a
request, locating a socket or executing a command. Paradex waits 1,500ms after
its final recorded event. Inputs and financial outputs are unchanged; neither
runtime's terminal behavior is filtered or rewritten. The marker counts as one
frame in both replays. Independent review and three timer/bypass/validation
regressions pass.

Final single-venue artifact and wait-final replay gates pass against the merged
source based on `a7330dc0b1`:

| Gate | Result |
| --- | --- |
| Native Paradex tests | 105 PASS |
| Replay harness | 247 PASS |
| Workspace/ WASM Clippy; bans; format | PASS |
| Repository typecheck / lint | 44 / 28 tasks PASS |
| Single Paradex WASM, web and Node | 1,234,198 bytes; unchanged budget 1,258,291 |
| Public replay, three runs | PASS: 12,261 frames, 66 markets/tickers, two full-depth books |
| Public main-thread CPU, TS / Rust | 1,007.816ms / 779.891ms (1.29x) |
| Public steady CPU, TS / Rust | 573.4ms / 373.8ms (1.53x) |
| Public retained memory, TS / Rust | 21,785,193 / 2,056,788 bytes (10.59x) |
| Private replay | PASS: 15 commands, 33 frames including the observation marker |

The public book comparison retains BTC's 113 bids/71 asks and ETH's 107 bids/34
asks. Private performance remains informational: main-thread CPU TS 600.751ms /
Rust 359.710ms; retained memory 42,522,561 / 7,417,683 bytes. The landing's early
retained-memory baseline now includes loaded runtime code; these measurements
use that shared baseline and unchanged pass thresholds.

All 21 Rust CI checks passed on `a7330dc0b1` before this observation-horizon fix.
The new commit requires fresh green CI. The default combined WASM build, full
host suite and Binance public/private regression gates are the remaining local
follow-ups; their earlier checkpoint results do not replace those reruns.

### Completed local landing gates

The remaining local gates also pass on `2ae66217b6`: the combined default WASM
build completes, all48 host tests pass, and Binance public/private replay passes
with that combined artifact. Public Binance compares873 frames and three books:
CPU TS193.5ms / Rust85.4ms, steady105.5ms /29.2ms, retained18.9MB /3.2MB.
Its14-frame private replay passes without changing correctness requirements.
No additional source change or single-Binance artifact was needed. All mandatory
local port gates are now complete. Final exact-head CI remains the merge gate;
posted independent adversarial reports have no unresolved blocking finding.

### KuCoin landing integration

KuCoin #1312 landed during final CI. Merged its workspace and host registrations,
signed request contracts and welcome-message bootstrap while retaining Paradex's
binary ingress, signature verification, lifecycle routing and explicit observation
horizon. A single-KuCoin credential cfg omission is corrected in the composed
host. Independent SDK/host review and both-parent harness review pass: 105 native
Paradex tests, Paradex and single-KuCoin Clippy, and 269 harness tests pass.
Final artifact/gate measurements and exact-head CI are tracked on PR #1320.

The complete repository test run before this integration passed all 27 tasks;
both safe-cex suites passed 7,048 tests across 481 files. A docs-only CI head
exposed a 0.3ms WOO steady-CPU sample reversal on its 77-frame tape; independent
review verified identical code/base/artifact and the prior passing control. No
threshold, tape or correctness check was changed. This integration still requires
green CI for every venue before merge.

### Standard benchmark correction and final proof

The latest local follow-up commands accidentally included the optional
`--no-egress` flag. Their performance figures above are diagnostic measurements,
superseded by this table. The required standard gate includes the real TS worker
packing, transfer and main-store application. All four gates were rerun with
default options and no `--no-egress`; correctness and performance thresholds were
unchanged. Exact argv, resolved options and artifact/tape hashes are posted on
PR #1320. CI has always used the standard invocation.

Standard gates pass on production source `d12352ccd0`, plus the test-only
`409b6cc354` compatibility fix. That fix binds the new binary flag in two KuCoin
socket tests and asserts its text protocol. Workspace/all-target and WASM Clippy,
format and all 99 KuCoin tests pass; no production code changed.

| Standard gate | Result |
| --- | --- |
| Paradex public, three runs | PASS: 12,261 frames, 66 markets/tickers, two full-depth books |
| Public CPU, TS / Rust | 888.495ms / 797.589ms (1.11x) |
| Public steady CPU, TS / Rust | 458.257ms / 414.250ms (1.11x) |
| Public retained bytes, TS / Rust | 21,798,177 / 2,042,548 (10.67x) |
| Paradex private | PASS: 15 commands, 33 records, independent signatures/writes/account/queries |
| Paradex single web/Node WASM | 1,234,198 bytes / unchanged 1,258,291 budget |
| Binance public / private | PASS / PASS with the CI-matching single-venue artifact |
| Binance public CPU, TS / Rust | 194.2ms / 85.4ms; steady 103.6ms / 30.5ms |
| Binance public retained memory, TS / Rust | 18.9MB / 2.6MB |
| Native Paradex / harness / host tests | 105 / 269 / 48 PASS |

Default combined WASM build and host tests also pass. Typecheck/lint pass after
KuCoin integration; the complete 27-task repository test run passed before that
merge and exact-head CI reruns the full suite. Earlier optional no-egress misses
were not standard-gate failures; the invocation error is corrected explicitly
rather than changing benchmark policy. Exact-head green CI remains required.

### AsterDEX landing integration

All 27 checks passed on `959925fa34`, but AsterDEX #1322 landed before merge.
Its typed public-route/deadline metadata, account read routing and native wallet
signing now coexist with Paradex's binary ingress, private JWT headers and recovery.
The roster retains both completed implementations. Both-parent independent review
passes; 279 harness, 54 host and 206 transport-routing tests pass. Workspace and
WASM Clippy, bans, format, native host tests, typecheck and lint also pass.

Paradex rejects the new caller STP option before any batch I/O rather than silently
dropping it. Five newly introduced Aster-specific engine commands return explicit
unsupported errors on Paradex instead of leaving callers waiting. Regression tests
cover these boundaries. Fresh standard artifact/replay gates and exact-head CI are
required after this integration; the latest results are posted on PR #1320.

### Restore the single-venue size contract

The Aster merge grew the single-Paradex artifact to 1,264,821 bytes, exceeding
the unchanged 1,258,291-byte budget by 6,530 bytes. Review traced the avoidable
retention to five Aster-only command wire parsers. Their enum variants, conversion
arms and imports are now selected by the Aster feature, matching the SDK's existing
Aster-only dispatch. Default combined builds retain the full API. Shared native
commands, STP parsing and Paradex's explicit rejection guards stay intact.

Independent review passes. Single-Paradex wire tests reject all five unavailable
operations, and Aster-feature tests verify exact forwarding. The rebuilt web and
Node artifact measures 1,239,936 bytes: 24,885 bytes smaller than the failing
control, with 18,355 bytes of budget headroom. No threshold or economic behavior
changed. Final standard replays and exact-head CI validate this candidate; results
and command/artifact provenance are posted on PR #1320.

### Kraken landing integration

Kraken #1323 landed while the remaining repository CI jobs were running. This
merge adds its registry and replay hooks while preserving the Aster-only parser
feature selection and Paradex binary/signature/routing contracts. Independent
both-parent review passes. Paradex's 107 tests, Paradex/Kraken Clippy and
single-Kraken WASM Clippy pass; merged harness/host suites pass 291/54 tests.
Kraken's standalone secret option is included in its feature guard. Fresh standard
gates and exact-head CI remain the landing conditions; final proof is posted on
PR #1320.

### ApeX Omni landing integration

All 24 Rust checks (19 venue replays) passed on `90520d5bb2`; the two full
repository test jobs were still running when ApeX Omni #1324 landed. The final
standard local gates on that head all passed: public CPU TS/Rust
1,044.070/833.963ms, steady 586.048/435.708ms, retained bytes
21,958,657/2,044,996; private 15 commands/33 records; Binance public/private
PASS; default combined WASM restored and host54 PASS. The exact invocation and
artifact proof is posted on PR #1320.

The new merge preserves both registry entries, ApeX's host-initialization/account
read flow and Paradex's private startup guards. The new ApeX-only `getAccount`
wire alias follows its feature, while Aster's five wire operations retain their
feature selection and shared native commands remain intact. Feature-specific
tests validate the aliases and explicit unsupported decoding. The ApeX standalone
secret guard is included. No engine, native, JS/Workers transport or crypto
primitive changes arrive with this landing. Independent review and fresh final
gates/CI remain required; their proof is posted on PR #1320.

### Port-review lows revalidation plan (Codex, 2026-10-21)

1. Recheck order-row isolation, leverage-zero config admission and closed-position
   UPnL against current native source and existing deterministic regressions.
2. Strike stale findings precisely, without claiming a dedicated zero-config test
   where only source plus adjacent invalid-config regressions exist.
3. Retain explicit zero TP/SL refusal and strict financial-history errors as
   intentional safeguards. Docs-only scope unless a real safe gap is found.
4. Run the relevant existing focused regressions, independent review and docs CI;
   merge under the exact revert guard and report the result.

- Current native lows require no runtime change. Account order snapshots use
  row isolation and retain invalid rows' stored identities while valid peers
  update. Existing tests bad_snapshot_rows_keep_their_stored_copies_until_a_valid_snapshot_omits_them
  and unmapped_perpetual_rows_are_skipped_and_protect_only_their_own_identity
  cover that behavior. Configuration admission explicitly requires leverage>0
  (account.rs); one_malformed_margin_row_keeps_its_stored_leverage_while_valid_peers_commit
  covers negative/bad peers, not a dedicated zero-config case.
- Closing a position removes it and calls update_upnl, summing remaining exposure.
  closed_positions_with_defaulted_optional_fields_remove_existing_exposure covers
  REST and WS removal and asserts balance.upnl=0; the reported stale-UPnL low is stale.
- Preserve zero TP/SL refusal before dispatch: treating zero as absent could place
  an unprotected entry despite requested protection. Preserve strict financial
  history failures: tests cursor_loops_and_malformed_financial_reads_fail_without_touching_store
  and malformed_financial_scalars_fail_instead_of_becoming_zero_or_missing_rows
  pin this safeguard. Silent partial financial reads and implicit protection
  removal remain explicitly risky. No signatures, API shapes or host contracts change.
- Validation: three existing account regressions and two strict-history
  regressions pass. Independent read-only review cleared the two-file docs diff,
  including the source-only zero-config claim. No runtime tests were invented
  for unchanged behavior; CI validates the documentation commit.

## Beta readiness (2026-10-26)

Gates ran through the JS/wasm host (`EngineHost`, `hosts/wasm/js/host.mjs`) on the
Paradex family build.

- **Live public:** `rust:record`, 60 s with #2240's managed-HTTP recorder. 369,866
  WS frames, 215 store updates, both books, and `/v1/markets` and
  `/v1/markets/summary` answering 200. PASS.
- **Replay gate on that tape:** one intermediate checkpoint differs, in
  BTC-USD-PERP asks only, and the final snapshot matches. **Rust is correct.**
  Decoding the native SBE book frames (template 3) shows a delta deleting the
  80402.8 ask and a later delta (seq 7700674538) setting it again at 0.02451,
  with no gap, no crossed book and no removal before the checkpoint. Rust holds
  that level; the TypeScript reference drops the re-insert after a delete. This
  is a TS reference defect, so no Rust change was made and no CI tape entry was
  added.
- **Hosted (payloadSigner) Paradex** refuses at construction, typed
  Unsupported/NotSent; only local-key Paradex is in the beta.
- **Entry or TP/SL legs whose outcome is unknown** fail the command Unknown with
  the accepted order IDs; they never report success.
- **A 401/403 from `/auth`** is terminal and typed `AuthInvalid`, with no retry loop.
