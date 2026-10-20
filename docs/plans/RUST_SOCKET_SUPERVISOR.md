# Socket supervisor audit fixes

Lane: `feat/rust-socket-supervisor`, based on `bd94abb524` on
`origin/feat/rust-rewrite`. Scope: socket authentication/liveness and the
shared supervisor; no TS venue deletion or unrelated trading fixes.

Production impact is the judging rule: silent/stale account or market data,
unseen failures, lost money/attribution or broken callers. Internal startup,
cleanup, disposal and retry choreography may differ freely from TS. The lane
does not treat cosmetic lifecycle differences as defects or require a TS-shaped
supervisor; native authentication/topic/heartbeat constraints remain necessary
to receive correct live data.

## Reverification

The rejected-login findings still hold for Bitget, BloFin, MEXC, WOO,
Crypto.com and ApeX Omni. Heartbeats can keep these unauthenticated sockets
alive indefinitely. MEXC/WOO also lack a pre-open deadline.

The audit's collective "no backoff" wording is too broad: AsterDEX, Bitrue,
BingX and Paradex public already use exponential delays. Their private paths
use fixed delays, and resetting attempts at transport open defeats protection
against repeated open/fail cycles. The remaining named paths use fixed retry.
KuCoin already re-POSTs its bullet token on reconnect; preserve that behavior
and remove immediate token-retry bursts.

## Phases

1. Add `engine::socket::Supervisor`: deterministic jittered exponential retry,
   pre-open deadline, independent authentication deadline, receive watchdog,
   protocol ping hook and stale-timer cancellation. Unit/integration tests
   cover timeout/heartbeat separation, retry deduplication, cap and stable
   session reset. Audit the API with the three venue reviewers.
2. Migrate rejected-login paths (Bitget, BloFin, MEXC, WOO, ApeX Omni) with
   native failure regressions. Preserve ApeX's immediate subscriptions: its
   proxy does not reliably expose login ACKs, so a validated account payload
   may establish readiness. Reject unrelated failures independently.
3. Migrate AsterDEX, Bitrue, BingX, KuCoin and Crypto.com, then Phemex public,
   Coinbase public, GRVT, Paradex and Polymarket. Each connection owns a
   supervisor. Keep native mandatory-pong deadlines, feed identity, token
   routing, generation fences, resync state and terminal GRVT auth policy.
   Add a reproducing regression for every corrected path and review each
   bounded venue group with a different sub-agent.
4. Run all brief gates for Binance and touched venues, including native
   tests/lints, WASM, public recording/replay and private replay. Record exact
   evidence and limitations. Update/strike only the fixed audit items.
5. Push; open PR into `feat/rust-rewrite`; publish adversarial reviews and
   repair findings. Merge only the reviewed head with green CI, then create
   a clean follow-up branch from the landing branch.

## Shared contract

The supervisor owns timers and retry policy. Venues retain socket IDs,
generation fences, authentication parsing, subscription/recovery and wire
ping formatting. A new connection starts its deadline before transport I/O;
an open connection transitions to authentication or ready. Inbound heartbeats
never extend the authentication deadline. Failure cancels active supervision
and schedules one retry; canceled timers cannot revive a retired connection.
Transport open alone never resets retry attempts; validated activity after a
stable ready interval does. Jitter derives from replayable clock/IO identity,
never credentials or hidden host randomness.

Authentication rejection retires ownership before closing, preserves proven
store data and backs off. Server-only heartbeat protocols (Coinbase, Paradex,
Crypto.com) retain their native behavior; no application ping is invented.
KuCoin and Polymarket retain explicit pong requirements even amid other data.

## Progress

- Plan and three independent current-code audits completed.
- Phase 1: `ca0639d13b` adds the shared policy. Ten new regressions and strict
  engine Clippy pass (the first nine plus a review-discovered fractional-clock
  regression). The initial full engine suite passed 88 tests in both default
  and product-semantics/options feature configurations.
- Phase-1 review caught subtraction/addition rounding disagreement at a
  fractional timeout, causing zero-delay timers. Expiry now compares the same
  absolute deadline used when scheduling. The reproducer includes
  `10731.000000000002 + 10000`, whose subtraction rounds below 10000.
- Phase 2: `d4726feebf` migrates five private auth paths, with 519 native
  tests passing (24 new regressions), formatting and strict Clippy. Peer review
  found ACK-less wallet-only ApeX readiness and a Bitget write-recovery timer
  bypassing retry; both have RED-before/GREEN-after native regressions and an
  independent passing rereview. TS has the same rejected-login liveness holes;
  `KNOWN_TS_FIXES` records them without replay-output normalization.
- Phase 3: `f0927afde2` migrates AsterDEX/Bitrue/BingX/KuCoin/Crypto.com
  (415 native tests); `9634dba18a` migrates Phemex/Coinbase public, all GRVT
  feeds, Paradex and Polymarket (530 native tests). Both groups passed scoped
  formatting, strict Clippy and independent correctness reviews. Focused
  baseline runs reproduce the original flap/token/auth/stale-keepalive defects.
- `1e5ba12c51` merges landing `1b8592dbd2` (PR #1338); the sole conflict was
  independent tests appended to KuCoin's public suite, both retained. Landing
  introduced KuCoin Spot with fixed reconnect; it is now included in phase 3.
- Bundle investigation before that merge: Paradex's first shared-policy build
  was 1,260,249 bytes against the unchanged 1,258,291 budget. Constructor
  constant folding removes redundant validation code; exact power-of-two
  backoff scaling uses an integer shift. Measurement/verification continues
  against the merged landing rather than extrapolating from old builds.
- Fresh 20-second Paradex public recording succeeded: 26,674 frames, two HTTP
  responses and both requested books. Performance/replay gates remain pending;
  no live private connectivity is claimed.
- KuCoin Spot extension: `07fb8c0295`; default 105 tests and Spot-enabled 158
  tests pass (overlapping suites, not additive), with six new regressions
  exercising both roles. Formatting, strict Clippy and independent review pass.
- `d106389e12` preserves constant-policy constructor folding and exact integer
  power-of-two retry scaling. On merged landing, Paradex is 1,252,515 bytes
  against the unchanged 1,258,291 budget (5,776-byte headroom); no further size
  refactoring was necessary. Engine default 93/all-feature 99 tests pass.
- Merged root TypeScript checks pass: typecheck 44 tasks, lint 28 tasks and
  tests 27 tasks. Workspace native tests pass (2,829 tests); strict workspace
  native and WASM Clippy pass. Public recording/replay verification continues.
- Gate-plan review covers 21 frozen public profile controls, 23 fresh public
  profile gates from 21 captures, 29 generated private correctness cases and
  17 single-feature size selectors. Before any CPU measurement, the initial
  six-second fresh trims were superseded by a complete-capture policy: retain
  all recorded timestamps (ceil of the raw maximum timestamp, at least 20s),
  with every HTTP anchor. This retains native heartbeat/watchdog opportunities.
  Frozen fixtures, metrics, thresholds and output normalizers are unchanged.

- `34552182ee` fixes a Rust-specific Crypto.com auth parser issue discovered
  during gate preparation: missing `code` inherited a generic envelope default
  of zero. Private auth now requires an explicit numeric success code. Missing,
  null, boolean and string codes cannot cancel the deadline; 49 native tests and
  an independent focused review pass. TS already checks explicit success here.
- `5a0c5fc088` merges landing `fc809a9d85` (consumer/shared public feeds).
  Independent module exports and GRVT documentation sections were the only
  conflicts; both additions were retained. A read-only integration review
  found no production-impact issues in shared-feed/Supervisor ownership,
  public-connect suppression or retained ApeX/GRVT private PnL hooks. Workspace,
  artifact and replay verification is being repeated on this integrated source.

## Integrated gate checkpoint

At Rust source `1d8bffe0d6`, the 12,223,951-byte combined artifact
(`8f04fde9ee1b389e539511b9df8301ef6344e12eca4e111029cebedd9ed88276`)
completed 72 replay cases: 29 private and 39 public passed. Four public cases
failed: frozen/fresh Paradex main CPU, and the same Coinbase spot estimate
rounding difference through its two aliases. All 17 integrated standalone sizes,
2,857 workspace native tests, native/WASM strict Clippy, full WASM build and ten
compiled integration stages passed. Root typecheck/lint/licenses passed; two
initial parallel test timeouts passed with unchanged deadlines on the serial
27-task rerun. Both initial failure logs remain evidence.

Paradex profiling measured 19.46 MB of startup REST JSON (9,322 catalog rows,
mostly options); HTTP chunk scanning dominated cold startup. Constructor cost
was only 1.529 ms. `500cfabdf7` batches ordinary string spans in `ArraySplitter`
without changing validation, metadata limits or item bytes. All 23 JSON tests,
strict engine Clippy and independent review pass. The new 12,228,900-byte
combined artifact (`d1ae98d3f497152280c15f82307069d04374f0e381085d490a5b565546d8f7d8`)
passes both previously failing Paradex cases with default three-run medians:
frozen main CPU TS 421.7 / Rust 352.2 ms; fresh TS 238.4 / Rust 232.9 ms.
Steady CPU and retained memory pass too. The fresh main-CPU margin is narrow;
this is a measured pass, not a claim of robust timing headroom. The original
misses (Rust 406.7 vs TS 401.5 ms, and 271.9 vs 254.3 ms) are preserved.

`79df01a475` applies Sam's production-impact rule to Coinbase's cosmetic spot
quote-volume estimates. Exact native decimal products are 49234.575 (DEEP) and
3419.645 (WET); the native advertised estimates match Rust's upper cent, while
TS's binary-product formatting selects the lower cent. Production mapping stays
unchanged. A pure assertion proves midpoint neighbors from an unambiguous
native public spot snapshot without rewriting outputs; all other checks remain
strict. It rejects private/futures sources, ambiguous provenance, nonmidpoints,
incorrect identities/prices/volumes and other field differences. Review caught
an unreachable catalog selector bypass, repaired with a fail-closed selector
check and regression. The 87 pure tests, 15 CLI tests and six saved-output
checks pass, with independent passing rereview. Broad checks and the remaining
replays are being repeated after the shared parser change.

## Final landing integration and verification

`622982dc35` merges landing `702a62050c` (broker attribution/cancellation).
The only manual conflict was independent Bitget regression tests; both were
retained. Independent merge review passed, including KuCoin role/token/pong
state, private auth guards and the signing-clock reset before startup.

The preceding v2 verification passed all 27 build stages, ten integration
stages and root checks; its replay matrix was intentionally stopped after six
completed passes to integrate the new landing. Those six results and the
in-flight cancellation are not a full-matrix pass. The new binary is different,
so no earlier performance result is reused in v3.

On source `622982dc354fdfb138a4c9eaf579142ee4cc0d91`, all 27 build stages pass:
2,877 workspace tests, 164 explicit KuCoin Spot tests, 109 all-feature engine
tests and 103 no-default engine tests (overlapping suites), formatting, bans,
strict native/WASM Clippy, workspace WASM build and 17 standalone size budgets.
The largest standalone is Paradex at 1,223,634 bytes against the unchanged
1,258,291 budget. The final combined artifact is 12,231,306 bytes, SHA-256
`1197834f0dcfbd499b1870538486b86213b5699990d0255a0d317699088a5476`.

All ten integration stages pass, including 683 harness tests and 444 compiled
SDK WASM tests. Root typecheck/lint/test pass (44/28/27 tasks); licenses cover 31 workspaces.
Final replay verification passes all 72 cases: 21 frozen public controls,
22 fresh public profiles and 29 private cases. Public cases use three runs and
pass main/steady CPU and retained memory thresholds; private metrics are
informational. Final fresh Paradex is TS 232.4 / Rust 229.8 ms main CPU,
63.9 / 41.3 ms steady CPU and 18.7 / 2.5 MB retained memory: a narrow main-CPU
margin, not a robust-headroom claim. Every final case is measured on the merged
artifact; prior source timings are not reused.

[Machine-readable gate evidence](venue-ports/SOCKET_SUPERVISOR_GATES.json)
records every source/artifact/input/output/log hash, three-run medians, exact
17 size selectors and parsed check counts. It separately retains the original
68-pass/four-failure matrix, interrupted six-pass v2 run, and the final stale
KuCoin input failure. Final artifact: 72 passing cases across 73 attempts;
71 inputs retain their original hashes and one uses the current generator.
No successful performance case was repeated to seek a larger margin.

The final-artifact replay attempt completed 52 cases before legacy KuCoin
private input failed in TS with one unconsumed taped write, producing no metrics.
Landing had already removed precisely that obsolete DELETE `stopOrders` record
from its generator. The current generator produces 76 records instead of 77,
with no other difference; its regenerated private gate passes. The old input,
failure log and interrupted collector exit are preserved, and the final matrix
uses the current generated KuCoin input. The 52 passes are retained verbatim;
remaining cases resumed and passed without rerunning successful performance measurements.

## Public alias discovery and coverage limitation

On 2026-10-20, Bitget USDC's live contracts/tickers returned 49 native symbols,
including `BTCPERP` and `ETHPERP`. Rust and TS both retain those native IDs.
The initial `BTCUSDC,ETHUSDC` book selection was rejected by the venue; the
recording matrix now uses `BTCPERP,ETHPERP`. The corrected capture passed its initial three-run public gate; the final
integrated matrix repeats it on the merged artifact.

Classic inverse contracts returned HTTP 200/code `00000` with an empty catalog
for both `coin-futures` and `COIN-FUTURES`; USDC also returned identical data
for lowercase/uppercase queries. Query casing is therefore not the cause.
The inverse capture's `BTCUSD,ETHUSD` book requests were rejected. Its two
remaining ticker rows, `BTCUSDU26` and `ETHUSDU26`, have null bid/ask prices,
zero volume and a delivery timestamp before capture; they do not establish
available perpetual books. Bitget's [official classic coin-margined delisting
announcement](https://www.bitget.com/support/articles/12560603893788) schedules
BTCUSD/ETHUSD and seven other classic perpetuals for delisting on September 17,
2026 at 15:00 UTC+8, distinguishing them from UTA products.

Fresh classic-inverse public-book replay/performance coverage is unavailable
against this empty live catalog. The rejected zero-book capture is discovery
evidence, not a passing gate; no replacement public fixture is fabricated.
Keep the Rust/TS inverse implementations and native/generated-private coverage;
this limitation neither retires the product nor adds UTA inverse routing.
Frozen inputs, thresholds and normalizers remain unchanged. Investigation
includes the original captures and four independent official public HTTP
responses; no authenticated live connectivity is claimed.

## Migration inventory (complete)

Every venue WebSocket now runs under `engine::socket::Supervisor`; the
follow-up below moved the remaining venue-owned supervision. BreakoutProp
uses polling rather than a venue WebSocket and needs no socket supervisor.
Native mandatory-pong/subscription/session state remains venue-owned even on
migrated feeds.

### Follow-up migration plan (lane `claude-sockets`)

Behaviour-preserving cutover of the inventory above, one `Supervisor` per
connection, deleting venue-owned watchdog/retry timers. Existing timeout and
ping cadences are kept as `Config` values and existing backoff constants as
base/cap; fixed delays become base = cap. Deliberate shared-contract changes:
transport open no longer resets backoff (stable ready traffic does), and
delays gain the shared deterministic 0.75-1.0 jitter. Every venue gets
reconnect/resubscribe/flap regressions.

1. Public ApeX Omni, Bitget, BloFin, MEXC, WOO: #1385.
2. Private Phemex and Coinbase; Binance; Bybit: #1406.
3. Backpack, Hyperliquid, Kraken, Lighter: #1423.
4. OKX, Orderly brokers, TxFlow, WEEX, plus the suppressed-feed gate below.

Suppressed public feeds: under a non-Direct `PublicFeedPolicy` the engine
hands back placeholder `SocketId(0)`, which never opens. Supervising it
looped connect-timeout/retry forever (and cleared books or re-fetched tokens
on some venues), so public `connecting()` runs only under `Direct`, on every
venue, with a per-venue regression. Stage-chained auth (Kraken futures, Bybit
Hyro), heartbeat-ack probes (Binance spot), payload-staleness recovery
(Lighter, Orderly, Coinbase snapshot stall) and listen-key/token acquisition
stay venue-owned on top of the supervisor.


## Published PR integration and adversarial review

PR [#1350](https://github.com/Tealstreet/tealstreet-sdk/pull/1350) targets
`feat/rust-rewrite`. `fe337b6611` merges landing `5f84b3e21d` (Binance/Bybit
trading and cross-venue leverage audit fixes). Manual conflicts were independent
audit closures and Phemex documentation; both additions were retained. Rust
source changed, so the prior verified checkpoint above is historical and all
builds, sizes, integration checks and 72 replay cases are being repeated.

Fresh adversarial auth and feed reviews found no production-impact defect.
The harness review found two P2 provenance gaps: mixed text/binary WS records
could be audited through the wrong payload, and a final taped catalog need not
have been consumed. The proof now rejects any binary property and requires
every validated catalog row that could produce the observed numeric fields to
corroborate the same exact rational notional, product units and native cent
neighbors. Different raw decimals that collapse to the same floating value
cannot supply ambiguous evidence. Candidate request ordinals are reported;
there is no claim the last response was consumed. Neither output is rewritten.

Seven new pure regressions and two new CLI controls reproduce these gaps;
690 harness tests pass after repair (94 pure midpoint tests and 15 CLI tests
included). The original binary controls had four failures; adding candidate
controls produced seven failures before the repair. Focused independent
rereview and final integrated verification remain pending before merge.


Focused rereview closed both P2s at `fcaf6d6723`, then identified duplicate raw
JSON members as another way for streaming and last-wins parsers to see different
catalog rows. Literal and escaped-equivalent duplicate `products` regressions
both fail before repair. Coinbase catalog and WS proof now use the same decoded
unique-key validation extracted from Kraken's existing independent harness;
Kraken retains that validation. Nested duplicates, escaped Unicode keys, valid
independent scopes and malformed grammar are covered. All 700 harness tests
pass (96 midpoint tests, 15 CLI tests, eight shared JSON tests included).
Outputs and production mappings remain unchanged; final focused rereview is
pending. Rust source has not changed since the latest landing merge.


## CI CPU follow-up and final verification plan

CI run `39434260840` on `59428c25a6` failed two unchanged public CPU
checks: Hyro's steady CPU (both displayed 5.6 ms; strict comparison failed)
and HyperliquidSpot main CPU (TS 66.3 ms, Rust 68.0 ms). All other jobs
passed. These are recorded failures, not successful gates.

Hyperliquid's numeric wire reader buffered each primitive through an untagged
enum and tried a failing float alternative for numeric strings. `6e29557181`
uses a direct numeric visitor. Five regressions preserve finite validation,
f64 rounding, signed zero, escaped/owned strings, optional/default behavior
and flattened contexts. Native tests and strict all-feature Clippy pass;
independent correctness review found no production regression. Performance
acceptance remains pending.

Hyro already uses the numeric ticker and reusable book paths. One instrumented
real-tape replay identified host ingress and Rust decoding as candidates;
its timings include instrumentation and are not acceptance evidence. A fixed
six-run comparison of the existing string and byte ingress APIs produced
identical full logical state in all runs. String main CPU won all three pairs,
but steady CPU lost two pairs (median bytes 9.355 ms, string 9.196 ms), so the
host API choice stays unchanged. No acceptance rerun was used for this result.
The narrower book decoder change replaces temporary owned-or-borrowed field
keys with Serde field identifiers, preserving the existing value visitors,
required fields, duplicate detection, ignored-value validation and complete
JSON validation. Escaped keys, duplicate aliases and malformed ignored values
have regression coverage. Its performance benefit is a hypothesis until the
unchanged official gates run.

`f09100c8ab` integrates landing `f895a98f26` (Paradex malformed-SBE,
partial-placement and listener isolation fixes). A separate integration review
checks that the Supervisor and native heartbeat paths remain intact. The final
verification will use one new combined artifact, 19 standalone size selectors,
86 replay cases (28 frozen public, 22 previously captured public, 36 scripted
private), all workspace native/WASM checks, root checks and integrations.
Seven new frozen/public and seven private cases cover all Bybit and Hyperliquid
profiles touched by the CI repairs. Existing public tapes, thresholds and
normalizers are unchanged. No fresh capture is claimed for those seven
additional profiles. The earlier complete 72-case report remains historical;
the interrupted v4 stage completed builds and integrations but no replay
matrix. Merge still requires green CI on the reviewed head.
