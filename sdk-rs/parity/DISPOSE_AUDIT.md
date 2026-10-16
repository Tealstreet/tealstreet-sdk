# Venue disposal source audit

Audited master `5e76c883d1` (2026-10-16), plus the indicated open binding heads.
Scope: invented BaseExchange/BaseWebSocket disposal calls in venue bindings,
including native adapters and the supplied-host body each wrapper actually invokes.
This is a super-call and wrapper-order audit, not a claim that every observable
cleanup detail or every unbound helper is fully equivalent.

The source census contains 31 exchange and 36 websocket disposal overrides across
27 Rust venue crates. Ten overrides omit `super.dispose()`: Backpack exchange and
both sockets, Crypto.com exchange and both sockets, GRVT exchange, Hyperliquid
private socket, Orderly exchange and Paradex exchange. Weex inherits the base
exchange member; txflow is a helper crate. Source bodies were inspected through
TypeScript class/property-arrow AST nodes; comments are not super-call evidence.

## Confirmed mismatches and ownership

| Surface | Source | Concrete mismatch / result | Owner |
| --- | --- | --- | --- |
| Backpack exchange | `backpack.exchange.ts:97` | Extra task cancellation and superclass disposal removed in #227; bound actual-source witness checks flag and socket order. | codex-l89ogr, repaired |
| Backpack private socket | `backpack.ws-private.ts:520` | Invented state/task/base cleanup removed in #227; source subscriptions/timeout cleanup and transport close retained. | codex-l89ogr, repaired |
| Backpack public socket | `backpack.ws-public.ts:755` | Invented handler/topic/task/base cleanup removed in #227; source local cleanup and transport close retained. | codex-l89ogr, repaired |
| Crypto.com exchange | `cryptocom.exchange.ts:80` | Extra wrapper base cleanup removed by merged #186; 22 current binding tests pass, including four old read-only-flag failures. | codex-f74ybe, repaired |
| Orderly exchange | `orderly.exchange.ts:129` | Extra base cleanup and private shadow flag removed on #179; source-writable shared flag + constructed disposal regression proof. | codex-ydwdec, repaired on open PR |

Backpack exchange's old `chosen-base` review and the two `socket-receiver` reviews
explain why the existing syntactic gate accepts these source mismatches. A receiver
type alone proves neither that a source override calls super nor the cleanup order.
The three obsolete Backpack delegation reviews are removed with #227's production
repair. Its owner reconciled the disposal manifest rows and supplied three bound
actual-source tests plus native/relay state-retention tests for the narrow close
primitive. Neither a receiver classification nor a source citation alone proves
all cleanup behavior.

Two further transport-close bridges need separate runtime repairs:
`safe-cex-exchange/src/js_runtime/sockets.rs` installs `ws.close` using
`BaseWebSocket::force_close`, and Hyperliquid's `realm/facade_socket.rs` does the
same. Source `base.ws.ts:1194-1202` distinguishes `forceClose` metadata from
`ws.close`; Crypto.com's two own socket disposals and Hyperliquid's private
disposal call the latter without super. They need #227's
`close_current_websocket` primitive and native/source forceClosed-retention
witnesses. The shared bridge repair is codex-nlry7m's #240, with 16 source/native
state-retention cases; the Hyperliquid bridge is assigned to codex-nkxoxx. These
routed runtime repairs remain separate from this classification gate's proof.

## Complete venue pass

Paths below are relative to each `sdk-rs/safe-cex-venue-<venue>/src/` directory.

| Venue | Rust disposal path inspected | Super-call / binding result |
| --- | --- | --- |
| apexomni | `exchange.rs; ws_private.rs` | Exchange base then sockets; private throttle cleanup then super. Binding197 remains venue-owned. |
| asterdex | `native_exchange.rs; exchange_runtime.rs; binding/sockets.rs` | Virtual clock/local socket cleanup then base; socket protocols call super in source order. |
| backpack | `adapter.rs; private_adapter.rs; public_adapter.rs` | CORRECTED227: source-only cleanup; direct transport close preserves superclass metadata and owned tasks. |
| binance | `exchange_classes.rs; private_classes.rs; public_classes.rs` | USD-M/PM/Spot exchange wrappers delegate once to source-mapped lifecycle; socket overrides retain super and inherited socket paths. |
| bingx | `binding/exchange.rs; exchange.rs; listen_key.rs` | Exchange base then sockets; private listen-key timeout cleanup then super. |
| bitget | `binding/exchange.rs; stream_lifecycle.rs; binding/hosts/{private,assembly}.rs` | Exchange source body owns super; secondary ticker sockets inherit BaseWebSocket.dispose. |
| bitrue | `binding/exchange.rs; runtime_lifecycle.rs; binding/sockets.rs` | Fast-refresh timer/listener cleanup then base and two sockets; private timer then super. |
| bitunix | `adapter_exchange.rs; adapter.rs; sockets.rs; exchange_lifecycle.rs` | Exchange super then all five socket groups; public override calls super; other sockets inherit. |
| blofin | `native_exchange.rs; exchange.rs; native_host.rs` | Exchange base then sockets; both websocket overrides retain super. |
| breakoutprop | `lifecycle.rs` | Exchange base then public socket; public socket inherits disposal. |
| bybit | `binding/exchange.rs; stream_delegation.rs` | Primary cancels source debounce/reconciliation timers before base then sockets; inverse/spot body ports retain super. |
| coinbase | `binding/exchange.rs; binding/lifecycle.rs; exchange_lifecycle.rs` | Source-mapped base, timers and sockets; no wrapper pre-cleanup. |
| cryptocom | `binding/exchange.rs; class_admin.rs; class_socket.rs` | CORRECTED186: exchange sets shared flag then sockets; both socket bodies clear local state and close ws without super. |
| grvt | `snapshots.rs; public_lifecycle.rs; private_commands.rs` | Exchange helper sets flag then sockets; both socket overrides call super. Pending183 wrapper b552bb010d6b reviewed: source flag+two sockets, no base pre-cleanup. |
| hyperliquid | `binding.rs; lifecycle_delegates.rs; realm/facade_socket.rs` | Core exchange local state/sockets then super; Spot helper super then sockets. Private socket stops ping and closes ws without base disposal; public calls super. |
| kraken | `binding/exchange.rs; binding/host_typed.rs` | Source polling cleanup then base; Spot inherits exchange disposal. |
| kucoin | `kucoin_exchange.rs; binding/futures.rs; binding/globals.rs` | Futures/Spot wrappers call source dispose once; source owns super and socket order, including snapshot sockets. |
| lighter | `binding/exchange.rs; exchange_reconciliation.rs` | Source body owns base then local cache/sockets/signer; socket overrides call super. |
| mexc | `exchange.rs; exchange_binding.rs; exchange_runtime.rs` | Bound path calls source body, which cancels pending snapshot timers then super; unbound native base fallback is source-super eligible. |
| okx | `binding/exchange.rs; socket_boundaries.rs` | Wrapper calls source dispose, which owns base then public/private/business sockets; family subclasses inherit. |
| orderly | `runtime_actions.rs` | Helper sets flag then sockets. CORRECTED179 by ydwdec: concrete wrapper extra base cleanup and shadow flag removed; constructed disposal proof catches both (6a1e35be86, subsequent head92a5f389b9). No chosen-base exception. |
| paradex | `native_exchange.rs; binding/{members,sockets}.rs; lifecycle.rs; ws_{private,public}_lifecycle.rs` | Concrete binding calls the source exchange body: shared writable flag, public then private socket, no BaseExchange cleanup. Both socket source bodies clear their timers then call the existing JS BaseWebSocket prototype dispose with the socket receiver (TS public163–172/private476–485). No new typed-base bypass or unused delegation exception. Constructed disposal checks both closed sockets, shared flag and retained exchange listeners. |
| phemex | `session_control.rs; private_session.rs` | Supplied-host body ports call exchange/socket super in source order; no concrete Exchange binding on audited master. |
| polymarket | `binding/exchange.rs; binding/members.rs; disposal.rs` | Source exchange only calls super; wrapper adds no early base disposal. |
| txflow | `lib.rs` | Transport flow/helper crate; no corresponding TS exchange disposal override or concrete Exchange binding. |
| weex | `private_feed.rs; public_feed.rs` | Exchange inherits BaseExchange disposal (no own override); both socket body ports retain super. No concrete Exchange binding on audited master. |
| woo | `binding/exchange.rs; binding/hosts/fill_recovery.rs` | Source fill/timer cleanup then base; wrapper only calls source member; sockets inherit disposal. |

## Exact source references

Every own exchange/socket disposal override found under the live source tree is
listed below, including family variants and pending concrete bindings. Inherited
members must resolve to their defining base method rather than inventing an own
source super call. The shared bases are `base.ts:1343` and `base.ws.ts:1033`.

| Source file | Class | Line | Calls super.dispose |
| --- | --- | ---: | --- |
| `apexomni/apexomni.exchange.ts` | `ApexOmniExchange` | 163 | yes |
| `apexomni/apexomni.ws-private.ts` | `ApexOmniPrivateWebsocket` | 572 | yes |
| `asterdex/asterdex.exchange.ts` | `AsterDEXExchange` | 1644 | yes |
| `asterdex/asterdex.ws-private.ts` | `AsterDEXPrivateWebsocket` | 858 | yes |
| `asterdex/asterdex.ws-public.ts` | `AsterDEXPublicWebsocket` | 595 | yes |
| `backpack/backpack.exchange.ts` | `BackpackExchange` | 97 | no |
| `backpack/backpack.ws-private.ts` | `BackpackPrivateWebSocket` | 520 | no |
| `backpack/backpack.ws-public.ts` | `BackpackPublicWebSocket` | 755 | no |
| `binance/binance.exchange.ts` | `BinanceExchange` | 310 | yes |
| `binance/binance.ws-private.ts` | `BinancePrivateWebsocket` | 41 | yes |
| `binance/binance.ws-public.ts` | `BinancePublicWebsocket` | 497 | yes |
| `binance/binancepm.exchange.ts` | `BinancePMExchange` | 130 | yes |
| `binance/binancepm.ws-private.ts` | `BinancePMPrivateWebsocket` | 105 | yes |
| `binance/binancepm.ws-public.ts` | `BinancePMPublicWebsocket` | 388 | yes |
| `binance/binancespot.exchange.ts` | `BinanceSpotExchange` | 76 | yes |
| `bingx/bingx.exchange.ts` | `BingxExchange` | 146 | yes |
| `bingx/bingx.ws-private.ts` | `BingxPrivateWebsocket` | 431 | yes |
| `bitget/bitget.exchange.ts` | `BitgetExchange` | 338 | yes |
| `bitrue/bitrue.exchange.ts` | `BitrueExchange` | 269 | yes |
| `bitrue/bitrue.ws-private.ts` | `BitruePrivateWebsocket` | 309 | yes |
| `bitunix/bitunix.exchange.ts` | `BitunixExchange` | 394 | yes |
| `bitunix/bitunix.ws-public.ts` | `BitunixPublicWebsocket` | 227 | yes |
| `blofin/blofin.exchange.ts` | `BlofinExchange` | 161 | yes |
| `blofin/blofin.ws-private.ts` | `BlofinPrivateWebsocket` | 33 | yes |
| `blofin/blofin.ws-public.ts` | `BlofinPublicWebsocket` | 732 | yes |
| `breakoutprop/breakoutprop.exchange.ts` | `BreakoutpropExchange` | 103 | yes |
| `bybit/bybit.exchange.ts` | `BybitExchange` | 610 | yes |
| `bybit/bybitinverse.exchange.ts` | `BybitInverseExchange` | 260 | yes |
| `bybit/bybitinverse.ws-public.ts` | `BybitInversePublicWebsocket` | 144 | yes |
| `bybit/bybitspot.exchange.ts` | `BybitSpotExchange` | 124 | yes |
| `bybit/bybitspot.ws-public.ts` | `BybitPublicWebsocket` | 144 | yes |
| `bybit/hyrotrader.ws-private.ts` | `HyroPrivateWebsocket` | 161 | yes |
| `coinbase/coinbase.exchange.ts` | `CoinbaseExchange` | 128 | yes |
| `cryptocom/cryptocom.exchange.ts` | `CryptocomExchange` | 80 | no |
| `cryptocom/cryptocom.ws-private.ts` | `CryptocomPrivateWebSocket` | 392 | no |
| `cryptocom/cryptocom.ws-public.ts` | `CryptocomPublicWebSocket` | 547 | no |
| `grvt/grvt.exchange.ts` | `GrvtExchange` | 197 | no |
| `grvt/grvt.ws-private.ts` | `GrvtPrivateWebSocket` | 923 | yes |
| `grvt/grvt.ws-public.ts` | `GrvtPublicWebSocket` | 663 | yes |
| `hyperliquid/hyperliquid.exchange.ts` | `HyperliquidExchange` | 1428 | yes |
| `hyperliquid/hyperliquid.ws-private.ts` | `HyperliquidPrivateWebSocket` | 717 | no |
| `hyperliquid/hyperliquid.ws-public.ts` | `HyperliquidPublicWebSocket` | 636 | yes |
| `hyperliquid/hyperliquidspot.exchange.ts` | `HyperliquidSpotExchange` | 209 | yes |
| `kraken/kraken.exchange.ts` | `KrakenExchange` | 132 | yes |
| `kucoin/kucoin.exchange.ts` | `KucoinExchange` | 178 | yes |
| `kucoin/kucoin.ws-private.ts` | `KucoinPrivateWebsocket` | 135 | yes |
| `kucoin/kucoinspot.exchange.ts` | `KucoinSpotExchange` | 83 | yes |
| `kucoin/kucoinspot.ws-private.ts` | `KucoinSpotPrivateWebsocket` | 190 | yes |
| `lighter/lighter.exchange.ts` | `LighterExchange` | 867 | yes |
| `lighter/lighter.ws-private.ts` | `LighterPrivateWebsocket` | 1324 | yes |
| `lighter/lighter.ws-public.ts` | `LighterPublicWebsocket` | 1445 | yes |
| `mexc/mexc.exchange.ts` | `MexcExchange` | 1580 | yes |
| `mexc/mexc.ws-private.ts` | `MexcPrivateWebsocket` | 124 | yes |
| `mexc/mexc.ws-public.ts` | `MexcPublicWebsocket` | 166 | yes |
| `okx/okx.exchange.ts` | `OKXExchange` | 249 | yes |
| `orderly/orderly.exchange.ts` | `OrderlyExchange` | 129 | no |
| `paradex/paradex.exchange.ts` | `ParadexExchange` | 262 | no |
| `paradex/paradex.ws-private.ts` | `ParadexPrivateWebSocket` | 476 | yes |
| `paradex/paradex.ws-public.ts` | `ParadexPublicWebSocket` | 163 | yes |
| `phemex/phemex.exchange.ts` | `PhemexExchange` | 88 | yes |
| `phemex/phemex.ws-private.ts` | `BinancePrivateWebsocket` | 23 | yes |
| `polymarket/polymarket.exchange.ts` | `PolymarketExchange` | 175 | yes |
| `polymarket/polymarket.ws-private.ts` | `PolymarketPrivateWebSocket` | 190 | yes |
| `polymarket/polymarket.ws-public.ts` | `PolymarketPublicWebSocket` | 162 | yes |
| `weex/weex.ws-private.ts` | `WeexPrivateWebsocket` | 114 | yes |
| `weex/weex.ws-public.ts` | `WeexPublicWebsocket` | 164 | yes |
| `woo/woo.exchange.ts` | `WOOXExchange` | 288 | yes |

## Hardened classification gate

`safe-cex-exchange/scripts/dispose-source.cjs` resolves every reviewed disposal
edge to the named TypeScript class's actual dispose definition. Each JSON citation
records its definition file/class, start/end lines, own/inherited status and AST
super-call presence. Method/property-arrow bodies and inherited socket methods are
handled explicitly. Comments and other classes' super calls are not evidence.

A source-super edge requires an own source super call. A socket-receiver edge must
have either its own source super call or an inherited BaseWebSocket definition.
The shared BaseExchange facade bridge cites BaseExchange.dispose itself with the
separate base-implementation classification. An own no-super override cannot be
accepted through chosen-base or socket-receiver labels. The three obsolete
Backpack rows are removed after #227; negative controls continue to reject their
former classifications against the actual no-super source bodies.

The Rust override_contract target runs the live validator and its Node controls.
The controls reject missing/stale/mismatched source claims, false superclass
assumptions, cross-venue evidence and wrong base-bridge evidence. Disabling the
source-super policy itself must make those controls fail; restoring it passes.
The shared JsProtocol::dispose dispatch/reentrancy repair is separately owned by
codex-nlry7m in #211 (`01d7db4123`), with direct/typed/reentrant source witnesses;
this audit does not edit that hook or claim to prove it.
