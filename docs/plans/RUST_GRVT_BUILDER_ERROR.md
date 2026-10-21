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
