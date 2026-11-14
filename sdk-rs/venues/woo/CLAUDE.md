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

# Provider signing

The registry consumes legacy credentials through trusted signer setup and gives
Woo opaque primary/read identities. All private REST calls use dispatch-time
`sign::dispatch::Http`; private WS auth uses `WsAuth`. GETs select the configured
read identity, while writes and WS auth select primary. Native direct-construction
compatibility still accepts raw Options for existing callers and replay controls;
production registry construction rejects raw material after setup. There is no
raw-key fallback from a provider failure. WOO signs timestamp-only freshness,
with no nonce or wire recvWindow. Held attempts re-sign after protocol clock
correction; invalid receipts/results never publish transport.
Local REST/WS signatures and provider dispatch use protocol UTC; scheduling time
continues to drive deadlines and timers after host-clock corrections.

Hosted RemoteSigner uses the existing bounded signing capture/provider harness.
PayloadSigner remains refused. `tests/fixtures/hosted-signing.json` pins every
private route plus ordinary/protective controls against independent Python HMAC
vectors; native and actual-WASM tests compare payloads and complete signed wires.
A signing failure before a write is `notSent`; loss after dispatch is `Unknown`
and never retries that write. Composite failures preserve earlier accepted IDs
and uncertainty.

The request inventory gains no legacy HTTP/completion sites: provider and direct
construction share the existing request site; credential selection attaches the
provider and its egress together. Identity signing errors reuse the existing
completion helper while retaining typed errors. The signing inventory freezes
the test-only local oracle and updates the existing raw compatibility guard/call
fingerprints; it adds no production credential holder or crypto primitive.

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

# Private socket egress and auth diagnostics

Mainnet WOO accounts using the Tealstreet REST proxy or HOW tunnel also send
their private socket through the fleet's existing `rpc.relay`, with the original
application-ID suffix. Public, unproxied and staging sockets stay direct. The
shared TS/native routing table owns the override. Native Hosted routing resolves
WOO mainnet/staging REST origins and carries the init application ID into the
private relay URL; signing admission still uses the registry policy. Roll the
production upstream allowlist on both tunnel hosts before merging this routing
change; no runtime feature flag gates the override.

Auth refusals expose only the numeric WOO code, an optional numeric response
status, and closed reasons from `https://docs.woox.io/#error-codes`. Free vendor
`message`/`error`, authentication parameters and unknown fields are discarded.
The broad `-1002` does not identify an IP refusal, so it adds no whitelist advice.
An auth-frame status is a vendor status, not a measured HTTP upgrade response.
