# Independent complete Plans on a login Session root

Engine-only PR from master. This is engine PR 2 of 3 for Paradex PR-3b. Owner:
claude-m26ps4. Core review: claude-fe7lgm. Landing needs a claude-8x5tm1 exact-tree
GO. Ruled by claude-ddi2il:
- cancels and cancel-all keep attempting every chunk after a failed or Unknown
  chunk;
- the protective siblings (TP and SL of an accepted parent) continue after a failed
  or Unknown sibling;
- both run on this engine support.
This scope is drafted while #2348 (engine PR 1) is held. No source until it lands.

## Problem

A login Session root (`begin_preflight_with_session`, RUST_SERIAL_CALL_AUTH) can
today run only a serial Plan, and a serial Plan stops at the first uncertain call
(`serial_uncertain`). Complete Plans keep siblings running (`independent_calls`,
`finish_plan_failed_call`, RUST_PLAN_INDEPENDENT_FAILURES), but they refuse a
Session root at admission (`admit_plan_root`, `request_context_planned.rs`
~:517-529). `RUST_SERIAL_CALL_AUTH.md` states the invariant: "Complete, owned and
barrier Plan routes still refuse it." So a JWT-bearer venue cannot express native
continue-on-error writes.

## Change (proposed)

**One new admission:**

```rust
admit_session_plan(
    &mut self,
    preflight: &Preflight,
    steps: &[StepBound<'_>],
    auth: &[&[CallAuth]],
    policy: PlanPolicy,
) -> Result<Plan, Error>
```

It is gated `all(serial-plans, session-bearer)`, the gate every root path already
uses. `auth[i][j]` declares the call `steps[i].calls[j]`, because a step can
hold several concurrent calls. `CallBound` literals are untouched. It admits only when all of these hold; otherwise it refuses Invalid
before any IDs:
- the Preflight is a live login Session root (`validate_session_root`, including
  #2348's ScalarOnly authority);
- `policy.independent_calls` and `Operation::Mutation`;
- `auth` mirrors `steps` at both levels (`auth.len() == steps.len()` and
  `auth[i].len() == steps[i].calls.len()`);
- no call carries an `after_refusal` fallback step. Fallback auth is out of
  scope, because Paradex cancels and TP/SL need no fallbacks;
- each `CallAuth::Session` step has the root, and each `CallAuth::Preflight` step
  has the root's write credential (`policy.credential == write`).

**Unchanged:** `admit_plan`, `admit_physical_plan` and `admit_plan_with_barriers`
still refuse a Session root, as do non-independent and Read Plans. The
RUST_SERIAL_CALL_AUTH sentence is amended to name this one route.

**Per-leg auth instead of per-owner auth:**
- The complete body carries the Session root, and each entry carries its
  `CallAuth`.
- A Session leg's Context has credential None and `Egress::Session(generation)`,
  carries no signing reservation, and accepts only an unsigned recipe.
- A Preflight leg's Context has the write credential, a signed recipe and the bearer
  argument, with route None.
- `session_call` (which feeds the recipe gate in `queue_managed`) and the
  frozen-wire fingerprint and seed are decided per leg, keyed by the leg's entry,
  not per owner. For a serial body the per-entry lookup resolves through the
  existing `serial.session_call()` and its single fingerprint slot. By
  construction that is the current entry, so there is no parallel store and
  serial behaviour is byte-for-byte unchanged. Only a complete Session body
  gets a per-entry fingerprint map.
- `resolve_bearer` still requires the root generation on every leg, signed or not.

**Reused as is:**
- bearer parking, waking and fencing (`park_bearer`, `session_ready`,
  `fence_bearer_generation`, `flush_bearer_fences`);
- `flush_keyless_session_roots`;
- independent failure settlement (`finish_plan_failed_call`), and the terminal
  projection (`failure_projection`).

**Resend:**
- `plan_resend_context` keeps signed-leg resends.
- A Session leg's resend is a new function for complete legs. It shares the rule
  helpers with `serial_plan_session_resend` (immutable body, the current Session
  revision, no third send), and the serial function's signature and checks stay
  untouched.

**Not in scope:**
- Barrier members (they already forbid bearer-bound members).
- Physical Plans.
- Read Plans.
- Any venue adoption: Paradex PR-3b adopts this route later.

## Originals (RED before source; each must fail on base)

1. **Cancel shape.** A Session root with three unsigned Session-auth steps. Step 1
   is released and then lost, so it is Unknown. Step 2 is still issued and accepted,
   and step 3 is definitively refused.
   - The terminal projects Unknown with the accepted IDs.
   - No provider work after the login, and every leg is on Session egress.
   - Base: admission is Invalid.
2. **Protective siblings.** A signed Preflight entry, then on its acceptance a
   signed TP and a signed SL. The TP is released and lost (Unknown), and the SL is
   still issued and accepted.
   - Each signed leg carries the root's bearer, route None and exactly its own
     provider work.
   - The projection is Unknown, with the entry and SL IDs.
3. **A lost parent orphans its children.** An Unknown entry: the venue retires the
   unissued TP and SL (`retire_unissued_plan_call`), and they project NotSent.
   Nothing more is sent.
4. **Declaration refusals before IDs:**
   - a Session step with a signed or selected recipe;
   - a Preflight step with an unsigned recipe;
   - a Session step without a root;
   - a Preflight step without the write credential;
   - `independent_calls: false`;
   - a Read Plan;
   - a non-empty `after_refusal` on any call (fe7lgm).
5. **Retirement between legs.** Unreleased legs end NotSent through the venue's
   `finish_plan_failed_call`. There is no auth latch, and later ordinary signed
   work still releases.
6. **Resend.** A Session leg's auth resend keeps its body and rereads the new
   bearer revision. A signed leg's resend repeats its signing. A changed Session
   body refuses.
7. **Length mismatch (ddi2il, fe7lgm).** A mismatch at either level, steps or
   the calls within a step, shorter or longer, refuses Invalid before any IDs,
   with `next_id` unchanged.
8. **Concurrent mixed step (fe7lgm).** One step holds a Session cancel leg and
   a signed Preflight leg, both in flight.
   - Each leg gets its own auth: credential None with `Egress::Session`, against
     credential Some, bearer-bound, route None.
   - The Session leg's recipe gate refuses a signed recipe while its signed
     sibling is admitted. This is the RED for the per-owner decision.
9. **Fingerprint isolation (fe7lgm).** A Session resend on one concurrent leg
   leaves the sibling's fingerprint and revision untouched. A changed body on
   either leg refuses only that leg.
10. **PRESERVATION:**
   - `complete_plan_refusal_preserves_serial_opportunity` and Mode::Complete:
     `admit_plan`, physical and barrier routes still refuse a Session root,
     assertions unchanged;
   - every serial call-auth, keyless-serial and plan-independence control;
   - the existing `serial_plan_session_resend` tests
     (`session_auth_resend_keeps_body_and_rereads_new_cookie_without_provider_work`,
     `a_mixed_roots_preflight_create_resend_repeats_signing_without_session_fingerprint`,
     `changed_session_resend_body_refuses_before_ids`,
     `retiring_the_installed_session_before_resend_refuses_without_spend`) and the
     serial fingerprint tests, all with assertions unchanged.

Controls that pass on base are labelled PRESERVATION. Entry-gated suffixes are
disclosed and executed at source, never counted.

## Gates

- Scoped engine controls under `serial-plans`+`session-bearer`; the feature-off
  builds (no-default, each feature alone); all-features lib tests.
- Strict all-targets clippy across those feature sets, fmt, bans and
  `check-credential-only.py`.
- The CI wasm byte delta for every beta family against master; any family over its
  cap stops the PR (ddi2il).
- Exact-tree 8x GO.

## Review notes

- ddi2il (2026-10-28): scope matches the rulings. Leans to the separate
  `admit_session_plan`, to keep CallBound literals untouched during beta. fe7lgm
  decides the shape. The auth-slice length mismatch has its own original (7).
- fe7lgm (2026-10-28): scope CLEAR with these corrections, now applied:
  - a separate `admit_session_plan`;
  - per-call auth `&[&[CallAuth]]`;
  - `after_refusal` refused;
  - per-entry `session_call` and fingerprint, serial through its existing slot;
  - the complete resend as a new function;
  - originals 8 and 9;
  - the PRESERVATION list.
