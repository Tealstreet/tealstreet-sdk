# Binance Options Rust port

This change adds the `binanceoptions` name to the existing Binance family. SDK,
WASM and beta constructors select `Variant::Options`; there is no TypeScript
fallback. Private operations use the same installed HMAC identities, role
selection and managed dispatch as the other Binance variants. Remote signing
uses the existing Binance construction policy.

Implementation in progress: eapi market discovery and derivative terms, ticker
and mark seeds, balances, positions, orders, history, supported limit-order
placement/cancellation and managed option streams. The TypeScript reference is
`packages/safe-cex/src/__tests__/reference/binance/binanceoptions.*`; its REST
polling and exact payloads provide replay fixtures. WebSocket support is pinned
separately to Binance's documented Options streams because that reference has
no socket implementation.

No funds or credentials are used to construct tapes. Unsupported order shapes
must fail before any transport release. Existing Binance variants keep their
endpoint tables and financial mappings. Every response publishes only inside
its live managed delivery; uncertain writes retain the shared placement fence
until a clean fresh order census.

Qualification, byte deltas and remaining gaps will be recorded on the draft PR.
This document does not claim execution, review clearance or beta readiness.
