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

A is the authenticated checkout commit's first parent, with the entire B tree
independently recomputed from A and the exact PR head. API and local Git must
agree on the checkout tree and exactly two parents; parent two is the run's PR
head. Parent one must be an ancestor of the authenticated live target and lie
on its first-parent landing chain; fixed A0 must be on that same chain. The target comes only from the base repo's
GET git/ref/heads/feat/rust-rewrite; the fetched local ref must agree exactly.
The run-associated base.sha, pull-metadata base.sha and GraphQL baseRefOid can
all be stale: log all three alongside the live target and selected parent,
without gating ancestry on any metadata SHA. A single-parent PR-head checkout
cannot qualify. GraphQL errors are recorded and cannot reject a selection.
If the landing ref advances between its API read and fetch, the disagreement
refuses; only a new authorized run can collect a fresh consistent snapshot.
Landing push runs use the
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

A0 is 38b69912e461e27198f5252c98cef73b7521b169, a post-#3091 master landing
with the current public Bitget amount limits. Only an explicit auditor GO with evidence may move
this source pin. There is no PR, CLI, workflow-input, report, or automatic
advance path. The coordinator derives anchor authority from the issued tip
comparison authority and the checked-in fixed pin; uploaded evidence cannot
issue either authority.

The previous anchor was 518d3953c09daf63c1345ea465bbc55bd313f53a. Its TS output
predated the conservative market cap, so its anchor preflight refused certification.
The approved reset first measured Rust-only drift on one EPYC 7763 runner in
[run 42125492893](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42125492893):
64 samples per arm, A/B/B/A then B/A/A/B, common tape and Node runtime.
The old median was 449.9615 ms; the new median was 458.1105 ms, ratio 1.0181104383
(+1.811%), below 1.023110. This is the **drift accepted at reset** baseline;
correctness was known-changed, and that diagnostic does not certify CPU.
The calibration freeze, tolerance, R0, and both normal authenticated witnesses
remain unchanged. Fresh normal paired CI must qualify the new pin.

The pinned A0 source is checked out for its TS preflight and BUILD. Its own checked-in build.sh,
tool pins, family profile, size checker and ci-wasm archive producer build and
seal its package. No current runtime source is copied into that checkout. The
external driver retains source seals, raw/web/node artifacts, metadata bytes
and file hashes before changing checkout.

All sixteen MEASUREMENTS execute from clean tested B source: the same replay.mjs,
gate.mjs and certifier, TS source and host helpers, tape/books and Node runtime.
The producer records B's full measurement tree, explicit gate/replay/policy/seal
file blob hashes and the Node SHA256 in the plan and every cohort, checking them
before and after each observation. The certifier derives those expected file
hashes from its own trusted candidate checkout, not an uploaded declaration.

A separate CI-only public Bitget nine-pair module contract validates each old
archive against its selected source git-tree fingerprint, original run, pinned
tools/flags and actual package bytes. Normal --ci-wasm validation still requires
the current HEAD build fingerprint. Selecting an old package does not authorize
acceptance: the independent tip/anchor provenance and both pooled contrasts
remain mandatory. No replay timing, sampling, arithmetic or threshold changes.

The candidate is independently rebuilt in both comparisons; its package hashes
must agree. Every cohort uses one canonical module path. Arm-specific code is
only compiled Rust/WASM and its sealed JS package; measurement code is common B.

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


## Changed TS oracle output

Before either comparison builds or measures Rust, a bounded TS-only replay runs
from each authenticated source checkout against the identical candidate tape,
books and Node executable. The original outputs, source/tree and runtime seals,
logs and tape digest are retained. The candidate checkout is restored on failure.
Every published replay field is compared structurally, excluding only `metrics`.
Source edits with identical output do not suppress the existing certification.

If the outputs differ, `steady-assessment.json` records `NOT-CERTIFIED`,
`certified: false` and `certificate: null`; the paired job exits nonzero. It grants
neither PASS nor an unchanged-bytes exception. This CPU job remains non-required;
required replay correctness, family sizes and coverage retain their own gates.
A failed TS replay is a collection error, not an oracle-change disposition.

The CPU planner has a separate Bitget-only reader for this failed producer.
It requires the original same-run physical job and checkout, exactly the failed
oracle step with its literal refusal, a successful sealed upload, and a matching
receipt whose raw TS outputs actually differ outside metrics. Cancelled jobs,
other failed steps, malformed evidence and metrics-only differences fail closed.
An anchor refusal also revalidates the completed tip's original raw records.
Successful artifact authentication remains unchanged for every other path.

The planner still authenticates and revalidates Bitget's complete strict first
measurement, including correctness and memory. With a valid oracle refusal it
records `not-certified`, binds the refusal artifact, and schedules no second
measurement. The certifier recomputes that plan and writes a refusal, never a
Bitget certificate: `certified=false`, the Bitget replay gate remains
NOT-CERTIFIED, and the other 31 suites can certify independently. A corruption
or memory failure is not converted into an oracle refusal. The post-#3091 anchor
reset removes the persistent historical mismatch; this remains defence in depth
for a later legitimate published-output change.

This guard prevents baseline Rust from being judged against changed candidate TS
output. It does not establish arm-matched correctness: a future change must pair
each Rust revision with its corresponding TS oracle before CPU comparisons of
changed published output can certify. When TS output is unchanged, all existing
strict baseline/candidate checks and frozen CPU thresholds still apply.

## Frozen anchor decoder compatibility (2026-11-10)

The cumulative source remains `38b69912e461e27198f5252c98cef73b7521b169`.
No anchor was regenerated or advanced, and no CPU threshold, calibration,
sample count, attempt rule or WASM budget changed.

The Position revision-2 migration in `7a25d90ce57e6047da765834b8d08821da1c3af9`
made the inherited frame reader reject revision 1, including empty sections.
The frozen anchor's `Store::write_positions` emits revision 1 even on the public
tape, where there are zero positions. Paired measurement builds each source's
WASM but runs both arms through the candidate replay and decoder. Thus a
byte-changing PR could pass all tip cohorts and then crash at the first anchor
cohort before a report or certificate existed.

Evidence is retained in [run 42759306820, paired job 129647202437](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42759306820/job/129647202437):
tip correctness was exact and pooled q was `1.0012688419761406`; anchor cohort 1
produced no report. In its `bitget-paired-steady` artifact (ID `13738342461`),
`anchor/records/01-baseline/gate.log` shows `Frame.positions()` throwing
`unsupported POSITIONS revision; expected 2` from the inherited decoder.
The artifact records anchor Node WASM SHA256
`ca011944b1b29fb293635da4af0b1eb31dd04fe61cc4c318aef9077f3adf61a6`
and candidate SHA256
`7fa02a4654cbb5cfd042c2d56efe2f95bbf95df03fc53803b147ffe69f2b698e`
(candidate checkout `d1c9dd6e85337a38cf0696ca7075287388bfae5a`).
These identify existing CI builds, not newly regenerated anchors.

[#3317's original attempt, job 129627843811](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42755059348/job/129627843811),
also completed the tip comparison then failed at anchor cohort 1 with no report.
Its later rerun fails the separate original-attempt requirement; that requirement
is unchanged, and validation after this fix requires a new workflow run.

The [Bitget CPU gate, job 129664357479](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42759306820/job/129664357479)
then fails closed with `CPU cohort: missing exact refusal inside failed step`:
a decoder crash cannot be authenticated as the distinct TS-oracle refusal.
[Master run 42738292322, paired job 129573360013](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42738292322/job/129573360013)
at `2337705f81a4c9ef2e79c0082d510369202ab48b` reported
`unchanged bytes, not measured`, which skips the anchor and masks the defect.

The common replay protocol now recognizes only the canonical empty revision-1
POSITIONS payload for a public Bitget tape: exactly eight bytes, a zero count,
and zero alignment padding. It represents zero rows without decoding historical
position values or modifying the frame/WASM bytes. Nonempty, truncated,
noncanonical and other-revision payloads still use the strict decoder, as do
all private tapes and other venues. Revision-2 decoding is unchanged.
The helper and inherited wire decoder are included in the common measurement
source seals, and the protocol digest records this compatibility rule. Both
measured witnesses, exact audited outputs and every existing failure check
remain required when the module bytes change.

Regression coverage decodes both empty revisions before issuing a byte-changed
build's combined tip/anchor certificate, and retains the existing slowdown and
missing-witness refusal controls. A replay of the retained anchor WASM with this
protocol completed 39,457 frames, 846 markets, 846 tickers and two books with no
protocol errors. Every published field excluding metrics matched the retained
byte-changed candidate's correctness output from the same CI artifact.

The Bybit gate is independent: [job 129664357448](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42759306820/job/129664357448)
refused the Hyrotrader public case because Rust remained slower than TS after
the in-job remeasurement. [Master job 129590732402](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/42738292322/job/129590732402)
failed for that same case while Bitget passed. This supports a separate
performance/runner-variation issue; it does not prove that every Bybit miss is
noise. The Bybit gate and its CPU limit remain intact.
