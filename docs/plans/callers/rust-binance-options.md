# Binance Options Rust port

This change adds the `binanceoptions` name to the existing Binance family. SDK,
WASM and beta constructors select `Variant::Options`; there is no TypeScript
fallback. Private operations use the same installed HMAC identities, role
selection and managed dispatch as the other Binance variants. Remote signing
uses the existing Binance construction policy.

This change provides eapi market discovery and derivative terms, ticker
and mark seeds, balances, positions, orders, history, supported limit-order
placement/cancellation, option-chain and account-Greeks queries. The TypeScript reference is
`packages/safe-cex/src/__tests__/reference/binance/binanceoptions.*`; its REST
polling and exact payloads provide replay fixtures. WebSocket support is deferred by the scope ruling below because the reference
has no socket implementation.

Only synthetic accounts and keys are used to construct offline tapes. Unsupported order shapes
must fail before any transport release. Existing Binance variants keep their
endpoint tables and financial mappings. Every response publishes only inside
its live managed delivery; uncertain writes retain the shared placement fence
until a clean fresh order census.

Qualification, byte deltas and remaining gaps will be recorded on the draft PR.
This document does not claim execution, review clearance or beta readiness.


## European eapi scope ruling

ddi ruled that this PR ships REST/polling parity with the retained TypeScript
reference. Options WebSockets are a separate follow-up: this PR creates no
Options feed or listen-key request, and streaming subscriptions report
Unsupported. No live probes are authorized or performed. The other Binance
variants retain their existing managed sockets.

The first codec checkpoint's id/tradeId alias and terminal-snapshot findings
have real assertion-RED regressions. A fill carrying both native fields selects
tradeId first. A census that contradicts known terminal or cumulative-fill
proof publishes nothing and cannot clear the uncertain-placement fence.
