# DELEGATED order frames v1

This opt-in protocol uses the existing authenticated account socket and the
existing method translation, native completion and order audit. It places one
order per submit through the joined venue's existing native plan. LOCAL and
COSIGNED retain their existing call protocol.
The frozen frame fixture is
`sdk-rs/hosts/server/tests/data/delegated-orders-v1.json`; synthetic router replies
are in `sdk-rs/hosts/server/tests/data/delegated-router-replies-v1.json`.
The router integration pins NX #9404 at `9f52873b76`
(`9f52873b76a64b4258ac38f51e70cd29a1e85941`): frame shapes, socket
renumbering, capability names and the reply semantics below.

Account amendments, venue cancellations, cancel-all and nuke use the additive
[versioned action protocol](delegated-action-frames.md); `order.submit` remains
placement-only and `order.cancelQueued` remains a router queue operation.

## DESIGN

### Activation and binding

Sam's 2026-11-11 activation policy permits DELEGATED with **place and reduce-only
close** while amend, venue cancel, cancel-all and nuke remain unavailable. The
host owns the per-user `DELEGATED` feature flag; the SDK introduces no
server environment variable with that name. Use
`outbox.activate(connection, { DELEGATED })` for explicit SDK admission.
It requires the flag, the two transport/ledger proofs below, and only
`delegatedActivationScopeV1.place === true` and `.close === 'reduceOnly'`. Later
actions are not prerequisites. Missing place/close support refuses with a named
activation error. The legacy low-level `connect` entry point remains for hosts
that already completed their own admission; it does not perform feature discovery.

`enqueueAction('place', params)` and `enqueueAction('close', params)` use the same
durable single-order outbox. Close requires literal `params.reduceOnly === true`;
it never silently changes a close into an opening placement. Unsupported string actions
reject with `DelegatedActionUnavailable` before persistence or a frame, and the
server also rejects their legacy method aliases before native handoff. No path
falls back to LOCAL or COSIGNED. `order.cancelQueued` remains the router's
undispatched-intent cancellation, with its existing conservative uncertainty
semantics; it is not a venue cancel capability.
Raw exec and chaser/TWAP starts or resumes also refuse before entering their
runtime, preventing an alternate route to unsupported venue actions. Existing
stop and cleanup controls remain available.

The server advertises `transports.accountWorkerWebSocket.delegatedOrderFramesV1`
in `capabilities_json`. It is false by default and true only in a build with the
`delegated-order-frames` Cargo feature, a hosted-supported venue with a configured server origin, and
`TEALSTREET_DELEGATED_ORDER_FRAMES=1` in the process environment. The production
image compiles support and defaults the runtime switch to `0`; unset/empty also
means off, and other values refuse startup. Change the container environment and
use the existing drain/restart procedure with the same image to toggle it. It is
not hot-reloaded. Both advertisement and order admission use that runtime control.
Query `/account/capabilities?exchangeName=<joined venue>` (or `/capabilities`
with the same query) on each admitted connection. The response adds
`delegatedVenueV1`, which the host preserves in the outbox capabilities.
Unscoped discovery advertises no actions; a missing or mismatched venue refuses
SDK activation. The neutral host flag is `DELEGATED`, and the disabled refusal
code is `delegatedDisabled`; the former Bybit-named API is removed.
The additive `delegatedActivationScopeV1` record advertises place and reduce-only
close for that hosted venue, with amend, cancel, cancelAll and nuke false. This action record describes
implemented support; the frame switch still controls live admission. The separate
boolean `delegatedOrderActionsV1` retains dedicated-action protocol support.
The typed `enqueueAction(action, options)` API and its action-ledger gate remain
available independently of this partial activation scope.


Hosts enable the outbox only when **both** that box advertisement and the
router-owned `transports.accountWorkerWebSocket.delegatedOrderLedgerV1` are
explicitly true. The latter attests that the routing hop provides the durable
ledger; an older pass-through router omits it. Neither flag implies the other.
The box never supplies the ledger flag. Read fresh capabilities for each connection.
Hosts copy the box's `delegatedOrderFramesV1` and the router's
`delegatedOrderLedgerV1` into the admitted outbox connection's `capabilities`.
Missing or false flags disable submits, queued cancellations and receipt handling;
durable queued work remains subject to its original expiry. Discovery changes
require a new captured ticket; mutating a discovery object cannot enable an old
connection. The router also guards socket admission until its durable-object
ledger is active; capability discovery alone does not make an upgrade ready.
A pending socket or any account/mode/venue/registration mismatch
receives a visible refusal.

Every request carries the account label and that connection's current
`instanceId`. The server requires the same registration and the resident account
held by that socket. A reconnect supplies its new registration; it does not
change the intent's business fingerprint. Account labels are at most 256 UTF-8
bytes and registration IDs at most 128 bytes. Legacy registrations without IDs
remain supported only when both sides omit the ID; LOCAL/COSIGNED envelopes are
unchanged.

### Rolling activation

1. Run the new image on every box with
   `TEALSTREET_DELEGATED_ORDER_FRAMES=0`. Check each box's own capability endpoint:
   `delegatedOrderFramesV1` must be false. A region endpoint samples only one box.
2. Deploy the router with its ledger on using a full deployment, keeping one
   ledger setting and epoch across the deployment. Allow the 300-second Durable
   Object adoption window and verify successful routed socket admission before
   enabling frames. Capability discovery alone does not establish socket readiness.
3. Set `TEALSTREET_DELEGATED_ORDER_FRAMES=1` and drain/restart every box with the
   same image. Verify every box's capability, then both capabilities through the
   router. Re-admit accounts and repeat delegated handover after the restart;
   memory-only key copies do not survive it.
4. Discover capabilities again on each new connection. Follow the router's
   per-user initial coverage interval: it is independent of the global
   adoption wait, and unresolved results stay unknown rather than being resubmitted.

The box accepts inbound routed sockets; it does not establish a connection to the
router. The hosted client owns retries on a 503 upgrade or 1013 close. Hosted
adapters use bounded backoff with a randomized base delay, retaining the retry
count across short opens. A 1013 close follows normal backoff, without an
immediate-move exception. Dropping the old client on logout retires its pending
retry work.

For rollback, first set the box switch to `0`, drain/restart and verify the false
capability on **every box** before turning off the router ledger or changing its
route. Use the router operator's rollout runbook for route restoration and
adoption windows, and schedule box restarts around running account work.
The frame switch does not turn off legacy calls or already-running algorithms.

### Frozen four-frame JSON contract

The top-level shape is `{ "t": "order.<kind>", "d": { ... } }`.
Every `d` includes `accountName`, optional `instanceId`, `intentId`, and `seq`.

| Kind | Additional fields |
| --- | --- |
| `order.submit` | `params` (one order), `ageMs`, `fingerprint` |
| `order.ack` | boolean `received` |
| `order.outcome` | `outcomeSeq`, `outcome` (`placed`, `refused`, `unknown`), `venueOrderId`, `clientOrderId`, `error` |
| `order.cancelQueued` | `fingerprint` |

Outcome ID and error fields are always present and nullable. Error objects use
`message` and optional `outcome: "notSent" | "unknown"`. Receipts echo the input
`seq`. `outcomeSeq` is a positive JS-safe transport delivery counter, not part
of a business result or its fingerprint. The box numbers its registration output;
the router may replace that number on **every** live or replayed outcome with its
own increasing counter for the receiving client socket, spanning accounts and
intents. Replaying a stored result must use the current request's binding and
that receiving socket's counter, rather than an old box/socket counter.

Native completions may finish in a different order from submits; gaps are allowed.
The outbox compares receipt numbers only for the **same intent on the same captured
ticket**, ignoring repeated or lower numbers there. An independent intent's higher
number cannot suppress a result delivered later by a deferred host callback. A
reconnect or replacement registration starts a new receipt-sequence domain; hosts
pass a new outbox connection ticket after admission, even if the registration
label is reused. Old tickets cannot deliver into the new domain. No counter is
added to persisted intents and the four frame shapes are unchanged. The intent
`seq` remains a positive JS-safe integer allocated by the account's outbox and
retained across resends. NX preserves admission order while awaiting durable writes.

Submits are bounded to 64 KiB; acknowledgements to 4 KiB; outcomes to 8 KiB,
including their envelope and registration ID. Native error messages are bounded
to 512 Unicode scalar values and venue order IDs to 256 UTF-8 bytes. Oversized
routing fields receive a visible ordinary error rather than an oversized receipt.

### Key, age, fingerprint and venue ID

`intentId` is a canonical lowercase UUIDv7: 36 ASCII bytes including hyphens,
version nibble 7 and RFC variant 8/9/a/b. Its first 48 bits encode immutable Unix
creation time in milliseconds. Keys are bounded to 128 bytes. Bybit receives
that exact UUIDv7 as `orderLinkId` (36 characters, lowercase hex and hyphens);
`requestId` is the same intent ID. A conflicting caller clientOrderId refuses.
Existing validation still requires one native order.

The box checks the key's own timestamp against its current time: future
timestamps refuse, and an initial submit at age 10,000 ms or more receives
`not sent: expired`. NX likewise declines to forward inadmissible keys, but
answers an unrecorded key `unknown`: absence cannot prove it was never sent
through an earlier route. A client-supplied small age cannot renew an old key.
`ageMs` additionally carries elapsed monotonic time and must be in [0, 10000).

The fingerprint is lowercase SHA-256 hex of UTF-8 bytes consisting of the domain
`tealstreet/delegated-order/v1`, one NUL byte, then canonical JSON of exactly
`{accountName, intentId, params, seq}`. Object keys are ASCII and sorted; arrays
retain order; strings use JSON escaping and valid Unicode; finite binary64
numbers use ECMAScript JSON serialization with negative zero normalized to zero.
Unsafe integers and nesting deeper than 32 refuse. The connection ID and elapsed
age are excluded, so reconnect and retry do not change the business fingerprint.
The SDK validates the fingerprint against the payload. NX must compare it with
the existing durable claim: the same key with a different payload is never
forwarded again or answered with the old payload's result. NX answers unknown
with an intent-conflict message. Shared golden tests pin the hash,
key encoding, frame shapes and Bybit ID mapping.

### NX durable interception and the crash boundary

The ledger and undispatched queue belong to NX; this SDK builds neither.
Before forwarding a first submit, NX resolves the authenticated identity and
registration, validates key time/size/fingerprint, and durably claims that identity
and intent. It does not forward a duplicate claim. It exposes `order.ack` only
after the durable commit and retains the native `order.outcome` for duplicates.
A conflicting fingerprint answers unknown before any second forward.

When ledger coverage begins or changes, the router guards initial admission.
An unrecorded key that may predate coverage, including keys encountered during
the coverage quarantine, answers unknown and is never re-forwarded on retry.
A failed ledger read/write, missing expired record or recorded cancellation
also proves no absence. Only evidence of the box's own refusal (or a key that
could never be a valid order key) permits a not-sent reply. The SDK accepts
these unknown results without resubmitting or labeling them not sent.

A durable claim without an acknowledgement after a DO or box interruption stays
UNCERTAIN internally and is never re-forwarded. The four frames define no
reconciliation request for that state and add no fifth frame. The device treats
it as an unknown outcome; normal order state and history reveal any eventual
venue result. Acknowledged and unknown intents are never submitted again.

NX consumes `order.cancelQueued`, canceling only undispatched work. Once handed
off, it returns the stored receipt/outcome and never converts this frame into a
venue cancellation. The server owns no such queue; an admitted, valid cancellation
that reaches it returns unknown without issuing a venue command. Unadmitted
cancellation frames visibly refuse. There is no HTTP write fallback.

### Router reply compatibility

Router replies use the frozen `order.ack` and `order.outcome` envelopes, with the
requesting socket's `accountName`, optional `instanceId`, `intentId` and input `seq`.
All outcomes carry `venueOrderId`, `clientOrderId` and `error`, including explicit
nulls. The synthetic router fixture pins these cases:

| Situation | Replies | Outbox result |
| --- | --- | --- |
| Claimed intent whose handoff/result cannot be proven | `unknown`, venue ID null, intent UUID as client ID, error `{message: "Order result is not yet known", outcome: "unknown"}`; an ACK is not required | Unknown; never resubmit |
| Recorded box refusal before venue dispatch | Replay ACK `received: false` and/or the recorded `refused` outcome, both IDs null, error `{message: "not sent: expired", outcome: "notSent"}` (the reason follows the box) | Not sent |
| Unrecorded cancellation before dispatch, including a repeated cancellation | The same `unknown` shape above; recording the cancellation blocks submission but is not proof of no prior placement | Unknown; cancellation settled |
| Queue cancellation after dispatch | Replay stored ACK/result when present, or await a live receipt; if the handoff can no longer be proven, the same `unknown` shape above | Stored result or unknown; no venue cancellation |
| Unrecorded expired/pre-coverage key, coverage quarantine, or ledger persistence failure | The same `unknown` shape above, without a fabricated ACK `received: false` | Unknown; never resubmit |
| Same key with another fingerprint | Unknown with message `Intent key already used for a different order` | Unknown; never resubmit |
| Malformed box receipt naming a forwarded intent | Unknown with message `Order receipt was malformed` | Unknown; never resubmit |
| Ordinary malformed/unbindable request error | Ordinary `{t: "msg", d: {op: "error", accountName, data: [message]}}`, without an order receipt | Keep cancellation pending for retry |

A replayed placement retains the stored venue ID and intent UUID as client ID,
with `error: null`. Unknown does not mean not sent. A router queue cancellation
never creates an outcome named `canceled`, a new acknowledgement kind, or a fifth
order frame. Ordinary malformed/unbindable request errors likewise remain ordinary
error messages, not fabricated order receipts. Hosts must not clear pending queue
cancellation merely on receipt of such an ordinary error.
Ignored box receipts preserve the current state (`sending` after physical handoff).
An ordinary request error likewise leaves a handed-off cancellation `unknown`
with `cancelPending: true`; only an authoritative receipt settles that request.

The router decodes box JSON before classifying it, including escaped kind names
and reordered fields. Every receipt the outbox can consume is intercepted or
classified malformed, never forwarded around the ledger. Placed results need a
nonempty venue ID; client IDs are null or the matching intent UUID. Malformed,
oversized and unknown order kinds are not trusted receipts. The fixture pins
valid and hostile encodings against the SDK predicate; receipt kinds and outcome
kinds must be literal strings, never values coerced through user data.

### Host-neutral outbox

Hosts inject persistence scoped by immutable owner/account, secure randomness,
a monotonic clock with an epoch, Unix time, and deadline scheduling. Connect only
after admission and capability discovery. Pass the captured ticket to `flush`
and `receive`, and close the outbox on logout or removal. Deferred storage work
can neither send through a replaced socket nor switch to another owner.

The outbox persists before handoff and marks the row `sending` at the physical
socket boundary. It never resubmits sending, acknowledged or unknown work.
Disconnect or connection replacement changes sending/acknowledged rows to unknown;
late receipts can still resolve those rows on the current admitted connection.
Direct `close()` immediately exposes those retained rows as unknown and fences
all later receipts, callbacks and socket work.
Only `markDelegatedNotSent(intent, evidence)` can construct a `notSent` state.
Its positive no-handoff or authoritative-refusal evidence is stored with its
kind and exact scope/intent/sequence/fingerprint binding. The private runtime
brand is never serialized: restore validates the stored evidence before
reissuing it. A legacy, missing or mismatched proof restores as unknown and
cannot enable a replay. Callers must retain the existing changed/persist and
retention handling after this constructor, including when a local claim is
refused and becomes unknown.
At ten monotonic seconds from creation, its deadline observer marks queued,
never-forwarded work not sent, even while storage is waiting, and marks in-flight
or acknowledged work unknown. Physical handoff rechecks the cap. The synchronous
send adapter must return true only for a physical handoff; only an explicit false
positively guarantees neither handoff nor buffering for a later connection.
A throw or any other non-true return is ambiguous and immediately becomes unknown
with no replay proof, even before the deadline; reconnect and restore cannot
submit it again. An adapter that can positively prove it never transmitted or
buffered the intent may return false.
Cancellation or expiry during a declined first send remains terminal; a legacy
pending cancellation restores as unknown. A queue cancellation after handoff remains
unknown until an authoritative receipt proves non-forwarding. Persisted `sending`
records restore as unknown, including when saving a later result failed. A queued
record from a different monotonic epoch becomes unknown rather than inferring
elapsed time from wall time. Queue cancellation
can be persisted and resent; order submission cannot resume after acknowledgement
or unknown. Rendering, consent and router ledger storage remain host concerns.
Legacy `notSent`/`canceled` records with pending router cancellation also restore
as unknown; their local labels do not establish a final delivery result.
Once the initial queued record is durable, temporary unavailability while preparing
handoff keeps enqueue successful with that same intent ID; retry preserves it.

### Storage retirement

Pending work is limited to 1,024 records; at most 32 completed records are kept
for recent history. Placed, refused, not-sent and canceled records can retire only
after their state is durable and no queue cancellation is pending. Acknowledged
work becomes unknown at the command deadline or disconnect. Unknown rows remain
unresolved for 24 hours from UUIDv7 Unix creation time, evaluated against the
injected Unix clock on timers and reload (a clock rollback extends retention).
Expiry emits `changed` with `state: "unknown", retentionExpired: true`, clears any
pending queue cancellation, and persists that explicit expired-history marker.
Hosts surface expired unresolved history and use ordinary order state/history for
reconciliation. Expired unknowns join the latest-32 completed history only after
the expiry marker is durable. Compaction never changes their outcome to not sent
or makes their identity resendable. Storage failure retains them until flush can
retry. A later authoritative result may still resolve a retained history row.

Adapters provide `compact(scope, {version:1, sequence, retiredIntentIds})` as one
atomic transaction. It deletes only those IDs for that owner/account and advances
the durable sequence checkpoint with `max(previous, sequence)`, preserving every
other record and other scopes. This also preserves newer writes during delayed
retirement. `put` upserts one record without changing that checkpoint. `load`
returns `{version:1, sequence, intents}`; legacy intent arrays are accepted and
compacted on restore. The next sequence exceeds both the checkpoint and every
loaded record. If compaction fails, memory retains the records and flush retries
later. Enqueue returns its durable intent ID independently of unrelated retirement,
including after a physical handoff. Hosts use one active outbox per scope and
close it before replacing it.
