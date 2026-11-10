# DELEGATED order frames v1

This opt-in protocol uses the existing authenticated account socket and the
existing method translation, native completion and order audit. It places one
Bybit order per submit. LOCAL and COSIGNED retain their existing call protocol.
The frozen frame fixture is
`sdk-rs/hosts/server/tests/data/delegated-orders-v1.json`; synthetic router replies
are in `sdk-rs/hosts/server/tests/data/delegated-router-replies-v1.json`.

## DESIGN

### Activation and binding

The server advertises `transports.accountWorkerWebSocket.delegatedOrderFramesV1`
in `capabilities_json`. It is false by default and true only in a build with the
`delegated-order-frames` Cargo feature and the existing Bybit DELEGATED flag.
Hosts enable the outbox only when **both** that box advertisement and the
router-owned `transports.accountWorkerWebSocket.delegatedOrderLedgerV1` are
explicitly true. The latter attests that the routing hop provides the durable
ledger; an older pass-through router omits it. Neither flag implies the other.
Hosts copy the box's `delegatedOrderFramesV1` and the router's
`delegatedOrderLedgerV1` into the admitted outbox connection's `capabilities`.
Missing or false flags disable submits, queued cancellations and receipt handling;
durable queued work remains subject to its original expiry. Discovery changes
require a new captured ticket; mutating a discovery object cannot enable an old
connection. A pending socket or any account/mode/venue/registration mismatch
receives a visible refusal.

Every request carries the account label and that connection's current
`instanceId`. The server requires the same registration and the resident account
held by that socket. A reconnect supplies its new registration; it does not
change the intent's business fingerprint. Account labels are at most 256 UTF-8
bytes and registration IDs at most 128 bytes. Legacy registrations without IDs
remain supported only when both sides omit the ID; LOCAL/COSIGNED envelopes are
unchanged.

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

The server and NX check the key's own timestamp against their current time:
future timestamps refuse, and an initial submit at age 10,000 ms or more receives
`not sent: expired`. A client-supplied small age cannot renew an old key.
`ageMs` additionally carries elapsed monotonic time and must be in [0, 10000).
A previously unrecorded expired key receives that refusal without creating a
long-lived tombstone; NX stores no tombstones beyond its ledger TTL.

The fingerprint is lowercase SHA-256 hex of UTF-8 bytes consisting of the domain
`tealstreet/delegated-order/v1`, one NUL byte, then canonical JSON of exactly
`{accountName, intentId, params, seq}`. Object keys are ASCII and sorted; arrays
retain order; strings use JSON escaping and valid Unicode; finite binary64
numbers use ECMAScript JSON serialization with negative zero normalized to zero.
Unsafe integers and nesting deeper than 32 refuse. The connection ID and elapsed
age are excluded, so reconnect and retry do not change the business fingerprint.
The SDK validates the fingerprint against the payload. NX must compare it with
the existing durable claim: the same key with a different payload is refused,
never answered with an old payload's result. Shared golden tests pin the hash,
key encoding, frame shapes and Bybit ID mapping.

### NX durable interception and the crash boundary

The ledger and undispatched queue belong to NX; this SDK builds neither.
Before forwarding a first submit, NX resolves the authenticated identity and
registration, validates key time/size/fingerprint, and durably claims that identity
and intent. It does not forward a duplicate claim. It exposes `order.ack` only
after the durable commit and retains the native `order.outcome` for duplicates.
A conflicting fingerprint refuses before any second forward.

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
| Submit refused before dispatch | ACK `received: false`, then `refused`, both IDs null, error `{message: "not sent: order ledger unavailable", outcome: "notSent"}` (other validation refusals use their reason) | Not sent |
| Queue cancellation before dispatch, including a repeated cancellation | ACK `received: false`, then `refused`, both IDs null, error `{message: "not sent: canceled", outcome: "notSent"}` | Not sent; cancellation settled |
| Queue cancellation after dispatch | Replay stored ACK/result when present, or await a live receipt; if the handoff can no longer be proven, the same `unknown` shape above | Stored result or unknown; no venue cancellation |
| Queue cancellation for an unretained expired key | The same `unknown` shape above | Unknown; no venue cancellation |
| Queue cancellation whose persistence fails | Ordinary `{t: "msg", d: {op: "error", accountName, data: ["order ledger unavailable"]}}`, without an order receipt | Keep cancellation pending for retry |

A replayed placement retains the stored venue ID and intent UUID as client ID,
with `error: null`. Unknown does not mean not sent. A router queue cancellation
never creates an outcome named `canceled`, a new acknowledgement kind, or a fifth
order frame. Ordinary malformed/unbindable request errors likewise remain ordinary
error messages, not fabricated order receipts. Hosts must not clear pending queue
cancellation merely on receipt of such an ordinary error.

### Host-neutral outbox

Hosts inject persistence scoped by immutable owner/account, secure randomness,
a monotonic clock with an epoch, Unix time, and deadline scheduling. Connect only
after admission and capability discovery. Pass the captured ticket to `flush`
and `receive`, and close the outbox on logout or removal. Deferred storage work
can neither send through a replaced socket nor switch to another owner.

The outbox persists before handoff and resends the same unacknowledged intent
across reconnects within ten monotonic seconds. Its deadline observer marks
not-sent state even while storage is waiting, and physical handoff rechecks the
cap. Persisted `sending` records restore as unknown, including when saving a
later result failed. A queued record from a different monotonic epoch becomes
not sent rather than inferring elapsed time from wall time. Queue cancellation
can be persisted and resent; order submission cannot resume after acknowledgement
or unknown. Rendering, consent and router ledger storage remain host concerns.
Once the initial queued record is durable, temporary unavailability while preparing
handoff keeps enqueue successful with that same intent ID; retry preserves it.

### Storage retirement

Pending work is limited to 1,024 records; at most 32 completed records are kept
for recent history. Placed, refused, not-sent and canceled records can retire only
after their state is durable and no queue cancellation is pending. Acknowledged
and unknown records remain retained and never become resendable through pruning.

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
