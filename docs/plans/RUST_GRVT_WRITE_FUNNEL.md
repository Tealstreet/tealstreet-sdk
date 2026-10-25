# GRVT write funnel and shared prerequisites

Owner: codex-zs5e83. Scope ruling: claude-ddi2il. Native base: actual
bcaaa4acc567e8eca588c067d67ea38e7f7c9b98. PLAN AND ORIGINAL REDS ONLY.
That original checkpoint authorized no production edits. The separately
assigned local provider recipe now exists; native queue/login and installed
construction remain held at G3/G6. The current dependency table below records
landed shared support separately from GRVT adoption. Hosted signing uses Sam's single
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
Today the write-completion path intercepts 408/5xx before on_response, so the
serial resend PR also owns defaulted Venue::status_exempt_refusal(pending,
response) -> bool. GRVT returns true ONLY for a parser-proven code 1000, allowing
that exact DATA into the codec and classify_write RefusedResend. Other bodies
stay on the shared Unknown path; no error-text or status-only exemption.

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

Landed shared support, originally owned by claude-fe7lgm:
- Streamed serial Plans: #2092, ff219538b302, plan RUST_PLAN_SERIAL_STREAM.md.
- Condition waits: #2094, ba6fb1912399, plan RUST_CONDITION_WAIT.md.
Both are non-default features. Seeded Session resend/status exemption landed
separately in #2146 at f895d0d7018, implemented by codex-zs5e83 and reviewed by
fe7lgm. The local recipe branch contains these landings; GRVT has not adopted them.

Proposed consumer flow, subject to exact API composition review:
1. In each genuine entry, begin_preflight(WriteAfterReads, selected read role or
   None, Primary write credential). Do not borrow an earlier job's root or ID.
2. Behind an active job or builder check, park_preflight_for(preflight, None).
   A native callback only calls request_preflight_ready. Admission or failure
   occurs later in that command's on_http_preflight_ready; re-park is progress.
3. When all native prerequisites are ready, admit_serial_plan with the selected
   Primary credential, no pacing floor and the appropriate first Read/Mutation.
   Declare attempts: 2 on EVERY Mutation and Read SerialCall, reserving the
   one auth resend. Reads with two attempts use the landed #2146 exception.
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

## Shared support and remaining adoption gates

| Dependency | Owner | Current state / required proof |
| --- | --- | --- |
| Serial Plan and condition waits | claude-fe7lgm | Landed #2092/#2094 as non-default features. GRVT opt-in and exact FIFO/ready-custody composition remain adoption work. |
| Same-call serial Session resend and immediate failed-Read continuation | codex-zs5e83; reviewer claude-fe7lgm | Landed #2146 at f895d0d7018. Same-call attempt 2 requires the exact RefusedResend grant or validated Read DATA and the released Session revision; GRVT must preserve its independent auth and network ledgers. |
| Revision-gated calls, refresh after Prepare/Bind and cached header bundle | claude-m26ps4 | Landed #2069, non-default session-bearer. Its shared unreleased park retains the call/window without another grant or attempt; GRVT still needs native refresh/revision ingress controls and checked revision updates. |
| Parsed auth-refusal delivery | codex-zs5e83; reviewer claude-fe7lgm | status_exempt_refusal is landed in #2146 and consulted only on complete Unproven responses. The GRVT override must parse code 1000 exactly; other 408/5xx stay Unknown. |
| Content-Type-first header ordering | claude-m26ps4 | Shared bundle placement is landed in #2069. GRVT must declare After("Content-Type") with the Session bundle and retain native wire controls. |
| GRVT EIP712 provider-payload HTTP recipe | codex-zs5e83; reviewers codex-9czdjh and claude-fe7lgm | Local recipe source 8c6950517eb is CLEAR; D1 accessor/control composition is under carry review. Installed constructor and actual sender remain unconverted. |
| Credential-only login intent and Optional absent-signing disposition | claude-m26ps4; source reviewer kuhwxi | #2149 landed the revision-4 design only. D3/D6 phase-3 source is local and must land before composition. No synthetic Identity/ref or local latch workaround. |
| Native login/header lifetime and serial FIFO cutover | codex-zs5e83 with core review | Held at the constructor boundary. Use shared setup facts, a real Session and one revision for all bundle values; preserve native eligibility/precedence and execute all currently gated originals. |

The amended serial API at 92f1ff3dcd9 consumes the exact current call's
DATA and, for a Mutation, its classify_write RefusedResend grant. It returns
attempt 2 immediately as a new Context of the SAME logical call, seeded with
the refused attempt's bundle revision. Queue that Context with the identical
Step body and Session binding. #2069 phase 3 (3900639fcf4) parks its Bind if the
bundle is NotReady or Ready at a revision no newer than that seed. A newer
revision announced by session_ready re-prepares the same attempt within its
existing window; the park/reprepare spends no further attempt or grant.

There is no separate serial Session park, waiter entry, Pending hand-back or
on_plan_session_wait_ended path. Expiry while parked is unreleased Timeout /
NotSent; retirement is Revoked / NotSent, and the common Plan projection owns
the whole command terminal. Attempt 2 receiving code 1000 again fails, with no
third attempt. No re-park/new-login loophole resets that budget. The venue keeps
auth_resent across all six immediate Read network attempts; a network retry is
a new serial Read call but cannot refresh the per-Step auth ledger. The shared support is landed; these are GRVT adoption obligations. The future
consumer must prove them through real managed transport and its native ledger.

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
is checked and overflow fails closed. EVERY successful refresh moves the
revision even when cookie/account values are identical, or seeded attempt 2
would never wake. Native login success already increments its generation in
that case; the native unchecked counter is not a provenance witness.

The bundle is exactly X-Ts-Grvt-Session, Cookie with native value
"rm=true; gravity=<cookie>", and X-Grvt-Account-Id. All are login-bound. Static
Content-Type remains recipe-owned. The recipe emits NONE of the three bundle
names, pinned by a recipe-header control. One revision covers the complete set.
Names colliding with recipe or provider headers refuse; values are redacted and
engine copies wiped. The reviewed fix validates combined recipe+signer+Session header
count and bytes, not separate lists. Host-owned copies follow existing signer
header disposal; do not claim JavaScript string zeroization.

#2069 currently inserts the bundle first. The overseer ruled that GRVT must
retain native Content-Type-first order. The reviewed phase 3 plan 3900639fcf4
uses SessionHeaders::placed(SessionPlacement::After("Content-Type")), declared
by session_bearer for the whole bundle and applied by shared assembly. The
recipe declares neither placement nor bundle headers. Source remains OPEN with
m26ps4. A native-wire original pins this order. The five-site funnel uses POST
with Content-Type; any future no-body GET adoption pins that native order too.

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

Plan and originals at bf61d06542a are CLEAR from fe7lgm (core), 9czdjh
(native) and m26ps4 (bundle/recipe fit), read-only reviews. The placement wording
above follows the newer reviewed #2069 plan; it adds no runtime or test change.

The later source rows now name fe7lgm's 7fd52ecee33 status-exempt parsed-refusal
hook and m26ps4's 3900639fcf4 successful-refresh revision obligation. These are
shared owner plans, not installed APIs or new GRVT source in this checkpoint.

## Provider prerequisite planning after the local cuts

RUST_GRVT_PROVIDER_RECIPE.md now specifies the EIP712 and login recipes and
installed constructor, with exactly the API login key and order-signing scalar
as the hosted/local secret inputs. Reviewers are 9czdjh for native recipe and
construction, and fe7lgm for same-call resend. This is plan-only authorization;
bf61 originals remain byte-exact and no builds are permitted during size builds.
GRVT will explicitly enable non-default serial-plans (#2092), condition-waits
(#2094, for re-parking a woken command from ready custody) and session-bearer
at adoption. All three are now landed non-default Cargo features; this
supersedes their proposal-only status in the original recipe review.
The adopter gets its own measured WASM budget review for these features plus
dispatch-signing and secp256k1. Exact source pins, credential-only construction
and external signature acceptance remain named gates in the recipe plan.
bf61's native full-body resend evidence is local-provider parity. The overseer
approved external r/s/v-only variation with identical digest, pinned actual
signer and every frozen order/unsigned field, verified low-s and fail-closed
wrong-signer/digest results. No cached signed bytes or renewed order identity.
The recipe plan and CALLERS carry that future contract; source still needs its
actual external-provider acceptance/refusal controls and shared dependencies.

On resumption at actual 76f1f7fc842, #2092 serial-plans, #2094 condition-waits
and #2069 session-bearer have landed with their non-default feature declarations.
This supersedes the earlier proposal-only feature status. Seeded serial auth
resend/status exemption and credential-only setup remain adoption gates.
RUST_GRVT_PROVIDER_RECIPE.md records G2-G6 consumer answers accepted by m26ps4
as plan fit: he owns the Empty login-intent seam and the proposed Optional
per-operation absence/latch distinction. No source clearance is inferred.

The overseer has approved G6: intentional Optional signing absence refuses
only the signed operation, typed NotSent, with no account latch; unsigned
operations remain usable. Required-family MissingKey latching remains #2109.
m26ps4 records the shared revision and kuhwxi reviews its source before adoption.
The command terminal still follows the Plan journal, including earlier settled
effects; an unissued final operation does not make the whole command NotSent.
The provider plan adds a serial prior-settlement/absent-ref control for this.
The seeded resend/status implementation has now landed as #2146 at f895d0d7018.
The local provider recipe contains that actual tip, with engine/scripts/harness
incoming-exact. This closes the shared resend source gate; G3/G6 implementation,
installed construction and native funnel adoption remain held. The bf61 original
target is unchanged and its thirteen intentional custody REDs are not called
GREEN by isolated recipe tests.
