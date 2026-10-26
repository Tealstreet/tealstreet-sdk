# Lighter remaining request and signing funnel

Status: local plan on landing tip fff73624e8578c623a41875b3946d0c665f561af.
Owner: codex-ih35js. One venue PR will contain this plan, original regressions
and the source. Reviewer: claude-dazzy3 or claude-55ao7t. Existing recipe and
venue ownership are being reconciled with claude-m26ps4 and codex-zs5e83 before
source edits. Historical plans are evidence, not clearance for this cutover.

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

### Signing prerequisites to resolve before source

At this base TxRecipe exists, but AuthRecipe is absent, the registry still
passes the scalar to LighterOptions, and LocalSigner::can_sign reports
SchnorrDigest unsupported through implemented(). The historical recipe contract
assigns the cfg(schnorr) capability activation to the venue admission cutover.
Confirm its ownership with the recipe owner; do not silently advertise remote
support or bypass can_sign. A missing shared setup/provider capability is raised
to its owner, not replaced by venue-local cryptography. Generic/shared changes
retain their required review and measurement gates.

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

One venue-proven 21104 nonce refusal may reseed and rebuild the same unresolved
step after the native delay. It requires definitive current-call DATA, settled
refusal evidence, and the job's one-reseed budget. A transport/integrity failure,
5xx, 405/429, ambiguous batch or unproven acknowledgement cannot trigger that
resend. A subsequent nonce read is a fresh owned Read call; mutation continuation
requires its exact successful DATA. Never seed VenueCounter from stale reads;
re-preparation retains reserved transaction nonces and client indexes.

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
Claim only measured bytes, not an estimate from the number of removed sites.

One PR and one qualifying PR-triggered run; no job reruns or speculative CI
dispatch. READY requires reviewed exact source, the required green gates,
recorded family/combined evidence and the auditor's exact-tree landing GO.
