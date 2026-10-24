# CPU producer step metadata convergence

Owner: codex-mxb4mq. Reviewer: claude-dazzy3 (overseer routing).
Base: actual landing `49f7d1857b66fc14f5164c8ca82ce42d48a53c1d`.

## Problem and evidence

GitHub can report a measurement job completed/success while its step list
still contains pending/in-progress steps and null conclusions/timestamps.
`evidenceInventory` caches that response. `authenticatedBundle` then rejects
the exact upload step through `successfulStep`, although its API metadata
becomes terminal shortly afterwards. This rejects the planner before a CPU
verdict and cascades into the certification wrappers.

- #1747, run40112254296: retained jobs response SHA256
  `55f58c320b0fcb3bfd70259d6768f7e35bafd6e16690fe7acfb8b54a4dc803f2`,
  `/tmp/codex-metadata-vec-cpu-plan-evidence/provenance`. All32 measurement
  jobs are completed/success/attempt1. OKX120926583857 and
  Binance120926584119 still have pending upload steps53–55.
- #2076, run40111709129: producer120924624369 has strict upload step55
  pending/null in the retained snapshot, completed/success in the later API
  observation. Receipt `/tmp/codex-zs5e83-pr2076-cpu-plan-infra-retry.json`.
  Its planner retry produced attempt2 aliases; this plan does not accept them.
- #2070/#2071 also reported by the overseer; no independently captured raw
  evidence for those two is claimed here.

## Proposed shared boundary

Change only `sdk-rs/harness/cpu-ci.mjs`, with scoped Node controls and this
plan/CALLERS. No workflow, policy threshold, replay fixture/tape, venue, engine,
measurement-attempt, or artifact format change.

Before `authenticatedBundle` authenticates/binds an artifact, resolve its unique
producer from the complete inventory. A producer must already be the original
successful completed job. Require the requested upload step to exist exactly
once. Missing/duplicate producers or upload steps, failed/cancelled jobs,
wrong source/runner/run, and any attempt other than1 fail immediately.
Artifact retrieval retains its existing order and bounded byte checks. A
download is not accepted as authority until producer convergence and all
existing authentication checks pass. Require its full artifact metadata to
remain identical in the refreshed inventory too.

If any producer step has a nonterminal status or null conclusion, wait for
metadata convergence. All steps must have `status: completed` and a non-null
conclusion before binding the whole job record. A completed skipped unrelated
step is terminal; the required upload/checkout steps must still pass the
existing success/timestamp checks. A completed step whose conclusion is null
remains nonterminal. This also avoids binding a partially populated post-step
list that would later differ at `evidenceEndCheck`.

Use the existing uncached complete run/jobs/artifacts API reader to refresh
`context.inventory`: at most6 refresh cycles per command/context, with5s
spacing (at most30s scheduled delay, in addition to existing transport time).
The budget is shared across suites/artifact bindings, not multiplied by32.
Do not poll the active planner itself. Do not retry HTTP refusals, malformed
responses, failed provenance checks, or measurement jobs.

Pin explicit producer identity fields before waiting: `id`, `run_id`,
`run_attempt`, `runner_id`, `name`, `head_sha`, `head_branch`, `status`,
`conclusion`, `started_at`. Pin run `id`, `run_number`, `run_attempt`,
`workflow_id`, `path`, `head_sha`, `head_branch`, `event`. This is dazzy3's C2
choice; late `completed_at`/labels may settle, but the final strict validator
still requires valid completion timestamps and exact whole-record bindings.
Recheck identity, unique job name/id and attempt1 on every refresh. Replaced jobs,
duplicate/attempt2 aliases or changed identity fail, including the aliases
observed after #2076's retry. Replace the complete cached inventory with each
fresh snapshot; never splice fields from different observations. Retain every
distinct response's raw bytes/digest through the existing reader. The final
strict authority and end-of-command checks remain unchanged.

After convergence, use the terminal record consistently for upload/checkout
authentication, returned bundle, measurement-window checks and bindings.
Keep exact artifact id/size/digest/producer/ZIP checks, logged checkout and
step windows unchanged. On exhaustion, fail explicitly with run/job/name,
pending step names/statuses/conclusions and the6-refresh cap; do not cache an
accepted bundle or mint plan/certificate/matrix output.

## Originals and controls

Tests call the existing exported `authenticatedBundle`, with deterministic
reader snapshots and Node's virtual timers. No absent API/import failure is
an original RED. Metadata-only binding exercises the same shared producer and
upload authenticator; existing ZIP/digest controls remain required separately.

Original assertion REDs:

1. Pending upload in a completed successful producer becomes terminal on the
   next same-job snapshot: accept only that fresh record and bind it.
2. Upload already successful, another producer step still in progress: wait
   for the whole job's terminal metadata rather than binding the stale record.
3. A producer step never becomes terminal: exhaust the bounded refresh budget
   with the explicit pending-step failure instead of accepting the record.

Working controls: already-terminal producer (zero extra polling), failed
upload, attempt2 job, duplicate upload step, corrupt digest/log authority and
HTTP refusal still reject; no bundle/binding is accepted on failure. Test
suffixes after original failing assertions are GREEN-stage requirements.

Implementation-stage controls also pin a changed physical identity/attempt
or duplicate producer on a refresh, completed/null convergence, skipped
unrelated steps, shared budget across producers, retained raw observations and
unchanged final provenance checks. No CPU remeasurement or replay rerun is
part of metadata convergence.

## Gates

Commit plan before originals; keep production unchanged in the initial draft.
Run scoped Node originals, record their exact assertion failures and working
controls, then get dazzy3's bounded review. Production follows that review.
After implementation: run scoped cpu-ci/cohort/artifact/request-budget tests,
independent source review, fresh full CI and auditor placement. Never merge
without a window. Backpack history work remains behind this priority fix.

## Implementation and review conditions

dazzy3 independently reproduced originals2839777933c as3 assertion REDs and6
GREEN, then cleared implementation with C1–C5 conditions. Production adds one
shared `terminalProducer` boundary. The original upload/checkout/digest/ZIP
authenticators and `evidenceEndCheck` are unchanged. A cached accepted bundle
keeps its original record; a later binding of the same producer may not
substitute a different whole job record.

GREEN-stage controls cover refresh-only attempt2 aliases, same-name duplicate
producers, changed job/runner IDs, two producers sharing six refreshes total,
late descriptive fields, active planner/certifier exclusion, immutable earlier
bindings with final drift rejection, completed/null convergence, skipped
unrelated steps, and preservation of both raw API observations. Exhaustion
assertions include run1000, job100 and the pending step name/status/conclusion.

Scoped execution:65/65 PASS,0 skipped across the producer originals/hostile
controls and existing cpu-ci/cohort/artifact/request-budget tests. This is not
fresh CI, a real CPU replay run, or merge clearance. CI's existing
`rust:test-harness` glob includes the new control file without workflow edits.
