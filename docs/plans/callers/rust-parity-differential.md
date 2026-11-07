### Differential parity: accepted differences and report

Owner: claude-tbz5hr. Design is in
[RUST_PARITY_GAPS.md](../RUST_PARITY_GAPS.md), section "Differential parity:
accepted differences and the parity report".

This change is harness-only (`sdk-rs/harness`). It changes no SDK source, no
engine behaviour and no CI suite membership, so no SDK, NX or server caller
obligation follows.

- **Harness callers:** `assessGateReport`, `assessCorrectnessReport` and
  `verifyGateSuite` load `accepted-differences.json` by default, so
  `verify-gates.mjs`, `cpu-ci.mjs` and `bitget-paired-ci.mjs` share one
  classification. Assessments gain an `accepted` list. `verifyGateSuite`
  returns `acceptedGates`.
- **Later venue PRs:** an accepted entry and any `known-gate-failures.json`
  removal for the same tape land together, in the venue's PR.
