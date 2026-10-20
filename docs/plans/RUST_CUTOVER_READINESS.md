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
