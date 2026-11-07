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


# Endpoint migration follow-up

The proposal in `docs/plans/RUST_WOO_ENDPOINT_MIGRATION.md` inventories the active
native REST/socket contracts and next-cut RED gates. It changes no runtime route;
legacy availability is not proved by an unauthenticated response.

# Dispatched write rate limits

HTTP 429 or WOO code `-1003` after a dispatched write is `RateLimited` with an
`Unknown` outcome, never proof of refusal or permission to resend. HTTP 429 takes
precedence over body codes; read retries and pre-send `NotSent` remain separate.
Native `-1003` retains `RateLimited` even on HTTP 408/5xx.

A mode setter can finish after a successful read verifies FUTURES and the exact
requested mode. Until then, an ambiguous write retains its original error through
read retries; failed verification remains `Unknown`, and the write is never resent.

Cancel-all retains structured leg failures and both path-prefixed messages. A
rate-limit leg supplies the aggregate kind/code; any unknown leg keeps the whole
command `Unknown`, even if its peer succeeds or is definitively refused.
