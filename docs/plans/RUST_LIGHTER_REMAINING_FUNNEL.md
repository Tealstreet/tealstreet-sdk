# Lighter remaining request and signing funnel

Status: source CLEAR from claude-dazzy3; composed qualification in progress.
The source stack consumes recipe PR #2204 and principal PR #2208; neither is duplicated.
Original RED/GREEN baseline: unchanged fff73624e8578c623a41875b3946d0c665f561af.
Owner: codex-ih35js. One venue PR will contain this plan, original regressions
and the source. Venue reviewer: claude-dazzy3. Credential reviewer:
claude-m26ps4. The recipe/provider prerequisite is consumed unchanged.

## Outcome and boundary

Finish the remaining HTTP and eager-signing paths for Lighter and
Lighter-Robinhood, including both deployments. Commands retain genuine entry
authority across their entire operation; private bytes are signed only by the
installed provider after managed admission. There is no raw HTTP fallback,
scalar-bearing production venue signer, or numeric command completion.

The standard is better than TypeScript. Keep order values, target accounts,
nonce safety, required sends and capabilities. Prefer simple conservative
refusals and exact delivery validation over reproducing diagnostic wording,
timing quirks or races. Pin behavior changes and describe them in CALLERS.
An extra write, a dropped required write or a less safe financial outcome still
requires the overseer's disposition.

This adopts existing engine admission, Read history, Preflight, serial Plan,
condition waits, dispatcher and signing APIs. It does not implement a venue
transport scheduler, retry journal, credential registry or signing primitive.
The socket Supervisor stays unchanged; only its eager private auth-token send
changes to the installed dispatch recipe. Managed socket adoption is separate.

## Exact inventory at the base

| Source | Request occurrences | Signing occurrences | Destination |
| --- | ---: | ---: | --- |
| account.rs | 5 | 3 | Managed Background reads, owned tier mutation, installed auth recipe |
| history.rs | 3 | 1 | Original caller Read root, managed page legs and ReadCursorHistory |
| limits.rs | 5 | 0 | Managed coalesced limits refresh and owned tier action |
| trading.rs | 7 | 0 | Bound Preflight, managed pre-reads, serial mutation Plan |
| lib.rs | 0 | 9 | Remove material retention and construct provider-bound venue |
| Total | 20 | 13 | Exact production request/signing inventory prune |

The two history enclosing-macro observations are not extra sends. Do not rename
their bindings to evade the scanner. Prune only observations that the actual
conversion removes or that the checker owner proves. Cleanup-only secret
erasure entries, if still needed by the setup API, are reported separately
from live material or signing paths; no broad checker exemption is added.

Other families' inventory entries remain incoming byte for byte. Public
discovery, ticker, depth, candle, identity and local completion migrations are
already landed and remain supported. The old request plan's 51-site count and
unlanded dependency claims do not describe this base.

## Installed signing and construction

The landed transaction codec and TxRecipe remain the canonical transaction
preimage, form body and nonce binding. Existing independent vectors for Create,
Cancel, Leverage, builder attributes, high market IDs, deployments and hedged
entropy remain unchanged. Do not copy a signing algorithm into the venue.

Replace account.signer with an installed Primary Identity and public Account
scope (chain, resolved account index and API-key index). Resolve account identity
with the existing public candidate restrictions; never sign another account's
requests merely because a fallback returned a different account. No secondary
identity or route is used for writes. Public account snapshot and nextNonce
reads remain unsigned where they are unsigned today.

An AuthRecipe uses the same padded deadline/account/API-index message and the
shared SchnorrDigest provider operation. Preparation draws signing entropy and
stamps expiry; apply validates the 80-byte result and writes only the intended
query auth token, authorization header or private subscription frame. HTTP
host/path/method and WS target are bound before signing. Re-preparation refreshes
time and entropy without changing transaction nonces or financial fields.

Use request_signed_managed for authenticated HTTP and send_signed for the
private order subscription. The public account subscription remains unsigned.
No expired signed token is cached or passed to an unsigned managed sender.

Construction declarations for both family names activate dispatch signing and
the existing Schnorr algorithm/hex setup policy. Registry setup installs the
provider and issued identities before start; raw scalar material never reaches
Lighter. Preserve public-only and keyless operation and account/API-index public
metadata. Refuse foreign identities and unsupported material/setup injection.
Fixture setup moves genuine synthetic keys into test support, not production.

### Signing prerequisite and pinned API

claude-m26ps4 owns parts 2+3 as one prerequisite PR: AuthRecipe and the missing
SchnorrDigest public-key arm/checks. This venue PR does not implement those
bodies and cannot land before that prerequisite. No source publication occurs
until the recipe owner confirms the local and hosted SchnorrDigest activation
contract; Required authentication has no unsigned fallback. LocalSigner capability
activation, implemented(SchnorrDigest) = cfg!(feature = "schnorr"), remains in
this admission cutover, in the same change that deletes the eager holder.
Remote recipe IDs and verification remain held behind #1926; no unsupported
remote capability is advertised.

The #2204 API is AuthRecipe::new(identity, account, origin, placement), with
AuthPlacement::Query, Header, or WsSubscribe { socket, url, channel }. The WS
context uses the explicit public socket URL; chain ID never supplies an origin.
Preparation binds exact method, host, path, body and outbound bytes. Apply
refuses altered bytes or a different socket. Auth query-name collisions and
foreign authorization headers refuse before signing. The provider public-key
result is canonical, on-curve and 40 bytes; malformed or zero scalar material
is refused at trusted setup. Existing recipe and financial vectors remain gates.

### Trusted setup capability pins

A local scalar plus account index and API-key index works today without key.
The generic local-provider capsule requires a nonempty credential value to issue
an admissible Primary. The reviewed principal API is LocalSigner::from_setup_with_lighter_principal
from shared PR #2208. Its typed LighterPrincipal API index issues opaque managed
membership through trusted setup, never a venue-created Identity/KeyRef or an
arbitrary label. The registry receives the original SetupFacts before any
mapping. Lighter never interprets primary_login; Blank remains Blank even though
other families use that fact for login intent. Preserve original KeyPresence
(absent, empty, blank or null), so no label becomes a claimed supplied API key.
The principal is not a wire credential, nonce subject or address proof; actual
signing capability and resolved Account/API-index provenance remain authority.

The optional key is also a public L1 address selector. Preserve only a validated
public address as explicit venue metadata before the bridge strips credentials,
keeping both application-index and L1-address lookup candidates. Scalar material
stays exclusively in trusted provider setup; public identity metadata is not a
signing secret and must not substitute for proof of the installed Primary.

## Background reads and publication

Lookup, snapshot, open orders, fill backfill and limits requests each carry an
engine-issued Background Read Context. Public reads have credential None;
authenticated reads carry the installed Primary credential and recipe. Physical
calls use a 20-second managed bound with RetryPolicy::Never. Preparation and
queueing are included in that bound; record this coverage in CALLERS.

Validate the exact delivery before any streamed staging, counters, revision,
cursor, selected-account or Store effect. Close each context on success,
malformed stream, typed failure, supersession and rejected queue. Provider or
integrity failures retain their kind; do not infer transport loss from text.
Late or unclaimed callbacks publish nothing and cannot wake a different owner.

Keep snapshot revision/coverage checks, all-market versus per-market order
fallback, outage epochs, execution deduplication and provisional staging. A bad
page contributes no rows. Existing accepted-page recovery remains explicit;
only accepted rows can be returned or published, with incomplete coverage where
required. Background retirement clears only its matching in-flight generation,
so the ordinary poll can recover without a command terminal.

Limits and account snapshot are coalesced producers. A trading waiter observes
the matching snapshot serial or limits result, then queues its own Preflight
ready intent. The Background DATA does not complete or issue a write on behalf
of that command. Stale/failed reads cannot confirm reduce-only exposure.

## Caller history and tier commands

Keep local validation before remote work, then admit one original caller Read
root and construct ReadCompletion/ReadCursorHistory from it. Every page and
stage is a child of that original root, never of the previous page, with a
20-second per-call bound capped by the original caller receipt.

Every item, envelope, cursor and stage transition first validates its exact
page delivery. Use the existing CursorAdmission to stage and advance; preserve
trade-to-funding stage boundaries, sorting, limits and deduplication. Retire the
old page before issuing another. A failed page discards only its provisional
rows; accepted prefix/incomplete behavior stays explicit and tested. Cursor
stall, page cap and saturated-without-cursor proof failures remain strict.
No unclaimed page, stale failure or old root can complete a reused command ID.

FetchAccountTier remains an admitted local Read of the cache. ChangeAccountTier
is a genuine managed Mutation with auth recipe, write facts and exactly one
owned terminal. Its subsequent limits refresh is Background work. Unknown
transport results never become definitive acceptance or automatic resend.

## Trading authority and nonce safety

Admit WriteAfterReads at the genuine command entry after local validation and
before progress or Store publication. Retain the installed Primary write
credential. Empty jobs use done_empty_plan. FIFO jobs park their exact Preflight
and resume only through on_http_preflight_ready. External limits/snapshot
completion requests readiness; it does not manufacture entry or DATA custody.
Use engine condition waits for the existing bounded exposure confirmation and
for unbounded FIFO/cache waits, with failure cleanup on owner retirement.

Approval and nextNonce reads are managed children of the job's Preflight.
Private limits reads stay on their coalesced Background producer. No resource
allocation precedes genuine admission in on_command. Account readiness, builder
tier/approval and confirmed reduce-only exposure remain write prerequisites.

When prerequisites are proven, admit a serial Plan for the finite job. Each
submission uses TxRecipe and one managed Mutation call with a 20-second bound
and no transport replay. Record facts before Store effects or continuation.
Accepted placement IDs use the native client-order indexes, in job order;
cancel/setting effects without IDs still count. done_plan/fail_plan own the only
terminal and journal projection. Partial or unproven batch acceptance preserves
proven IDs and uncertainty; do not invent individual batch acknowledgements.

Choose serial continuation (not a same-leg RefusedResend) for one venue-proven
21104 nonce refusal: record the rejected Mutation, delay 150 ms on the Plan
timer before an unsigned nextNonce Read, then issue a NEW Mutation for the
same unresolved step. SerialPolicy.refusals_complete is true: a later Accepted
step must finish as plain success, despite the retained refusal report. The
original proven_21104_reseed_then_acceptance_is_plain_success pins that result. It requires definitive current-call DATA, settled
refusal evidence, and the job's one-reseed budget. A transport/integrity failure,
5xx, 405/429, ambiguous batch or unproven acknowledgement cannot trigger that
resend. A subsequent nonce read is a fresh owned Read call; mutation continuation
requires its exact successful DATA. Only the fresh nonce Read’s exact DATA may call the engine counter seed API;
never seed from stale reads, message text or a venue-local nonce assignment.
Re-preparation retains reserved transaction nonces and client indexes.

Core/nonce fit is CLEAR from codex-uufub4 at 42293b511fd against fff73624e85:
retain engine::nonce::VenueCounter<MAX_SAFE>, ticket/seed/take/invalidate and
the issued (Transaction, u64) through TxRecipe re-preparation. Only exact fresh
nextNonce DATA seeds the captured original SeedTicket; stale and duplicate
DATA is refused. The one-reseed budget belongs to the original job. There is
no reviewed P5 seeded-counter API in clock-based dispatchNonceClaim, so this
consumer does not enable dispatch-nonce or import its clock maxima/repair
budget. A second 21104 or failed seed explicitly calls fail_plan; permission
from refusals_complete never converts that decision into done_plan. This is
narrow API fit, not source/adoption clearance. Receipt is
/tmp/codex-uuf-lighter-core-plan-review.json.

Preserve financial reducers, safe integer ranges, order expiry, amendment fill
checks, builder fees and the unknown-setting reconciliation barrier. Optimistic
state/progress is published only after admission. Engine-driven Plan retirement
must remove the exact active job, clear waits and reconcile released uncertainty
without issuing another command terminal or resending an unknown write.

## Original regressions and controls

Add tests before each production cut and retain their assertion bodies. Use
genuine engine commands, caller receipts, prepared/bind/claim delivery, installed
LocalSigner identities and monotonically advancing time; no forged contexts.

1. All remaining HTTP paths wait for route preparation; refusal releases no
   physical bytes. Cover lookup, snapshot, orders, backfill, history, limits,
   tier change, approval, nextNonce and transaction submission.
2. Unclaimed, duplicate and superseded streamed DATA causes no publication,
   stage advance, retry, command terminal or unrelated readiness wake.
3. Caller expiry bounds the whole history walk and both stages; exact equality,
   parked pages and reused command IDs settle once. Preserve accepted prefixes
   plus incomplete coverage on later-page failure; reject bad provisional rows.
4. Provider/integrity refusals keep their type, never retry a write, clear only
   matching in-flight state and allow later independent Background recovery.
5. Genuine installed signer produces existing transaction/auth vectors; route
   parking, async provider refusal and re-preparation keep nonce/account/target
   bindings and never release unsigned private bytes.
6. No scalar reaches venue options, serialized bridge options or Debug; keyless,
   public-only, malformed scalar, identity lookup and both deployment variants
   retain supported behavior. Unsupported remote capability fails closed.
7. FIFO and coalesced waits cannot send before tier/approval/exposure proof;
   stale snapshots and other-symbol results cannot confirm a reduce-only order.
8. A proven 21104 permits exactly one reseed; released Network, physical Timeout,
   malformed proof, 5xx and a second 21104 never resend. Keep accepted IDs and
   effective id-less actions through later failure.
9. Tier success, definitive rejection and Unknown each settle once. Preparation
   refusal mutates no settings; a later limits poll owns no command authority.
10. Preserve existing order values, vector/signature oracles, record revisions,
    fill deduplication, snapshot coverage and amendment race controls.

## Freeze, size and landing

Run scoped native Lighter tests, strict all-target clippy and fmt; request,
signing, credential and socket inventories/bans must match actual source. Keep
all original financial/vector assertions, not rewritten expected outcomes.
Obtain exact source/custody review and fix findings before the qualifying push.

CI is the family-size record for Lighter and Lighter-Robinhood. Locally measure
one combined artifact on the actual composed landing tip before READY. Use the
unchanged shipping harness, Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132,
jobs 4 and default flags; delete each scratch target after measurement. No local
family matrix. Venue-only growth up to 5% follows the standing allowance, with
the CI-rounded KiB cap and combined still under its enforced ceiling. Any
generic engine change additionally measures Bybit and Bitget on that same tree.
Measure exactly the features enabled by this consumer; no unused dispatch-nonce
feature is silently enabled. Combined growth above 30K is referred to the
overseer, including possible Engine<V> monomorphization cost and 55ao7t’s
per-venue gating work. Claim only measured bytes, not an estimate from site counts.

One PR and one qualifying PR-triggered run; no job reruns or speculative CI
dispatch. READY requires reviewed exact source, the required green gates,
recorded family/combined evidence and the auditor's exact-tree landing GO.

## Original execution ledger

The first local original target is request_context_remaining_funnel.rs, 14
controls on the unchanged fff73624e85 runtime. Execution on local original head 86a543ed20f produced 11 assertion REDs and
3 GREEN controls, zero ignored, with unchanged runtime. All nine preparation
REDs fail at the physical-before-preparation assertion; unclaimed history fails
at terminal authority; expiry fails at the one-terminal-at-equality assertion. Preparation controls cover lookup, snapshot,
orders, private history, limits, approval, nonce, submission and tier mutation.
Separate controls cover unclaimed history and receipt equality. Native controls
cover unsigned public history, exact transaction values/result IDs and successful
21104 reseeding. Their assertion bodies are frozen after baseline execution.

Explicit source-stage controls still to add and execute: backfill_waits_for_
preparation; typed_provider_failure_does_not_retry_a_write; stale_nonce_read_
cannot_seed_after_invalidation; second_21104_has_no_second_reseed; accepted_
history_prefix_survives_a_failed_later_page; rejected_page_staging_is_discarded;
coalesced_snapshot_only_wakes_its_matching_waiter; owner_retirement_releases_
active_fifo_once; auth_recipe_reprepare_retains_the_reserved_transaction_nonce;
raw_scalar_never_reaches_venue_or_bridge_options. Existing independent
financial/signing/coverage assertions remain required alongside these additions.

Original-review pins: the reseed control selects exactly reply-time + 150 ms
and checks the successful result is the original ClientOrderIndex, not merely
a Done notice. The strengthened native-GREEN body passed on the unchanged fff73624e85 runtime
using original head 1513268d185 (one selected test, zero failures or ignored);
its scratch target was deleted. Source-stage gates explicitly include typed provider
refusal kind/no write retry, scalar absence from options/serialized bridge/Debug,
FIFO/coalesced tier/approval/exposure waits, tier success/refusal/Unknown once,
and the second-21104 explicit failure/no third send.

Two additional native capability controls passed on unchanged fff73624e85:
public_l1_setup_keeps_the_address_lookup_candidate and
secret_only_setup_keeps_the_explicit_account_lookup (two selected tests, zero
failures or ignored; scratch target deleted). They preserve the first public
lookup URL and unsigned headers for a key-supplied L1 selector versus an absent
key with explicit account index. The original fourteen bodies are unchanged
from the strengthened 1513268d185 set.

## Local source dispositions

Sequential prerequisite DATA hands off with finish_preflight_call, preserving
its Preflight generation; a generic HTTP retirement is insufficient. A native
position confirmation uses shared condition readiness and may start immediately
after a prior accepted leg rather than the old 100 ms venue timer. A fill already
observed before the cancel ACK rejects an amendment immediately, before pacing.
Neither permits an extra placement.

Definitive documented provider refusals remain Rejected with native codes;
classification may expose Rejected instead of the old RateLimited diagnostic.
Unproven released responses and transport loss remain Unknown without replay,
and need not retain untrusted body text. Transport history failures retain only
accepted prefixes and an incomplete marker; signing/provider preparation refusal
and cursor-integrity proof failure stay strict. Provisional failed pages never
enter accepted history.

The frozen eager signing codec remains only in tests/support/reference_signer.rs
as a compatibility codec sharing production DTOs and cryptography. Its authority
comes from tests/signing.rs, which pins it to the independent TS and Go-WASM
corpus. Production Options contains public
account selectors and opaque issued Identity refs; no key or scalar holder.
Native and JSON setup controls pin all original key presences, nonblank Present
precedence, principal nonpresentation, invalid scalar/index/secondary/provider
refusal, and public L1 extraction before credential stripping. Longer hex material
is never copied as a truncated address. Raw injected public lookup metadata must
be exactly an address.

The reviewed source passes 320 native tests with none ignored, strict all-target
Clippy and fmt. The retirement FIFO control is assertion RED when only its two
settlement hooks are removed, then GREEN when restored. The composed tree still
requires its registry, bridge, inventory, replay and shipping-size qualification
before READY; the PR records the exact heads and receipts.

The request map retains three unchanged opaque observations: trading's
0bc3151ddc1e8889eb3a752d5431f3ab5643750a38409c18c5e239d38dcfc4b2
(matches!(&request, Request::Submit(_))), and history's
5b3fa60c35b90d7d54c1c78a6208717da15fe27f3581b790ac0bc0f5cc640ca1 and
2cb4373904a9bd645f5f03689ee4a667afacb9de41ec48ddb42d1b05efe59004
(matches!(request.read, Read::Trades / Read::Income(IncomeKind::All))). They
are inherited checker false positives, not sends. Seventeen observations are
removed; every actual remaining send is managed. No source rewrite hides these
payloads; the checker owner receives the shapes for its separate precision work.

Expired Background preparations and physical reads release their own inflight
state from the engine failure callback even when the Context is past its clock.
Live DATA still needs exact delivery validation before any Store effect. Two
additive expiry controls fail before that cleanup and pass after it; a late
preparation never binds and a later independent poll recovers.

Malformed/noncanonical scalar setup, a private API index outside 0..=254, and an
unsupported local signer provider now refuse at trusted setup rather than
silently degrading that invalid local configuration into unsigned snapshots.
Valid remote-only configurations keep public account snapshots and unsigned
public history, with no private signing or order submission. Their controls
remain separate from the invalid-setup refusal controls.
