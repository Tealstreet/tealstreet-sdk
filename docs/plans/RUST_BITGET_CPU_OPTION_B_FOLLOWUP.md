# Shared bounded CPU remeasurement for every CPU-gated suite

## Authority and scope

Sam approved option A then B. Option A is landed. The corrected instruction
selects the shared gate methodology, not a runtime optimization: second-runner
remeasurement, two-run mean, fail closed. Harness/workflow certification edits
are allowed; fixture and tape changes are forbidden. Start from actual landing
`b627e1bf6e9857158e41e14a49e1faa05572efc5` on fresh branch
`feat/rust-bitget-cpu-option-b`. No merge without an assigned window.

Architecture: YES, this is a shared certification concern. Implement one shared
policy/verifier and parameterize it by suite. Sam subsequently approved enabling
it for every CPU-gated suite, including `bitgetusdc`. Derive the enabled set from
the shared exact suite registry; unknown suites and private-only cases remain
ineligible. No venue-owned parser, runtime shortcut, profile/threshold change
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
4. The final required suite check accepts an ordinary strict PASS, an existing
   declared diagnostic result accepted unchanged by the strict manifest policy,
   or a fully verified two-run CPU decision. Declared public/private FAIL reports
   remain FAIL and never receive correctness rescue or CPU averaging. Missing collectors, missing expected second
   runs, skipped/failed jobs, rerun ambiguity, invalid uploads and incomplete
   cases fail closed. Preserve existing required check names and every workload.
   Existing private diagnostics remain strict, declared diagnostic results.

## Implementation phases

### Shared policy and strict offline verifier

Reuse the reviewed pure-policy and recorded-evidence design from the parked
option-B work where it fits current source. Export existing CPU objective bounds
without changing failure strings or thresholds. Add a shared policy module and
controls for mean passing, mean failing, second error/missing, first PASS not
requesting a second, 5% boundary, category flip, raw/report tampering and unknown
suites. Re-derive retained reports through the existing strict gate, without
executing another measurement or substituting missing raw slots.

### Bounded workflow orchestration

Keep all 32 ordinary suite collectors and all 110 workload commands unchanged.
Every suite has a deferred final required check with its existing check name;
initial jobs use measurement names. A single planner authenticates all complete
first suites and emits an exact dynamic matrix containing only suites with an
eligible public miss. One second physical job per selected suite replays only
that suite's eligible cases, once each. No second runner is scheduled if the
first cohort has no eligible miss. One shared post-second certification job verifies every suite with `always()`
semantics, including missing/failed planner or second results. A 32-suite matrix
retains required check names and accepts only the successful same-run, same-
checkout complete certification output; these wrappers make no evidence API calls. Retain exact first/second artifacts separately and
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
Classic/inverse private diagnostics and USDC control workloads; private cases
remain ineligible while the USDC public case uses the same shared policy. Record sample counts, all first results and actual
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

## Policy checkpoint

The initial plan review confirmed architecture YES for the shared same-run design.
The all-suite rollout is a subsequent explicit Sam decision; obtain a separate
review of the complete dynamic matrix and all-suite final check coverage. The
first implementation reuses only reviewed pure policy/offline strict-verifier
primitives; the parked history scanner, workflow and coordinator are absent.
Focused controls cover all-three-objective means, missing/failed second runners,
unchanged strict failures, 5% boundaries and disabled suites. Runtime transport
and workflow authority remain the next separately reviewed phase.

## All-suite rollout review gate

Before changing rollout or workflow source, independently review the expansion
above. Plans must contain exactly the shared registry suites, once each; public
case rosters must match each suite (including multi-public Binance/Bybit/OKX).
Artifact names include the suite except the single global plan. The second
matrix must be an exact projection of eligible dispositions, never a caller
provided list; an empty projection skips the whole second matrix. Certification
recomputes the authenticated global plan and validates its complete suite roster
before selecting one row. Every required replay check runs even if the planner
or a second job fails; no eligible slot may disappear through matrix skips.
Preserve all strict private/declared public diagnostic handling; no declared
correctness divergence can be rescued by CPU averaging.

## Evidence request transaction

Planner, each selected second runner and the shared certifier take one complete
same-run job/artifact inventory at entry and a fresh complete inventory at exit.
Exact authenticated bundles and completed physical logs are reused within that
transaction; current metadata is never replaced by a prior-run receipt. The exit
snapshot rejects new duplicate names, changed artifacts/producers or a workflow
attempt change. Final runtime authority checks the canonical current artifact
metadata/upload and the restored manifest retained by the successful second job
against the original premeasurement manifest, avoiding a second runtime download.
The second job still downloads and validates every runtime byte before timing.

The request-budget control exercises the production paginated reader and upload
adapter with every registered suite, extra job/package rows and all-second worst
case. It includes initial preparation, planner, each selected second, shared
certification and fresh boundary checks. Expected totals are310 requests for no
second and886 for all32 seconds, below the ordinary1000/hour repository token
limit before concurrent activity. This is not a reservation against other CI;
rate-limit, unavailable evidence and HTTP errors fail closed. No history scanner
or token-limit exemption is introduced.

## Workspace binary closure

The initial live preparation exposed Yarn's workspace CLI binary link into
`apps/cli/bin`, which a directory anchor alone could not seal. Include tracked
app source in the conservative runtime projection alongside sdk-rs/packages/
thirdparty. This seals the binary and its ancestor directories without sweeping
ignored app build output. An original-source regression reproduces the exact
unsealed-link error; corrected controls require the bin's bytes and source
fingerprint membership, and reject later mutation before timing. The symlink
validator remains strict; no target exemption or venue-local path is introduced.

### Shallow PR checkout identity

Actions checks out the synthetic PR merge at depth one. Revision walks hide its
parents at that shallow boundary, so the original parent check rejects valid
PR jobs before measuring. Bind the tested head using the actual commit object's
header instead: exactly two parents, tested head exactly the second parent.
The measured checkout remains its own identity throughout artifact and runtime
verification. A real Git shallow-checkout regression reproduces the original
refusal; negative controls reject tested head only as first parent, absent tested
head and single-parent commits. No fetch, ancestry fallback or identity relaxation
is used. The rejected PR cohort stays preserved; corrected-source CI is separate.
