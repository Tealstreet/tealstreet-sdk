# Rust Preflight Inherited Uncertainty

Owner: claude-fe7lgm. Reviewers: claude-uufub4, claude-dazzy3. Source: dazzy3's
H2 plan, follow-up 3; `RUST_PREFLIGHT_READY.md` ("local no-I/O activation alone
cannot label the whole recovered intent NotSent").

## Problem

A WriteAfterReads Preflight that fails seals through a fresh, empty
`CommandWrites` (`request_context_preflight.rs` `seal_preflight`, and the
admission-failure path in `request_context_preflight_call.rs`). That journal has no
released attempt, so `failure_projection` returns `NotSent`. When the command
is a retry of an intent whose earlier attempt may have reached the venue
(released with no terminal answer, or imported maybe-sent facts), a retry that
re-reads and then sends nothing reports `NotSent` for the whole intent.
That is false: the earlier attempt may have landed.

## Design

One monotonic owner fact, `inherited_uncertain`, on the command owner:

- `Cx::inherit_preflight_uncertainty(&Preflight) -> Result<(), Error>`, valid
  at any point in genuine custody, including before any read is admitted.
  - It requires genuine custody (`preflight_owner`) and
    `PreflightPurpose::WriteAfterReads`. A ReadOnly Preflight gets `Invalid`;
    a stale, revoked or foreign handle gets `Revoked`.
  - Repeating it is idempotent. There is no way to clear it.
- The flag lives on `CommandOwner`, not on Preflight `State`, so it survives
  the Preflight -> Plan phase change.
- Every `CommandWrites` created for that owner carries it: the Preflight seal,
  the admission failure, first registration and the declared-mutation seal.
- In `failure_projection`, an inherited fact counts as uncertainty:
  `!any_released && !inherited` gives NotSent; `uncertain || overflow ||
  inherited` gives Unknown. Rejected and partial projections cannot hide it.
- Success is unchanged. A retry that proves the outcome by a successful
  terminal seals Success.
- The fact grants nothing. It never grants a resend, resolves uncertainty,
  imports facts, renews a deadline or changes a budget. It can only make a
  failure projection more conservative, so a caller cannot misuse it to hide
  sent effects.

Planned executions (#1919): H2 resumes intents as Plan-backed physical
executions with one shared journal, so the fact must carry there too (codex-uufub4).
`begin_planned_execution` exists only in frozen #1919. Once #1919 lands, this
PR merges the landing and adds:
- an atomic carry of the fact into the physical owner and its journal before
  any founder, group or credit commit;
- retention of the fact on a refused admission;
- the same Unknown projection for the founder and every follower;
- no ID, attempt or budget reset, and no new grant.

The fact only affects projection. It does not replace retained per-leg IDs and
budgets, and it does not replace proof-gated uncertainty resolution. Success
still needs H2's own proof and ledger gates.

## Original REDs (committed before the implementation)

1. A WriteAfterReads Preflight with an inherited fact whose read fails seals
   `Failure` with `Outcome::Unknown`. Control: the same Preflight without the
   fact seals NotSent.
2. The admission-failure path (a capped or refused read) with an inherited fact
   gives Unknown. Control: the same path without the fact gives NotSent.
3. The fact survives Plan admission: a Plan that is refused all of its legs
   (effects-free Rejected) still gives Unknown, not Rejected. Another Plan with
   no released attempt gives Unknown, not NotSent.
4. A successful Plan with the fact seals Success (no Unknown on success).
5. Custody: a ReadOnly Preflight gives `Invalid`. A stale-generation, retired or
   foreign-issuer handle gives `Revoked`. Each refusal leaves the owner,
   journal and seal unchanged.
6. Idempotent and monotonic: calling it twice does not change the projection,
   and no API clears it.
7. After #1919: an inherited-fact Preflight founding a planned execution
   carries the fact into the physical journal; the founder and followers fail
   Unknown, with no ID, attempt or budget reset.

Committed REDs: the inert `inherit_preflight_uncertainty` validates custody
only, so every inherited case below fails with NotSent or Rejected, and each
fact-free control passes.

- `inherited_uncertainty_seals_a_sendless_write_preflight_unknown` (1, 6;
  the fact is set before any read, twice).
- `inherited_uncertainty_survives_the_central_read_admission_refusal` (2).
- `inherited_uncertainty_survives_an_engine_sealed_read_failure` (H2: the
  retained ledger says Unknown, the read fails).
- `inherited_uncertainty_survives_plan_admission_without_a_released_attempt`
  and `inherited_uncertainty_is_not_masked_by_an_effects_free_rejection` (3).
- `inherited_uncertainty_leaves_a_proven_plan_success_successful` (4, control).
- `inherited_uncertainty_requires_live_write_preflight_custody_and_mutates_nothing`
  (5, control).

RED 7 becomes the execution carry RED. It is added after #1919 lands,
together with the founder and follower Unknown controls. The custody and
idempotence tests are new-contract controls, not original REDs.

H2 mapping (dazzy3): RED1 (`29c154338ff`) needs no fact; it is Unknown from its
own released journal. RED2 (`b180d0f9a31`, amended to one send per call) call 3
re-reads with two sends spent, sends nothing, and fails its Preflight. That
failure is this fact's case.

## Gates

Engine focused tests (`request_context_preflight*`, `request_context_planned*`),
clippy in every feature mode, including `--no-default-features`, then fmt. Fresh
CI. No venue adoption is in this PR.
