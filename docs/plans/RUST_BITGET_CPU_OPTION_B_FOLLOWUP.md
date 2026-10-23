# Shared bounded CPU remeasurement, Bitget rollout

## Authority and scope

Sam approved option A then B. Option A is landed. The corrected instruction
selects the shared gate methodology, not a runtime optimization: second-runner
remeasurement, two-run mean, fail closed. Harness/workflow certification edits
are allowed; fixture and tape changes are forbidden. Start from actual landing
`b627e1bf6e9857158e41e14a49e1faa05572efc5` on fresh branch
`feat/rust-bitget-cpu-option-b`. No merge without an assigned window.

Architecture: YES, this is a shared certification concern. Implement one shared
policy/verifier and parameterize it by suite. Enable only the `bitget` suite;
other suites, including `bitgetusdc`, remain strict. Enabling another suite is
Sam's decision. No venue-owned parser, runtime shortcut, profile/threshold change
or private-diagnostic exemption belongs in this PR.

## Policy

1. Preserve the first strict reports, raw outputs, ledger and diagnostic lists.
   A strict PASS needs no second runner. Only a complete public case with CPU-only
   misses strictly below 5% of the existing objective bounds may request one.
   Private cases, missing data, correctness/memory/size failures and a miss at or
   above 5% remain failures. Recompute eligibility from retained raw evidence and
   the existing strict validators; do not trust failure strings alone.
2. Launch exactly one separate runner only for an eligible first miss. Replay
   the same complete case: correctness audit plus all nine interleaved pairs.
   Bind source tree, tape, Node/runtime dependencies, original compiled JS/WASM,
   tools, policy and exact job/attempt/artifact identities. Never rebuild WASM
   for the second measurement. Both runners must have equivalent declared
   environments and identical measured inputs. Missing or ambiguous identity,
   failed second execution, output drift or incomplete evidence means FAIL.
3. Preserve both strict verdicts. For all three CPU objectives, normalize each
   runner's Rust median by that runner's unchanged TS-derived objective limit.
   The arithmetic mean of the two normalized ratios must be <=1 for every CPU
   objective. Never select the better run, mix metrics between runners, average
   mismatched objective categories or ignore non-CPU failures. No third attempt.
4. The final required suite check certifies only an ordinary strict PASS or a
   fully verified two-run decision. Missing collectors, missing expected second
   runs, skipped/failed jobs, rerun ambiguity, invalid uploads and incomplete
   cases fail closed. Preserve existing required check names and every workload.
   Existing private diagnostics remain strict, declared diagnostic results.

## Implementation phases

### Shared policy and strict offline verifier

Reuse the reviewed pure-policy and recorded-evidence design from the parked
option-B work where it fits current source. Export existing CPU objective bounds
without changing failure strings or thresholds. Add a shared policy module and
controls for mean passing, mean failing, second error/missing, first PASS not
requesting a second, 5% boundary, category flip, raw/report tampering and disabled
suites. Re-derive retained reports through the existing strict gate, without
executing another measurement or substituting missing raw slots.

### Bounded workflow orchestration

Keep ordinary suite collectors and private workload commands unchanged. Only
the enabled suite has a deferred final required check; other suites retain their
existing strict jobs. Collect the first result, emit a typed eligible plan,
conditionally execute a single second-runner job, then verify the final decision
with `always()` semantics. Retain exact first/second artifacts separately and
bind physical runner IDs and original attempt1 jobs; rerun/duplicate evidence
cannot authorize a third measurement.

Use a bounded same-run coordinator rather than importing the parked all-history
API scanner and its unresolved quota blocker. Per workflow cohort, there is one
first measurement and at most one second; automatic and per-job retries cannot
create additional candidates. An explicitly requested fresh workflow remains a
new CI run, with its own immutable evidence, rather than a way to substitute one
sample into an earlier failed pair. Do not claim lifetime same-source quota
proof from incomplete GitHub retention.

Restore/reuse original compiled package bytes and verify identical source/runtime
inputs before a second measurement. Any dependency installation or loader cache
handling must be explicit and content-checked; no silent runtime fallback. Review
this transport/provenance boundary before wiring workflow execution. Preserve
current source/package schemas unless a separately reviewed extension is needed.

### Verification, measurement and draft

Pin baseline landing Rust run `39953368561` to the base above; its Bitget and
BitgetUSDC physical jobs are `120413470535` and `120413470400`. Capture exact
artifact IDs/digests, source fingerprints, raw outputs, tools and budget reports.
Cancelled/failed/incomplete evidence is retained as such, never called PASS.

Compare candidate first-run Bitget public nine-pair steady/process medians and
private correctness-audit steady/process CPU against baseline. Private metrics
are informational single samples, not public-style throughput medians. Include
Classic/inverse private diagnostics and USDC control workloads; the USDC suite
stays disabled for rescue. Record sample counts, all first results and actual
second-run execution or its correctly skipped state. This policy does not claim
a runtime speedup. Compare complete canonical nonmetric outputs after removing
only top-level metrics, with all ordered diagnostics retained. Report optimized
WASM size and headroom; build arguments remain identical.

Fix stale build.sh comments to name Bitget among the landed speed defaults;
change no argument. Add as-landed CALLERS and nearby audit/quality docs, keeping
CI status in the PR body. Run focused policy/provenance/workflow controls and root
typecheck/lint/tests. Obtain independent correctness/adversarial reviews before
publishing the draft. No fixture/tape edits, gate threshold changes or merge
without a window.
