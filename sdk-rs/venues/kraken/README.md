# Kraken venue adapters

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
  In particular, a routine fills unsubscribe could hide a new execution in a
  replacement history snapshot, suppressing its live fill notification and
  order progress. Keep the live subscription uninterrupted while it is healthy.

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
