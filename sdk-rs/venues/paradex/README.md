# Paradex perpetuals

The sans-I/O venue names the real production/testnet REST and WebSocket URLs.
Use the SDK's resolved Tealstreet transport in JavaScript, or pass its serialized
routing plan and proxy-token callback to the native host. Paradex proxy requests
carry `Tealstreet-Authorization` separately from the venue's `Authorization`
JWT. The current routing table declares no Paradex WebSocket relay override.

WebSockets require SBE schema 1, version 2. Public and private sockets share an
upstream URL but have separate channel roles, authentication and recovery.
Hosts must support the explicit binary transport path; no JSON-only fallback
is available from the current upstream service.

Private mode supports local StarkNet subkeys and the SDK's Ethereum-key account
derivation. It needs the Paradex account/subkey addresses and system configuration
for the selected environment. Remote/payload signing rejects before account I/O.
Public-only mode needs no credentials. The SDK registration is opt-in until
application realm-loader adoption and default-runtime retirement land.

Orders support limits, market orders, capped market IOC limits, conditional and
bracket orders, native amendments, and per-market cross leverage. Fixed cross
margin and one-way mode requests complete without a write; isolated/hedged mode
requests reject. Failed or ambiguous writes are never automatically replayed.

`docs/plans/RUST_PARADEX.md` records phase reviews and replay gates. The harness
verifies emitted authentication/order signatures independently and replays
recorded public binary frames without replacing market data.
