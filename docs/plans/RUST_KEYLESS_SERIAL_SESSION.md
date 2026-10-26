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
This Session Preflight binds no caller receipt cap.

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
again. Earlier released calls still determine the command's journal projection;
NotSent describes an unreleased operation, not necessarily the whole command.

## Originals before source

Use genuine engine command admission, real Session creation and readiness,
Prepare/Bind/Claim, exact DATA and real timers. An inert entry stub may expose
the new API while refusing without effects; missing methods and compiler errors
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
