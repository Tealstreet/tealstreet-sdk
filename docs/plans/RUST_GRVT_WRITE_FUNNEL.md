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

Base trading inventory is8. The separate fixed-settings #2076 owns3, leaving5;
this plan does not prune them early or claim any current removal. After the four
pending local/catalog slices GRVT has25, projecting20 only after complete future
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

Code1000 authentication refusal is handled only from on_response DATA. If
!auth_resent, the same Pending is reset, queued for login and sent once afterward.
The per-Step auth_resent flag and the network retry count are independent and
neither resets the other. Existing trading.rs control
explicit_session_rejection_resends_identical_signed_order_once proves the resent
Step.body is byte-identical, while cookie/account headers change. A second1000
fails with no third send or second login. This includes cancel and verification
Reads, not just creation. Unknown, 408 and 5xx grant no Mutation resend.

Keep every native client/native ID, large integer, accepted-ID order, bracket
reduce-only/side/trigger, cancellation reconciliation and amend execution/lifecycle
proof. An ACK alone never authorizes replacement. Only native decoded facts enter
http_write_fact/http_write_batch; a provisional Store Creating/Canceling effect is
not an Accepted fact. Reserve owner/call/provider work before any provisional
native effect, and pin its native cleanup on refusal. Shared journal projection
must preserve all earlier accepted IDs and Unknown; no numeric terminal or
successful-empty substitution may bypass it.

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
   Issue only the one current Slot, through managed transport and the recipe.
4. Validate exact Plan DATA before every codec/reducer/continuation. Record only
   proven write facts; continue_serial_plan/continue_serial_plan_after or park
   through a reviewed shared seam. No next call follows uncertainty.
5. Empty native jobs use done_empty_plan. Completion and failure use the common
   Plan projection, which alone releases queue ownership and order locks.

No WriteAfterReads Preflight binds the caller receipt. Serial roots have no
wall-clock; receipt expiry while queued or mid-chain is not a terminal. Each call
has its own bound. A proposed20s call bound would newly include route, Prepare,
Bind, provider and physical I/O: native explicitly has no timeout. This remains
an OPEN behavior/source ruling for GRVT, to be disclosed in CALLERS; it is not a
claim that native had a20s cap. No new command step/order/page cap is proposed.
The existing256 command-owner capacity becomes a typed NotSent admission refusal
before queue, signing or provisional Store effects; disclose this new capacity
precedence. GRVT's existing order-lock configuration must remain the audit base,
not be silently replaced with another venue's64-scope configuration.

Builder check geometry does not need a new Preflight delay-then-ready API:
get_authorized_builders is a separate account read with10s native timeout and
three attempts with2s/4s backoff. Commands park while it runs. Its final DATA or
failure releases the command's ready callback, which proceeds or returns the
native typed builder cause. The retry timer does not grant command custody.
Keep the existing builder transport/account scope unless separately assigned;
this five-site plan does not claim its account inventory removal.

## Shared gaps and proposed owners

| Gap / dependency | Owner proposal | Exact status / required proof |
| --- | --- | --- |
| Serial Plan and condition wait actual composition | claude-fe7lgm | Source pins above CLEAR but unlanded. Actual #2012/#2037 plus refrozen pins required; one owner, one reserved Context, no positive pacing floor. |
| Same-call serial Session resend and immediate failed-Read continuation | claude-fe7lgm | RUST_SERIAL_SESSION_RESEND.md at606488cba17 (initial239c705f2b6) is a PLAN, OPEN before source. Same call attempt2 after code1000, Mutation ResendGrant or validated Read DATA, Session park with no third send; immediate Read failure continuation carries no fabricated DATA or1ms delay. |
| Cached Session header bundle | claude-m26ps4 | #2069 generalized source dd7fd51d03a is consumer-reviewed CLEAR locally, unlanded. Needs actual #2037/G2b and final pin. Bundle generation/revision, aggregate packet budgets, collision refusal and cleanup must compose with serial waits. |
| GRVT EIP712 provider-payload HTTP recipe | codex-zs5e83, reviewed by m26ps4 and core/custody owner | OPEN consumer recipe/constructor work. Existing provider Secp256k1Digest/PublicAddress primitives suffice; no new crypto algorithm or legacy SignPayload bypass. Exact recipe API and construction witness reviewed before source. |
| Login completion / credential and header lifetime | codex-zs5e83 with claude-55ao7t login ownership and m26ps4 core review | OPEN consumer composition: native login_session is a login-operation engine Session, distinct from cached cookie/account state. Connect honest Session-ready/failed transitions and provider installation; never mint a substitute Session or treat a native counter as engine provenance. |
| GRVT new per-call bound and header ordering | claude-ddi2il ruling, native review codex-9czdjh | OPEN source decisions: proposed20s policy and general bundle-first ordering differ from native unset timeout / Content-Type-first headers. Keep them explicit before production. |

The Session resend proposal consumes the exact current call's DATA and, for a
Mutation, its classify_write RefusedResend grant. It parks the reserved remaining
attempt, not a new logical write or fake Read. On Session ready, the same call
resumes through on_http_plan_ready with a new Context/window and provider work.
Session failure/retire may re-park that same remaining attempt on a fresh Session
or fail through the shared terminal. The venue retains auth_resent across the
six immediate network Read attempts; there is no extra login after the flag is
spent. The604/606 proposal, not today's serial implementation, owns this gap.

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
cleared, so writes must park and issue only after login is Ready. A stale packet
must reprepare; it cannot release its old bundle. Failed login replacement is a
new engine generation. Revision increment must be checked and fail closed on
overflow; the native unchecked generation counter is not a provenance witness.

The bundle is exactly X-Ts-Grvt-Session, Cookie with native value
"rm=true; gravity=<cookie>", and X-Grvt-Account-Id. All are login-bound. Static
Content-Type remains recipe-owned. One revision covers the complete set. Names
colliding with recipe or provider headers refuse; values are redacted and engine
copies wiped. The reviewed fix validates combined recipe+signer+Session header
count and bytes, not separate lists. Host-owned copies follow existing signer
header disposal; do not claim JavaScript string zeroization.

#2069's general bundle-first policy differs from GRVT's native Content-Type-first
order. Name this wire ordering adjustment rather than claiming exact header order;
native header values, spelling, duplication policy and authentication remain.

Minimum hosted signing inputs are the existing login API credential and the
secp256k1 scalar needed for GRVT order signing, plus public account/chain/order
metadata. Hosted mode does not promise to keep that scalar only in the browser.
The provider owns installed secret refs; the venue recipe receives the actual
selected immutable identity and public derivation, never reconstructs a key from
KeyRef, signer_address, native cookie or an anchor. A stale expected signer_address
keeps its native warning behavior and does not replace the actual derived signer.
API-key login presentation and installed identity must match the same Primary
setup; this plan does not claim the existing raw API-key login input is already
migrated. Proposed constructor/setup changes need the named owners' exact review.
No credential is added to logs, Debug, Context or policy evidence.

The recipe preserves native EIP712 domain GRVT Exchange/version0, chain325 mainnet
or326 testnet, Order/OrderWithBuilderFee schema, builder addresses/0.03 fee,
integer scaling and65-byte r/s/v serialization. Signing nonce uses the native
UUID(signing entropy)->Keccak first4-byte construction; generated client IDs use
identity entropy, not signing entropy. Preserve the collision guard and explicit
caller IDs. Materialize once per logical native Step and re-sign its immutable
canonical nonce/expiration/client IDs on auth resend. No cached signature bytes,
raw private-key fallback or hand-written transport may bypass managed dispatch.
Native expiration is planning time+1h in nanoseconds; changes to that or nonce
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
schema/financial outcomes, exact one code1000 login/body-identical resend, six
immediate Read network attempts and independent auth ledger, builder10s/2s/4s,
FIFO, typed builder causes and public responses unable to replace private state.

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
