# Binance table lookup inventory

TypeScript AST inventory (comments excluded): 72 uppercase-root table accesses, of
which twelve use closed testnet/livenet keys and 60 accept venue values. Five
lookups feed truthiness/`||` fallbacks; three further lookups feed `??` fallbacks.
Function-valued Object.prototype hits are non-null as well as truthy.

The historical-order mapper uses shared TableHit/ProtoMember for lookup and
nullish fallback decisions. Its MIXED divergence now concerns the closed typed
output: inherited callables produce a CHOSEN whole-record refusal, while the
__proto__ object retains a separate FINDING. TypeScript returns the record in
both cases; serialized forms do not converge. Other affected mapper rows remain
pending. No shared enum is widened and no venue-local prototype map is introduced.

Counts: BASE_URL=3, POSITION_SIDE=4, ORDER_TYPE=22, ORDER_SIDE=15, ORDER_TRIGGER_TYPE=16, ORDER_STATUS=1, BASE_FAPI_URL=1, ORDER_TYPE_SPOT=2, BASE_WS_URL=8.

| Source location | Lookup | Result control flow |
| --- | --- | --- |
| binance.api.ts:16 | `BASE_URL[options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binance.api.ts:57 | `BASE_URL[options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binance.exchange.ts:647 | `POSITION_SIDE[pSide]` | YES: truthy result suppresses &#124;&#124; fallback |
| binance.exchange.ts:923 | `ORDER_TYPE[o.type]` | No; direct output value |
| binance.exchange.ts:924 | `ORDER_SIDE[o.side]` | No; direct output value |
| binance.exchange.ts:933 | `ORDER_TYPE[o.type]` | No; strict enum equality only |
| binance.exchange.ts:934 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binance.exchange.ts:936 | `ORDER_TYPE[o.type]` | No; strict enum equality only |
| binance.exchange.ts:936 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binance.exchange.ts:943 | `ORDER_TYPE[v(o, 'orderType')]` | No; later strict enum equality only |
| binance.exchange.ts:951 | `ORDER_SIDE[o.side]` | No; direct output value |
| binance.exchange.ts:963 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binance.exchange.ts:966 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binance.exchange.ts:1039 | `ORDER_SIDE[entry.side]` | YES: truthy result suppresses &#124;&#124; fallback |
| binance.order-history.ts:73 | `ORDER_TYPE[String(row.origType ?? '')]` | No truthiness; non-null inherited result suppresses ?? fallback |
| binance.order-history.ts:73 | `ORDER_TYPE[String(row.type ?? '')]` | No truthiness; non-null inherited result suppresses ?? fallback |
| binance.order-history.ts:74 | `ORDER_STATUS[String(row.status ?? '')]` | No truthiness; non-null inherited result suppresses ?? fallback |
| binance.order-history.ts:80 | `POSITION_SIDE[String(row.positionSide ?? '')]` | No; direct output value |
| binance.ws-private.ts:166 | `ORDER_SIDE[data.S]` | No; direct output value |
| binance.ws-private.ts:195 | `ORDER_TYPE[data.ot]` | No; direct output value |
| binance.ws-private.ts:196 | `ORDER_SIDE[data.S]` | No; direct output value |
| binance.ws-private.ts:204 | `ORDER_TYPE[data.ot]` | No; strict enum equality only |
| binance.ws-private.ts:204 | `ORDER_TRIGGER_TYPE[v(data, 'wt')]` | No; direct output value |
| binance.ws-private.ts:205 | `ORDER_TYPE[data.ot]` | No; strict enum equality only |
| binance.ws-private.ts:205 | `ORDER_TRIGGER_TYPE[v(data, 'wt')]` | No; direct output value |
| binance.ws-private.ts:249 | `ORDER_TYPE[data.o]` | No; later strict enum equality only |
| binance.ws-private.ts:269 | `ORDER_SIDE[data.S]` | No; direct output value |
| binance.ws-private.ts:278 | `ORDER_TRIGGER_TYPE[data.wt]` | No; direct output value |
| binance.ws-private.ts:279 | `ORDER_TRIGGER_TYPE[data.wt]` | No; direct output value |
| binance.ws-private.ts:308 | `POSITION_SIDE[p.ps]` | YES: truthy result suppresses &#124;&#124; fallback |
| binancepm.api.ts:76 | `BASE_FAPI_URL[options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binancepm.exchange.ts:440 | `POSITION_SIDE[pSide]` | YES: truthy result suppresses &#124;&#124; fallback |
| binancepm.exchange.ts:581 | `ORDER_TYPE[o.type]` | No; direct output value |
| binancepm.exchange.ts:582 | `ORDER_SIDE[o.side]` | No; direct output value |
| binancepm.exchange.ts:590 | `ORDER_TYPE[o.type]` | No; strict enum equality only |
| binancepm.exchange.ts:590 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binancepm.exchange.ts:591 | `ORDER_TYPE[o.type]` | No; strict enum equality only |
| binancepm.exchange.ts:591 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binancepm.exchange.ts:629 | `ORDER_TYPE[o.strategyType]` | No; direct output value |
| binancepm.exchange.ts:630 | `ORDER_SIDE[o.side]` | No; direct output value |
| binancepm.exchange.ts:638 | `ORDER_TYPE[o.type]` | No; strict enum equality only |
| binancepm.exchange.ts:638 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binancepm.exchange.ts:639 | `ORDER_TYPE[o.type]` | No; strict enum equality only |
| binancepm.exchange.ts:639 | `ORDER_TRIGGER_TYPE[v(o, 'workingType')]` | No; direct output value |
| binancepm.ws-private.ts:255 | `ORDER_SIDE[data.S]` | No; direct output value |
| binancepm.ws-private.ts:282 | `ORDER_TYPE[data.o]` | No; direct output value |
| binancepm.ws-private.ts:283 | `ORDER_SIDE[data.S]` | No; direct output value |
| binancepm.ws-private.ts:291 | `ORDER_TYPE[data.o]` | No; strict enum equality only |
| binancepm.ws-private.ts:291 | `ORDER_TRIGGER_TYPE[v(data, 'wt')]` | No; direct output value |
| binancepm.ws-private.ts:292 | `ORDER_TYPE[data.o]` | No; strict enum equality only |
| binancepm.ws-private.ts:292 | `ORDER_TRIGGER_TYPE[v(data, 'wt')]` | No; direct output value |
| binancepm.ws-private.ts:376 | `ORDER_SIDE[data.S]` | No; direct output value |
| binancepm.ws-private.ts:388 | `ORDER_TYPE[data.st]` | No; direct output value |
| binancepm.ws-private.ts:389 | `ORDER_SIDE[data.S]` | No; direct output value |
| binancepm.ws-private.ts:397 | `ORDER_TYPE[data.st]` | No; strict enum equality only |
| binancepm.ws-private.ts:397 | `ORDER_TRIGGER_TYPE[v(data, 'wt')]` | No; direct output value |
| binancepm.ws-private.ts:398 | `ORDER_TYPE[data.st]` | No; strict enum equality only |
| binancepm.ws-private.ts:398 | `ORDER_TRIGGER_TYPE[v(data, 'wt')]` | No; direct output value |
| binancespot.api.ts:15 | `BASE_URL[options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binancespot.exchange.ts:341 | `ORDER_TYPE_SPOT[o.type]` | No; direct output value |
| binancespot.exchange.ts:342 | `ORDER_SIDE[o.side]` | No; direct output value |
| binancespot.ws-private.ts:152 | `ORDER_SIDE[data.S]` | No; direct output value |
| binancespot.ws-private.ts:187 | `ORDER_TYPE_SPOT[data.o]` | YES: truthy result suppresses &#124;&#124; fallback |
| binancespot.ws-private.ts:188 | `ORDER_SIDE[data.S]` | No; direct output value |
| binance.ws-private.ts:78 | `BASE_WS_URL.private[key]` | No; closed testnet/livenet keys (not venue controlled) |
| binance.ws-public.ts:124 | `BASE_WS_URL.market[this.parent.options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binance.ws-public.ts:291 | `BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binancepm.ws-private.ts:142 | `BASE_WS_URL.private[key]` | No; closed testnet/livenet keys (not venue controlled) |
| binancepm.ws-public.ts:40 | `BASE_WS_URL.market[this.parent.options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binancepm.ws-public.ts:184 | `BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |
| binancespot.ws-private.ts:34 | `BASE_WS_URL.private[environment]` | No; closed testnet/livenet keys (not venue controlled) |
| binancespot.ws-public.ts:33 | `BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']` | No; closed testnet/livenet keys (not venue controlled) |

The dynamic messageHandlers registry is separate from the72 uppercase-table
accesses above. dispatch-findings.mjs executes192 fresh-realm source cases: native
callable length affects selection, and statistics writes mutate inherited native
functions or Object.prototype. At6000ms, unthrottled reconciliation can throw on
that prototype mutation before parsing. The dispatch row stays pending for shared
arity/statistics operations; no venue-local prototype registry is introduced.
