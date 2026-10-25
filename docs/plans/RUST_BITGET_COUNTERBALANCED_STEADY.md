# Bitget public steady CPU certification

Only the public Bitget family steadyCpuMs objective uses this supplement.
The original strict reports remain unchanged. USDC, combined-layout replay,
other CPU objectives, memory, correctness, known failures and size keep their
existing policy. This introduces no venue, engine, build-profile or budget edit.

The paired job runs two complete comparisons serially on one runner: trusted
landing A versus complete candidate B, then pinned cumulative anchor A0 versus
the same B. Each comparison measures A/B/B/A followed by B/A/A/B. Each cohort retains
nine alternating TS/Rust pairs plus the original audited correctness pair.
Before each cohort it restores the selected independent package to the same
canonical module path, verifies every source/package seal and records a physical
log window. Missing, stale, partial, duplicate or retried evidence fails closed.

A is selected from the authenticated original run base snapshot, with the entire B tree
independently recomputed from A and the exact PR head. Landing push runs use the
authenticated actual first parent. No workflow input or report may choose A or
the tolerance. Later target landings cannot move that issued A. A changed PR
head, source tree or physical producer invalidates the result. Final
certification re-authenticates the unique same-run attempt-1 producer, upload,
ZIP digest, physical logs and all sixteen windows. It re-derives every original
strict report from retained raw output and binds B's package hashes to the
original family runtime capsule. A family result cannot authorize a combined
module. The original strict FAIL remains visible when replaced by this narrow
steady certificate; an unresolved collection is never final certification.

Pool the 36 Rust samples and 36 TS samples separately for each arm:

    rA = median(Rust_A) / median(TS_A)
    rB = median(Rust_B) / median(TS_B)
    q = rB / rA

The per-PR blockers are q_tip <= 1.023110 AND q_anchor <= 1.023110, with
equality accepted. Missing or failing either witness blocks certification. This frozen cutoff
changes only through auditor GO. REQUIRED_IMPROVEMENT remains 0.10. The rB <=
0.90 requirement is reported with the recorded CPU model as cutover readiness;
it does not decide the per-PR non-regression result. EPYC 7763's approximately
0.92 readiness ratio remains performance work owned by codex-5c57jl.

## Pinned cumulative anchor and job cost

A0 is 518d3953c09daf63c1345ea465bbc55bd313f53a, the actual landing used by
the counterbalanced study. Only an explicit auditor GO with evidence may move
this source pin. There is no PR, CLI, workflow-input, report, or automatic
advance path. The coordinator derives anchor authority from the issued tip
comparison authority and the checked-in fixed pin; uploaded evidence cannot
issue either authority.

The old A0 source is checked out directly. Its own checked-in build.sh, tool
pins, family profile, size checker and ci-wasm archive producer build and seal
its package. No current runtime source or helper is copied into that checkout.
The external collection driver retains source seals, raw/web/node artifacts,
metadata bytes and file hashes before changing checkout. Each cohort executes
that arm's checked-in gate and validates its independent module package. The
candidate is independently rebuilt in both comparisons; its package hashes
must agree. Every cohort uses one canonical module path, and the original
candidate checkout is restored before final authority authentication.

Layout is tip A/B/B/A/B/A/A/B, then A0/B/B/A0/B/A0/A0/B: sixteen cohorts,
144 alternating performance pairs and sixteen complete audited pairs, four
source builds with immediate target cleanup. This approximately doubles the
single-comparison runner time and build cost. One serial physical producer
runs under the existing 45-minute job limit; there is no parallel measurement,
extra gate run, adaptive wait, sample selection or retry. Each comparison pools
36 samples per implementation per arm independently. Both must pass the same
frozen T; their q values are never averaged together. The same physical job,
candidate, tape, CPU, module hashes and canonical path bind the two witnesses;
their measurement windows must be disjoint and ordered.

Controls include an identical A0=B comparison, a passing tip with failing
anchor, a failing tip with passing anchor, missing/copied witness refusal,
foreign producer and overlapping-window refusal, and PR-supplied pin rejection.
The identical-source control validates the acceptance path with recorded
fixture samples; the qualifying PR run supplies actual old-source build and
measurement evidence. No study result or tolerance is refitted for this guard.

## Calibration and detection limits

Fixed ABBA alone falsely rejected identical source because the third position
was slower under either label. The accepted statistic counterbalances both
labels at every position; it does not average cohort ratios or discard samples.

| Evidence | Run | Pooled q | CPU model |
| --- | --- | ---: | --- |
| Original pilot | 40220271900 | 1.008097143161 | EPYC 9V74 |
| Calibration 1 | 40222646682 | 0.981890917786 | EPYC 9V45 |
| Calibration 2 | 40223419224 | 1.006495594496 | EPYC 7763 |
| Calibration 3 | 40224312654 | 1.010431488528 | Xeon Platinum 8370C |
| Sealed holdout | 40225426856 | 1.002647515417 | EPYC 9V45 |
| Fixed 16 ms live injection | 40226080446 | 1.034847863364 | EPYC 7763 |

E is max(abs(q-1)) over the pilot and three calibrations: 0.018109082214268435.
T is ceil((1+E+0.005)*1e6)/1e6 = 1.023110. It was frozen before the holdout,
which passed both q <= T and rB = 0.808225598423 <= 0.90. The offline B-only
5% slowdown gives q = 1.052779891188 and is rejected. The live fixed 16,000 us
thread-CPU burn also exceeds T. Neither the injection nor scratch workflows
are included in the production gate.

The observed live Rust median increase was 23.1415 ms, versus the fixed 16 ms
burn. Overseer and auditor ruled it valid: the excess 7.1415 ms lies within
E times A's 467.105 ms median, approximately 8.46 ms. The initial magnitude
wording was tighter than the noise it measured against. The observation and
the explicit validity ruling are retained; no delay, tolerance or sample was
adjusted and no run was retried.

The empirical noise envelope plus tolerance implies a detection floor around
(T-1)+E = 4.12%. Smaller regressions may pass. Four calibration jobs and one
holdout do not establish a statistical tail guarantee across future runners.
The cumulative anchor comparison prevents a series of accepted relative changes
from silently accumulating beyond the same frozen tolerance against the anchor.
It does not guarantee detection of smaller regressions within that tolerance.

Frozen calibration bundle SHA256:
ee6b8fd762184969b2eacf7a5ef030bc8872acfbc474ca229377f1b181e99b62.
Completed study record SHA256:
58af9bde4e2e3ab74c562b6ef37a6141c7248edb82640f07cd30b59c5c0ba251. The auditor re-approved implementation after
reviewing the holdout, both detection controls and the explicit live ruling.

Independent integrity review, one qualifying PR run, and the auditor's exact
tree GO remain mandatory. The owner performs the guarded head-matched merge.
