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
