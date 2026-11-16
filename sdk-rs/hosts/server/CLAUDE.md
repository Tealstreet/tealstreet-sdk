# Hosted exchange server

Read the repository root `AGENTS.md` and this directory's `DESIGN.md` before
changing server behavior. Validate scoped Rust changes with
`cargo test -p safe-cex-exchange-server` and
`cargo clippy -p safe-cex-exchange-server --all-targets` from `sdk-rs/`.

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
passes opaque refs to the engine and creates no remote signer link. Public mode
is recorded explicitly. Unproved/trusted construction still refuses cross-mode
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
resident public-key join uses the local key proof, not an unsigned startup read. Coinbase
CDP remains explicitly proof-unavailable pending its COSIGNED JWT setup and
venue-proof implementation; it never silently falls back to another signing mode.
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
as a fallback when its possession proof is unavailable. Exact Bybit base/inverse/demo/spot,
Binance base/PM/inverse/spot, OKX base/demo and EEA mainnet/demo, Bitget base/USDC,
Blofin, KuCoin, Phemex, BingX, MEXC and Crypto.com names have native private-startup HMAC proof coverage.
Bybit inverse retains its inverse-product and Brazil-site session controls. Phemex
uses its native USDT/USDC account reads and expiry-bound REST/private-socket
HMAC proof with wrong-key, replay and foreign-socket controls.
Exact WOO additionally uses its native v1/v3 private account reads and private
socket HMAC authentication, with wrong-key, replay and foreign-socket controls.
Exact Bitunix uses its existing native private-startup venue round trip and
double-SHA256 secret-suffix signer, with independently checked synthetic private
reads and wrong-key, replay, foreign-socket and resident-preservation controls.
There is no server-local proof shortcut or trading write in that probe.
Crypto.com proves its signed native JSON positions, balance and regular/advanced
open-order reads, with wrong-key, replay, foreign-socket and login-switch controls.
Variant tests pin product routes, origins and demo headers as well as the
wrong-key, replay, foreign-socket and resident-preservation controls. BingX perpetuals
prove signed private account reads independently of API-key-only listen-key acquisition;
Spot and testnet do not inherit that proof. MEXC mainnet uses native signed
position-mode and account reads, with private-stream HMAC and wrong-key/replay/socket
controls. Its missing sandbox remains refused; public discovery is not proof. Other
private COSIGNED joins return the typed `proof-unavailable` init refusal.
A pending COSIGNED signer belongs only to its joining connection; probe frames
are consumed internally. The shared local-proof strategy captures canonical
server-owned identity facts before entering the same proof queue and pacer.
It issues a closed admission challenge and verifies the raw reply against the
actual receiving socket, or verifies the temporary protected DELEGATED provider.
The permit records its socket only after verification; final attachment checks
that receipt, the original instance/token/lease and the local admission deadline.
Exact Backpack selects this strategy using its captured canonical base64
Ed25519 API public key and authenticated owner; malformed keys refuse before
signing, without falling back to private-startup reads.
Exact Paradex selects this strategy using the captured account address and
subkey public key, authenticated owner and network. Public decimal/hex spellings
normalize before projection; invalid or mismatched fields refuse without a
private-startup fallback. COSIGNED init.key must identify the account address;
key-only DELEGATED init.key must identify the public subkey and retains its sealed
scalar. Possession proof does not replace ordinary venue authorization at startup.
The prepared Paradex family links the device recipe; its real-WASM Session
controls use SAFE_CEX_PARADEX_JOIN_WASM and explicit --ignored selection. This
WASM change does not remove either separate live Hosted test gate.
Other public-key venues remain unavailable unless a synthetic fixture supplies
a binding; their device integration and authoritative per-family facts must
precede activation. Matching COSIGNED repeats
require the same socket-admitted mode and identity/settings; refused proofs never
change that mode. Resumed COSIGNED
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
