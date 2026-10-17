# Coinbase production binding

`CoinbaseExchange::new` selects the native Coinbase or Coinbase Spot class from
`ExchangeName`, with the caller's options and shared Store. It composes the existing
portfolio, market/position, order, lifecycle, tick and public/private stream bodies.
The shared JS runtime supplies HTTP interception, hot producer ownership, timers,
BaseExchange and BaseWebSocket. Starbase remains inert.

`CoinbaseDependencies` requires a shared `CdpSigningCache`, HTTP/runtime hosts,
optional external public-data factory and explicit local-wall-time conversion.
Share the cache between instances in one SDK module. The native registry host
retains that cache and uses its existing UTC date profile. It constructs both
`coinbase` and `coinbasespot` through `ConstructionPath::Rust`; module-host loading
keeps its existing behavior. Live source metadata is checked for each class;
Coinbase Spot retains its source refusal of external public-only mode. Hosted signing uses the shared `JsHttpHost` path from
#151, including the account name and executable source options capability.

The crypto host imports P-256 keys with the opaque production ES256 primitive
from #148 and uses shared Ed25519 for normalized seed credentials. Its native
JWT test executes the current TS/WebCrypto oracle, compares the exact unsigned
claims and verifies Rust signatures in WebCrypto. Imported-key identity and the
original native PEM decoder failure are asserted separately. RFC6979 nonce and
platform error choices remain documented by that primitive's manifest rows.

## Executed integration witnesses

`tests/exchange_binding.rs` constructs the actual production class with scripted
HTTP and WebSocket leaves. It exercises both constructors; account selection and
validation; pure market/ticker/position/order/balance reads; client-ID placement;
cancellation; fail-closed native spot edit; alias-resolved candles; startup and
owned socket disposal; the concrete WS signing hook; public candle/book listeners;
and shared hosted WS signing with exact request fields. A failed hosted signature
sends no subscription and closes the connection for the base retry path.

The duplicate-price book witness delivers actual public socket snapshots and
deltas for both variants and compares the listener output with executed source
`listenOrderBook` and its real helper imports. Copyback from shared book helpers
consumes each retained level once, preserving distinct same-price amounts and
running totals. The per-declaration exemption review is in
[EXEMPTION_REVIEW.md](EXEMPTION_REVIEW.md); retaining a capability exemption does
not close a separately recorded mapped-body finding.

The native-number witness checks negative-zero bits and infinity in the Store
after the real tick loop. The subscriber witness checks market commit before
failure, original subscriber cause identity, stopped later fanout and absence of
socket connection after the failed startup publication. A cached-balance fallback
keeps optional info/coins, nested currencies, infinity and negative zero directly
from the native Store snapshot while applying the source balance field updates.

`safe-cex-registry/tests/native_host_coinbase.rs` constructs both names through
the registry and drives the production native host over real local HTTP/WS
servers, including JWT headers, market reads and private subscriptions. It uses
only explicitly synthetic credentials. `tests/decoder_boundaries.rs` inventories
the exact host implementation/method sets and checks that PEM/P-256 and WS JSON
parsing delegate to their reviewed primitive boundaries.

The inherited unsupported leverage/mode operations retain the source
BaseExchange `Not implemented` rejection. Optional historical capabilities retain
the branded base defaults because Coinbase declares no corresponding override.
The binding does not invent venue capability support.

## Scope and remaining parity findings

Direct typed model fields use the shared native-number bridge. Retained raw
response/info/error records keep the existing finite JSON profile; public Rust
strings keep the shared UTF-8 boundary. Typed callbacks expose owned model values,
not arbitrary JavaScript function/property mutation or reference identity.

The existing fourteen notification findings in
[NOTIFICATION_REACHABILITY_REVIEW.md](NOTIFICATION_REACHABILITY_REVIEW.md) remain
classified individually. The new startup native-origin witness does not assert
that every previously recorded throw path has been independently closed. This
binding supplies the runtime join and does not silently promote that audit floor.

All 21 newly exposed constructor/balance/trait/stream entrypoints were individually
replaced with a compiled runtime failure. Each selected integration test failed;
the restored 13-test suite passed. The final suite adds the fourteenth cached
balance test and passes. A separate compiled control removes the native cached
snapshot and fails that new test; restoring the snapshot passes it. Final command results are recorded with the PR.
No external exchange, funded key or live account was used.
