# Phase 9: consumer cutover readiness

Branch: `feat/rust-cutover-readiness`, based on `origin/feat/rust-rewrite`.
This change proves consumer contracts and records the deletion prerequisites;
it does not delete TypeScript venues or enable Rust by default.

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
consumer adoption remain explicit blockers.

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
