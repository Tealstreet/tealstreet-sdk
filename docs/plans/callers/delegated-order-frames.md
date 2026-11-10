# DELEGATED order frames v1

This opt-in protocol uses the existing authenticated account socket and the
existing method translation, native completion and order audit. It places one
Bybit order per submit. LOCAL and COSIGNED retain their existing call protocol.
The shared fixture is `sdk-rs/hosts/server/tests/data/delegated-orders-v1.json`.

## DESIGN

### Activation and binding

The server advertises `transports.accountWorkerWebSocket.delegatedOrderFramesV1`
in `capabilities_json`. It is false by default and true only in a build with the
`delegated-order-frames` Cargo feature and the existing Bybit DELEGATED flag.
NX and client hosts must check this advertisement before using the protocol.
The outbox also requires it in the captured connection ticket. A pending socket
or any account/mode/venue/registration mismatch receives a visible refusal.

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
`seq`. Each account on a connection has an increasing `outcomeSeq`, assigned
atomically when an outcome is queued for that socket. Native completions may
finish in a different order from submits; sequence gaps are allowed, reversals
are ignored by the outbox. A reconnect or replacement registration starts a new
receipt-sequence domain; hosts pass a new outbox connection ticket after admission.
The intent sequence is a positive JS-safe integer allocated by the account's
outbox and retained across resends. NX preserves admission order while awaiting
its durable writes.

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
At ten monotonic seconds from creation, its deadline observer marks queued,
never-forwarded work not sent, even while storage is waiting, and marks in-flight
or acknowledged work unknown. Physical handoff rechecks the cap. The synchronous
send adapter must return true only for a physical handoff; false or a throw
positively guarantees no handoff. A queue cancellation after handoff remains
unknown until an authoritative receipt proves non-forwarding. Persisted `sending`
records restore as unknown, including when saving a later result failed. A queued record from a different monotonic epoch becomes
not sent rather than inferring elapsed time from wall time. Queue cancellation
can be persisted and resent; order submission cannot resume after acknowledgement
or unknown. Rendering, consent and router ledger storage remain host concerns.
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
