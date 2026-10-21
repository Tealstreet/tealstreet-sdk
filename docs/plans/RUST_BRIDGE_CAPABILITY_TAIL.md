# Rust bridge capability tail

Base: `0cd8f2b1657` on `feat/rust-rewrite`. Scope: RUST_QUALITY item 8 tail.

Current inventory: 36 explicit venue predicates in `rust.exchange.ts`: 21
literal equality/inequality expressions, 11 array membership expressions and
four `okxNames`/`orderlyNames` membership expressions. Count expressions, including
both demo comparisons; exclude venue registries, metadata and keyed dispatch
tables. Client-ID rule ownership and hosted TS signer loaders stay separately
scoped. FrameApplier already takes only `store`; quantity policy comes from
MARKETS flags. Its `applyBooks` switch controls forwarding, not venue policy.

1. Preserve synchronous construction through the existing shared declarative
   construction rules: primary-only read credentials, spot/demo identity and
   signing transport selection. Keep additions compact: the generated rules
   ship in every family. Verify native/TS parity before engine startup and on
   worker mirrors; no separate venue-name fallback.
2. Add engine capability flags/enums by behavior: journal/closed-row formats,
   history bounds and supported filters, trading/STP/execution validation,
   account validation, and isolated book subscriptions. Replace all 36 explicit
   predicates with those policies. Preserve existing messages, method exposure,
   validation order, worker forwarding and runtime outcomes. Preserve existing
   wire discriminants; append capability encoding only where needed.
3. Regress behavior with capability records independent of venue names, cover
   pre-start and worker consumers, and keep existing regression assertions.
   Verify the predicate count reaches zero. Do not rename `contractsNative` or
   `remainingNative`, move their computations, or rewrite FrameApplier sizing.
4. Run focused Rust tests/Clippy/fmt (CARGO_BUILD_JOBS=4), SDK typecheck/lint and
   bridge/wire tests. CI owns release builds, all-family size budgets (especially
   Bybit headroom), combined WASM and replay gates. Independent reviews per
   batch; document tealstreet-next impacts for the caller-doc lane in the PR.
   Integrate current landing as required, merge only green after a separate
   revert guard, and report the actual merge pin to the overseer.

Overseer approved runtime capabilities plus shared pre-engine construction
rules, with no second name fallback. No downstream application/deployment writes.
