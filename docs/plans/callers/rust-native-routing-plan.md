# Native routing-plan resolver

**Change.** `safe_cex_native::resolve` ports `resolveEngineRoutingPlan` and the
hosted account transport defaults (`normalizeExchangeAccountConfig` then
`resolveAccountTransport`) to Rust. It covers the Tealstreet and direct
providers, with the browser `WS_RELAY_PRIVATE` relay gate off as on the hosted
server. The per-venue data lives in `sdk-rs/hosts/native/src/routing/table.json`,
generated from the TS routing table and metadata.

**Callers.**
- The native exchange server (`sdk-rs/hosts/server/DESIGN.md`) calls
  `account_transport` and then `resolve_routing_plan` once per init, and passes
  the plan to `NetworkTransport::with_routing`.
- TS and NX are unchanged.

**Obligation.** A change to the TS routing table, metadata transport flags or
hosts must regenerate the table and plans with `UPDATE_NATIVE_ROUTING_PORT=1`
until the TS resolver is deleted. After that, `table.json` is the source.

**Gaps.** Secondary read keys are refused with `SecondaryUnsupported`; the hosted
payload carries none.

**Security.** `RoutingInputs` has no `overrideProxyUrl` and refuses unknown fields.
An authorized route would carry the account's proxy token to the host it named,
so every authorized prefix is a table proxy host (pinned by `routing_port.rs`).
`account_transport` sets `has_key`; an unauthenticated public account must set
`public_data_only: Some(true)` itself, or its plan resolves as a private one.
