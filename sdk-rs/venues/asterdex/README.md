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
Cancellation-only batches retain their existing best-effort behavior.

Quantities retain nearest-step normalization; maximum quantities are floored to
native units. A short tail borrows only from lots that remain above their own
minimum. Every serialized lot must satisfy its quantity bounds and step, plus
opening `MIN_NOTIONAL` at the rounded limit price or current mark for market
orders. Market orders use `MARKET_LOT_SIZE`; other orders use `LOT_SIZE`.
Reduce-only and full-position closes retain the native notional exemption.
Attached protection is split when necessary, in entry/SL/TP order, and every
resulting protection counts toward the symbol's algo limit across the batch.

Replacement planning completes before cancellation. Protective amendments must
have one native replacement so their exact put-back contract stays intact.
After dispatch, an ambiguous answer stops the plan without resending; a later
venue refusal or concurrent fill can still leave a partial result.
