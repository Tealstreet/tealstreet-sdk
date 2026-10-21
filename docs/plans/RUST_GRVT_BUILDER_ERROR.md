# GRVT typed builder authorization failure

Re-verified after the Cutover-tail audit: native builder retries and FIFO blocking
already work. Definitive Unauthorized/CapTooLow still become strings; the native
host and worker command allowlist cannot carry the legacy UI code/reason/caps.
No default NX/CLI/MCP REST-book or dead-man producer was found. WEEX #1675 is merged at
22c6c6133f2; this batch follows it as approved. REST books and the Hyperliquid
dead-man API are assigned separately to codex-5c57jl.

1. Extend engine VenueError with one optional boxed typed builder record (stable
   code, closed reason, requiredRaw and optional authorizedRaw). Ordinary error
   JSON and legacy text remain unchanged. Construct it only from the definitive
   GRVT placement arms, preserving validation order. Retain it through planning,
   amend inspection and failure/outcome adjustment; no new retry or writes.
2. Validate the full record at the JS ExchangeError boundary and expose the
   familiar flat GRVT_BUILDER_UNAUTHORIZED/reason/caps. Add these explicit fields
   to SDK command serialization for structured clone and hosted JSON. No arbitrary
   error-property copying, class-prototype reconstruction or new dependencies.
3. Regression: unauthorized and low-cap placement, amendment inspection before
   cancellation, ordinary validation precedence, unknown/pending/network/malformed
   controls, exact native serialization, real host rejection and worker sender/
   receiver round trips. Preserve missing/unreadable-cap behavior and all existing
   no-write, partial and unknown-outcome assertions.
4. Update adjacent specs and B8/Cutover evidence. NX current live handler accepts
   only instanceof GrvtBuilderUnauthorizedError: REQUIRED downstream migration to
   a validated structural code/reason/caps handler, preserving cleanup/rethrow and
   modal/toast behavior. Do not edit NX. Other manual APIs and browser timing stay
   open; executable TS references remain through cutover.

Validation: focused engine/GRVT/Binance tests, JS host and SDK RPC regressions,
SDK typecheck/scoped lint, touched native Clippy/fmt; CI owns full replay/timing,
WASM size and compiled consumers. Independent correctness/adversarial review;
short plan first, fresh landing branch, green CI, separate exact-file revert
guard, merge commit and actual-parent/tree audit; report pin to overseer/callers.
No local release WASM builds, deployments, budget increase or TS deletion.

## Regression evidence

The native placement regression failed against the unchanged implementation:
`builderAuthorization` was absent, before any order write. Both real JS-host
rejection cases also failed for missing code; the eight initial SDK/RPC cases
failed for lost fields or unvalidated reserved codes. After implementation, all
41 GRVT trading cases pass, including both reasons across placement and amend
inspection, earlier validation precedence, pending/exhausted proxy failures and
malformed builder-list controls. Missing/unreadable caps retain their existing
allow behavior. Exact native JSON keeps ordinary-error and legacy text intact.

Focused engine/GRVT/Binance tests pass (667 tests across 39 targets including
zero-test doc targets); the shared host suite passes 146 tests and SDK error/RPC
suites pass 44 tests. Typecheck passes. CI must still check the full dependency
matrix, compiled transports, replay/timing and fixed WASM budgets before merge.
No local release WASM build or NX runtime test supplies these claims.
