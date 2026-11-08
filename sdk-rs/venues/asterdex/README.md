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

# Write-plan validation

Placement builds and validates the whole batch, including every split entry and
attached TP/SL lot, before requesting the write clock or submitting an order.
Any unsupported row, malformed price/trigger, invalid native lot, or cumulative
algo-count overflow refuses the entire intent with `notSent` and no requests.
Cancellation batches use the same fail-stop journal; an invalid reference refuses the whole batch before dispatch.

Quantities retain nearest-step normalization; maximum quantities are floored to
native units. A short tail borrows only from lots that remain above their own
minimum. Every serialized lot must satisfy its quantity bounds and step, plus
opening `MIN_NOTIONAL` at the rounded limit price or current mark for market
orders. Market orders use `MARKET_LOT_SIZE`; other orders use `LOT_SIZE`.
Reduce-only and full-position closes retain the native notional exemption.
Attached protection is split when necessary, in entry/SL/TP order, and every
resulting protection counts toward the symbol's algo limit across the batch.

Replacement planning completes before cancellation. Protective amendments retain
one native replacement and the final live stop guard. The shared engine
`WritePlan` validates the exact immutable payloads before the first request and
journals every dispatched write. A runtime refusal or ambiguity stops the suffix;
there is no automatic resend, protective put-back, cancel or rollback after failure.
Engine-granted signature/clock correction for a proven refusal keeps its existing
custody and the same pending logical element.

Terminal failures carry every planned element's indexed `placed`, `applied`,
`notSent` or `unknown` outcome. Successful cancel/settings effects are `applied`
with existing-target references, never invented placement IDs. A successful cancel
followed by a refused replacement leaves the original order gone and reports both
facts for the user to act on. Lost replies and engine-sealed HTTP ambiguity retain
the placed prefix, unknown pending write and untouched notSent suffix.
