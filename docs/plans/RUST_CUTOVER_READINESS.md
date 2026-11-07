# Phase 9: consumer cutover readiness

Sam's current rollout mechanism is the [per-user hard-cut bundle switch](RUST_BUNDLE_ROLLOUT.md):
frozen TS app/server versus new Rust app/server for every held venue. Its cohort,
shared-state and rollback rules supersede older per-account/default-switch
recommendations below; the evidence and reconciliation requirements still apply.
This is preparation only, with no deployment or flag change.

For current landing → master blockers, see [Sam’s merge checklist](RUST_MASTER_MERGE_BLOCKERS.md).

Branch: `feat/rust-cutover-readiness`, based on `origin/feat/rust-rewrite`.
This change proves consumer contracts and records the deletion prerequisites;
it does not delete TypeScript venues or enable Rust by default.

GRVT's apparent non-perpetual catalog gap is resolved separately by an
[explicit supported-product retirement decision](GRVT_PRODUCTS_DECISION.md):
TS discovery defaults to perpetuals, and Future/Call/Put are unexposed latent
mapper paths. Retain TS and all other consumer/dependency gates; this decision
does not enable non-perpetual products or default cutover.

## Plan and verification

1. **Consumer conformance.** Add a deterministic Vitest `*.wasm.test.ts`
   suite selected by `SAFE_CEX_WASM_TESTS=1`. Enumerate `RUST_VENUES` rather
   than a hand-maintained subset. Exercise real WASM venue startup from
   committed public recordings, then independent documented flush-frame
   scenarios through `RustEngineExchange` and the worker/main store bridge.
   Compare against the existing store consumer contracts: bulk update IDs,
   terminal-order protection, serialization/stub shape, ticker deltas,
   placement units, post-processors, chasers, TWAP and CLI account reads.
   Fix shared adapter gaps exposed by those scenarios. Review the plan and
   implementation with an independent sub-agent, fix findings, run focused
   tests, package typecheck and lint, then commit.
2. **Deletion inventory.** Audit all `packages/` and `apps/` imports,
   construction, inheritance and type references to `BaseExchange` and venue
   classes. Record concrete replacements, per-venue readiness and outstanding
   product/consumer blockers in phase 9 of `RUST_REWRITE.md`, with a linked
   exhaustive caller inventory. Preserve TS implementations and test oracles.
   Independent sub-agent review, fix findings, then commit.
3. **Acceptance and landing.** Run full SDK WASM tests, SDK/unit and repository
   typecheck/lint/tests, host/harness tests, Rust bans and applicable checks.
   Rebuild combined WASM and rerun the Binance public/private regression gates.
   Venue API behavior is unchanged; existing per-venue CI replay matrices
   remain the venue correctness/performance gate. Push and open a PR into
   `feat/rust-rewrite`, publish independent code-correctness reviews and fixes,
   repeat reviews, merge only on green exact-head CI, then cut a fresh branch.

## Evidence boundaries

Public recordings prove engine registration/startup and actual emitted frame
compatibility. Independent binary fixtures exercise private and lifecycle
consumer edge cases without credentials; they do not claim live trading or
replace the per-venue private protocol tapes. A registered venue is not thereby
ready for deletion: unsupported products, legacy helpers, runtime imports and
consumer adoption remain explicit blockers. Live evidence (public and private
shadow, smoke, canary and rollback) is defined in [RUST_LIVE_GATE.md](RUST_LIVE_GATE.md).

## Progress

- Fresh branch created; venue brief, engine spec and store/worker contracts
  read. Consumer plan audit and caller inventory delegated to independent
  sub-agents. Real venue fixture support is isolated to test helpers.
- Phase 1: 38 real WASM startup/worker/snapshot cases pass. The independent
  boundary corpus has 190 cases across all aliases plus ten focused metadata
  cases. All 200 pass after implementation review. Package typecheck and
  explicit lint pass; the combined WASM build, Rust bans, format and WASM
  Clippy pass. Broader acceptance remains in progress.
- Three SDK gaps repaired: explicit `engine: 'rust'` was ignored by the SDK
  factory; OKX aliases lost native fill/position sizing metadata; main-frame
  orders lost JS-derived chaser tags. The last fix uses a public order mutation
  path, equality checks and a reentrancy guard so derived-only tag changes bump
  update IDs while identical/ticker-only notices do not. Tests cover both
  arrival orders, observer counters, native extras and disposal.
- Phase-1 independent reviews found and corrected inverse-fixture flags,
  terminal opt-in evidence, inactive algorithm singletons, optional metadata
  coverage, and the first enrichment fix's missing update-ID bump. Final
  reviewed focused tests pass; no additional actionable finding remains.
- Phase 2: exhaustive caller inventory and phase-9 replacement/deletion plan
  written. Independent source audit verified 49 registered names, 38 Rust
  aliases and 11 absent names, and required six explicit consumer/product
  blockers. Main plan and inventory pass the second documentation review.
  No TS implementation has been deleted.
- PR correctness review round 1: production changes reviewed clear. Fixed two
  proof gaps: the inventory now includes all frozen-reference callers (79
  imports across 40 files), and the real-venue suite rejects zero websocket
  delivery or any connected URL outside its recorded fixture. It also requires
  a native ticker/delta frame before snapshot and compares an immutable snapshot
  with the worker's regular store-flush catch-up. All 38 strengthened native
  cases pass; independent re-review is clear.
- Baseline acceptance at the Coinbase landing: full SDK/WASM 321 tests, harness
  359, JS host 63 and native workspace 1,992 tests/163 suites pass. Root
  typecheck (44 tasks) and lint (28 tasks) pass. Binance public/private controls
  pass: public main CPU TS/Rust 284.3/174.2 ms, steady 157.2/81.6 ms,
  retained memory 19.8/3.6 MB (2.0 MB Rust linear memory included). Combined
  WASM is 9,579,478 bytes. Resource-concurrent full unit tests timed out in
  existing import-heavy cases; a bounded full package rerun is pending.
- Polymarket landed during acceptance. Integrate its landing commit, extend
  the automatically enumerated matrix and caller readiness, rebuild combined
  WASM and repeat affected gates before the PR is ready to merge.
- Integrated Polymarket landing `a0c11afd87` without conflicts. The native
  matrix now passes 39/39 aliases, including its scoped Gamma input, lazy
  subscriptions through the real worker, both captured books and exact outcome/
  holding metadata. Registry inventory is now 49 names / 39 Rust / eight Rust
  defaults / 31 opt-in / ten absent names. Generic algorithm fixtures do not
  enable product features disabled by metadata. Independent integration review
  is clear.
- Post-integration acceptance: 7,240 SDK unit tests across 497 files pass with
  four workers, as do the other 25 workspace test tasks. Harness 368, JS host 64,
  root typecheck 44 tasks, lint 28 tasks, Rust bans/format/WASM Clippy and the
  combined WASM build pass. Combined WASM is 9,897,015 bytes. Binance controls
  pass again: main CPU TS/Rust 276.6/157.9 ms, steady 145.3/75.9 ms, retained
  memory 19.7/3.6 MB including 2.0 MB Rust linear memory. Full native and SDK/WASM
  aggregate reruns are finishing before the PR checkpoint.
- PR checkpoint: full post-integration SDK/WASM aggregate passes 343 tests in
  ten files, including all 39 native startup cases and 205 independent consumer/
  metadata cases. Native workspace passes 2,081 tests. No outstanding local
  gate or actionable correctness-review finding remains. The three SDK fixes
  introduce no venue protocol divergence or new `KNOWN_TS_FIXES` entry.
  Push/open the PR into `feat/rust-rewrite`, publish review findings/fixes,
  verify its exact-head CI, merge on green and cut the follow-up branch.
- Opened [PR #1330](https://github.com/Tealstreet/tealstreet-sdk/pull/1330)
  into `feat/rust-rewrite` and posted phase/pre-PR findings and fixes. Final
  production code-correctness review is clear; its documentation clarification
  now states Polymarket's development-only status, disabled chasers and native
  1x rails explicitly. Generic algorithm cases do not enable venue features.
  The native proof/inventory review is also clear after adding the metadata
  conformance file to the caller inventory. Remaining independent PR review and
  exact-head CI are in progress.
- GRVT landed as `5d7589f296` during PR CI. Merge `836bbbec96` preserves its
  fill-verification query hook and all four OKX native-unit flags. The matrix
  now enumerates 40 Rust aliases; native GRVT assertions independently decode
  its recorded Lite instruments and check identity/precision through the worker
  and late snapshot. Inventory is 49 registered / 40 Rust / eight defaults /
  32 opt-in / nine absent names. A seventh deletion blocker records Rust's
  perpetual-only GRVT catalog against TS's other instrument kinds. Independent
  integration review is clear after adding five test-caller rows and fixing a
  table break. Rebuilding combined WASM and repeating affected acceptance gates
  before the final PR checkpoint.
- GRVT acceptance passes: 353 SDK/WASM tests (40 native aliases and 210
  independent consumer/metadata cases), 7,283 SDK unit tests across 502 files,
  five lint-rule tests and the other 25 workspace test tasks. Native workspace
  passes 2,168 tests across 176 suites; harness 386 and JS host 65 pass. Root
  typecheck 44 tasks, lint 28 tasks, explicit fixture lint, Rust bans/format,
  strict WASM Clippy and combined build pass (10,367,335 bytes). Binance public
  and regenerated private controls pass: public main CPU TS/Rust 288.1/173.6 ms,
  steady 162.6/79.5 ms, retained memory 16.9/3.7 MB including 2.0 MB Rust linear
  memory. All independent integration findings are resolved. Push the refreshed
  PR checkpoint, publish this review/gate update and merge only on green CI for
  its exact head.
- BreakoutProp landed as `74b0a7dd14` during the final CI wait. Merge
  `a0413b6520` is clean. Its public mode requires a DXtrade session and uses
  authenticated HTTP polling, so native coverage must prove a real subsequent
  quote poll rather than require a nonexistent websocket. Shared boundary cases
  supply explicit offline replay credentials, preserving the wrapper's guard.
  The automatically enumerated matrix now includes 41 aliases. Refresh the
  inventory, review polling evidence and rerun affected gates before landing.
- Breakout acceptance passes: full SDK/WASM 359 tests in ten files, including
  all 41 real native aliases and 215 independent consumer/metadata cases. SDK
  unit 7,305 tests / 503 files, five lint-rule tests, other 25 workspace test
  tasks, root typecheck 44 tasks and lint 28 tasks pass. Native workspace passes
  2,272 tests / 183 suites; harness 418, JS host 65, and four compiled Breakout
  frame tests pass. Rust bans/format, strict WASM Clippy, explicit fixture lint
  and combined build pass (10,620,599 bytes). Breakout documented public/private
  and regenerated Binance controls pass. Binance public main CPU TS/Rust is
  280.5/153.9 ms, steady 156.0/73.6 ms, retained memory 17.0/3.7 MB including
  2.0 MB Rust linear memory. Fixed the polling fixture clock's availability
  advance and a fragile positional quote assumption; independent re-review is
  clear. Inventory is 49 registered / 41 Rust / eight defaults / 33 opt-in /
  eight absent names, with seven explicit deletion blockers and additional
  live/product scope prerequisites. No newer landing commit at this checkpoint.
  Push the refreshed PR, publish the review/gates and wait for exact-head green
  CI before merging and cutting a follow-up branch.
- All 35 checks passed on `22edf9f141`; the exact-head merge attempt was refused
  because Binance Spot/COIN-M landed concurrently as `e391337054`. Merge
  `3627ddfaab` retains both the new COIN-M sizing flag and all four OKX flags;
  quantity-contract guidance preserves both. Refresh native/generic coverage
  for 43 aliases, independently audit inverse contract multipliers and caller
  inventory, then repeat affected gates and exact-head CI. GRVT's concurrent
  profiling handoff is documentation-only and retained in the merge.
- Binance integration acceptance passes: full SDK/WASM 376 tests / 11 files,
  including all 43 native aliases and 225 independent consumer/metadata cases.
  SDK unit 7,313 tests / 503 files plus five lint-rule tests, other 25 workspace
  tasks, root typecheck 44 tasks and lint 28 tasks pass. Native workspace passes
  2,342 tests / 189 suites; harness 436 and JS host 65 pass. Rust bans/format,
  strict WASM Clippy, explicit fixture lint and combined build pass (10,751,800
  bytes). Independent integration review is clear: COIN-M fixtures distinguish
  native counts, USD notional and base positions, while Spot/COIN-M raw catalog
  oracles preserve both receivers' identity/flags/multipliers. Inventory is
  49 registered / 43 Rust / eight defaults / 35 opt-in / six absent names;
  seven explicit deletion blockers remain. All four Binance public three-run
  controls and regenerated private controls pass. USD-M main CPU TS/Rust is
  188.0/98.2 ms, steady 98.5/36.2 ms, retained memory 17.0/3.7 MB including
  2.0 MB Rust linear memory. Latest landing is integrated at this checkpoint;
  push/review the refreshed head and merge only after its CI is green.

- TxFlow integration updates the current inventory to **49 registered / 44 Rust /
  nine defaults / 35 opt-in / five absent names**. Both production registries
  select Rust for testnet USDC perpetuals; mainnet rejects before I/O and
  production/preview availability remains disabled. Its independent TS replay
  reference is frozen under `__tests__/reference/txflow`; the retained
  production client-ID helper and exact dependencies are in
  [the caller inventory](RUST_CUTOVER_CALLERS.md#appendix-d-txflow-frozen-reference-and-retained-helper).
  The real-venue consumer fixture explicitly supplies `testnet: true` and a
  test-only public-CORS provider, then serves captured venue input without
  altering production guards or seeding the SDK store. Existing matching-socket
  delivery and native ticker-frame assertions remain required. The newly
  integrated combined WASM passes all **six targeted TxFlow tests in two
  files**: five independent consumer cases and one real native startup/frame/
  late-snapshot case. Scoped helper lint and safe-cex typecheck pass;
  independent helper/inventory review has no findings. Earlier checkpoint
  counts and gate results above describe their original heads.

- Hyperliquid/Kraken spot acceptance ([PR #1339](https://github.com/Tealstreet/tealstreet-sdk/pull/1339))
  extends existing crates and retains TS venues/defaults. After the latest
  Lighter/GRVT landing sync the inventory is 49 names / 47 Rust aliases / nine
  defaults / 38 opt-in / two absent products. All ten spot/parent/Binance
  replay gates pass with live public and documented private inputs. Combined
  WASM passes 418 SDK tests including every native worker/late-consumer alias,
  68 host tests and eight compiled spot controls; native touched crates pass
  218 tests, integrated harness 571, and root typecheck/lint/test pass (7,482
  safe-cex tests). Three independent PR correctness reviews are posted and
  their three findings are fixed and independently closed. Exact measurements,
  scope and credential limits: [spot ledger](RUST_SPOT_PRODUCTS.md). CI and
  merge status are recorded on the PR; no TS/default cutover is implied.
