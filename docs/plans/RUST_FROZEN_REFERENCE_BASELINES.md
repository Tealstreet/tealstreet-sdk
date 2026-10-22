# Frozen TypeScript references and Rust regression baselines

Status: read-only audit and implementation proposal. No harness, workflow,
package or venue implementation changes are authorized by this document.
Source inventory begins at actual landing
`d05c5ede4b86409e1a8d9798e64b79b1c1c888f4` (2026-10-23).

## Work plan

1. Inventory each live TypeScript execution dependency in CI replay/parity,
   native protocol oracles, audited fixers, recorder/input generation, performance
   and memory measurements, and supporting package tests. Distinguish executable
   TS venue code from ordinary TS infrastructure that remains after venue deletion.
2. Define an immutable, content-addressed frozen-reference bundle: exact tapes,
   case roster/options, complete raw outputs and per-case outcome/gate evidence,
   original TS source/tool/build provenance, accepted corrections/variants and
   integrity/retention rules. Preserve failed cases and gaps; a freeze is no pass.
3. Define independent Rust-vs-frozen-TS compatibility and Rust-vs-Rust regression
   checks, including checkpoint/command/request evidence, declared behavior changes,
   new-case bootstrap, performance policy and fail-closed source/schema binding.
4. Sequence baseline capture, dual-run validation, corruption/negative controls,
   immutable publication, TS-free rehearsal and eventual venue/reference deletion.
   Keep the long-lived frozen TS rollback package independent from active support.
5. Obtain independent correctness/adversarial plan review and record bounded
   verification plus open decisions. Any later harness implementation is a separate
   authorized PR with original-RED controls; this phase changes docs only.

Maintain all five history CI/proof watches and the #1769 merge hold/queue while
preparing this proposal. Do not run local release WASM, full replay, size or timing
sweeps. Corepack Yarn only; never bare/Volta Yarn, npx or the account-policy check.
