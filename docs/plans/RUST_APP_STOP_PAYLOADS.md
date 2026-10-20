# App stop payload audit and future implementation plan

Status: **audit only; implementation and regression gates are pending**. This
belongs to the third follow-up PR, after broker/cancellation and public-startup
work. Do not interpret a source-path assessment below as tested native parity.

The user explicitly authorized read-only inspection of
`~/cs/tealstreet-next/apps/web/src/order/`. Its nearest `CLAUDE.md` was read.
No upstream app files were edited. Source-of-truth files are the app builders,
current TS adapters (or their deterministic reference), and Rust formatters.
Orderly implementation remains owned by `feat/rust-order-races`; other
Binance/Bybit audit items remain owned by `feat/rust-audit-binance-bybit`.

## Exact app payloads

`createStopLossOrder.ts:314` emits the following for stop-form/chart/hotkey
protection and post-chase legs (`attachChaseTPSL.ts:113`):

```ts
{
  symbol,
  side,
  type: StopLoss | TakeProfit | options.type,
  notional: amount * price,
  timeInForce: GoodTillCancel,
  ...selectedTpSlTriggerTypes,
  ...lighterSlippageFields,
  ...options,
  reduceOnly: true,
  price: roundedPrice,
  triggerPrice: roundedPrice,
  amount: roundedNativeAmount,
  ...(close !== undefined ? { close } : {})
}
```

`price` and `triggerPrice` deliberately match and override supplied options.
SL/TP type comes from the chosen reference price, independently of this
redundancy. The builder mints no caller ID. Explicit raw amount zero means Full;
a positive quantity rounded to zero is refused. Full requires metadata support.
`close` is supplied only when the venue advertises that attribute. Lighter alone
gets its normalized `maxSlippagePct` (default1%, cap50%).

| Surface | Exact relevant shape |
| --- | --- |
| Chart amend | `updateOrder.ts:244`: `{order:{...storedOrderWithoutClientOrderId,amount,remaining,...lighterSlippage},update:{price?,amount?}}`. New fields round to nearest. Stop amount, including explicit zero, is preserved. Rust receives ID plus changes and uses its current store for provenance. |
| Bracket entry | `createLimitOrder.ts:169` / `createMarketOrder.ts:174`: regular parent with `stopLoss?`, `takeProfit?`, selected leg references and default GTC. No redundant generic trigger on the parent. |
| Lighter scaled follow-up | `createScaledOrder.ts:609,625`: SL/TP type, `price`, native `amount`, IOC, leg reference, slippage and reduction flag; no generic `triggerPrice`. |
| Opening conditional | `createConditionalOrder.ts:153`: Conditional type, `triggerPrice`, optional execution `price`, positive native amount, GTC, selected `triggerType`, reduction checkbox. This is a different shape from protective SL/TP. |
| Trailing | The stop primitive can receive `options.type=TrailingStopLoss`, retaining matching price/triggerPrice. Expected app support is limited to venues advertising trailing; an old formatter branch alone is not native capability evidence. |

## Venue matrix

Rust source references below are under `sdk-rs/venues/<venue>/src/`.

| Registered names | Protective app shape / required action |
| --- | --- |
| `bitget`, `bitgetusdc`, `bitgetinverse` | **Confirmed source regression**: `trading.rs:266` rejects all generic triggers before existing regular/UTA protective formatting. TS algo and UTA strategy formatters use `opts.price`. Accept faithful redundant protective trigger; keep unsupported opening conditional rejection. |
| `bingx` | **Confirmed source regression**: `trading.rs:587` rejects all generic triggers. TS protective/trailing formatters use `opts.price`. SL, TP, Full and advertised trailing app submissions are affected. |
| `bitrue` | **Confirmed source regression**: `trading.rs:323` rejects generic trigger before existing protective mapping from price to native triggerPrice. Requires reconciled position. Infinity remains unsupported. |
| `kraken` | **Confirmed source regression**: `trading.rs:267` rejects generic trigger before protective stopPrice formatting. Includes advertised trailing app submissions. Owner decision: market entry with attached TP/SL remains rejected; its error must explicitly say TP/SL legs are unsupported on market entry. Pin that message and zero-write behavior. |
| `krakenspot` | Pending native/Wasm payload coverage. Current Market entry path silently drops attached TP/SL; apply the owner decision to reject those legs explicitly before a write, matching futures. Do not infer bracket support from standalone native stop kinds. |
| `phemex` | **Confirmed source regression**: `trading.rs:223` rejects generic trigger before supported SL/TP stopPxRp mapping. Full zero is supported. Trailing code exists but UI metadata disables it. |
| `binancespot` | **Caller-shape mismatch**: `trading.rs:863` refuses mandatory app reduceOnly=true. TS intentionally omits that unsupported native field while producing valid explicit-size STOP_LOSS/TAKE_PROFIT. Investigate narrow protective normalization, preserving refusal of genuinely unrepresentable close controls; coordinate Binance lane. |
| `binance`, `binancepm`, `binanceinverse` | Existing SL/TP formatter accepts redundancy. Preserve regular/algo routing, quantity units and close-position semantics. Advertised trailing paths exist. |
| `bybit`, `bybitinverse`, `bybitdemo`, `hyrotrader` | Existing trigger resolver accepts triggerPrice.or(price). Full/trailing may acknowledge without IDs; preserve later order reconciliation and accepted-empty-ID contract. |
| `bybitspot` | Rust rejects reduceOnly=true; TS itself sends that unsupported spot field. Classify as TS/native capability mismatch needing explicit normalization decision, not proof Rust should transmit it. Spot trailing position stops remain unsupported. |
| `okx`, `okxeu`, `okxdemo`, `okxeudemo` | Existing protective formatter accepts app shape, uses price. Preserve Full and normal/algo provenance. |
| `blofin` | Existing TPSL formatter accepts redundancy and explicit-zero Full. Opening conditional restrictions are separate. |
| `backpack` | Existing protective formatter accepts duplicate trigger, reference and positive explicit amount. Full/trailing unsupported. |
| `cryptocom` | Existing advanced formatter accepts triggerPrice.or(price), explicit-size SL/TP. Full/trailing unsupported. |
| `kucoin` | Existing stop formatter accepts redundancy. Preserve native lots/multiplier conversion and Full-position behavior. |
| `kucoinspot` | **Genuinely unsupported**: TS explicitly rejects non-Limit/Market and attached protection. Retain no-write rejection. |
| `weex` | Pending exact app payload/native request coverage for SL/TP, supported Full and reference/size conversion; preserve existing protective provenance and broker tag. |
| `paradex` | Pending exact app payload/native request coverage for native protective types, sizing and authenticated builder attribution. |
| `mexc` | Existing protective formatter accepts trigger/price and native size. Full resolves current closing-side position. Trailing unsupported. |
| `woo` | Existing algo paths accept app shape including Full and advertised trailing. Preserve sibling protection and position-sized trailing. |
| `tealstreet-dex`, `woofi`, `kodiakfi`, `mode`, `salsadex`, `what` | Orderly fixes belong to `feat/rust-order-races`; do not duplicate. Include its eventual change in end-to-end matrix. |
| `hyperliquid` | Existing trigger mapping accepts shape using price; retain explicit/full grouping. Trailing unsupported. |
| `hyperliquidspot` | **Genuinely unsupported**: TS SL/TP branch ends with `throw new Error('Not supported')`. Keep Rust rejection. |
| `grvt` | Existing EIP-712/protective formatter accepts redundancy. Requires positive explicit size and authorized builder. Full/trailing unsupported. |
| `asterdex` | Existing STOP_MARKET/TAKE_PROFIT_MARKET accepts redundancy; zero sets closePosition. Trailing unsupported. |
| `apexomni` | Existing protective formatter accepts duplicate trigger. Explicit-size only; preserve last/index references. |
| `breakoutprop` | Existing preparation accepts generic trigger, emits STOP with CLOSE effect. Explicit zero CLOSE is represented. Trailing unsupported. |
| `coinbase`, `coinbasespot` | StopLoss accepts matching price/triggerPrice. TP/attached brackets unsupported: TS TP fallthrough builds an ordinary limit and must not be copied as support. |
| `lighter`, `lighter-robinhood` | Existing protective mapper accepts redundancy and normalizes market-style protection to IOC, matching TS. Preserve slippage-derived execution price and positive amount. Metadata disables trailing; the current Rust TrailingStopLoss fallthrough becomes an ordinary Market order. Replace that with explicit unsupported/no-write rejection; TS mapping it to Market is not protection evidence. |
| `polymarket` | **Genuinely unsupported**: prediction-market limit placement has no protective/bracket/trailing contract. Keep fail-closed. |
| `txflow` | Protective/conditional/reduction semantics intentionally unmeasured and disabled. Keep explicit rejection. |

`bitunix` is TS-only, not a missing Rust fix.

Adjacent broken caller to decide in this phase: Coinbase Rust rejects
Market+GTC (`trading.rs:260`) whereas `createMarketOrder.ts:171` defaults GTC
and TS explicitly creates market_market_ioc. Normalize only when a faithful
native market mapping is intended; do not turn unsupported TP into limit.

## Regression and implementation phases (pending)

1. Add native fake-transport regressions for literal app-shaped SL/TP in the
   five confirmed families. Cover both sides/references, positive size, supported
   Full-zero, correct native endpoint and existing broker attribution. Include
   BingX/Kraken advertised trailing payloads.
2. Implement narrow protective redundancy handling: accept only values the
   native shape can faithfully represent; keep conflicting trigger/execution
   values and unexpected triggers on ordinary orders fail-closed. No global
   removal of triggerPrice at the SDK bridge.
3. Decide spot reduction and Coinbase market-GTC normalization from explicit
   SDK/native contracts. Do not copy malformed TS fallthroughs or add unmeasured
   venue capabilities merely to make a matrix green.
4. Test mixed regular+invalid-protective batches: no partial placement before
   preflight refusal. Caller-ID preflight remains first and verbatim.
5. Run actual Wasm SDK boundary regressions using the literal app payload, then
   amend the observed store row with `{price}`. Pin correct native family,
   sibling leg retention, no client-ID reuse and visible failures.
6. Unsupported/spot cases assert explicit errors and zero writes. Complete
   per-phase correctness review, gates, PR review and green CI before landing.
