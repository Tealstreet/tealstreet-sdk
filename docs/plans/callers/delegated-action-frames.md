# DELEGATED account action frames v1

Additive to [the placement protocol](delegated-order-frames.md). Shared synthetic
TS/Rust fixtures: `sdk-rs/hosts/server/tests/data/delegated-actions-v1.json`.

## Implementation and release boundary

Box-side action dispatch lives in `sdk-rs/hosts/server`. Its adapters call
the existing native write paths; this version adds no crates and changes nothing
under `sdk-rs/engine` or `sdk-rs/venues/*`.

The full-write fix round also extends `sdk-rs/algos/exec/src/native.rs` with a
native-only guarded EXEC entry point. This explicit boundary exception is needed
because deferred EXEC commands can enter the shared chaser/TWAP factories without
calling the command-port adapter. Factory identity and resource counters must
remain account-scoped while each action retains its own continuation fence.

The server's nuke account-port adapter carries the original registration and
checks it before every native command, including later close legs after an
awaited cancel. Retirement fences those new legs without undoing an earlier
handoff or turning its uncertainty into a not-sent proof.

`hosts/server` is excluded from the beta-wasm fingerprint, but the shared EXEC
file is a fingerprint input. The guarded entry point and checks compile only for
native hosts; ordinary LOCAL/COSIGNED and WASM EXEC paths retain their existing
semantics. Nevertheless, this PR now requires the SDK release/wasm pin and lane
fingerprint gates, including CPU certification: Rust slower than TS fails with
no new threshold or tolerance. No byte-equivalence claim replaces those gates.
The hosted service requires a box roll. The TypeScript API and router capability
still require their own integration; action admission remains gated by all
required capability flags.

## Capability and admission

The box advertises transports.accountWorkerWebSocket.delegatedOrderActionsV1.
It is true only with delegated-order-frames compiled, hosted signing/origin support available,
and `TEALSTREET_DELEGATED_ORDER_FRAMES=1` at process startup. It defaults off;
activation changes require a box drain/restart and fresh admission, as placement
does. Each session captures its activation at creation.
The action-aware router advertises
transports.accountWorkerWebSocket.delegatedOrderActionLedgerV1. This is a NEW
router attestation; delegatedOrderLedgerV1 alone does not attest action support.
An SDK action requires all four flags explicitly true on the captured admitted
socket: delegatedOrderFramesV1, delegatedOrderLedgerV1, delegatedOrderActionsV1,
and delegatedOrderActionLedgerV1. Missing/false flags never downgrade to an
unledgered callMethod. Existing placement calls retain their two-flag gate.
Only the connection's admitted private DELEGATED account/current
registration may dispatch actions. Auth, lease/drain, key age and owner checks
apply before execution and again after deferred work; replacement/logout retires
old work and receipts. The existing DO-start/socket admission guard also applies.

## DelegatedAction (closed discriminated union)

amend: {"kind":"amend","order":{"id":"venue-order-1","symbol":"BTCUSDT"},"update":{"price":50001,"amount":0.002}}
  update has price and/or amount; all supplied values finite and positive.
cancel: {"kind":"cancel","order":{"id":"venue-order-1","symbol":"BTCUSDT"}}
cancelAll: {"kind":"cancelAll"}
nuke: {"kind":"nuke"}

No extra fields. Target id and symbol are nonempty strings, at most 256 UTF-8
bytes each. cancel targets ONE venue order; cancelAll targets the account's open
orders. nuke stops account algos and uses the existing native account nuke path:
cancel all orders plus close all positions, awaiting every leg. It must never be
implemented as cancelQueued, a placement shim, or a browser-side second write.
Placement remains enqueue(params); these four actions use enqueueAction(action).
`enqueueAction` returns `Promise<string>` with the durable intent ID, like
`enqueue`. Observe `changed` / `entries()` for its typed result and state; an ACK
is not a completed action.

## New versioned frames

All frames use the existing authenticated JSON socket. d.version is the literal
number 1. A different/missing version is refused, never interpreted as placement.
The binding has the existing accountName, optional instanceId, UUIDv7 intentId,
and positive JS-safe seq. instanceId is omitted when absent (never synthesized).

Request (at most 64 KiB UTF-8 including envelope):
{"t":"order.action","d":{"version":1,"accountName":"main","instanceId":"registration","intentId":"018bcfe5-6800-7101-8101-010101010101","seq":1,"ageMs":0,"fingerprint":"<64 lowercase SHA256 hex>","action":{"kind":"cancel","order":{"id":"venue-order-1","symbol":"BTCUSDT"}}}}

ACK (at most 4 KiB):
{"t":"order.actionAck","d":{"version":1,"accountName":"main","instanceId":"registration","intentId":"018bcfe5-6800-7101-8101-010101010101","seq":1,"received":true}}

Outcome (at most 8 KiB):
{"t":"order.actionOutcome","d":{"version":1,"accountName":"main","instanceId":"registration","intentId":"018bcfe5-6800-7101-8101-010101010101","seq":1,"outcomeSeq":41,"outcome":"applied","result":{"kind":"cancel"},"error":null}}

Queue cancellation (at most 4 KiB; router-only, NEVER venue cancellation):
{"t":"order.actionCancelQueued","d":{"version":1,"accountName":"main","instanceId":"registration","intentId":"018bcfe5-6800-7101-8101-010101010101","seq":1,"fingerprint":"<original action fingerprint>"}}

The server returns unknown if an admitted actionCancelQueued reaches the box;
the box has no undispatched router queue. Old four placement frames are unchanged.
Malformed/unbindable frames keep the ordinary msg/error envelope and cannot
fabricate an ACK or a not-sent proof.

## Typed outcomes

applied: result is exactly {kind:"amend",orderIds:["venue-order-1"]},
{kind:"cancel"}, {kind:"cancelAll"}, or {kind:"nuke"}; error is null.
Amend IDs: 1–32 nonempty strings, each at most 256 UTF-8 bytes.
The result kind must match the original action. Applied means all requested legs
completed successfully; it does not promise a venue fill or an empty future store.

refused: result is null; error is {message:<bounded text>,outcome:"notSent"}.
This requires proof that the ENTIRE action was not sent. Pre-dispatch validation,
unadmitted account, unsupported capability, expired key and explicit native
notSent are examples. ACK(false) is permitted only with that proof.

unknown: result is null; error is {message:<bounded text>,outcome:"unknown"}.
Any uncertain/partial action, post-dispatch interruption, malformed completion,
unproven error, pre-coverage/unretained key or ledger failure is unknown, never
notSent. An ACK is not required. SDK keeps unknown intents non-resendable.
Message bounds match placement: 512 Unicode scalar values, within total limit.
No success is inferred from an ACK or from a missing failure. A failed nuke after
any completed leg is still unknown even when a later leg was itself not sent.

## Fingerprint and key identity

Action fingerprint = lowercase SHA256 hex of UTF-8:
  "tealstreet/delegated-action/v1" + NUL + canonical JSON of exactly
  {accountName, action, intentId, seq, version:1}.
Use the SAME canonical value algorithm as placement (ASCII object keys sorted;
array order retained; JSON strings/Unicode; finite binary64 ECMAScript numbers,
negative zero -> 0; unsafe integers/depth >32 refused). New domain prevents a
placement/action collision. Action kind, every target ID/symbol and all update
fields are hashed. instanceId, ageMs and transport outcomeSeq are excluded.
No caller clientOrderId override; actions target existing venue IDs and do not
invent placement client IDs. UUIDv7 intentId is the router dedupe key unchanged.
Its immutable timestamp and monotonic age both have the existing 10-second cap.

## Durable ledger semantics (router follow-up)

Use the SAME authenticated-user + UUIDv7 key space as placement, not a new
per-action bucket. Retain protocol family/version and action fingerprint with
the claim. A key reused for a different action, target, payload, sequence,
account or placement family is a conflict: unknown, no second forward, no replay
of the other operation's result. Same key/payload replays stored typed receipts
using the current requester binding. Commit/flush claim BEFORE forwarding, and
record receipts BEFORE exposing them. Crash gap remains uncertain; never resend.
Coverage/retention/DO-start guards apply to both protocol families.

Renumber EVERY delivered actionOutcome (live/replay) with the receiving client's
positive JS-safe socket counter; it may share that counter with order.outcome.
SDK ordering is per intent within captured connection, resetting on replacement.
Pre-start/pre-coverage/unretained keys, recorded cancellation without proof and
storage failure answer actionOutcome unknown with result:null. A router queue
cancellation blocks a later submit, but alone never proves notSent. Recorded box
refusals can replay actionAck(false) and refused/notSent. A queued-cancel after
dispatch returns stored receipts/live completion/unknown, NEVER a venue cancel.
No action is automatically re-forwarded after an ACK or any unknown outcome.

## Compatibility / persistence

Existing placement frames, fingerprint and capability names remain unchanged.
Persisted version-1 rows remain readable under the common evidence migration. Action rows use persisted version 2 with the exact action
and action fingerprint; checkpoint/put/compact scope and atomic retirement
semantics remain unchanged. Applied results are terminal and eligible for the
same bounded history; unknown/acknowledged remain retained. Deferred storage work
captures immutable owner/account/ticket, rechecks before send and has a two-owner
switch-during-deferral regression. Shared synthetic TS/Rust fixtures pin all new
frame shapes, action hashes, target identity and typed outcomes.

## SDK evidence and caller identity

Actions use the same evidence constructors as placements. Only
`markDelegatedNotSent(intent, evidence)` constructs a not-sent result: local
proof that the whole intent never handed off, or an authoritative admitted-router
refusal. An action ACK(false), or refused outcome with result:null and
error.outcome:notSent, is normalized into that existing bound proof schema.
Restore validates its owner/account/UUID/sequence/fingerprint binding and reissues
the runtime brand. Legacy or mismatched claims remain unknown. A rejected
queue-cancel produces an ordinary frame error; it provides no evidence about the
original action and cannot emit an action ACK(false) or refused outcome.

Physical handoff is sending; disconnect, close, replacement or its deadline
preserves unknown and never replays it. Deadline expiry follows the common
outbox lifecycle; queue cancellation is an explicit host request. A declined
first send may retry only through `markDelegatedReplayable` with persisted,
whole-intent no-handoff proof. Uncertainty never acquires that proof.

`enqueueAction(action, {intentId})` optionally accepts the caller's canonical
UUIDv7, following placement's `enqueue(params, {intentId})` identity contract.
An identical retry serializes behind persistence and returns the original ID,
sequence, fingerprint and deadline without sending or changing handoff/replay
state. Changed action/targets/parameters or protocol family rejects as
intentConflict. A failed original put cannot be acknowledged by a retry.
A failed later state update does not erase the original identity's durability;
lookup can repair that metadata without resubmitting or granting replay proof.
The additive persisted `callerSuppliedIntentId?: true` reservation survives
completed-history compaction and restore, including adoption of an existing SDK
key. It remains subject to the existing 1,056-record total bound; exhaustion
refuses new keys rather than forgetting reservations. Frame shapes, fingerprints
and the storage methods/checkpoint atomicity are unchanged.

## Full LOCAL write coverage (additive v1 vocabulary)

The same `order.action` / `order.actionAck` / `order.actionOutcome` /
`order.actionCancelQueued` frame family now also carries the local native write
surface. Its version, canonical domain, UUIDv7 namespace, ten-second admission
bounds, durable claim-before-forward rule, no-replay rule and registration fence
are unchanged. `order.submit` remains byte-compatible.

The entire typed DELEGATED API has one wire version: **v1**. Every added
operation belongs additively to v1; operations have no individual version field.
A future v2 is reserved for a major protocol redesign, not another write verb.
The existing version-2 *storage row* discriminator distinguishes actions from
placement records and is not a WS API version.

A bounded, authenticated v1 action whose operation or fields are not supported
fails closed before any dispatch, returning `order.actionOutcome` with
`version:1`, `outcome:"unsupported"`, `result:null`, and
`error:{message:"Unsupported v1 action schema",outcome:"unsupported"}`.
This includes unknown envelope/request/action fields, nested order/target/update
fields, unknown algo operations, and unknown nested algo fields. Invalid action
shapes also receive this schema rejection. The server validates registration,
UUIDv7, sequence, age and the fingerprint over the untouched raw action first;
it never strips unknown fields and executes a recognized subset. Invalid or
unbounded identity/binding data still receives the existing generic error or
admission refusal and never dispatches. A schema rejection emits no positive ACK.

The SDK preserves `unsupported` as a terminal result across persistence and
restore. It never brands that result as replayable or forwards the intent again;
later uncertainty or an inconsistent applied receipt cannot revoke it. The NX
router must preserve and durably record this outcome under the same v1 identity,
forward no rejected request, and return the same typed rejection for unknown
operations/fields it detects itself. This is only a pre-dispatch rejection of
that exact intent: an already-forwarded or uncertain ledger row must remain
uncertain, never be reclassified as unsupported by a newer schema check.
Queue-cancel rejections must not fabricate
proof about an earlier action; the existing queue-cancel contract is unchanged.

Expanded operations additionally require the box's
`delegatedFullWriteActionsV1` and the router's `delegatedFullWriteLedgerV1`,
both explicitly true on the captured admitted connection, together with the four
original action/placement flags. The box advertises the new flag under the same
compiled/runtime switch; an older router's four-action attestation is insufficient.
Missing flags keep new writes queued without sending or accepting receipts. Never
fall back to legacy `callMethod`, another channel, HTTP or browser signing.
The existing private Bybit-only DELEGATED admission is not widened. Venue-specific
operations on an incapable account return a proven pre-dispatch refusal.

| Local operation | Action payload (all include `kind`) |
| --- | --- |
| Single/batch placements, bracket/TP/SL/trailing/opening triggers | `place {orders:[DelegatedOrderParams]}` |
| Native amend | Existing `amend {order,update}` |
| Single/batch venue cancel | Existing `cancel {order}`, or `cancelOrders {orders:[{id,symbol,type?}]}` |
| Cancel symbol / account | `cancelSymbol {symbol}`; `cancelAll {symbols?:string[]}` adds explicit coverage to known and attempted symbols |
| Native account-wide cancel (also used internally by nuke) | `cancelAccountOrders {}`; requires `nukeAccountCancel`, with no symbol-census fallback |
| Symbol/hedge leverage | `setLeverage {symbol,leverage,side?:'long'|'short'}` |
| All leverage | `setAllLeverage {leverage}`; native account setting or positions/known published rails fan-out |
| Margin / position mode | `setMarginType {isolated,symbol?}`; `setPositionMode {hedged,symbol?}` |
| One/partial/hedge position close | `close {order}`; explicit native size, `reduceOnly:true`, market/limit only |
| Close all / cancel and close / cancel and stop algos | `closeAll {}` / existing `nuke {}` / `nukeOrders {}` |
| Stop only account algos | `stopAlgos {}` |
| Dead-man cancel / clear | `scheduleCancel {time:<safe UTCms integer> \| null}` |
| Venue settings | `setStpMode {mode:'EXPIRE_TAKER'\|'EXPIRE_MAKER'\|'EXPIRE_BOTH'}`; `setAccountTier {tier:'standard'\|'premium'\|'plus'}` |
| CORE runCommand / hosted CLI | `algo {op:'exec',data:{command,symbol?}}` |
| Chaser lifecycle | `algo {op:'chaser:start',data:ChaserOrderOptions}`; terminate takes a job ID; terminateAll takes `{}` |
| TWAP lifecycle / restore | `algo {op:'twap:start',data:TwapOptions}`; resume/pause/stop/cleanup take a job ID; state takes persisted rows |

New placement/close results are `{kind,orderIds}` (1–32 bounded IDs), only after
native completion. They do not replace caller-supplied client IDs with the action
intent ID: protective and split shapes use the native ID policy. Explicit caller
IDs retain the native whole-batch preflight. A close has no second browser-side
placement. Other completed synchronous writes return `{kind}`; setting results
must be native unit completions. Malformed results remain unknown. Any partial
native effects prevent a whole-action `notSent` proof, even if a later leg refused.
The generic command-group completion waits for all leverage/cancel-all legs.

`DelegatedOrderParams` is a closed native placement shape: symbol, type, side,
amount and the existing optional price/trigger/protection, time-in-force,
reduce-only, caller-ID, slippage, STP, Bybit BBO and implied-volatility fields.
Native guards and capabilities still decide whether the venue supports a shape.
There are at most 32 orders/targets/symbols per action and the original 64 KiB
UTF-8 frame limit still applies. Use explicit intents for larger work; a transport
retry never splits or automatically replays it. Single `cancel` also retains an
optional order type for protective routing and requires the expanded flags when
that context is supplied.

`cancelAccountOrders` invokes only the native account-wide cancellation command,
and refuses before dispatch if the venue lacks `nukeAccountCancel`. It does not
stop algos or approximate account coverage with currently observed symbols.
Its internal method translation is DELEGATED-only; LOCAL/COSIGNED are unchanged.
The new internal account-cancel/STP/tier methods are reachable only after typed
action validation; legacy DELEGATED `callMethod` does not acquire these names.

TWAP-state rows may carry `ownerId` and a nonnegative safe-integer
`ownerGeneration`. Both fields remain in the v1 fingerprint; neither is filled
from the current account. Missing generation stays wire-compatible but cannot
authorize restoration when the native host binds owner and generation.

Algo actions preserve the existing native algo protocol, including interruption
restore pauses and drain admission. Their ACK means admitted, not completed.
The action outcome is conservatively unknown: job state/ordinary correlated
`exec-result`, `chaser:*`, `twap:*` and store events reconcile the asynchronous
operation. The SDK refuses an `applied` receipt for `kind:'algo'`; dispatch alone
cannot manufacture a completed venue write. The router records the unknown
outcome and must never re-forward the action. EXEC receives the intent as its
`reqId`, rather than accepting a second caller-controlled correlation ID.

Typed `chaser:start` validation closes the nested `terminate` shape to `type`,
`value` and optional `anchorPrice`; types are `price`, `percent` or `usd`.
Size, maxAttempts, optional delay and termination numbers must be finite and
nonnegative. Optional booleans, bounded identity strings and origin values are
checked before native decoding. Unknown nested fields or malformed scalar values
return the existing v1 typed `unsupported` outcome with no positive ACK, job or
native write. This stricter wire boundary leaves legacy native chaser parsing
and native cadence unchanged; existing valid optional shapes remain supported.

Queued algo admission captures the original registration and deadline and checks
both before and after the actor's store read. A typed EXEC uses the resident
account port for factory identity and shared parallel/nuke counters, while its
own adapter retains the captured registration/deadline. It rechecks the fence
before each native command, after awaits before completing the next instruction,
and before direct shared chaser/TWAP factory handoffs. Expired or retired work is
dropped; late replies cannot start sibling jobs. Overlapping typed EXECs and
legacy EXECs share one account factory without sharing action guards.
Accepted long-lived chases/TWAPs belong to the native
account scope, as in LOCAL, and retain the account/user/router-lease authority;
a socket handover does not restart them. TWAP options/restored state must name
the admitted account. No payload may name another account to route a command.
Persisted TWAP `activeMarket` is just `{symbol}` on this typed API; the hosted
engine's catalogue supplies all market facts. The existing algo validation still
rejects invalid schedules and enforces restoration/uncertainty rules.

The expanded corpus `delegated-full-writes-v1.json` pins the method mapping and
canonical hashes in both SDK and Rust tests. The original four-kind corpus and
placement protocol remain unchanged. Server implementation is in `hosts/server`
with the guarded native shared-EXEC exception described above. No engine, venue,
budget, CPU threshold or tolerance changes are included.

A zero-amount amend is also an expanded action: native Bybit Partial protection
uses zero for full-position sizing. It requires the two expanded capability
flags and remains subject to the native protective-amend guard. Ordinary orders
whose venue requires a positive amount still refuse before dispatch. Price
updates remain strictly positive. The original positive-amount amend contract
and its existing fingerprints are unchanged.

DELEGATED placement translation also enforces LOCAL's execution-control
capabilities before any batch leg: unsupported STP, options, Bybit BBO/tick
controls and a venue's refusal of caller slippage are whole-intent pre-dispatch
refusals. This covers both `place`/`close` actions and the existing `order.submit`
family; a venue cannot silently discard a requested execution constraint.

A completed, explicit reduce-only protection-only placement may have no native
order ID: Bybit's trading-stop command accepts a setting and the stream supplies
the protection row later. After authoritative native completion, that case returns
`{kind:"place",orderIds:[],protectionApplied:true}`. The marker is derived only
from the captured protective command shape plus native completion, never from an
ACK or a missing reply. The SDK accepts it only for a protection-only request;
ordinary empty placement results and interrupted protection writes stay unknown.
No venue order ID is invented.

## Existing algo lifecycle and deferred parameter changes

The typed v1 vocabulary already carries chaser start/terminate/terminateAll and
TWAP start/pause/resume/stop/cleanup/state through the resident hosted factories.
Pause/resume keeps the TWAP job ID and its remaining schedule; it does not replace
parameters, create a new job, or imply that any venue write completed. Algo
action outcomes remain `unknown`, reconciled through the existing account events.

Parameter amendment is deferred. `chaser:amend` and `twap:amend` are unknown
operations and return the existing typed `unsupported` outcome before dispatch.
Do not emulate amendment with automatic stop-and-replace or change the wire
version. Client controls can expose the existing lifecycle only.

## Recursive JSON validation

The server checks original typed-action payloads recursively for duplicate object
keys, including placement order arrays, close orders, scheduling values and algo
data/restore arrays. A duplicate cannot discard an earlier unknown or malformed
field before the closed schema is checked. The admitted,
fingerprint-bound request receives typed `unsupported` before positive ACK, job
admission or native IO. Other admission, age and fingerprint refusals still take
precedence. LOCAL/COSIGNED method forwarding is unchanged.
