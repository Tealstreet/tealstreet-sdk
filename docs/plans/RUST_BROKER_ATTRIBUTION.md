# Rust broker attribution audit

Audit scope: every name registered by `RUST_VENUES` in
`packages/safe-cex/src/exchanges/rust/rust.exchange.ts`. Source paths below are
relative to `packages/safe-cex/src/`; Rust paths are relative to
`sdk-rs/venues/`. Aliases listed together use the same request formatter.
`reference/` means the deterministic TS oracle in `__tests__/reference/`, not
removed exchange implementation restored to the production registry.

Caller client IDs remain verbatim. Broker prefixes only apply to generated
IDs. A missing caller-ID capability remains a preflight rejection; attribution
must never make a previously accepted ID unfindable. No network or credentialed
orders were used for this audit.

| Venue names | Attribution sent by TS; gate | Rust path and audit result |
| --- | --- | --- |
| `binance`, `binancepm`, `binanceinverse`, `binancespot` | Generated `newClientOrderId` / USD-M `clientAlgoId` start `x-s3vFFRQp`; no production/proxy gate. `exchanges/binance/*.exchange.ts`. PM trailing formatter uses a plain generated UUID; Rust retains its attributed generated ID. | `binance/src/trading.rs`: existing prefix preserved; caller IDs bare. PM trailing difference does not lose attribution or change caller IDs. |
| `okx`, `okxeu`, `okxdemo`, `okxeudemo` | Normal `clOrdId`, algo `algoClOrdId` generated as `e847386590ce4dBC` + UUID, truncated 32; `tag=e847386590ce4dBC` on orders; unconditional. `exchanges/okx/okx.exchange.ts` (`formatNormalOrder`, algo formatters). | `okx/src/trading.rs`: existing `tag` and generated prefixes on normal/algo/protection retained across regions/demo; caller IDs bare. |
| `blofin` | `brokerId=b3cdedc0a20880ba` only when `NODE_ENV=production` **or** `tealstreetProxy`; generated `clientOrderId` always starts broker value and truncates 32. Normal, TPSL and opening-trigger formatters plus replacements. `exchanges/blofin/blofin.exchange.ts:2028` and following formatters. | **Fixed** `blofin/src/trading.rs`: common `Placement` now serializes gated `brokerId` for all three order families and replacements. TS bridge computes `brokerAttribution`; Wasm forwards to `Options.broker_attribution` (native callers set the same bool). False is direct non-production default, independent of testnet. Generated prefixes remain unconditional; caller IDs bare. |
| `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader` | Factory header `X-Referer=Tealstreet` on all REST, including public; unconditional. `exchanges/bybit/bybit.api.ts`. Hyro inherits this factory. | `bybit/src/sign.rs` already private; **fixed** `bybit/src/lib.rs` public requests now retain same attribution. All variants share helper. |
| `bitget`, `bitgetusdc`, `bitgetinverse` | Factory header `X-CHANNEL-API-CODE=Tealstreet`, all REST; unconditional. `exchanges/bitget/bitget.api.ts`. | `bitget/src/sign.rs` already private; **fixed** public helper in `bitget/src/lib.rs` for all products/testnet. |
| `kucoin`, `kucoinspot` | Factory `X-CHANNEL-API-CODE=3zexp` on all REST. Optional nonblank trimmed `extra.kucoinBroker` (`partner`, `brokerKey`, `brokerName`) adds `KC-API-PARTNER`, `KC-API-PARTNER-SIGN` (HMAC-SHA256/base64 of timestamp+partner+API key), `KC-BROKER-NAME`, `KC-API-PARTNER-VERIFY` (default true) to authenticated REST. Futures authenticates public catalogue/ticker/bullet reads as well; Spot explicitly exempts its public endpoints and keeps those requests unsigned/channel-only even with configured credentials and partner. Public-data-only factories remain unsigned/channel-only. No production/proxy gate. `exchanges/kucoin/{kucoin.api,kucoinspot.api,kucoin.broker}.ts`. | `kucoin/src/sign.rs` already signs optional partner config with the actual request timestamp; **fixed** public futures catalogue/ticker/bullet helpers to preserve channel code and configured partner signatures with the primary identity. Spot public catalogue/ticker/bullet/candle reads preserve the channel code without API or partner signatures; private requests retain configured signed attribution. |
| `backpack` | Factory `X-Broker-Id=1200`, all REST; unconditional. `exchanges/backpack/backpack.api.ts`. | `backpack/src/sign.rs` already private; **fixed** public helper in `backpack/src/lib.rs`. |
| `bingx` | Factory `X-SOURCE-KEY=Tealstreet`; generated client ID `ts`+lowercase UUID, max40; unconditional. `exchanges/bingx/{bingx.api,bingx.exchange}.ts`. | `bingx/src/sign.rs` adds source header for public/private requests; `trading.rs` generated prefix preserved and caller IDs bare. Existing parity. |
| `bitrue` | Factory `X-Referer=Tealstreet`, unconditional. `exchanges/bitrue/bitrue.api.ts`. | `bitrue/src/lib.rs` unified request helper already carries header for public/private. |
| `phemex` | Generated `clOrdID=Tealstreet_`+UUID on normal/protective/trailing; unconditional. `exchanges/phemex/phemex.exchange.ts`. | `phemex/src/trading.rs` common placement formatter retains prefix; caller IDs bare. |
| `apexomni` | Order payload `brokerId=6587`, unconditional. `exchanges/apexomni/apexomni.exchange.ts`. | `apexomni/src/trading.rs` form payload already includes `brokerId=6587`. |
| `paradex` | `PARADEX-PARTNER=TEALSTREET` on POST path including `/orders` (normal/batch placement, not PUT amend); generated `client_id=ts_`+UUID. `exchanges/paradex/{paradex.api,paradex.exchange}.ts`. No production/proxy gate. | `paradex/src/account.rs` header on POST `/orders*`; `trading.rs` generated prefix/caller IDs preserved. |
| `woo` | POST normal `broker_id`; algo `brokerId`: production `ab82cb09-cfec-4473-80a3-b740779d0644`, testnet `e708a644-9ce5-46c6-b50c-74b15f62d8ca`. `exchanges/woo/woo.api.ts`. | `woo/src/trading.rs` normal/algo wire shapes already select same environment value, independent of proxy/production build. |
| `tealstreet-dex`, `woofi` | Normal `order_tag=TEALSTREET`, and normal `broker_id` / positional `brokerId` using same prod/testnet UUIDs as WOO. `reference/orderly/orderly.exchange.ts`, `orderly.api.ts`. | `orderly/src/{lib,trading}.rs` existing matching tag/environment broker. |
| `kodiakfi` | `order_tag=TEALKODIAK`; same Orderly normal/positional broker UUID policy. `reference/orderly/orderly.exchange.ts`. | `orderly/src/{lib,trading}.rs` existing parity. |
| `mode` | `order_tag=TEALMODE`; same Orderly normal/positional broker UUID policy. `reference/orderly/orderly.exchange.ts`. | `orderly/src/{lib,trading}.rs` existing parity. |
| `salsadex` | `order_tag=TEALSALSA`; same Orderly normal/positional broker UUID policy. `reference/orderly/orderly.exchange.ts`. | `orderly/src/{lib,trading}.rs` existing parity. |
| `what` | `order_tag=TEALWHAT`; same Orderly normal/positional broker UUID policy. `reference/orderly/orderly.exchange.ts`. | `orderly/src/{lib,trading}.rs` existing parity. |
| `kraken` | Futures sendorder form `broker=AA73 N84G 7YOF J26Y`, unconditional. `exchanges/kraken/kraken.exchange.ts`. | `kraken/src/trading.rs` existing form value. |
| `krakenspot` | No broker/referral/partner tag in TS spot placement/API factory. `exchanges/kraken/krakenspot.{api,exchange}.ts`. | `kraken/src/spot/` does not invent a futures broker field for spot. |
| `hyperliquid`, `hyperliquidspot` | `action.builder={b:lowercase address,f:feeTenthsBps}` only if matching `maxBuilderFee` approval covers fee; defaults mainnet `0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d`, testnet `0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba`, fee10. Account `hyperliquidBuilder` overrides both address+rate; no production/proxy gate. `exchanges/hyperliquid/{hyperliquid.constants,hyperliquid.exchange}.ts`. | `hyperliquid/src/{account,trading}.rs`: same defaults/override, lowercase and approval gate. Existing tests pin fee10/30/50 pair rather than independently mixing wallet/rate. |
| `grvt` | Signed and wire builder address mainnet `0xee8f3559f298fffa79b8253437969d15b5dccff3`, testnet `0xb0108ae4c07218ce320e5e22dd911a01c2042162`; REST builder fee `0.03`, signed uint32=300; refuse when builder authorization cap absent/unknown/below300. `exchanges/grvt/{grvt.builder,grvt.exchange}.ts`. | `grvt/src/{account,trading,sign}.rs`: existing same addresses, fee and fail-closed authorization. |
| `asterdex` | V3 builder+decimal `feeRate`: default mainnet `0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca`, maker/taker `0.0001`; overrides from extra. Zero-rate order or testnet placeholder omit builder; configured invalid address/fee rejects. Legacy V1 omits V3 builder. `exchanges/asterdex/asterdex.builder-fee.ts`. | `asterdex/src/trading.rs`: exact decimal maker/taker values and same zero/testnet/V1 gates; existing regression tests. |
| `lighter`, `lighter-robinhood` | Signed integrator attributes: mainnet default index158385, maker0/taker250; testnet placeholder and Robinhood empty default omit. Explicit nonnegative decimal-string index overrides; required fee approval/account tier before attributed orders. `exchanges/lighter/{lighter.builder-fee,lighter.exchange}.ts`. No production/proxy gate. | `lighter/src/{trading,sign}.rs`: existing same deployment defaults/override and approval/tier guards. Static/dynamic fee mode changes signer strategy, not emitted attribution tuple. |
| `polymarket` | V2 signed `builder=bytes32(0)` from official order builder (no configured builder code, referral tag, or builder L2 headers). `exchanges/polymarket/polymarket.signing.ts`. | `polymarket/src/{trading,sign}.rs`: signed zero builder retained; no invented nonzero code. |
| `weex` | Generated normal `newClientOrderId`, trigger/trailing `clientAlgoId` prefix `ts`; unconditional. `exchanges/weex/weex.exchange.ts`. Websocket User-Agent is Tealstreet, an identification header rather than broker fee attribution. | `weex/src/trading.rs` generated prefix/caller contract retained. Private WS identification retained; public WS generic User-Agent difference has no money/state/caller effect and is unchanged. |
| `coinbase`, `coinbasespot` | Generated `client_order_id=tealstreet-`+UUID; no broker code/header. `reference/coinbase/coinbase.exchange.ts`. | `coinbase/src/trading.rs` generated prefix retained; caller IDs bare. `/brokerage` is API namespace, not attribution. |
| `txflow` | Generated `cloid=ts_`+UUID; no broker code/header. `reference/txflow/txflow.trading.ts`. | `txflow/src/trading.rs` generated prefix/caller IDs retained. |
| `breakoutprop` | Generated entry/replacement `clientOrderId=tealstreet-`+UUID; no referral/header fee tag. `exchanges/breakoutprop/breakoutprop.exchange.ts`. | `breakoutprop/src/trading.rs` generated prefix retained; caller IDs refused by venue contract. |
| `cryptocom` | No broker/referral/partner/builder tag in placement or authenticated API factory. `exchanges/cryptocom/{cryptocom.api,cryptocom.exchange}.ts`. | `cryptocom/src/{sign,trading}.rs` no invented attribution fields. |
| `mexc` | No broker/referral/partner/builder tag in placement or authenticated API factory. `exchanges/mexc/{mexc.api,mexc.exchange}.ts`. | `mexc/src/{sign,trading}.rs` no invented attribution fields. |

`bitunix` remains TS-only (no `RUST_VENUES` entry/crate). Its API and placement
have no broker/referral/partner tag; this audit does not register a new venue.
Incoming `affiliateId`/referrer account metadata is not an outgoing attribution
tag and was inspected separately from order/header writes.

Regression evidence: BloFin native tests cover attributed/unattributed normal,
opening-trigger and TPSL payloads, bare caller ID and unconditional generated
prefix. The TS wrapper test covers all four production/proxy combinations; the
actual Wasm BloFin SDK boundary test covers proxy false/true payloads. Public
request-action regressions cover Bybit, Bitget, Backpack, KuCoin futures and spot.
Existing venue trading/signing tests pin the unchanged broker/builder paths.
