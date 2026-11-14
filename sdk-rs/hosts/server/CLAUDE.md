# Hosted exchange server

Read the repository root `AGENTS.md` and this directory's `DESIGN.md` before
changing server behavior. Validate scoped Rust changes with
`cargo test -p safe-cex-exchange-server` and
`cargo clippy -p safe-cex-exchange-server --all-targets` from `sdk-rs/`.

## Public topic diagnostics

`POST /admin/public-topics` accepts an empty body or `{}` (1024-byte limit).
It uses the same on-box peer guard and forwarded-header refusal as
`/admin/router-lease/summary`, returns `Cache-Control: no-store`, and adds no CORS.
The response is `{ "venues": { "<venue>": { "book": B, "candles": C,
"other": N, "total": T, "peakTotal": P, "peakAt": ms } }, "checkedAt": ms }`.

Only public-only engines are observed; private account engines are excluded.
The observer API, state and callbacks compile only on native targets; wasm
retains its original engine layout and code.
Counts aggregate all live public-only engines for a venue, including multiple
engines if present. Managed topics count at final subscription frame handoff,
not when desired or queued; legacy topics count at native transport handoff.
These measure released subscriptions, not exchange acknowledgements. Unsubscribe,
socket retirement and engine teardown remove current counts. Per-venue peaks
and their wall-clock millisecond timestamps survive engine replacement until
process exit. Venues without an engine are omitted.

For Hyperliquid and Hyperliquid spot, `book` counts distinct coin identifiers
across l2Book/bbo/precision tiers; `candles` counts distinct coin/interval pairs.
`total` counts every upstream topic, so it need not equal `book + candles + other`.
Other venues report their actual topics in `other` without inferring categories.
REST requests and polling candles are not upstream socket subscriptions.

One server-owned, self-rescheduling task emits a single INFO journal line every
60 seconds: `public topics over 60s: <venue> book=B candles=C other=N total=T
peak=P peakAt=ms; ...`. It stops on shutdown and does not catch up missed intervals.
Both the route and log are counts-only: never add user IDs, account names, topic
keys, symbol lists, request payloads, credentials, or per-user breakdowns.

## DELEGATED construction

Authenticated admissions and socket commands recheck the token expiry captured
by `auth::Verifier`, shared by COSIGNED and DELEGATED. Init, re-init, resume,
verification completion and deferred startup dispatch do not extend that expiry.
The existing socket close remains at expiry plus `TOKEN_GRACE` (30 seconds);
the grace permits reconnection, not new admissions or commands. Expiry belongs
to the requesting socket and never fences another holder's account transport.

Typed action guards retain that socket expiry through actor queues/store awaits,
EXEC continuations and native command dispatch, alongside registration and age.
Already accepted chaser/TWAP jobs retain their existing account lifetime.

The trusted `Registry::obtain_delegated` entry point consumes an already-verified
handover pair. DELEGATED and COSIGNED share hosted eligibility: a supported
`remoteSigning` policy and a configured server origin, with the existing
unhosted refusal preserved. There is no separate DELEGATED venue allowlist or
`delegated.enabled` declaration. The account retains the existing LocalSigner provider,
passes opaque refs to the engine and creates no remote signer link. Public mode
is recorded explicitly. COSIGNED and DELEGATED accounts never attach across
modes; a refused init leaves the running account intact. Socket handover and
private-read verification must complete before calling this entry point.
Legacy and typed full-write method admission both use this shared eligibility.

DELEGATED uses `prepare_local_signer_for`, including wallet key-only/agent
layouts. Guarded handover strings move into the zeroizing native options for
that setup; only its provider, identities and required public metadata remain.
Hyperliquid wallet addresses survive construction and resident reconfiguration.
Opaque `TrustedSignerState` carries non-JSON setup facts through native config,
including Paradex derived-key validation, for both the probe and resident actor.
It cannot be claimed by client JSON or applied to a different venue. The shared
admission/probe algorithm, lifetime checks and refusal classification are unchanged.

Key-only setup keeps its private scalar in the sealed credentials' `signingKey`,
never `InitPayload.key`. For that mapping the public init key equals `subkeyAddress`
and the sealed public key; it remains in the unchanged HPKE context. Other
mappings reject `signingKey` and empty signing secrets. The canonical registry
setup validates the material before the unchanged private-startup proof. There
is no fallback to the old scalar-in-init layout.

The existing Bybit identity-sharing namespace and remote `keyId` scheme remain
unchanged in both modes. Other hosted venues retain their current account-label
identity; that distinction does not gate DELEGATED or COSIGNED admission.

Private COSIGNED admissions require possession proof independently of
DELEGATED hosted eligibility. The COSIGNED proof-ready catalog does not restrict
DELEGATED's shared probe. Both modes share refusal classification, token/lease
checks and account attachment. A private account is never attached
as a fallback when its possession proof is unavailable. Exact Bybit, Binance,
OKX, Bitget, Blofin and KuCoin base venues have private-startup proof coverage;
other private COSIGNED joins return the typed `proof-unavailable` init refusal.
A pending COSIGNED signer belongs only to its joining connection; probe frames
are consumed internally. Matching COSIGNED inits on an admitted connection and
COSIGNED resumes retain their existing snapshot path. The exact wire names and
rollout boundary are in `docs/plans/callers/cosigned-join-proof.md`.

Shared admission serializes temporary proofs by verified user, credential and
venue/network/site scope. One proof runs with at most four waiting reservations;
overflow uses the existing transient refusal. Labels, settings, routing and
signing mode do not split the queue. Waiting uses the existing proof deadline
and rechecks captured token, init and lease authority before native work. The
active reservation remains on the engine thread through probe cleanup, including
cancellation. Reservation counts retire under the same map lock as admission,
so concurrent final cancellation removes idle entries even while destructor
fields still retain queue references. No successful proof is cached.

Admission refusal diagnostics use `hosted.admission_refused`, with a server-issued
attempt ID (or null before an offer), a catalog-validated venue, refusal kind,
stage, transient flag, classified error kind and integer venue code. The attempt
context follows its original pending verification. Never add credential identity,
account labels, tokens, signatures, handover bytes, raw venue messages or payloads
to these lines. Diagnostics do not change admission or retry classification.

## TWAP restore binding

Native TWAP restore authority carries both the verified owner and generation.
Identity-keyed accounts seed that binding from the registry identity digest;
other native accounts retain their verified user binding. Label changes do not
change identity or generation. Native identity restoration can remap matching
persisted labels only after preserving both owner tags; missing/stale generations
never inherit the current generation. Algo inputs capture this binding alongside
any typed action receipt, and both guards survive deferred dispatch. Typed v1
TWAP-state rows carry optional safe-integer `ownerGeneration` through validation
and fingerprinting without backfill; the native restore gate remains mandatory.

## DELEGATED order frames

Sam's 2026-11-11 policy allows activation with place and reduce-only close before
amend, venue cancel, cancel-all or nuke. The additive `delegatedActivationScopeV1`
advertisement exposes exactly that partial set. The existing compiled feature,
hosted venue support, runtime frame switch, router ledger and per-connection
ownership/registration checks remain required. `DELEGATED` is the host's
per-user feature flag, not a new box environment variable. SDK `activate` checks
it before using the advertised partial set. Hosts query `/account/capabilities`
with `?exchangeName=<joined venue>` (or `/capabilities` with the same query).
`delegatedVenueV1` must match the joined venue; the unscoped response advertises
no actions. Unjoined handover requests must include `data.exchangeName`;
a joined account may supply its stored venue. The init remains
`signingMode:"delegated"` plus `sealed`, with no venue-specific mode field. Unsupported legacy method aliases
also return `DelegatedActionUnavailable` before a native plan is produced. A
close remains a placement with `reduceOnly: true`, using the existing outbox and
venue enforcement. Queued-intent cancellation remains a separate router operation.
Raw exec and chaser/TWAP starts, resumes or running-state restoration return the
same named refusal. Paused TWAP restoration and existing stop/cleanup controls
remain available; this does not resume or replay an interrupted intent.

DELEGATED `order.submit` messages use the existing method translation,
native command completion and order audit, with the intent UUIDv7 as both request
ID and client order ID. Other modes keep `callMethod` unchanged. Pending joins
and elapsed ages of ten seconds or more refuse without a write. New replies
use the existing instanceId registration helper. The NX router must durably
record submits before forwarding and expose acknowledgements only after commit;
the server supplies no dedupe ledger or queued-order storage. `order.cancelQueued`
is a router operation and never becomes a venue cancellation. The interception
contract is documented in `docs/plans/callers/delegated-order-frames.md`.

Receipt sequencing belongs to each registration output and is shared by its
clones. Replacing or retiring that registration also retires its order receipts;
successful same-resident re-verification follows the same replacement lifecycle.
Existing method/signing envelopes retain their lifecycle. Sequence exhaustion
signals reconnect without wrapping the counter.


## DELEGATED action frames v1

The additive `order.action` family dispatches amend, venue cancel, cancel-all and
nuke on the admitted DELEGATED account, using its own venue. `delegatedOrderActionsV1` follows
the existing feature/hosted-venue gate and `TEALSTREET_DELEGATED_ORDER_FRAMES=1` startup
activation. Each session captures activation at creation; changes require the
existing box drain/restart and fresh admission. Hosts also require the router's separate
`delegatedOrderActionLedgerV1` and the two original frame/ledger flags. Requests
bind UUIDv7 identity, version, action kind and target IDs in a separate canonical
fingerprint domain. Dedicated action receipts use the full method translator
after that admission; legacy aliases keep the separate place/close restriction.
Placement's four frames and fingerprints are unchanged.

Amend/cancel/cancel-all use the existing method translator, native command
completion and audit. Internal action dispatch carries the original deadline
through cancel-all symbol reads and command translation. Each native command
rechecks both expiry bounds and its captured registration after any startup wait and
immediately before dispatch. Expiry stays unknown without a notSent proof.
Nuke uses the account's existing native algo-host nuke,
awaiting cancellation and closing legs; a partial/interrupted action stays
unknown. Actor-queued nuke work carries the original registration and rechecks
its liveness, account holder and IO authority after load waits and after the
actor's store read. A server-owned account-port adapter also rechecks that
registration and both captured expiry bounds before every native command,
including close legs after an awaited cancel. Nuke receipts carry the original
monotonic age deadline and UUID expiry through startup and actor store waits;
neither wait resets the budget. Expiry blocks every remaining native leg and
returns unknown with an expired message, without inventing a notSent proof.
Retirement fences new legs and drops stale outcomes; already handed-off work
remains uncertain. An aborted/abandoned nuke settles unknown on
its captured output, never another owner's registration.

Typed action outcomes are `applied`, `refused` with a proven `notSent`, or
`unknown`. Multi-leg failures and errors lacking no-dispatch proof are unknown.
Every raw typed-action payload checks recursive key uniqueness before `Value`
conversion: placement arrays, close orders, scheduling values and algo data.
Schema rejection follows binding/hash checks and precedes ACK or native work.
Typed TWAP starts validate the connection-local label before translating it to
the shared identity actor's original label. Restoration retains the persisted
identity owner tag and the native book's ownership checks.
Every cancel-all leg settles through its shared completion group, including
pre-dispatch lease, capacity and command-ID refusals. A locally unsent leg
cannot refuse the whole action after another leg may have crossed physical IO.
Receipt counters share the existing registration lifecycle and size bounds.
`order.actionCancelQueued` belongs to the router and never issues a venue cancel.
A matching live registration keeps its output and shared receipt sequence while
replacement verification is pending. This does not admit new writes; unmatched
or retired bindings retain their existing rejection and fallback routing.
Rejecting that frame uses the ordinary error envelope: it supplies no proof
about an earlier action and cannot fabricate an action refusal or negative ACK.
See `docs/plans/callers/delegated-action-frames.md` and the shared synthetic
`tests/data/delegated-actions-v1.json` contract.
