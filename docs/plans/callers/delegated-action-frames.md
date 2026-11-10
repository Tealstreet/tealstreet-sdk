# DELEGATED account action frames v1

Additive to [the placement protocol](delegated-order-frames.md). Shared synthetic
TS/Rust fixtures: `sdk-rs/hosts/server/tests/data/delegated-actions-v1.json`.

## Implementation and release boundary

Box-side action dispatch stays entirely within `sdk-rs/hosts/server`. Its
adapters call the existing native amend, cancel, cancel-all and nuke paths;
this version adds no crates and changes nothing under `sdk-rs/engine`,
`sdk-rs/venues/*` or `sdk-rs/algos`. No exception to this boundary is required.

The server's nuke account-port adapter carries the original registration and
checks it before every native command, including later close legs after an
awaited cancel. Retirement fences those new legs without undoing an earlier
handoff or turning its uncertainty into a not-sent proof.

`hosts/server` is excluded from the beta-wasm fingerprint, so the Rust changes
require a box roll without a full SDK release, a new wasm pin or the lane
fingerprint gate. The TypeScript API and router capability still require their
own integration; action admission remains gated by all required capability flags.
If a future change genuinely requires Rust code outside `hosts/server`, record
an explicit decision here with the reason and the resulting SDK release, wasm
pin and lane fingerprint gate requirements before implementing that exception.

## Capability and admission

The box advertises transports.accountWorkerWebSocket.delegatedOrderActionsV1.
It is true only with delegated-order-frames compiled, DELEGATED_BYBIT enabled,
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
Only the connection's admitted private Bybit DELEGATED account/current
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
The additive persisted `callerSuppliedIntentId?: true` reservation survives
completed-history compaction and restore, including adoption of an existing SDK
key. It remains subject to the existing 1,056-record total bound; exhaustion
refuses new keys rather than forgetting reservations. Frame shapes, fingerprints
and the storage methods/checkpoint atomicity are unchanged.
