# AsterDEX socket recovery

Public and private sockets use the engine `Supervisor` for a 10-second connect
and token-acquisition deadline, a 20-second receive watchdog, and capped
exponential reconnect with deterministic jitter. Opening a connection does not
reset failures; a stable session does. Both sockets retain their native
`LIST_SUBSCRIPTIONS` heartbeat every five seconds.

Private recovery obtains a new listen key. Retiring a session cancels its
30-minute keepalive and invalidates pending key and keepalive responses by
session epoch, so an old failure cannot close a replacement socket. Account
reconciliation and retained public subscriptions keep their existing ownership.

# Legacy write parity

Split lots, batch rows and separate entry/SL/TP children retain the legacy
TypeScript driver's partial placement behavior. Rejected elements do not stop
later writes. Caller client IDs are checked across the batch before I/O, while
algo capacity is checked per intent without a cumulative batch reservation.
Cancellation batches continue after a refusal; a refused single cancel in an
amend prevents its replacement. Accepted placement IDs survive partial failures.

The shared write-plan types report indexed placed, applied, refused and uncertain
outcomes on terminal failures. They do not gate payloads or decide continuation.
Engine-owned transport ambiguity still seals writes under its existing custody
rules. No new validation or retry policy is introduced here.

Known legacy limitations remain: a split tail or later row can be refused after
an earlier write takes effect, and an entry can succeed without both protective
children. A replacement can fail after its original order was canceled.

The class-A restore kept pending policy is the inherited Rust exception: after
a confirmed protective cancel and a definite replacement refusal, its existing
single put-back is unchanged. Legacy TS `updateOrder` has no corresponding
restore; this exception follows the lead's explicit HOLD/no-removal instruction.

Evidence: `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts`
lines 742–867 (per-intent checks and splitting), 955–997 (placement and batch
continuation), and 1000–1081 (cancel-replace and cancellation fan-out).

Stop-after-Unknown is a recorded TS parity exception (Sean/daxx, 2026-11-08).
An uncertain row ends the action with its placed/applied prefix, Unknown row
and unsent suffix in the journal, standard Check Open Orders guidance and a
`write.unknown` event for venue-decoded failures. Engine-owned HTTP ambiguity
seals still need a venue telemetry callback; this PR remains held for that helper
change. Definite refusals keep the TS continuation behavior.
