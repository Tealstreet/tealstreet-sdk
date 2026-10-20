# Engine-reported capabilities (RUST_QUALITY item 8)

The bridge in `rust.exchange.ts` decides most per-venue behaviour from name
lists. The engine already knows each venue's answers; it reports them and the
bridge routes from the record.

## PR 1: capabilities record

- `safe_cex_engine::Capabilities`, declared by `Venue::capabilities()` with a
  conservative default; replaces `supports_options` and
  `supports_amend_order_with_symbol`. The engine gates commands from it.
- The wasm host exports `capabilities()`; `EngineHost.capabilities()` parses it.
- `RustEngineExchange` reads it once the engine exists and replaces the name
  lists: account identity, client-order lookup, history axes, fills by order
  id, native cancel-all, leverage reads (on read, on metadata, maximum),
  account-wide leverage and margin type, symbol-routed cancel and amend,
  order-book queries, options. `RUST_VENUES` becomes a name list.
- Coinbase and Orderly answer `FetchAccount` natively, so every account read is
  one command.
- `BaseExchange.capabilities` answers the history axes for every venue, so
  tealstreet-next callers can stop reading brands (`collectAccountAxes.ts`,
  `ClosedPositions.tsx`, `symbolScopedFills.ts`). Brands keep working.
- Equality: a host test pins every venue's record to the former name lists.

## PR 2: market quantity units in the frame

- MARKETS flags carry each market's quantity convention (base, native
  placement, inverse native, contract count, inverse contract-sized), set from
  the venue. `FrameApplier` reads it per market; its positional booleans go.
- `contractsNative` / `remainingNative` keep their names and values; tests
  prove each former profile produces identical rows.
