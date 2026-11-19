# Hosted exchange server

Read the repository root `AGENTS.md` and this directory's `DESIGN.md` before
changing server behavior. Validate scoped Rust changes with
`cargo test -p safe-cex-exchange-server` and
`cargo clippy -p safe-cex-exchange-server --all-targets` from `sdk-rs/`.

## Bybit acknowledgement audit

Order audits decode the native legacy error wrapper before their unchanged
160-character message cap, including algo dispatch failures. The closed
`bybitBatchAcknowledgement` field retains list shapes/counts and at most eight
child summaries, plus at most eight bounded native IDs as reconciliation evidence.
It accepts only the native diagnostic grammar, never arbitrary JSON metadata,
client echoes or raw responses. This journal field changes no outcome, algo
ownership, retry or resend authority; it is not forwarded to Sentry.

## History reply completeness

Command-owned `history.incomplete` notices retain a closed reason and optional
later-page failure record in that command's pending reply. Successful hosted
history rows carry it in `data.completeness`; unknown metadata falls back to the
positive incomplete marker. No venue text or response values enter this record.
Native floating-point and integer JSON command IDs must both name an exact u32;
malformed or unrelated owners cannot mark another reply or become command zero.
Replacement assertions clear previous failure detail. Read results, rejection,
paging, retries, deadlines and write behavior retain their existing contracts.

## Connection-local public pacing

The optional `publishIntervalMs` init field and `set-publish-interval` account op
are documented in `DESIGN.md` under Connection-local publication interval.
Keep cadence in the socket/account pump, never in shared engine identity or
configuration. Private financial sections and outcomes bypass the public coalescer.

## Sentry owner identity

The native Sentry allowlist and captured authentication-subject contract live in
`DESIGN.md` under native error capture. Diagnostic ownership is separate from
journal labels and never supplies account or IO authority. Only the trusted
report capture supplies identity; event extras and SDK scopes are untrusted.

## Router lease-fence diagnostics

Router lease fences snapshot `Running::jobs()` before disposal. The journal
includes `running_jobs`, `paused_jobs` and `pending_work`. Only a positive running
algo count logs error and sends `router_lease.fenced` to Sentry with the fixed
summary “router lease fenced; running algos stopped on this box”. Zero running
algos logs info with “router lease fenced; no running algos on this box”, including
paused-only or pending-only work; info remains journal-only. Only unsigned numeric
counts are allowed for that event; job IDs, labels and payloads stay excluded. Lease retirement,
account disposal and algo stop/pause behavior are unchanged.

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

DELEGATED is available on every venue admitted by the shared hosted server gate,
just like COSIGNED. One runtime `DELEGATED` flag provides the kill switch and
per-user pin; `DELEGATED_BYBIT` is only a compatibility alias. Bybit was the first
test account, not an admission boundary. Mobile is DELEGATED-only; DELEGATED
clients have no WASM engine/local signer and send every write as a typed v1
action over the existing hosted WebSocket.

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
passes opaque refs to the engine and creates no remote signer link. Trusted
construction also retains original typed identity facts bound to the same provider,
venue and serialized refs; JSON must never restore ScalarOnly credential absence.
Public mode is recorded explicitly. Unproved/trusted construction still refuses cross-mode
attachment. Socket joins first prove their own credentials: COSIGNED can attach
to a DELEGATED resident without downgrading it; a proved DELEGATED join upgrades
a supported COSIGNED resident without replacing its actor, orders or algo jobs.
Public-key handovers locally match the candidate to the original provider's
completed Ethereum/Stark binding or canonical Ed25519 public identity. Schnorr
pins its routed registered key before the original provider is published, then
rechecks registration against that immutable binding and the candidate on join.
Already-promoted actors compare their actual installed resident key. Unpinned
legacy actors refuse explicitly with `mode_transition_unavailable`; a new lookup
cannot invent their old key. Captured init/lease/expiry guards and exact actor
binding survive every lookup. Same settings never revive a retired proof.
All outstanding original sign futures/setup work must join before handover; busy
handovers refuse transiently without canceling or replaying them. Engine-owned
address seals, nonce ownership and original opaque refs survive unchanged;
only dispatch refs map by identity role to the protected resident refs.
Account-owned signer control retains the protected erasure authority; engine
dispatch handles never retain it. Reconfiguration shares that authority between
accounts, and the last account owner drop erases retained engine handles.
Confirmed joins and upgrades publish `account-signing-mode` to each confirmed
registration's label/instanceId. Refusals have attached=false and no actualMode.
Socket handover and possession verification precede any transition. A verified
resident public-key join uses the local key proof, not an unsigned startup read.
Coinbase COSIGNED proves its opaque CDP key through native JWT/private startup,
with server-owned claims and a signature-only reply. DELEGATED CDP handover
remains explicitly proof-unavailable; no fallback selects another signing mode.
Legacy and typed full-write method admission both use this shared eligibility.

DELEGATED uses `prepare_protected_local_signer_for`, including wallet key-only/agent
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
as a fallback when its possession proof is unavailable. Exact Bybit base/inverse/demo/spot,
Binance base/PM/inverse/spot/options, OKX base/demo and EEA mainnet/demo, Bitget base/USDC,
Blofin, KuCoin Futures/Spot, Phemex, BingX, MEXC and Crypto.com names have native private-startup HMAC proof coverage.
Exact HyroTrader challenge accounts reuse native Bybit-shaped private-startup
HMAC reads on the server-owned canonical native origin. REST and derived public/
private WebSocket endpoints come from that origin only, never init URLs or a
Bybit fallback. Testnet has no native origin and remains refused before I/O.
The same positive, different-key, replay, foreign-socket, consumed-response and
resident-preservation controls cover this exact catalog name. Private socket
fixtures also check the real HMAC and correlated handshake before snapshot reads.
Bybit inverse retains its inverse-product and Brazil-site session controls. Phemex
uses its native USDT/USDC account reads and expiry-bound REST/private-socket
HMAC proof with wrong-key, replay and foreign-socket controls.
KuCoin Spot uses its own production origin and native account/active-order reads,
plus the signed private read-feed token request. Its proof fixtures verify request
and passphrase MACs; public discovery cannot substitute for private proof. Testnet
remains unavailable. Each new COSIGNED socket proves its own key before attachment.
Exact WOO additionally uses its native v1/v3 private account reads and private
socket HMAC authentication, with wrong-key, replay and foreign-socket controls.
Exact Bitunix uses its existing native private-startup venue round trip and
double-SHA256 secret-suffix signer, with independently checked synthetic private
reads and wrong-key, replay, foreign-socket and resident-preservation controls.
There is no server-local proof shortcut or trading write in that probe.
Crypto.com proves its signed native JSON positions, balance and regular/advanced
open-order reads, with wrong-key, replay, foreign-socket and login-switch controls.
Kraken Futures keeps nonce-free REST. Its temporary native probe also requires
a fresh venue-issued private challenge, matching primary-key subscriptions and
all three private-feed acknowledgments; REST loading alone cannot admit a join.
Connection replacement, close and captured-owner retirement revoke that evidence.
Variant tests pin product routes, origins and demo headers as well as the
wrong-key, replay, foreign-socket and resident-preservation controls. BingX perpetuals
prove signed private account reads independently of API-key-only listen-key acquisition;
Spot and testnet do not inherit that proof. MEXC mainnet uses native signed
position-mode and account reads, with private-stream HMAC and wrong-key/replay/socket
controls. Its missing sandbox remains refused; public discovery is not proof. Other
private COSIGNED joins return the typed `proof-unavailable` init refusal.
AsterDEX readiness is V1 HMAC only: both Session and Registry checks inspect
the init shape. Address-prefixed keys or nonempty main-wallet application IDs
retain `proof-unavailable` before signing or probe IO. The admitted V1 shape
carries explicit native `asterdex.apiVersion: "v1"` through setup and settings;
its signed native account reads and real device codec have wrong-key, replay,
foreign-socket and pending-isolation controls. Wallet V3 proof remains separate.
A pending COSIGNED signer belongs only to its joining connection; probe frames
are consumed internally. The shared local-proof strategy captures canonical
server-owned identity facts before entering the same proof queue and pacer.
It issues a closed admission challenge and verifies the raw reply against the
actual receiving socket, or verifies the temporary protected DELEGATED provider.
The permit records its socket only after verification; final attachment checks
that receipt, the original instance/token/lease and the local admission deadline.
Exact Hyperliquid and Hyperliquid Spot select this strategy from captured
`hyperliquidSignerAddress`, network and main/vault scope. Their device family
signs the closed admission statement with its existing opaque secp256k1 ref;
missing facts refuse without unsigned-read fallback.
Exact GRVT selects this strategy with the authenticated owner and the captured
`grvtTradingAccountId` and `grvtSignerAddress` used by engine construction.
Both fields are required before proof; API-key login cannot prove its separate
ECDSA scalar. Before holding an existing DELEGATED actor, local admission also
compares the captured public key with the actual installed resident key; stale
init metadata cannot substitute for that binding. Local secp256k1 recovery verifies
a fresh account/attempt-bound
admission statement, never a venue transaction. This proves possession, not venue
authorization: ordinary startup retains login and private-read validation.

Exact Lighter and Lighter Robinhood select it after a routed, bounded registration
read on the captured account/API-key slot. The projection includes the registered
Schnorr public key and exact network. A slot queue prevents label-based parallel
proofs. An existing actor's incarnation and immutable pin are captured before
lookup; retirement or replacement invalidates that attempt. Final attachment
compares the proved key with that pin or a fresh cold-construction registration
read in both modes. DELEGATED also matches the actual provider before publication;
rotation refuses without constructing an actor or changing a resident.
Exact Backpack selects this strategy using its captured canonical base64
Ed25519 API public key and authenticated owner; malformed keys refuse before
signing, without falling back to private-startup reads. Other public-key venues
remain unavailable unless a synthetic fixture supplies a binding. Their device
integration and authoritative per-family facts must precede activation.
Matching COSIGNED repeats require the same socket-admitted
mode and identity/settings; refused proofs never change that mode. Resumed COSIGNED
accounts retain that admission provenance. The exact wire names and
rollout boundary are in `docs/plans/callers/cosigned-join-proof.md`.

OKX EEA mainnet and demo prove signed config before keyed discovery and require full
private startup. One re-signed `50113` retry is allowed per bootstrap read; a
second consecutive refusal is permanent. Exact EEA demo additionally requires
one `x-simulated-trading: 1` header on every private startup read.
The captured admission budget and socket/init/token/lease fences are unchanged.

ApeX remote signing uses the existing device `apexomni-batch` codec for native
REST/login graphs and atomic contract dependencies. Only opaque OmniKey refs
are installed. Exact `apexomni` COSIGNED admission proves API credentials through
native account, account-balance and open-orders reads, with wrong-key, replay
and foreign-socket controls; this never proves OmniKey/L2-seed possession.

Exact `binanceoptions` mainnet proves possession through its existing signed
EAPI margin-account, position and open-order startup reads. Public exchange-info,
ticker and mark responses never establish possession. Tests cover wrong keys,
replay, replies from other sockets, consumed responses and retired connections
through the same native probe and Session fences; Options testnet stays refused.

COSIGNED catalog accounting runs in `sdk-rs/harness/cosigned-catalog.test.mjs`.
Each exact `remoteSigning.enabled` venue owns one JSON record under
`src/delegated/cosigned-proof-catalog/`: excluded records carry a nonempty reason;
ready records name their actual probe and Session test sources. The server build
validates all declarations and derives runtime readiness and shared fixture
cohorts. There is no shared list to edit when a venue proof lands. Follow
`src/delegated/cosigned-proof-catalog.md` for the record schema and migration.
The helper/production census is 41 / 40 / 33; disabled metadata remains visible
in accounting. Cargo signing features are only a family cross-check. The catalog
changes no DELEGATED eligibility, client fallback or signing/origin policy.
Partially-ready `asterdex.json` requires the closed `excludedShapes: ["v3"]` field;
V1 readiness never accounts for its unimplemented wallet proof. The accounting
test rejects missing, duplicate, stale or unknown shape exclusions.

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

The v1 `init_unverified` error metadata includes closed `reason_class`:
`key_rejected_ip_or_permissions`, `key_invalid`, `venue_unavailable` or `unknown`.
It derives only from the captured venue and typed private error kind/numeric code.
Permission failures and the Binance -2015/-2014, Bybit 10003/10010 and OKX
50110/50111 auth refusals use the broad key-rejection class; other invalid/expired
keys use `key_invalid`. Network, timeout, rate-limit and venue-unavailable errors
use `venue_unavailable`; missing/unrelated evidence stays `unknown`. Numeric
namespaces are scoped to exact venue families, not prefixes. HyroTrader's
ambiguous 10003 stays `unknown`: a disabled challenge can reject an unchanged
valid key, as its Bybit CLAUDE contract documents. This is coarse
recovery guidance, never proof of a specific IP or permission defect. Raw venue
text never enters this frame; typed JS-safe venue codes may accompany the class. The fixed message, transient flag, registration
binding and existing eight-field private diagnostic remain unchanged.


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

Settings rebuilds share the protected resident owner with the published successor.
Retiring the replaced actor preserves that successor; explicit disposal or the last
account-owner drop still revokes the provider while engine handles remain.

Retention captures disconnected labels until its deadline, so identity-keyed
lookup, explicit disposal and public-fallback refusals survive resume-grace expiry.
An unjoined label matching multiple identities refuses instead of choosing one.

Stopped DELEGATED accounts keep live holders for final stop notices. A resumable
disconnect releases that stopped holder through the normal release path instead
of parking it or restarting retention; the final live holder leaving evicts the
stopped account. Owner and account-incarnation checks still isolate late cleanup.

## DELEGATED order frames

Sam's 2026-11-11 policy allows activation with place and reduce-only close before
amend, venue cancel, cancel-all or nuke. `delegatedActivationScopeV1` preserves
that activation minimum; its four later-action fields reflect runtime typed-write
availability for the selected hosted venue. This metadata grants no admission.
The existing compiled feature,
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
native command completion and order audit. The request ID stays the intent UUIDv7;
Hyperliquid/Spot client order IDs are `0x` plus its 32 lowercase hex digits;
Bitget, OKX, BloFin and Binance PM use the bare 32 hex digits. Bybit and other
compatible text adapters retain the UUID. Lighter/Robinhood, Binance inverse,
AsterDEX, Kraken and MEXC receive no caller venue ID, per the spec's supported-ID
condition; receipts keep `clientOrderId: null`. WOO also receives none: until
2026-11-19 it got the UUID, which WOO refuses, so every plain market submit failed
before the venue. A numeric WOO ID needs NX router and SDK outbox outcome checks
to accept it before the box emits one. The router UUID/fingerprint ledger
remains primary dedupe in every case. Backpack and GRVT retain their current
refusal pending their separate bounded-hash mapping. Bybit protection also omits
its unsupported caller ID and reports null. HyroTrader retains its native
caller-ID refusal. Other modes keep `callMethod` unchanged. Pending joins
and elapsed ages of ten seconds or more refuse without a write. New replies
use the existing instanceId registration helper. The NX router must durably
record submits before forwarding and expose acknowledgements only after commit;
the server supplies no dedupe ledger or queued-order storage. `order.cancelQueued`
is a router operation and never becomes a venue cancellation. The interception
contract is documented in `docs/plans/callers/delegated-order-frames.md`.

Placement completions (`settle_intent`) send `refused` with `error.outcome:
"rejected"` and the venue's message only when the engine marked the whole order
`rejected` and its detail carries no partial IDs/count, non-`notSent` element or
`protection`. Every other failure without `notSent` proof, including a partially
accepted split/multi-leg placement, is `unknown`/`unknown`. NX shows `rejected`
as the venue's refusal; the SDK still keeps the intent unknown and never resends.

Whole-action guards before any native command return their real `notSent`
reason; guards after possible effects remain unknown. Typed completion audits add
only closed `result_shape`; initial refusals log `stage: preflight` and the real reason.
Expired-token submits/actions receive correlated `notSent` receipts during the
unchanged socket grace. Queue cancellations cannot prove an earlier intent was
unsent and keep their existing expiry handling.

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

Pre-dispatch order refusals emit journal-only INFO `order.pre_dispatch_refused`.
Its closed reason class, catalog-validated venue (or null) and closed operation
never include IDs, labels, credentials, payloads or raw error text. Admission
reasons distinguish runtime, pending proof, mode, holder, registration and drain
without changing those predicates. Receipt-bearing actor Nuke/Algo work also
logs its owner, expiry, IO, admission and load refusals before its first dispatch.
Receipt completion and later native-leg failures never mint this whole-action
pre-dispatch event. Grouped Session command refusals use the separate bounded
INFO `order.leg_pre_dispatch_refused` with the same closed fields: no earlier-leg
handoff proof is inferred, and group uncertainty/settlement remain unchanged.
Normal amend targets remain id/symbol only;
reduce-only LIMIT price amendments use the ordinary native amend path.

Typed action outcomes are `applied`, `refused` with a proven `notSent`, or
`unknown`. Multi-leg failures and errors lacking no-dispatch proof are unknown.
An amend whose engine-store target is a stop/take-profit/trailing row, read
before dispatch, settles an authoritative ID-less completion as applied with
`protectionApplied` (Bybit Full stops edit via trading-stop; AGT-40 reported
9/9 such edits as unknown). Ordinary ID-less amends stay unknown.
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

## Algo outcome notices

`warning` and `chaser:complete` reach every live tab/device holding the account,
as algo errors do, not only the signing socket: a non-signing tab that missed a
chase end reported it as lost ("Hosted server stopped reporting it"). The unheard
record lists every socket told, so a rejoining page still gets its replay.
Correlated `callMethod:done` replies stay on the signing socket.

## Classified admission refusals

`init_unverified` carries optional typed `error_kind`, JS-safe integer
`venue_code`, and a closed host-owned `stage` from the actual ProbeRefusal.
Probe/session completion and early handover refusal paths preserve those fields
through the initiating account and registration output. Unknown classification
and absent venue codes stay absent; no raw venue text enters the new fields.

## ApeX encrypted seed

ApeX handover accepts optional `omniKeySeed` exclusively inside the sealed
credential schema. The in-place parser erases input and owns the field in a
zeroizing string. Admission copies and construction transfer it to canonical
ApeX protected setup, which installs `primary.extra.zklinkSeed` with hex encoding.
The field is never public init/engine JSON, a fingerprint or wallet signingKey.
Other venues, mixed wallet layouts and non-65-byte hex seeds refuse. Monitoring
without a seed remains unchanged and supplies no placement-signing capability.

Retained DELEGATED accounts share the canonical protected owner through the sole
`AccountSigner`; escaped engine handles have no independent erasure authority.
The 48-hour clock starts only after the last admitted holder disconnects and is
cancelled only by admitted reconnect. Removal/off/key removal erase immediately.
For a selected proxy transport, token expiry has an explicitly interim terminal
stop/erase with recorded reason `proxy-token-expired`; no issuer renewal, direct
fallback or authority extension is implied. Sam still owns the long-term policy.
See `DELEGATED_RETENTION.md` for the retention and restart contracts.


## DELEGATED intent clock skew

UUIDv7 intent time permits at most 5 seconds ahead of the box clock. Past age
still refuses at 10 seconds. Accepted positive skew emits journal-only INFO
`order.intent_clock_skew` with only closed `skew_bucket`: `le_1s`,
`gt_1s_le_3s` or `gt_3s_le_5s`; never add identifiers or raw clock values.
Action deadlines use the same bounded age, clamp future-key wall expiry to
arrival-time plus 10 seconds and retain the client-age monotonic bound. Deferred
work cannot renew that budget. The paired router policy must preserve its
coverage-quarantine and cross-clock safety bounds when adopting this tolerance.
