# Keyless Session-authenticated serial Plans

Status: plan and compiling-originals phase, on actual landing tip
174f6a4c90c6a08dbabe72623fe3a4d8226109f2. Owner codex-zs5e83;
engine reviewers claude-fe7lgm and claude-dazzy3; credential fit claude-m26ps4.
This is isolated engine work. GRVT adoption, D3/D6 and public-address setup
remain separate changes.

## Problem and approved contract

A trusted Empty login intent opens a genuine Session without an Identity,
Credential, KeyRef or Present operation. Its authenticated cancel and leverage
calls need an unbounded serial root with bounded physical calls. Today
PreflightPolicy rejects WriteAfterReads without a write credential and
SerialPolicy requires one. An invented credential or a Read masquerading as a
Mutation would violate custody and credential policy.

The overseer and fe7 approved a direct cutover of SerialPolicy.credential to
Option<Credential>. Existing signed callers wrap the same value in Some;
selection, signing validation, bytes, attempts and pacing do not change.
None never authorizes provider signing, a nonce reservation or a signing grant.
No additional engine feature is introduced.

## Genuine entry authority

Add begin_preflight_with_session(policy, &Session), available only with both
serial-plans and session-bearer. This is the one new boundary accepting
WriteAfterReads with read_credential None and write_credential None. Validate
the actual live issuer-checked Session before issuing the command generation,
and store its generation privately in Preflight state. Existing begin_preflight,
PreflightPolicy::validate and caller-wait receipt admission retain their rules.
This Session Preflight binds no caller receipt cap. The retained generation is
issuer-bound; equal generation numbers from different engines never match.
The engine cannot inspect trusted setup facts. GRVT may use this seam only when
trusted SetupFacts::primary_login() is Some(PrimaryLogin::Empty), from landed
phase 3 D3. Installed and Absent never downgrade into it; those negative controls
belong to GRVT adoption, not this engine prerequisite.

admit_serial_plan keeps its three arguments. Its Optional credential must equal
the Preflight write credential exactly. None requires the retained live Session;
copy that generation to Serial before any admitted call can issue. A normal
Preflight cannot acquire Session authority from an endpoint, cached cookie,
configured account, recipe, absent reference or later bearer argument.

Every complete-Plan route, including admit_plan_owned and barrier admission,
refuses this keyless Session Preflight. The root is serial only. ReadOnly and
credential-bearing policies cannot borrow this exception.

## Issue, continuation and resend

The current request_managed bearer parameter carries each physical call's
Session. It must match the retained live Session; missing, foreign, changed or
retired Sessions refuse before HTTP/signing IDs, Bind or reservation. Reads in
this root are unsigned under the same Session, without secondary/read-key
fallback. A keyless root refuses request_signed_managed, sign_http, selected
signing and nonce work at the earliest boundary that observes that intent.
SerialCall contains operation/window/attempt data, not a recipe; an API cannot
infer an unseen recipe's future intent. A signing-capable read credential is
therefore refused at entry, and any later recipe attachment is refused at issue.
Exact validation ordering and no-spend controls are source-review gates.

Serial continuation retains the root's Session and checks liveness before
retiring the old leg or allocating the next. Retirement while the Preflight is
parked, or between calls, follows existing owner retirement and journal rules;
the next issue is Revoked. No replacement Session can inherit the root.

serial_plan_session_resend remains exact DATA, Session and revision based.
A parsed definitive refusal may select the declared remaining physical attempt
under the same live Session, with the refused revision as the floor. It does
not mint a provider-signing authorization. Attempts, timers, generation,
auth ledger and immutable body are not reset. A second refusal cannot select
again. Its own Bind re-reads the newer bundle and sends that cookie, never the
refused revision. Earlier released calls still determine the command's journal projection;
NotSent describes an unreleased operation, not necessarily the whole command.
Foreign/retired Session refusal is NotSent per operation and changes no auth
health or #2109/#2119 account latch. A Session Preflight without an admitted Plan
can fail_preflight or done_empty_plan without retiring the Session.

## Originals before source

Use genuine engine command admission, real Session creation and readiness,
Prepare/Bind/Claim, exact DATA and real timers. An inert entry stub may expose
the new API by delegating unchanged admission; it adds no keyless write authority.
It need not refuse an already-valid ordinary ReadOnly policy; missing methods and compiler errors
are not REDs. No synthetic issuer, fake Credential or manually forged delivery.

Required controls:

1. An Empty-login Session (no installed Identity or credential) serially cancels
   then sets leverage. Exact native request bodies and Session headers; zero
   provider signing operations, no private egress before the live Session.
2. Ordinary keyless WriteAfterReads stays Invalid; missing Session cannot admit.
   Foreign, retired and non-finite Session entry refuses without resource spend.
3. ReadOnly, a supplied read key and credential mismatch do not enter the keyless
   exception; a signed Some path retains its existing behavior and wire bytes.
4. Keyless Session Preflight refuses ordinary complete-Plan, owned and barrier
   routes without consuming its serial admission opportunity.
5. Signed/selected recipes, sign_http and nonce attachment refuse before provider
   work, new IDs, Bind or reservation. Existing signing validation remains intact.
6. Cancel settles then leverage issues; a Session retired between those calls
   refuses the continuation and seals through existing custody/journal handling.
   Also cover retirement while the Preflight is parked.
7. RefusedResend on a keyless Mutation reissues unsigned with the same immutable
   body only after the Session revision moves. No grant authorizes signing;
   second/foreign/stale delivery spends nothing and cannot emit another request.
8. A keyless serial Read uses the same Session, no secondary fallback, and its
   auth resend needs no financial write fact. Exact expiry and late callbacks
   remain inert, with operation and command outcomes pinned separately.

Each first RED and its gated suffix are recorded separately. Preserve all
existing signed serial control assertions; source must execute the gated
suffixes rather than count them as original execution.

## Verification and publication

Run scoped engine originals and signed serial/session/condition-wait controls,
fmt, strict clippy, bans/ratchets and required workspace checks on the composed
landing tip. Any mechanical Some wrappers are reviewed separately from logic.
Measure paired Bybit, Bitget, one signed serial adopter and one local composed
combined bundle. Existing adopters must show zero shipping growth under the
assignment; no budget change or inferred dead-code proof. Delete each own
scratch target after measurement and retain source/log/size receipts.

Originals review precedes source publication. Source needs exact fe7/dazzy review
and m26 credential fit, fresh PR CI, READY and auditor 8x5tm1 exact-tree GO.
Owner then performs the guarded head-matched merge. No activation of GRVT or
unlanded public-address setup is inferred from this engine prerequisite.

## Original checkpoint receipt

There are thirteen mechanical edits: d373d770924 contains twelve single-line
Option/Some edits, and 7e7584505b5 carries the required PlanPolicy projection
Some(policy.credential) -> policy.credential. d373d770924 is not independently
buildable: before that thirteenth edit its projection is a nested Option. No
history is rewritten and no assertion changes are part of those edits. The following checkpoint adds
only an inert begin_preflight_with_session delegator to existing admission,
a mandatory-Some guard preserving the old serial behavior, the new test include,
and its controls. It grants no keyless write authority.

Thirteen controls execute: twelve assertion REDs and one GREEN, zero ignored.
They expose four distinct behaviors: valid keyless entry is refused, a foreign
Session yields Invalid instead of Revoked, a retired Session yields Invalid
instead of Revoked, and ReadOnly is admitted rather than refused by the new
boundary. Nine RED tests stop at the valid-entry precondition: six at admitted(),
and three direct entry checks (mixed policy, empty cleanup, missing/foreign
bearer). Their intended runtime suffixes are entry-gated, not nine independent
runtime failures. Development compiler errors are excluded. The ordinary
keyless refusal is the GREEN.
Every fixture first completes a real managed Empty-body login using the typed
PrimaryLogin selection to obtain SessionAuth egress, with no Identity/KeyRef or
provider work. This is engine custody evidence, not constructor certification.

All suffixes after those first assertions remain unexecuted: serial admission,
complete/barrier rejection preserving serial opportunity, actual cancel/leverage
wire, per-operation NotSent/auth-health/latch assertions, signed recipe and
wrong/missing bearer refusal, between-call retirement, Read routing, and seeded
resend/new-cookie/body/no-third-send. They must execute after source. The full
nonce/selected attachment, parked-Preflight retirement, late/equality expiry
and keyless Read resend variants remain required source controls; they are not
represented as executed originals or empty passing stubs.

Existing signed Session controls: 35 PASS; existing serial controls: 32 PASS,
zero ignored. Scoped strict clippy passes with serial-plans, session-bearer,
condition-waits and mutation barriers. Logs and exact hashes are retained in
/tmp/codex-keyless-serial. No size, runtime source or CI clearance is claimed.

The exact original test command was `cargo test --manifest-path sdk-rs/Cargo.toml
-p safe-cex-engine --lib --features
serial-plans,session-bearer,http-mutation-barriers,condition-waits keyless_serial
-- --nocapture`. Signed controls used the same command with filters
`serial_session::` and `serial_plans::`. Strict used `cargo clippy` with the same
manifest/package/features plus `--lib --tests -- -D warnings`; its cached
Finished line is not evidence of a separate rebuild. The no-Session control used
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-engine --lib --features
serial-plans serial_plans::` (32 PASS). All logs retain owner execution attribution.

## Source checkpoint contract

The source retains the full issuer-bound Session handle privately in both
Preflight and Serial state. Only the dedicated entry can bypass the existing
mandatory-write-credential policy check, with both read and write credentials
absent. The ordinary entry is unchanged. Normal, owned physical and barrier
complete-Plan routes all refuse the retained Session root before reservation.
Every new Context uses only its Session's declared egress, without interpreting
missing credentials as a Primary selection. Each call must supply that exact
live Session; continuation and Session resend recheck the retained authority.

Keyless managed recipes refuse before inspecting signing freshness, nonce
claims or provider work. Prepare explicitly has no signing credential, recipe,
attempt, role or signer headers. The legacy sign_http attachment cannot attach
to a managed request. An installed secondary remains unused by the keyless Read.
The ordinary signed Some path and its request-level Session bearer are retained.

A keyless serial physical call freezes a SHA-256 fingerprint of length-delimited
method, URL, optional body and static codec headers on successful first queueing.
Its authentication resend must match before any new Prepare or reservation;
Session headers are deliberately excluded and re-read at Bind. Only the digest
is retained, not cached signed bytes or secret headers. A fresh serial logical
call clears this fingerprint, so cancel then leverage can have different bodies.
A changed body or attempted signed resend returns Invalid and seals through
existing Plan admission/journal handling. Earlier released work may project the
command to Unknown; an accepted earlier call projects None, not whole-command
NotSent. These projections do not change the shared journal policy.

Session retirement fences unreleased bound packets and retires a root with no
pending delivery through the existing Preflight/Plan seal and owner-retirement
paths. Released work keeps its delivery and financial evidence; its next call
is refused and its final command outcome follows the journal. Foreign/retired
refusals change no auth health or signing latch. Empty completion preserves the
Session and its revision. Equality expiry is Timeout/NotSent for an unreleased
operation, and late completion cannot re-open it.

The first source execution exposed the disclosed second-refusal grant.unwrap
fixture bug. Its correction maps absent grants to RetryForbidden, as the
existing serial Session controls do, and retains every original assertion.
Additional source-stage controls cover physical-owned admission, non-finite
entry, nonce/signing-intent refusal, legacy/selected attachment, genuine installed
secondary isolation, duplicate old DATA, parked retirement, exact equality and
late completion, changed resend body, and accepted-prefix journal retirement.
They are source evidence, not retroactively counted as original REDs.


### Bounded source review corrections

The Session entry requires credential-free private Session egress; a live Session
without that egress cannot fall back to Primary. Root egress resolution also
refuses a missing declaration. The installed-secondary Read control checks the
actual Context's Session generation, in addition to the resulting route.

Retirement sweeps skip command-entry and Plan-expiry custody. A failed seal stops
the sweep, with a debug assertion, rather than panicking in release or repeatedly
finding the same root. Successful seals drain owner retirement callbacks and
wake deferred commands at that flush. A live-root count increments only after
successful Session entry and decrements when its owner enters retirement. Zero
roots skip the command scan; test-only visit counters pin that fast path before
keyless work and after empty completion. A custody-scope control uses genuine
issued Plan/Preflight handles and the engine's existing scope markers to pin
that retirement waits for expiry/entry custody to end.

The resend fixture now explicitly fails its Plan when Session resend selection
refuses, using the existing journal rather than leaving an owner open. This is
source-stage collector plumbing, not original evidence. Per-operation refusal
controls additionally stage a later ordinary signed request on the same engine,
through genuine Prepare into provider work, proving that no account signing latch
was set. A separate pre-refusal positive validates that probe itself.
