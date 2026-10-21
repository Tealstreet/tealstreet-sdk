# Architecture and replay-evidence re-audit

Base: landing `b6cb612bc7c7c611a17dff5900e6977a7e8b5ab8` (#1688).
Scope: the Architecture and Gate honesty sections of `RUST_AUDITS.md`.
Use current code and actual merged PRs; historical audit counts are not a
current inventory. Production impact determines whether a remaining finding
needs a code fix, a cutover prerequisite or an owner decision.

1. Re-verify every Architecture bullet against shared engine helpers, models,
   feature declarations, hosts, venue implementations and tests. Distinguish
   remaining venue-specific semantics from stale duplication counts. Inspect
   manifest/script/config comments and documentation inventory without deleting
   useful evidence or changing trading behavior to satisfy an old shape.
2. Re-verify Gate honesty against current comparison/fixer/measurement policy,
   CI coverage, budgets, tape-time receipts and complete-output proofs. Pin
   merged PR evidence for stale findings. Preserve FAIL versus exact declared
   known failure, repeatability versus parity, incomplete evidence, missing
   metadata, fixture provenance, independent-oracle limits and live prerequisites.
3. Coordinate with codex-mxb4mq, which owns the harness sweep. This first PR
   changes documentation only: this evidence ledger and the two audit sections.
   Do not edit its harness, tapes, declarations, clocks, comparators or policy.
   Its unmerged WEEX follow-up is not landed evidence. Record any remaining
   harness-owned gaps and owner decisions with exact boundaries.
4. After the evidence PR, fix any demonstrated SDK/engine-owned gaps in fresh
   branches, batched by behavior. Write a concrete short plan before each batch,
   reproduce the failure with focused regressions, preserve external contracts,
   run relevant checks and CI, obtain independent reviews, then separate guard
   and exact-head merge. Naming/feature refactors must not change computations,
   command routing, public schemas or WASM budgets. Never delete TS venues or
   executable TS performance references before cutover.
5. Track completed landing-push browser measurements introduced by #1688.
   Once at least three complete real-landing runs exist, examine all samples
   and runner/browser provenance and propose a cold-start threshold to the
   overseer. Do not implement a threshold. Own-head dispatch evidence and
   skipped/cancelled/incomplete runs do not substitute for landing baselines.

Verification: source/history and merged-PR pin checks; focused existing controls
for assertions cited in the docs; links and diff checks; independent plan and
evidence review. No local release WASM builds, replay sweeps, timing matrices,
live trades, deployments, NX edits, budget changes or release automation.
Require green PR checks and a fresh separate trial/revert guard under the lane
rules. Report every actual merge pin to the overseer.
