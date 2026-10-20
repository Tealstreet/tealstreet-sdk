# Public startup resilience (second PR)

Active branch: `feat/rust-public-startup-resilience`, created from landing
`9c34020741` after [broker/cancel PR #1347](https://github.com/Tealstreet/tealstreet-sdk/pull/1347)
merged with 40/40 successful CI checks and three posted correctness reviews.
Local first-PR evidence: 2,811 native cases / 222 targets, 590 harness tests,
76 JS host tests, 444 Wasm SDK cases, 44 typecheck / 28 lint / 27 test tasks,
12 public gates and 11 private gates. The failed Hyro steady-CPU sample passed
an isolated rerun at the same source and unchanged gate thresholds.

The previous public checkpoint `bfce1c1012` was cherry-picked as `21c6b8ab38`.
Both integration conflicts preserve the landing's shared-public-feed module
and docs alongside the new startup policy. Broker and cancellation fixes remain
on the landing branch; no stash, rebase, force-push or master change.

## Execute and review

1. Integrate engine policy and all public family migrations; rerun full native
   targets and repair outdated scripted drivers. Independent reviewer audits
   all-malformed responses, paging, caches and safe retry identities.
2. Repair funded private valuation boundaries and SDK readiness. A private
   account with valid markets/account data must not be disposed by the worker's
   ticker-dependent startup timeout. Preserve shared-feed compatibility and
   real quote handshake requirements. Never synthesize prices or readiness.
3. Run engine/venue/host, root, private/public replay gates; review this phase,
   repair findings, open a PR into `feat/rust-rewrite`, post independent
   correctness reviews, and merge only after green CI. Private row tolerance,
   ticker selection and app payload work remain the following phase.

Shared engine policy: independent public read identities retry at 1/2/4/8/16/30
seconds (30 second cap), deduplicate pending retries and reset after success.
Venues rebuild typed requests and signatures and discard failed accumulators.
Malformed public rows warn without logging payloads; private snapshots/ACKs keep
their old strict semantics until the third phase. Supported blank-number
coercions remain public-only. Private reads start after catalogue availability,
independently of ticker read success. Store::ensure_ticker recovers a known
catalogue row only when real validated live data arrives, preserving the REST
loaded flag. After the seed loads, live data creates no row the seed omitted
(TS parity) unless a venue registered that base as a funded asset awaiting
valuation (`Store::want_ticker_for_base`). Optimized ticker index caches recover
through the same path.

## Checkpoint before first-PR gates

All public source changes compile (engine plus all 26 families; family-specific
checks). New scripted regressions include backoff, malformed peers, independent
private startup, MEXC/WEEX zero/last-price fallbacks, and Bitget/BingX/WOO live
recovery (4 tests each). The eight additional families have 12 passing cases plus
Kraken spot. KuCoin spot has its own migration/conformance cases.

Source and fixture repairs are not yet complete. Do not merge this WIP commit:

- WOO private positions/orders depend on tickers; starting earlier must not hide
  positions or calculate losses using a zero placeholder. Repair using native
  data or explicit retained/deferred valuation; test funded snapshots before
  tickers. The first reviewer identified this dependency before landing.
- Root full tests `/tmp/startup-root-full3.log`: only two obsolete OKX expectations
  failed after lazy recovery; both updated, rerun required. Other root families
  and new public cases passed. Engine ensure_ticker test needs final rerun.
- 12-family full tests `/tmp/startup-all12-second.log`: four old fixture targets
  failed (Bitget USDC catalogue fixed; MEXC early mode read; Phemex watchdog timer;
  WEEX public request counts/watchdog). Repair remaining drivers then rerun.
- Additional eight-family checks passed; final numeric narrowing pending full
  rerun. Coinbase all-malformed empty catalogue should retry rather than claim
  ready. TxFlow fixture assumes private startup waits for every ticker. Preserve
  its explicit ambiguous-native-identity rejection (wrong-asset routing risk),
  visible log and read backoff.
- Review all other ticker-dependent account valuations before landing. No
  placeholder may manufacture an unrealized loss or erase native exposure.
- Final phase sub-agent review, all root/native/Wasm/harness/replay gates,
  correctness PR review and green CI remain required.

Third phase is separately planned in `RUST_APP_STOP_PAYLOADS.md` and the private
rows/ticker subscription audit. It has not been implemented or verified.

## Review repairs in progress (2026-10-20)

- `b8b858568f`: independently reviewed ApeX/Bitrue funded valuation fixes,
  five focused regressions and both crates' all-target Clippy passed. These
  explicit TS bugs are documented in `KNOWN_TS_FIXES`; no replay output is
  normalized by this change.
- Public all-malformed source responses preserve existing state and retry;
  a successful earlier product/page must not hide a wholly malformed later
  required source. Good peers continue to commit.
- Public read retries emit warnings so an otherwise usable authenticated
  account is not rejected by SDK validation. Auth/command failures retain
  their existing error behavior. SDK private readiness requires markets and
  actual private snapshots, independently of ticker readiness; public-only
  and shared-feed readiness still require genuine public data.
- Early socket data must survive delayed REST ticker seeds. Field-level
  overlays retain only fields actually received live, so a BBO cannot suppress
  valid REST mark/last data. Backpack must have one owned public socket.
- Hyperliquid discovery retains native positional identities, preserves
  quotes already received while missing contexts retry, and does not mark a
  wholly unusable catalogue loaded. During partial discovery, private reads
  cannot claim an empty account before an authoritative snapshot. Standalone
  spot still retains raw unpriceable holdings with zero notional after a
  complete catalogue; temporary missing contexts preserve prior valuation.

**Explicit scope boundary:** BreakoutProp's authenticated Instruments/Details/
Quotes discovery remains for the third phase alongside its account row audit.
Those catalogue/ticker reads still need the shared retry and malformed-peer
policy before the overall cross-venue task can be called complete. This public
PR must not claim all 26 families have finished that migration.

Independent review reports and test totals remain provisional until the final
source is committed and all native/Wasm/root/replay gates have passed.

## Reviewed source checkpoints

- `06ec88eb21`: shared live-field overlay, SDK readiness, and twelve-family
  review repairs. Engine + families: 1,359 native cases / 113 targets passed;
  subsequent owned-position boundary suites: 760 and 385 cases passed. Strict
  all-feature/all-target Clippy passed. SDK readiness/loader: 14/14 passed.
- `6f4bd666d7`: eight-family recovery, numeric narrowing, exact midpoint-batch
  coverage and account boundary repairs. 858 native cases / 73 targets and
  strict Clippy passed; final Paradex account follow-up: 24 passed. Independent
  quote overlay/extra-family source review passed after two Polymarket repairs.
- `3c82fe2883`: root-family source repairs. OKX full targets: 124 cases passed,
  then six startup race/settings tests and fifteen account tests passed.
  Three-family ownership/valuation run: 306 cases / 25 targets passed;
  subsequent selected account/public targets: 120 cases / seven targets passed.
  BloFin/Lighter account guards: 44 passed; public retry/overlay regressions
  passed. Independent root-family/SDK source review passed after repairs.
- Latest landing documentation merged normally, including owner decisions and
  `RUST_QUALITY.md`; no source conflict, stash, rebase or force-push.

Root verification completed before the final Rust-only safety follow-ups:
44/44 typecheck tasks, 28/28 lint tasks, 27/27 test tasks, 590 harness cases.
These are **checkpoint numbers, not final gate totals**. Final workspace/Wasm,
all public/private replay gates, posted PR reviews, green CI and merge remain.

The final narrow working-order boundary sweep covers every family: a malformed
catalogue peer cannot newly turn an owned working/protective order into successful
absence. KuCoin initial readiness must cover both normal and stop books; unknown
contract metadata cannot authorize guessed fill quantities. WOO parent completion
alone cannot prove each protective child terminal. Full rejected-row isolation
and row-by-row retained-state merges remain the third phase.

## Funded-wallet gate repairs

The frozen `16c7b2f830` checkpoint passed the full native workspace (2,971
cases / 249 targets), strict native/Wasm workspace Clippy, wasm32 build, bans,
format, 590 harness cases, 76 JS host cases and root checks. Its actual Wasm SDK
run passed 443/444: COIN-M startup exposed missing non-USD wallet FX. The private
matrix passed 61/64; USD-M/COIN-M wallet valuation and a MEXC harness socket-role
assumption were the three failures. These are checkpoint results, not final
proof for the repaired source.

The funded-wallet audit repaired Lighter native PnL/value authority, OKX native
currency FX and coherent inverse handoff, Hyperliquid portfolio/unified
collateral readiness and Kraken Spot valuation recovery. Missing prices retain
native quantities and prior USD rather than publishing invented zero/one-dollar
values. A rejected catalogue peer cannot prove a funded asset unsupported.
Kraken Spot retries incomplete metadata and subscribes recovered symbols on its
existing socket; Binance has a parallel catalogue/valuation repair under final
verification.

Independent OKX/Hyperliquid review found and closed inverse FX inconsistency
and spot-before-core principal errors. Both crates passed 242 cases / 19
targets, then the final OKX valuation target passed 7/7 including an unsupported
native currency requiring real `totalEq`. Lighter account/valuation targets
passed 29 cases. Kraken passed 150 cases / 14 targets, then its final malformed
pair-identity guard passed 19 focused cases. All four crates passed strict
all-target Clippy. The MEXC harness repair preserves signature validation and
routes recorded public/private ordinals by sent semantic role; 121 focused
harness cases and the default three-run private gate (final Store egress
enabled) passed. Full final Wasm/replay/root gates remain required.

Shared-provider follow-up: a repaired native catalogue can gain a funded
market after the provider sent its first full snapshot. The SDK requests a
fresh provider snapshot only for genuine symbol gains (compatibility is
validated first, and symbol state is installed before sending to prevent a
synchronous feedback loop). Four focused SDK suites passed 24/24; SDK typecheck
and source lint passed. Independent review passed this boundary and Binance's
partial metadata repair; final native edge tests are still being completed.

Landing moved with #1348 (Binance/Bybit audit) and #1349 (side leverage). Merge
those before final artifacts/gates. Integration review requires retaining current
unknown-owned snapshot safety alongside incoming side settings, especially
Bitget's all-unmappable-only guard, and preserving native wallet valuation
alongside incoming Binance execution/identity fences.

Binance final wallet recovery: 129 native cases / 13 targets and exact-source
12/12 wallet cases passed, plus strict all-target/all-feature Clippy. Native
asset caches are fenced when a newer private account event makes them stale;
existing health cadence obtains a fresh account response. A clean native
catalogue census distinguishes unsupported routes from rejected metadata,
without imposing a new barrier for unvalued dust or BTC-only spot pairs.
Initial catalogue publication remains linear; repair retains market indices,
settings, live quotes, books and socket ownership. A provisional combined
Wasm SDK run after wallet fixes passed all 444 cases / 16 files. Rebuild and
repeat against the merged landing source before final proof.

## Landing integration checkpoint

Merged landing `5f84b3e21d` (#1348, #1349) normally. Lighter's only textual
conflict preserves native PnL/notional, unknown-owned metadata rejection and
incoming leverage-bound validation. Bitget's semantic integration first kept a
whole-snapshot freeze; per-row retention has since replaced it: an unmappable
owned row keeps its own stored position or order (and preset SL/TP children)
while valid peers update. Incoming side leverage and config fences are
retained. Independent integration and Binance/SDK source reviews passed; merged
Binance/Bybit suites passed 323 cases / 24 targets, and all workspace targets
compile. Root typecheck44/44 and lint28/28 passed.

Lighter's full suite exposed two outdated shared-feed Balance assertions.
Independent review confirmed native ACCOUNT PnL0/equity1000/free900 remains
the wallet authority while shared quotes separately estimate displayed Position
PnL40. Tests now pin both sources; runtime output is unchanged by this fixture
repair. Full native/Clippy/Wasm/replay gates still run against the merged source.
Root tests passed all 7,803 SDK unit cases, but the parallel test-ci copy timed
out on Bybit's dynamic-import capability check (7,802/7,803). That unchanged
32-case file passed in isolation; rerun the root test gate after heavy builds,
without increasing timeouts. Do not count that root gate green yet.

Merged artifacts: release combined Wasm, native/wasm32 workspace Clippy and
format checks passed. Actual Wasm SDK passed444/444; Lighter full crate
passed119 cases/16 targets after the independently reviewed native-account
assertions. Bitget passed178 cases/18 targets and strict Clippy. Two Phemex
host fixtures discarded private actions before ticker seeding; their drivers
now retain all effects and answer the exact endpoint. Real compiled host
tests passed76/76. Full harness initially hit its unchanged5s TxFlow child
process timeout during heavy builds; the complete rerun passed592/592 without
changing tests or timeouts. Final private/public matrices, root test rerun,
consumer checks and native workspace completion remain pending.

## Complete checkpoint before wire landing integration

At runtime source `7cc5c2a23f` / host-driver checkpoint `623cce1a26`, all
3102 native cases /259 targets passed; 64/64 scripted private replays passed;
26/26 standalone bindings passed the unchanged CI budget1258291 bytes
(maximum Paradex1235382). The size builds kept combined Node bindings unchanged
(SHA256 checked). Root44 typecheck/28 lint/27 test tasks,444 actual Wasm SDK,
76 actual JS host and592 harness cases passed. Four consumer Wasm suites
passed20 cases; frame-wire/server-adapter/product checks passed1/1/16, and
MCP packaging/smoke passed. Public40-run matrix had not started.

Landing moved to wire PR#1345 at `c44f22bd8e`. Integrate before final artifacts:
12-column midpoint-aware ticker rows/revisions, symbol-keyed settings and
failed-frame recovery must coexist with lazy ticker creation, discovery overlays,
private ticker-independent readiness and catalogue-gain shared snapshots.
Independent reviews require real catalogue identity before startup, keep null
midpoint clears from satisfying first shared quote readiness, and preserve the
catalogue census before deferred snapshot dispatch. Repeat affected gates against
the integrated artifact. The Paradex lane defers public blank startup and the
narrow optional REST leverage/liquidation blanks here; SBE/trading/errors stay
with `feat/rust-paradex-emitter`. Generic private row isolation remains later.

## Wire integration and Paradex boundary fixes

Resolved wire landing `c44f22bd8e` while retaining lazy ticker rows, shared
catalogue-gain snapshots and ticker-independent private startup. Independent
reviews caught and fixed stale decoded-section evidence after rejected indexed
identity and a shared compatibility check racing the startup store callback.
Readiness now requires accepted sections and the current validated shared
catalogue, with valid empty tables and independent good peers preserved.
The expanded SDK integration suite passes110 cases/eight files, TypeScript and
strict runtime lint. Native engine tests pass107 cases with a new midpoint-clear
overlay assertion. Whole-workspace format and banned-pattern checks pass.

Paradex summary fields coerce null only within Tick, retaining valid last/mark
prices beside nullable bid/ask/funding. Optional position leverage/liquidation
accept empty and null with the existing cached/default leverage fallback.
Required size/PnL/margins, instrument metadata, depth and signing remain strict.
REST/JSON tests cover funded startup, CLOSED removal, null/empty and mixed bad
required-size snapshot preservation. Full Paradex passes128 cases/nine targets
and strict Clippy. Combined release Wasm and full integrated gates are running;
the earlier complete checkpoint is not proof for these newer artifacts.

Actual Wasm SDK tests exposed a first-zero-balance hole: native Store marked
balance loaded but suppressed BALANCE because its value equalled the initial
zero. Publish the first validated balance once; later unchanged zero remains
silent. Native and actual-Wasm regressions failed before the fix, independent
source review passed, engine108 cases and strengthened BloFin2/2 real-Wasm
cases pass after rebuilding. This preserves decoded startup proof and never
fabricates funding. Final all-source gates continue against this artifact.

Owner wind-down: finish this public-startup PR/reviews/green CI/merge, then stop
without starting the planned private-row/ticker/app-payload follow-up. Record
all unmerged assignment items with file locations in the requested aimux handoff.

Latest landing `f895a98f26` includes Paradex/listener PR#1352. Normal integration
retains both the nullable-summary and malformed-SBE public regressions; independent
Paradex and SDK emitter integration reviews pass. The zero-balance artifact
passed445 actual Wasm SDK cases before this new integration. Full root tests
have exposed additional old mock loaders that preload JS flags without native
frames; repair their scripted startup evidence, keeping runtime decoded proof
and existing timeouts. Repeat final affected gates against the integrated source.

## Wind-down checkpoint: pushed branch handoff, no public PR yet

The owner authorized pushing and handing off if no PR had opened and the
remaining verification was substantial. Use that route; do not claim a merged
or ready public PR. Twelve wrapper fixtures now publish real scripted native
startup tables after fake.start, retaining their actions/spies/identity-only
lookup behavior. Shared test helper reuses the existing consumer frame encoder.
Focused201/201 cases, package TypeScript and independent source review passed;
root lint28/28, format and bans passed. No runtime proof or timeouts were relaxed.

The earlier integrated native workspace passed3118 cases/259 targets before
the zero-balance and latest Paradex landing. Rebuilt zero-balance engine passed108
cases; its actual Wasm SDK passed445/445. The64 private replay integration
checkpoint passed, but a fresh final-source matrix is still required. The final
workspace/standalone-size sweeps were deliberately interrupted for wind-down;
five of26 sizes had passed the unchanged1258291-byte budget. Public40 performance
gates were not started. Root full tests must rerun after the fixture repairs;
remaining host/harness/consumer/Clippy/final artifact checks and PR/CI/merge also
remain. No all-gates pass is claimed.

Detailed handoff: `/Users/sam/cs/tealstreet-sdk/.aimux/plans/handoff-codex-u1pmrg.md`.
`claude-nfncln` accepted PR3 (private rows, ticker verification, app payloads,
Kraken market-entry TP/SL errors and Lighter trailing rejection), including
BreakoutProp authenticated startup and MEXC private mode/plan bootstrap. MEXC
ticker/order-row changes remain with `feat/rust-venue-highs-a`; Orderly remains
with its race lane. No PR3 implementation or branch was started here.
