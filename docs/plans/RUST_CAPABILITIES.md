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


## Aster API-version STP correction (2026-10-23)

The positive-dispatch follow-on found an original-source contradiction: Aster's
capability record advertises `stpControls=true` for V1, but both native STP
commands refuse V1 before I/O with “Aster self-trade prevention requires V3”.
Owner decision: correct that advertised bit in a separate small PR, preserving
the strict positive conformance matrix. Base: actual SafeIDs #1789
`7641650e7b1fc54c55593880ac16299171f60a90`.

Scope: `venues/asterdex/src/lib.rs` changes only `stp_controls` to
`self.version == Version::V3`; existing trading tests cover explicit V1 and V3
records and native STP dispatch/refusal. The resolved version is already stored
on AsterDex, so there is no new option, plumbing, handler or signing behavior.
Add a CALLERS section explaining that V1 capability consumers stop offering STP
controls; V3 remains advertised. No account/reporting/history code changes.

TS comparison: `BaseExchange.CAPABILITY_METHODS` / `ExchangeCapabilities` has no
STP capability field. Aster TS exposes fetchStpMode/changeStpMode as methods for
both profiles, but both call assertV3Feature before I/O (asterdex.exchange.ts
710-739). RustEngineExchange also explicitly checks its Aster V3 profile before
these commands. Therefore the native advertised bit is inconsistent today; this
audit does not claim that the separate NX UI currently renders an STP control.
No TS production change is included.

Original RED: an unchanged-source native host-graph test constructs valid V1
HMAC credentials, reads stpControls=true, decodes setStpMode(EXPIRE_TAKER), and
receives CommandError plus InvalidInput CommandFailed, with no native request.
Before changing production, add independent explicit V1 false / V3 true record
assertions; run to capture V1 RED and V3 compatibility GREEN. Then apply the
one-line correction, run Aster and engine tests/strict checks plus existing
conformance controls, and obtain independent source/caller review. Full CI and
source-bound proof remain subject to the owner's landing windows.
