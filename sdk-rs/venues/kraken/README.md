# Kraken venue adapters

## Position liquidation prices

Futures [`open_positions`](https://docs.kraken.com/exchange/api-reference/futures-websocket/open_position)
publishes `liquidation_threshold` in the instrument's quoted price units.
Only finite, positive numbers and numeric strings are usable prices.
Missing, null, empty, malformed or nonpositive thresholds are unknown. Some
Futures accounts publish `0.0` when no per-position threshold exists; a zero
or negative price cannot be used as a position's liquidation price. Normalize
these sentinels to unknown without preventing the position from updating.
This deliberately corrects the TS reference's preservation of explicit zero.

REST [`openpositions`](https://docs.kraken.com/api-reference/account-information/get-open-positions)
does not publish that threshold. It preserves a previously received WS value;
without one, the price is unknown. [`accounts`](https://docs.kraken.com/api-reference/account-information/get-wallets)
has different wallet shapes: `flex` / `multiCollateralMarginAccount` publishes
portfolio and margin totals, while single-collateral `fi_*` margin accounts can
include account-level `triggerEstimates.lt`. The TS reference uses accounts for
balances, not liquidation-price derivation; this adapter likewise does not
invent a per-position estimate from those totals.

Unknown prices use the packed frame's NaN representation and serialize to JSON
`null`. The SDK frame applier preserves that as `Position.liquidationPrice: null`;
finite positive prices remain numbers. Spot currently exposes wallet
balances and an empty positions table, without margin-position REST/WS mapping.

## Futures socket liveness

Every public and private socket subscribes to Kraken's documented
[`heartbeat` feed](https://docs.kraken.com/exchange/api-reference/futures-websocket/heartbeat)
on open, including reconnects and before private authentication. Futures rejects
the Spot-style JSON `event: ping`; it is not a WebSocket control ping. The
heartbeat feed supplies periodic receive evidence without that invalid request.

Both Futures sockets expire after 20 seconds without accepted receive evidence,
matching the TS reference's base watchdog. The managed supervisor checks expiry
at the actual callback time, including a monotonic jump after sleep. It retires
the old socket before accepting a late frame, applies bounded reconnect backoff,
then replays ticker/book interests or obtains a fresh private challenge and
subscribes fills, open orders and positions. Account reload is not required.
Connect/auth deadlines, retry caps and Spot policy remain unchanged.

Heartbeats prove socket liveness only. Separate venue-owned clocks watch data:

- Each subscribed ticker symbol must produce a decoded payload with at least
  one ticker value within 30 seconds (the documented
  [delta throttle](https://docs.kraken.com/exchange/api-reference/futures-websocket/ticker)
  is one second). Silence retires the public socket so its ticker/book interests are
  replayed; a busy sibling symbol, empty envelope or subscription ack cannot
  refresh that symbol. New interests start fresh clocks; removed ones stop.
- Each subscribed book retains its existing validated-data clock: after more
  than 120 seconds without a snapshot/delta it requests unsubscribe/subscribe,
  retrying at 30-second intervals until a valid snapshot arrives.
- Private fills, open orders and positions can legitimately stay quiet after
  their initial snapshots. Heartbeats and the 20-second socket receive watchdog
  maintain that connection; inactivity does not unsubscribe or reconnect it.
  Keep the live subscription uninterrupted while it is healthy.

After a required reconnect, `fills_snapshot` executions newer than the last
emitted execution are emitted as live fills and can advance order progress.
The initial cutoff is account startup, so old startup history stays quiet.
The cutoff is fixed for each snapshot batch to accept newest-first history and
distinct executions sharing a timestamp. Only emitted fill IDs enter the bounded
deduplication cache, which survives reconnects; skipped history, missing snapshot
timestamps and unmapped markets cannot suppress a later live delivery. Live
deltas without timestamps still emit and use receipt time for the next cutoff.

The existing five-second venue timer checks these clocks independently of the
scaled account REST poll. Ticker expiry is also checked before accepting late
data. Initial private authentication and the 180-second first-payload bound
remain independently enforced; heartbeats cannot satisfy missing initial data.

The deterministic `socket_supervision::futures_wake_recovery_measurement` test
models paused timers, a clock jump, 50 ms socket handshakes and 50 ms server/auth
response stages. It measures first frames separately from complete private
snapshots; its five-second assertion is a simulation bound, not a network SLA.

Kraken Spot treats a dispatched write's HTTP 408, 429, or 5xx response as
`Outcome::Unknown`, even when the body contains a native signature, credential,
or rate-limit error. HTTP 429 retains `ErrorKind::RateLimited` for diagnostics.
These responses cannot authorize a resend or terminal credential rejection;
uncertain placements trigger an `OpenOrders` read before another placement.

Definitive signature refusals retain the single re-sign attempt. Private reads
also retain their existing bounded signature and transport retry policies.

Futures writes likewise keep HTTP 408, 429, 5xx, and native `apiLimitExceeded`
responses unknown, including a bracket leg after an accepted entry. Their user
text says the outcome is unknown and directs the caller to Check Open Orders.
These failures never resend or place a replacement stop. Definitive refusals and
private reads retain their existing behavior.

All Futures Unknown trading failures include reconciliation guidance, including
HTTP 200 native server errors and unproven cancellation statuses. Batch errors
retain an unknown leg's message and native diagnostics together when a later
leg is definitively refused.
