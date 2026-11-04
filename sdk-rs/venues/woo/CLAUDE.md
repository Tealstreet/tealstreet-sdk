# Private replay checkpoint timing

For `woo-private.jsonl`, subscribing to `BTCUSDT` adds a public REST snapshot to
TS's shared HTTP rate limiter. `ReplayClock.wait` advances timers while TS awaits
mode confirmation and a position refresh, so a checkpoint's tape label is not
necessarily its observation time: checkpoint 14 is labelled 9.5s, but TS has
advanced to 11s and read a reopened 0.01 long. The tape closes the preceding
0.023 long at 8.6s and returns empty positions until 11s. Rust correctly captures
flat at 9.5s; at checkpoint 15 (11.5s) its last read is still empty, then its
natural 12s poll observes the reopened position. Rust behaves identically with
and without the book subscription. The 14 TS order rows at these checkpoints
are all terminal (two filled, twelve canceled); Rust drops them after complete
empty `INCOMPLETE` ordinary/algo snapshots, and TS drops them after its later
census finishes. Both retain the fills; order history is queried separately.
This is checkpoint timing and polling latency, not a book-triggered Rust wipe.
Check selected HTTP responses and actual virtual-clock time before diagnosing
these differences; retaining terminal rows or future position state would be
the wrong runtime fix.

# Private socket egress and auth diagnostics

Mainnet WOO accounts using the Tealstreet REST proxy or HOW tunnel also send
their private socket through the fleet's existing `rpc.relay`, with the original
application-ID suffix. Public, unproxied and staging sockets stay direct. The
shared TS/native routing table owns the override; native Hosted WOO admission
remains refused. Roll the production upstream allowlist on both tunnel hosts
before merging this routing change; no runtime feature flag gates the override.

Auth refusals expose only the numeric WOO code, an optional numeric response
status, and closed reasons from `https://docs.woox.io/#error-codes`. Free vendor
`message`/`error`, authentication parameters and unknown fields are discarded.
The broad `-1002` does not identify an IP refusal, so it adds no whitelist advice.
An auth-frame status is a vendor status, not a measured HTTP upgrade response.
