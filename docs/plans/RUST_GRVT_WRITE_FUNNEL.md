# GRVT write funnel and shared prerequisites

Owner: codex-zs5e83. Scope ruling: claude-ddi2il. Native base: actual
bcaaa4acc567e8eca588c067d67ea38e7f7c9b98. PLAN AND ORIGINAL REDS ONLY.
No signer, queue, constructor, host, engine, existing fixture or inventory edits
are authorized by this checkpoint. Shared pieces below are reviewed proposals or
local sources, not landed production seams. Hosted signing uses Sam's single
model: the server receives exactly the minimum material needed to place orders;
local mode or remote signing serves users who do not send that material.

## Five occurrences, one owner

The requested five trading.rs occurrences are one coherent command lifecycle:

| Native location | Fingerprint | Count | Future authority |
| --- | --- | --- | --- |
| trade_command initial planning refusal | fbafcc48aaedbe70a22d054c16acc0858b261af3d5f711681c39d0e866da7e30 | 1 | Genuine command Preflight |
| trade_next deferred planning refusal | same fingerprint | 1 | That queued command's ready custody |
| trade_step common self.request | fbede0a47e0e356cdd283dca1fff0b6dc1c5762e047963add336e0869ee07b1b | 1 | Current serial Plan call |
| trade_finish failure | b780b823ba70712169e1be98fecae4d2a2655f7ca35f27bea4b99f09bd684696 | 1 | Shared Plan failure projection |
| trade_finish success | d6abf86165880819d2e37443be1918451a6c2b07fb046edbc4503019bdc79eff | 1 | Shared Plan success projection |

Base trading inventory is 8. The separate fixed-settings #2076 owns 3, leaving 5;
this plan does not prune them early or claim any current removal. After the four
pending local/catalog slices GRVT has 25, projecting 20 only after complete future
write adoption. The common lib sender also serves account, identity and history;
its references remain until their own adoption. No rename-only prune is allowed.

## Native geometry to preserve

The FIFO accepts PlaceOrders, CancelOrders, CancelSymbolOrders, AmendOrder and
SetLeverage, with fixed local setting exits outside this slice. trade_command
runs native planning validation, then enqueues. trade_next replans when it becomes
front and sets busy. The current planner eagerly derives the wallet and signs
placements (including a discarded validation plan); that key work must eventually
move to provider dispatch, not be retained as an admission shortcut.

A native job is an unbounded finite sequence: input orders are uncapped, each
placement can add two bracket children, amendment resolves/inspects, cancels,
verifies final cancellation and then replaces, and symbol cancel reads a complete
open-order snapshot before its native destructive request. There is no whole-job
wall-clock and no native fixed step/attempt count for the command.

Each current Trade HTTP uses the shared POST sender, native JSON/header paths,
and timeout_ms=None. Inspect, Verify and ReadSymbol are Reads; other effects are
Mutations. A released Read transport failure retries immediately up to five times
(six physical network attempts), preserving one per-Step attempts counter.
Mutations do not retry a transport failure. HTTP/protocol failures are DATA,
never transport evidence. Typed preparation/provider/credential/integrity refusal
must be strict, not inferred into a retry from error text.

Parsed code 1000 authentication refusal is handled only from on_response DATA. If
!auth_resent, the same Pending is reset, queued for login and sent once afterward.
The per-Step auth_resent flag and the network retry count are independent and
neither resets the other. Existing trading.rs control
explicit_session_rejection_resends_identical_signed_order_once proves the resent
Step.body is byte-identical, while cookie/account headers change. A second code 1000
fails with no third send or second login. This includes cancel and verification
Reads, not just creation. The native branch checks parsed code 1000 without an
HTTP-status guard: 401, 408 or 503 carrying that code still refresh and resend
once. Per claude-ddi2il's ruling, preserve this narrow venue-declared definitive
refusal grant regardless of status. A 408/5xx without parsed code 1000, or an
Unknown transport result, grants no Mutation resend. The serial resend owner
must add the explicit parsed-refusal grant hook; no generic status gate widens.

Keep every native client/native ID, large integer, accepted-ID order, bracket
reduce-only/side/trigger, cancellation reconciliation and amend execution/lifecycle
proof. An ACK alone never authorizes replacement. Only native decoded facts enter
http_write_fact/http_write_batch; a provisional Store Creating/Canceling effect is
not an Accepted fact. Reserve owner/call/provider work before any provisional
native effect, and pin its native cleanup on refusal. Shared journal projection
must preserve all earlier accepted IDs and Unknown; no numeric terminal or
successful-empty substitution may bypass it. Native trade_finish strips a
Rejected/NotSent outcome to None after effective work, created IDs or cancellation
attempts. The shared journal owns the future terminal instead; CALLERS must
disclose this difference, with original controls preserving the native evidence,
rather than claiming outcome parity. Another explicit difference: native parsed
408 + non-1000 is Rejected (as is 401 + code); 503 + non-1000 is Unknown.
Originals pin those native outcomes. The future shared codec keeps 408 without
parsed 1000 as Unknown, as required by the shared write rule, with CALLERS
disclosure. Only the parsed-1000 exception grants the single auth resend.

## Queue custody and existing shared pieces

Exact local pins, both owned by claude-fe7lgm:
- Streamed serial Plan source d4efb3f6b15, reviewed CLEAR, plan
  RUST_PLAN_SERIAL_STREAM.md. It stacks on #2037, opening after #2012 lands.
- Condition wait source fbddb4851f7, reviewed CLEAR, docs d3517762f2f,
  RUST_CONDITION_WAIT.md, stacked on serial Plan.

Proposed consumer flow, subject to exact API composition review:
1. In each genuine entry, begin_preflight(WriteAfterReads, selected read role or
   None, Primary write credential). Do not borrow an earlier job's root or ID.
2. Behind an active job or builder check, park_preflight_for(preflight, None).
   A native callback only calls request_preflight_ready. Admission or failure
   occurs later in that command's on_http_preflight_ready; re-park is progress.
3. When all native prerequisites are ready, admit_serial_plan with the selected
   Primary credential, no pacing floor and the appropriate first Read/Mutation.
   Declare attempts: 2 on EVERY Mutation and Read SerialCall, reserving the
   one auth resend. Reads with two attempts depend on the resend PR's source.
   Issue only the one current Slot, through managed transport and the recipe.
   A Read without a read-role key uses request_managed unsigned, still bound
   to the same Session header bundle and the same parsed-1000 Read path.
4. Validate exact Plan DATA before every codec/reducer/continuation. Record only
   proven write facts; continue_serial_plan/continue_serial_plan_after or park
   through a reviewed shared seam. No next call follows uncertainty.
5. Empty native jobs use done_empty_plan. Completion and failure use the common
   Plan projection, which alone releases queue ownership and order locks.

No WriteAfterReads Preflight binds the caller receipt. Serial roots have no
wall-clock; receipt expiry while queued or mid-chain is not a terminal. Each call
has its own bound. The ruled 20 s call bound newly includes route, Prepare,
Bind, provider and physical I/O: native explicitly has no timeout. The no-root
clock/per-call-bound contract is already ruled, and claude-ddi2il has approved
20 s for GRVT, with CALLERS disclosure. SerialCall.timeout_ms is a positive u32;
expressing native None instead would require fe7lgm to change the engine API,
which this plan does not request. Physical Read timeouts also feed the immediate
Read-retry ledger, a new behavior to disclose alongside the bound. No new command step/order/page cap
is proposed.
The existing 256 command-owner capacity becomes a typed NotSent admission refusal
before queue, signing or provisional Store effects; disclose this new capacity
precedence. GRVT's existing order-lock configuration must remain the audit base,
not be silently replaced with another venue's 64-scope configuration.

Builder check geometry does not need a new Preflight delay-then-ready API:
get_authorized_builders is a separate account read with 10 s native timeout and
three attempts with 2 s/4 s backoff. Commands park while it runs. Its final DATA or
failure releases the command's ready callback, which proceeds or returns the
native typed builder cause. The retry timer does not grant command custody.
Keep the existing builder transport/account scope unless separately assigned;
this five-site plan does not claim its account inventory removal.

## Shared gaps and proposed owners

| Gap / dependency | Owner proposal | Exact status / required proof |
| --- | --- | --- |
| Serial Plan and condition wait actual composition | claude-fe7lgm | Source pins above CLEAR but unlanded. Actual #2012/#2037 plus refrozen pins required; one owner, one reserved Context, no positive pacing floor. |
| Same-call serial Session resend and immediate failed-Read continuation | claude-fe7lgm | RUST_SERIAL_SESSION_RESEND.md at 92f1ff3dcd9 (earlier 30703266220 / 606488cba17) is a PLAN, OPEN before source. Same call attempt 2 after parsed code 1000, Mutation ResendGrant or validated Read DATA, seeded attempt 2 with no third send; immediate Read failure continuation carries no fabricated DATA or 1 ms delay. |
| Revision-gated new call and same-call auth resend | claude-fe7lgm with claude-m26ps4 for the revision signal | OPEN source under amended plan 92f1ff3dcd9: native first calls and resends both wait during refresh even if the engine Session is still Ready. Capture the previous revision and resume only on Ready with a newer bundle revision, not merely Session-ready. |
| Refresh after Prepare/Bind, before Claim | claude-m26ps4 (#2069 engine-wide Bind path), fe7lgm offers serial fallback | OPEN #2069 shared unreleased-call park/reissue within its existing window. Revision None makes Claim Stale; reprepare then NotReady must retain the same call until fresh headers, with no ResendGrant, auth_resent spend, second login, budget reset or extra logical write. Require Mutation and Read ingress REDs. |
| Cached Session header bundle | claude-m26ps4 | #2069 generalized source dd7fd51d03a is consumer-reviewed CLEAR locally, unlanded. Needs actual #2037/G2b and final pin. Bundle generation/revision, aggregate packet budgets, collision refusal and cleanup must compose with serial waits. |
| GRVT EIP712 provider-payload HTTP recipe | codex-zs5e83, reviewed by m26ps4 and core/custody owner | OPEN consumer recipe/constructor work. Existing provider Secp256k1Digest/PublicAddress primitives suffice; no new crypto algorithm or legacy SignPayload bypass. Exact recipe API and construction witness reviewed before source. |
| Login completion / credential and header lifetime | codex-zs5e83 with claude-55ao7t login ownership and m26ps4 core review | OPEN consumer composition: native login_session is a login-operation engine Session, distinct from cached cookie/account state. Connect honest Session-ready/failed transitions and provider installation; never mint a substitute Session or treat a native counter as engine provenance. |
| Parsed auth-refusal grant and header ordering | claude-fe7lgm (grant) and claude-m26ps4 (ordering), ruled by claude-ddi2il | OPEN shared source: parsed code 1000 grants the one resend regardless of status; other 408/5xx do not. Preserve Content-Type-first through a recipe-controlled ordering seam in #2069, not a venue header insertion bypass. GRVT 20 s per-call value is ruled; disclose the new bound in CALLERS. |

The amended serial API at 92f1ff3dcd9 consumes the exact current call's
DATA and, for a Mutation, its classify_write RefusedResend grant. It returns
attempt 2 immediately as a new Context of the SAME logical call, seeded with
the refused attempt's bundle revision. Queue that Context with the identical
Step body and Session binding. #2069 phase 3 (e04cf305e87) parks its Bind if the
bundle is NotReady or Ready at a revision no newer than that seed. A newer
revision announced by session_ready re-prepares the same attempt within its
existing window; the park/reprepare spends no further attempt or grant.

There is no separate serial Session park, waiter entry, Pending hand-back or
on_plan_session_wait_ended path. Expiry while parked is unreleased Timeout /
NotSent; retirement is Revoked / NotSent, and the common Plan projection owns
the whole command terminal. Attempt 2 receiving code 1000 again fails, with no
third attempt. No re-park/new-login loophole resets that budget. The venue keeps
auth_resent across all six immediate Read network attempts; a network retry is
a new serial Read call but cannot refresh the per-Step auth ledger. These are
OPEN source dependencies, not today's serial implementation.

## Session header audit and hosted material

Native account::Session stores cookie and account_id. Engine login_session is a
separate handle. send_login reuses a live engine handle after an earlier success;
Request::Login success changes the native fields and increments native generation.
account_response_headers can rotate either field and increment native generation
without retiring the engine Session. Failed/unproven login and authentication
failure retire the engine handle. account_refresh clears the native cache and
queues the original Pending while login may reuse the same engine Session.

Therefore the typical refresh is the same engine generation with a new header
revision, not a replacement anchor. The hook is NotReady while the cache is
cleared. Both first calls and auth resends must park until a Ready header set
has a newer revision; the still-Ready engine Session alone is insufficient.
A stale packet must reprepare; it cannot release its old bundle. If refresh
starts after Prepare/Bind, the shared same-call unreleased park must retain the
Pending instead of converting NotReady into a native terminal or spending an
auth resend. Under m26ps4's #2069 park plan, NotReady never reaches venue
failure code: the engine waits within the current window until a newer Ready
revision and re-prepares the same call. Expiry is Timeout/NotSent with health
unchanged. Retired/header-invalid remain terminal NotSent with existing non-auth
InvalidInput kinds. No new cause accessor, VenueError field, error-text inference
or auth-health evidence is needed. Failed login replacement is a
new engine generation. Any cookie OR account-ID change moves one shared revision;
session_bearer and session_bearer_revision read that exact same counter. Increment
is checked and overflow fails closed; the native unchecked generation counter
is not a provenance witness.

The bundle is exactly X-Ts-Grvt-Session, Cookie with native value
"rm=true; gravity=<cookie>", and X-Grvt-Account-Id. All are login-bound. Static
Content-Type remains recipe-owned. The recipe emits NONE of the three bundle
names, pinned by a recipe-header control. One revision covers the complete set.
Names colliding with recipe or provider headers refuse; values are redacted and
engine copies wiped. The reviewed fix validates combined recipe+signer+Session header
count and bytes, not separate lists. Host-owned copies follow existing signer
header disposal; do not claim JavaScript string zeroization.

#2069 currently inserts the bundle first. The overseer ruled that GRVT must
retain native Content-Type-first order, so a recipe-controlled ordering seam in
#2069 is OPEN with m26ps4. A native-wire original pins this order. Do not reorder
outside shared packet assembly or claim the current hook already fits.

Minimum hosted signing inputs are the existing login API credential and the
secp256k1 scalar needed for GRVT order signing, plus public account/chain/order
metadata. Hosted mode does not promise to keep that scalar only in the browser.
The provider owns installed secret refs; the venue recipe receives the actual
selected immutable identity and public derivation, never reconstructs a key from
KeyRef, signer_address, native cookie or an anchor. A stale expected signer_address
keeps its native warning behavior and does not replace the actual derived signer.
Roles are explicit: the login uses Slots::select(CredentialRole::SessionAuth)
and session_egress; order signing uses Primary (the scalar). Login API-key
presentation must use the existing SessionAuth seam so the API key never reaches
venue code. This plan does not claim the current raw login key is migrated.
Proposed constructor/setup changes need the named owners' exact review.
No credential is added to logs, Debug, Context or policy evidence.

The recipe preserves native EIP712 domain GRVT Exchange/version 0, chain 325 mainnet
or 326 testnet, Order/OrderWithBuilderFee schema, builder addresses/0.03 fee,
integer scaling and 65-byte r/s/v serialization. Signing nonce uses the native
UUID(signing entropy)->Keccak first 4-byte construction; generated client IDs use
identity entropy, not signing entropy. Preserve the collision guard and explicit
caller IDs. Materialize nonce, expiration, client IDs and canonical payload
ONCE per native Step. SignStep::prepare only reads these fields on every Stale
reprepare and auth resend; it never generates replacements. The random UUID
nonce MUST NOT use monotonic NonceClaim/reserve_nonce. Re-sign the same immutable
canonical payload on auth resend. No cached signature bytes,
raw private-key fallback or hand-written transport may bypass managed dispatch.
Native expiration is planning time + 1 h in nanoseconds; changes to that or nonce
placement under delayed preparation require explicit recipe/native review.

## Original controls and held suffixes

Commit this plan before compiling the new dedicated target. Observe actual
Engine/GRVT ingress, native startup/login and real Prepare/Bind/Claim where they
exist today; accept raw HTTP only in explicitly marked native/reply-ingress
probes. Managed-contract assertions require Prepare and never fabricate a permit,
Selection, Session generation or provider result. No missing API compile error
counts as a RED. Shared engine owners own their new-contract REDs separately.

Pin independent placement/cancel/symbol-read/amend/leverage first-call custody,
queued command wake from a foreign callback, bracket continuation, full owner
capacity before provisional effects, absent/held Prepare/Bind/provider cleanup,
unknown/rejected/partial progress and no replacement after execution. Pin the new
bound as proposed policy, not native evidence. Working controls retain native
schema/financial outcomes, exact one code 1000 login/body-identical resend, six
immediate Read network attempts and independent auth ledger, builder 10 s/2 s/4 s,
FIFO, typed builder causes and public responses unable to replace private state.
Add original 401/408/503 x parsed-1000/non-1000 cases and second-1000/no-third-send
controls. Pin native partial-progress outcome stripping separately from the
ruled future journal terminal. Refresh-between-Prepare-and-Claim controls cover
both a Mutation and a Read; their exact shared reissue suffix stays gated until
the named shared API exists. Pin account-ID-only revision movement, recipe-header
exclusion, immutable payload on Stale reprepare and native Content-Type-first.

Provider/Session-bundle/serial-resend suffixes that cannot run until the actual
recipe and shared pins exist remain explicit source gates after their first
Prepare assertion. Their presence in a test is not execution evidence. Pure
constructor API controls wait for the real typed installed-binding API; no fake
witness is allowed merely to make an original compile. Existing native tests and
financial assertions remain untouched. Proposed production only follows a new
scope instruction, exact shared API/source review and actual dependency landings.

Local Cargo is scoped to GRVT/new target with CARGO_BUILD_JOBS=4 and no concurrent
own build. No local workspace/all-feature/performance sweep, WIP push or new CI
run is part of this plan-and-originals-only assignment.

## Original checkpoint at 6944495298b

Plan-first 89c9448b92b and corrected plan/index 10fa03e4bf3 precede the new
request_context_write_funnel target. The target includes the existing trading.rs
unchanged, retaining all 42 native financial assertions. A thin delegator boots
native GRVT in the same Engine and admits 256 genuine Command Read peers for the
two capacity probes; no engine Session or selected identity is transplanted.

Scoped execution: 66 controls, 13 intended assertion REDs / 53 GREENs / 0 ignored.
The 24 new controls split into 13 REDs / 11 GREENs; all 42 existing trading
controls pass. No compile/setup failure is included in those numbers. The first
compile found a struct-pattern typo and a missing command limit field; both were
fixed before execution. A native no-seventh-Read fixture initially rejected the
normal account refresh after terminal; it now uses Inspect's /order endpoint and
pins no seventh Inspect while allowing native post-terminal account refresh.

First failures are ten genuine Prepare-before-transport assertions (five entry
families, bracket continuation, FIFO wake, new 20 s policy, held Prepare and
unsigned Bind/Claim), two capacity assertions (raw send for valid planning;
native InvalidInput rather than shared Internal for invalid planning), and an
unclaimed raw DATA completion assertion. The 20 s/expiry/Bind/Claim suffixes,
capacity recovery and 255-peer validation are gated, not executed evidence.

The 11 new GREENs pin the 401/408/503 x 1000/non-1000 matrix (with identical body,
rotated headers, second-1000 terminal and no third send), Content-Type-first,
six immediate Read network attempts and no seventh Inspect, auth spend retained
across a Read network retry, unanswered Mutation Unknown with no resend, and
native partial-progress stripping to None. The future shared journal terminal
is deliberately not claimed to equal that last native assertion.

Exact provider construction/recipe-header, account-ID-only revision, immutable
payload on Stale reprepare and Mutation/Read refresh-between-Prepare-and-Claim
controls are still source gates: the reviewed shared API and typed provider
constructor do not exist in this native base. #2069 owns the actual unreleased
wait REDs, not a message-based GRVT workaround. This original checkpoint neither
implements nor certifies those gates. No production instruction or push is
inferred from plan/original review.

Scoped strict Clippy for the new target passes with -D warnings; rustfmt and
diff-check pass. No workspace/all-feature build, WIP push or CI dispatch was run.

### Source-gate ownership and later controls

The following GRVT ingress controls land with the provider recipe and actual
shared dependencies, before any production claim; they are deferred, not tests
that compiled/executed in this original target:

| Named later control | Owner / dependency |
| --- | --- |
| refresh_between_prepare_and_claim_mutation_preserves_the_same_call | zs5e83 recipe integration; #2069 P1 owned by m26ps4 |
| refresh_between_prepare_and_claim_read_preserves_the_same_call | zs5e83 recipe integration; #2069 P2 owned by m26ps4 |
| account_id_only_rotation_moves_the_shared_revision | zs5e83 Session cache / #2069 revision signal |
| recipe_emits_none_of_the_session_bundle_headers | zs5e83 actual recipe and installed constructor |
| stale_reprepare_keeps_nonce_expiration_client_ids_and_payload | zs5e83 actual SignStep plus #2069 reprepare |

The engine-side refresh REDs are #2069 phase 3 P1/P2. GRVT ingress versions use
those reviewed semantics after the hook and typed recipe exist; no no-op stub
or missing-method test is offered as evidence.

### Corrected original receipt

The review correction keeps the unclaimed DATA assertion but removes the
included prepared_actions driver from that probe. It takes either native raw
HTTP (explicit reply ingress) or Prepare.request directly from raw wf_command,
without http_prepared, Bind or Claim; after adoption it still sends DATA against
an unclaimed request. The delegate now forwards cancel_needs_stored_order and
on_public_feed. Native status controls explicitly assert 401/408 Rejected and
503 Unknown for coded non-1000 and second-1000 terminals. Unparsed 408/503 are
Unknown with no resend. The reverse-ledger positive spends five Read network
retries before auth refresh, then proves the one auth resend does not reset the
remaining network budget.

The corrected target runs 69 controls: 13 intended assertion REDs / 56 GREENs /
0 ignored (27 new = 13 RED / 14 GREEN, plus the unchanged 42 native positives).
Strict scoped Clippy, rustfmt and diff-check pass. The same Prepare/capacity
suffixes remain gated; no added control claims the missing provider or shared
Session hooks execute at this base.
