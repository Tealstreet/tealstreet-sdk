# Explicit authentication per serial call

Plan on actual b37013c4c1d, after #2197 landed at aa606430ed1. Owner zs5e83;
reviewers fe7lgm/dazzy3 (engine) and m26ps4 (credentials). One isolated engine PR
contains the plan, genuine originals and source. GRVT adoption stays separate.

## Problem and scope

GRVT amendment is an unsigned Session-authenticated inspect, cancel and verify,
followed by signed CreateOrder. The serial root must retain the write credential
for create without forcing provider work on the earlier calls. Installed-key
cancel and leverage also need only the authenticated Session. No filler Present,
PublicAddress or signing operation is allowed on those unsigned legs.

This is the approved follow-up to #2197: every call declares its auth mode;
recipe presence never chooses authority, and no signing command falls back to
Session auth. Existing unsigned prerequisite Reads keep their declared behavior.

## API and account binding

- `SerialCall` adds `auth: CallAuth`.
- `CallAuth::{Preflight, Session}` is an unconditional public enum. Default is
  Preflight. No public variant set changes with additive feature unification.
- Preflight is today's credential policy: Mutation takes the root write
  credential; Read takes its declared read credential, including existing
  unsigned prerequisite Reads when that credential is None.
- Session requires serial-plans plus session-bearer and the exact retained live
  login Session. Without session-bearer it refuses Invalid before IDs.
- `begin_preflight_with_session` retains its existing three-argument admission
  downstream. It may now admit a root with a write credential; reads remain
  credential-free. Complete, owned and barrier Plan routes still refuse it.

For an Empty credential-free Session, retain the existing explicit private
egress and refuse any Some write credential. For a credential-bearing login
Session, compare its exact issuer, generation, Primary slot and credential ref
against the engine's OWN immutable installed Primary identity. A Some write
credential must be the exact Selection::credential for that slot: its reference
equals the installed identity's credential KeyRef. A signing ref, passphrase or
extra-role ref cannot stand in as the root credential. Signed calls also require
that identity's signing ref.
Foreign refs, accounts, slots, stale or retired Sessions refuse before IDs.
Never trust a venue-supplied Identity or Selection as this comparison's source.
The one real Present-only login Session suffices; no dummy header Session exists.
The engine proves issuer, account, slot, refs and liveness, but cannot distinguish
login from another Primary-credential Session such as a listen key. The venue
must select its actual login Session; GRVT's account state owns that choice.
`Sessions::open` keeps its global credential-xor-private-egress rule unchanged.

Recheck this binding and liveness at issue, continuation and resend. A Session
call's Context has credential None and explicit Egress::Session of the retained
generation, resolving to the verified Primary slot. It never uses generic
bearer-bound Public-to-Primary inference or the installed Secondary.
A Preflight Mutation still requires Some root write credential. A root with
write None refuses EVERY Preflight auth declaration, including Reads.

## Recipes, journal and resend

Session calls accept only an Unsigned recipe. Signed, selected and nonce recipes
refuse before IDs, Bind or reservation. Preflight calls retain the existing
credential-presence-equals-recipe-presence rule per leg. The root write credential
is never swapped, consumed or downgraded by a Session call.

All calls enter the same serial journal with their existing per-call window,
unbounded root, exact DATA custody, caps and two-attempt reservation. Session
resend uses the current call's Session/revision and frozen wire fingerprint;
Signed resend preserves its existing signing authority. The bundle is re-read
at Bind. No retry, grant, attempt reset or third send is added.

Session retirement refuses remaining unreleased calls per operation NotSent;
the command terminal follows the journal, including accepted prefixes and Unknown
when prior released work was unproven. No auth-health or signing latch changes.
Already admitted signed work keeps its declared authority and existing fences.

## Consumer amendment

The earlier #2197 Empty-only consumer restriction is deliberately superseded:
an Installed unsigned-only command may choose a Session root with write None.
A closed GRVT command table must name its unsigned cancel/leverage commands and
its signing-required place/amend commands. This is a deliberate choice, never
an error fallback. Sign-requiring admission still proves signing availability
before queue/provisional effects or the cancellation before replacement.
The earlier credential-bearing Session negative is refined: a matching installed
login Session is accepted; a foreign/mismatched one stays refused.

## Originals and source gates

1. Real provider Present login on a credential-bearing Session, with a Secondary
   installed. One mixed root issues Session inspect, cancel, verify without any
   provider work, then Preflight CreateOrder with exactly its declared provider
   work. Same root credential throughout; exact DATA and Prepare/Bind/Claim.
2. Installed unsigned-only cancel then leverage under write None; no provider
   work after login, explicit Session egress, never Secondary.
3. Cross-engine equal-generation, foreign ref, mismatched write/ref-kind and cross-slot
   Sessions refuse before IDs; real matching identity is accepted.
4. Empty Session cannot carry Some write. A write-None root refuses every
   Preflight declaration. Session without a retained Session or with feature off
   is Invalid before IDs. A signed root without a credential cannot sign.
5. Recipe/declaration mismatch on each side refuses before IDs and provider
   effects; selected and nonce attachments cannot sign a Session call.
6. Retirement between calls and before resend refuses; journal projection and
   later ordinary signed work prove NotSent per operation, no auth latch.
7. Session auth resend keeps immutable body, refreshes its cookie at Bind,
   consumes only its reserved attempt and rejects stale/foreign delivery.
8. Existing signed serial and unsigned prerequisite-Read controls retain every
   assertion, with mechanical Preflight tags only. Existing keyless controls
   explicitly tag Session and preserve their authority and assertions.

Compile failures and entry-gated suffixes are disclosed, never counted as runtime
RED evidence. All suffixes must execute at source. The mechanical tag commit is
reviewed separately from authority logic; history is fixed forward, not rewritten.

## Gates

Scoped engine controls, default and feature-off controls, strict all-targets
Clippy, formatting, bans and correctness CI. Paired Bybit, Bitget, one signed
serial adopter and local combined on the landing tip, jobs 4 and one Cargo at a
time; delete each scratch target after measurement. Report exact deltas without
inferring zero from enum layout. Existing caps hold; auditor exact-tree GO is
required for this engine PR. No venue activation or hosted allowlist widening.

## Original qualification

The inert declaration checkpoint compiles the genuine login/command controls:
with serial-plans, session-bearer, condition-waits, http-mutation-barriers and
sign-meta, twelve controls execute: seven assertion REDs and five GREENs. Five
REDs stop at matching credential-Session entry; their later custody, mismatch,
retirement and provider-count suffixes are source-stage obligations. Two REDs
pin ignored Session/no-root and Preflight/keyless declarations. With serial-plans
without session-bearer, the additional control is an assertion RED because the
Session tag is ignored. An initial Option<bool> fixture compile error is excluded.
The original credential-Session negative is deliberately amended at source:
matching installed credentials become valid; foreign or absent installed refs
remain refused. No old financial or signed-adopter assertion is removed.

## Source checkpoint

Session linkage is revalidated against immutable installed identities at entry,
issue, continuation and resend. Credential-free Sessions retain their explicit
egress and cannot carry a write credential; credential-bearing Sessions require
the installed Primary credential ref and any retained write must equal it.
Session calls alone carry credential-free Contexts, explicit Session egress and
the frozen codec fingerprint. Preflight calls keep their prior credential and
provider rules. No Sessions::open constraint or global recipe validation changes.

The twelve original first assertions are preserved. Source fixture completion
adds the required genuine reconciliation capture and finish_plan_call before
done_plan; both were gated by entry in the inert checkpoint. The harness also
enables guard_order_mutations and seals transport failure through fail_plan
instead of its original panic, so the new write/lock/Unknown control can execute. Source controls add
selected/nonce refusal, refreshed-cookie immutable resend and late duplicate,
changed-body refusal, retired resend, mixed Preflight create resend with fresh
provider work, and a released Session Mutation failing Unknown. A terminal after
a real classified auth refusal retains that earlier journal error and its health
evidence; the retirement itself neither latches signing nor invents auth evidence.
The old credential-Session negative still refuses without an installed identity;
the new matching-identity positive explicitly records the intended amendment.

The first-adopter combined measurement additionally enables serial-plans and
session-bearer as GRVT will. Report its cost separately; growth above 30K needs
the overseer. Release-domain pacing, if composed later, keeps its stamping at
plan_leg_context and its managed-boundary checks for Session calls too.

### Workspace corrections

The mechanical tag commit is not independently workspace-buildable: two native
constant SerialCall declarations in BingX and OKX need explicit Preflight enum
values, rather than Default::default(). Commit 1b8ae5f881c fixes both without
changing assertions or policy. That commit also moves Session credential matching
into the existing Identity policy file, as required by the credential-only bans.
No checker or inventory allowance is widened.

The source checkpoint 250ba475edf actually merged 923246ecf96, the CI fast-path
revert. Later composition 931d35faee5 merges actual 3e5ccf01acb, preserving the
incoming repair source automatically and docs as incoming plus own suffix once.
Full correctness CI remains required. Workspace all-targets passes after the
constant fixes. Shipping sizes are measured against the actual composed tip.
