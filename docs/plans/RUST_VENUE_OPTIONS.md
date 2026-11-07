# Venue options and construction in Rust (RUST_QUALITY item 6)

Today the venue registry lives only in the wasm host (`WasmEngine::new`, with
inline WEEX/Kraken/Bitget/OKX checks), and `rust.exchange.ts` repeats ~130
lines of per-venue credential validation and builds a per-venue options blob.

## PR 1: `safe-cex-venues`

- New crate `sdk-rs/registry` (`safe-cex-venues`, one cargo feature per venue):
  `EngineOptions` (moved from the wasm host), a `from_options` per venue
  returning `Result<V, VenueError>` with every construction rule the host
  checks today, and `build(venue, &EngineOptions, visitor)`, which hands the
  concrete `Engine<V>` to a host-supplied visitor (no boxing in the crate).
- The wasm host delegates to it; the native host gains
  `FrameServer::add_venue(name, options_json, transport)` behind a `venues`
  feature. Construction errors are `VenueError` (`InvalidInput`).
- Rust tests for every construction rule, including the ones without tests
  today (OKX/Bitget passphrases, unsupported venue, Bybit endpoints).

## PRs 2-4: TS validation into Rust, about five venues each

Each rule in `createHost` moves into the registry (a rule function per venue,
run before that venue's construction) with the TS message unchanged and a Rust
regression test; the TS check and its imports go.
Batches: Polymarket, Orderly family, BreakoutProp, Paradex, AsterDEX; then the
generic "private accounts require key and secret" rule; then the options blob
becomes uniform (no venue-name spreads), with venue-specific derivations
(Lighter key index, AsterDEX auth context, BloFin attribution) in Rust.

## Polling

#1392 routes the resolved polling policy to every venue at runtime
(`setPollScale`). After it lands, `tick_interval`/`polling_enabled` leave
`EngineOptions` and BreakoutProp/Orderly read only the runtime policy.
