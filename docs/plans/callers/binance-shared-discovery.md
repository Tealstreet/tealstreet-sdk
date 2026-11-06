# Shared Binance anonymous discovery

SDK account workers share only anonymous public GET exchangeInfo on the closed
Binance mainnet/testnet endpoint set. Network, exact origin/path/query and resolved
public route remain isolated. Original packets must still match at bind; no key,
token, account identity, private header, write or candle enters the coordinator.

The host uses one SharedCatalogue implementation for HL and Binance policies.
Every native engine parses exact response bytes and publishes its own Store.
Binance success TTL is 5 s, reply limit 8 MiB, entry limit 64 and lease limit 25 s,
clamped to the original managed deadline. Its shared failure cooldown grows
1/2/4/8/16/30 s; Retry-After can extend it to at most five minutes. No follower
bypasses a live lease or cooldown, and stale leases refuse before claim/handoff.
Future-dated strict leases/cooldowns remain blocking; rollback never authorizes
an election or cached-byte reuse. Expired leases retain their minimum failure
window while the owner settles its deadline. A dispatched deadline starts an
owner-fenced cooldown before abort cleanup, without delaying the caller;
never-dispatched expiry and user/follower cancellation do not publish failures.
The original deadline callback is authoritative even if wall time rolls back.
Queued failure and success publications recheck the transport lifetime at the
storage write, independently of the timed-out request's cleanup signal.
Cancellation and late completion are owner-fenced.

Without working storage at startup, workers keep independent discovery and native
backoff so Binance remains usable. A coordinator that has completed a transaction
must refuse or time out during later storage failure; it cannot infer that an
unknown peer lease is absent. Successful HTTP responses do not wait for cache
publication. Existing HL bounds, freshness checks and quota admission stay intact.

This host-only change depends on #3045/#3052. No native watchdog, discovery backoff,
financial reducer, wasm binary or host rollout changes. Do not delete per-engine
backoff or publication: sharing is optional when initial storage is unavailable.
