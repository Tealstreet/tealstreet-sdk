# Re-verify the Rust cutover audit tail

Base: `bba1f43126cd` (landing after SDK B11 #1666). Re-check the eight
Cutover bullets in RUST_AUDITS.md against current source and existing tests.
No TypeScript venue deletion, NX writes, deployment or release is authorized.

1. Commit this short plan first. Audit socket health/recovery and CLI stale-stream
   refusal; venue methods and capability refusals; downstream BloFin identity
   checks and deletion deep imports; MCP packaged engine delivery; WASM budgets,
   layout and cold-start evidence; replay golden-fixture prerequisites.
2. Attach exact source, regression and actual merged-PR evidence to each stale
   finding. Preserve narrower unsupported-product, delivery and downstream
   requirements instead of inferring complete cutover readiness.
3. Request a read-only NX re-check from codex-cxxyqr for caller-owned gaps.
   List owner decisions separately for claude-ddi2il. Keep the audit evidence
   distinct from deployed-runtime or npm-publication verification.
4. Land one docs-only PR striking stale Cutover findings and listing the exact
   remaining SDK/NX/owner work. Independently review correctness/adversarial
   claims, run focused documentation checks, require green CI and a fresh
   separate guard, then merge and report the actual pin.
5. Fix any remaining SDK-owned defects in separate behaviour batches on fresh
   landing branches, with regressions reproducing the failures before each fix.
   Use focused local checks; CI owns release WASM builds and replay/timing
   matrices. Preserve lane priorities: shared source merges hold for Kraken
   #1663's current proof cycle. Apply normal integration/trial and guard rules.
6. Report every merge to claude-ddi2il and precise caller follow-ups to
   codex-cxxyqr. Stop when the assigned SDK scope and evidence refresh land;
   owner decisions and NX implementation remain explicit handoffs.
