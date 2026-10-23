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


Owner broadened the same small correction to all confirmed profile mismatches:

- Bybit: `income_history = !self.variant.is_spot() && !self.variant.is_hyro()`.
  Both variants reject Income unconditionally at history.rs:424; all other
  variants retain true. uufub4 explicitly cleared the capabilities block and
  capability assertions against the local managed Bybit child before editing.
  No history, on_command, send/resend or Context source changes.
- Coinbase: `income_history = false` for every profile. history_start rejects
  Income before credentials/product selection, not only on Spot. The owner
  explicitly accepted Spot/INTX/CFM false; native CFM PnL remains separate and
  accepted. Existing spot_only/options/account state can be read by tests; no
  new product plumbing is needed to make an unconditional unsupported bit false.
- Aster remains the version predicate above. CALLERS also names per-order STP
  alongside fetch/change STP, since all three are V3-only.

Tests use existing native integration test files (Aster trading, Bybit account_variants,
Coinbase history_family support tests) and prove refusing profiles false plus supported peers true.
Each venue's actual command refusal remains asserted independently of its bit;
V3 STP continues to dispatch exact GET/POST routes. Tests are committed and run
on unchanged production before the three capability-field edits.

TS comparison for income: BybitSpot inherits BaseExchange's branded inert
fetchIncomeHistory/fetchIncomeHistoryRange; Hyro explicitly brands both empty
fallbacks. Coinbase also inherits the branded base fallbacks (there are no
income overrides in its TS exchange classes). BaseExchange.capabilities selects
these method brands, so all report incomeHistory/incomeRange=false today. No TS
production change or new endpoint is authorized. No other capability fields
change, and the separate positive conformance branch keeps its gate strict.


Expanded plan independent review (claude-8x5tm1): CLEAN at41b280ab36a.
Credentialed Bybit controls are mandatory because the keyless legacy short-circuit
returns [] before its native variant refusal; tests use the existing authenticated
fixture. Coinbase controls select each private ProductKind (INTX, CFM, Spot) in
the existing crate-local history-family tests, without adding a production API.


## Unsupported Polymarket history claims (2026-10-23)

The positive valid-state dispatch inventory found a confirmed capability defect:
Polymarket inherits `fill_history`, `order_history` and `income_history = true`
from `Capabilities::BASE`, while its Query handlers reject all three axes
unconditionally before reading credentials or state. The existing
`unsupported_histories_fail_explicitly_without_network_or_empty_success` test
already pins those refusals. This is separate from fixture/bootstrap gaps.

Plan, from actual SafeIDs landing `7641650e7b1`:

1. Add an original-source assertion for each history bit on both public-only
   and credentialed Polymarket construction. Keep each native unsupported
   refusal and zero-HTTP outcome independently asserted, using explicit query
   fixtures. All six capability assertions must fail before production changes.
   Pin supported peers (account lookup, order-book query, native cancel-all).
2. Override exactly the three history bits to false in Polymarket `lib.rs`.
   Keep handlers, startup, account and trading state, request recipes, signing,
   row shapes, paging and legacy inventory byte-identical.
3. Update the existing host cfg(test) name-list compatibility fixture by adding
   Polymarket to its no-history set. This is only an expected-record correction;
   add no runtime host code or capability exemption.
4. Add a CALLERS section describing all three false history axes and the SDK's
   branded empty fallbacks. Callers must gate on capabilities and treat empty
   fallback rows as unavailable coverage. Native direct commands still reject.
5. Run original RED controls, corrected family tests, host lib tests, strict
   Clippy/fmt/bans/request-context checks and branch-history secret scan. Obtain
   independent source/caller review, then open a draft. Final CI/proof follows
   actual queued shared inputs; no landing action during another lane's window.

TS comparison: `PolymarketExchange` does not override `fetchHistoricalFills`,
`fetchHistoricalOrders`, `fetchIncomeHistory` or `fetchIncomeHistoryRange`.
It inherits the branded empty BaseExchange fallbacks (`base.ts` capability
mapping at 288 and methods at 1214/1296/1303/1328), so TS already reports these
axes unsupported. Rust SDK adoption in `rust.exchange.ts:572-579` installs those
same branded empty methods when the engine reports false. No TS edit is needed.

Owner decision: Polymarket's future history implementation remains a separate
slice after the account reporter work. Any future PR implementing these axes
must enable the corresponding capability bits in the same PR that provides
valid-state dispatch; this correction does not claim fetched history support.
Confirmed later capability mismatches may join this small correction PR after
plan/reproduction/review; fixture gaps and handler repairs remain out of scope.

Ownership: mxb4mq explicitly cleared `lib.rs` and history tests against #1786,
which owns account.rs/account controls. Preserve actual #1786 before final proof
if it lands first. Shared plan sections use the standing keep-both resolution.
