# WOO table lookup audit

Source snapshot: 27a71d9c83. TypeScript AST enumeration (ElementAccessExpression,
not indexed-access types) of every non-test WOO source file found **31 static or
reverse-table access sites**, plus **35 mutable registry reads**. The mutable
registries also have 10 writes and 3 deletes, excluded from the read count.
Arrays, fixed record-field projections, and closed `side`/`field` loops are not
table lookups. No Rust row is closed by this audit.

## Static and reverse tables: 31 sites

Locations below are under packages/safe-cex/src/exchanges/woo.

| File:line | Lookup | Result truthiness or fallback | Prototype reachability |
|---|---|---|---|
| woo.exchange.ts:487 | ORDER_SIDE[rawSide] | `||` BUY/SELL fallback, then strict side validation | rawSide uppercased first; seven named prototype functions unreachable |
| woo.exchange.ts:1850 | ORDER_TYPE[o.type || o.order_type] | none; `||` chooses the KEY | raw venue key; function reaches order.type |
| woo.exchange.ts:1851 | ORDER_SIDE[o.side] | none | raw venue key; function reaches order.side |
| woo.exchange.ts:1896 | ORDER_TYPE[v(co,'algoType')] | no truthiness/fallback; strict equality chooses slTriggerType versus tpTriggerType | raw venue key; function reaches type and takes non-SL branch |
| woo.exchange.ts:1906 | ORDER_TRIGGER_TYPE[rawTriggerPriceType] | none | raw venue key; function reaches trigger field |
| woo.exchange.ts:1911 | ORDER_SIDE[co.side] | none | raw venue key; function reaches side |
| woo.ws-private.ts:191 | ORDER_SIDE[v(data,'side')] | RHS of updatedOrder.side `||`; resulting side tested by `if (!side)` | raw venue key; inherited function bypasses missing-side return |
| woo.ws-private.ts:232 | ORDER_SIDE[row.side] | none | raw venue key; function reaches emitted fill.side |
| woo.order-history.ts:92 | ORDER_TYPE[String(row.type ?? '').toUpperCase()] | `??` Limit fallback | uppercasing prevents named prototype hits |
| woo.order-history.ts:93 | ORDER_STATUS[String(row.status ?? '').toUpperCase()] | `??` Closed fallback | uppercasing prevents named prototype hits |
| woo.exchange.ts:2022 | inverseObj(ORDER_TYPE)[opts.type] | none | caller enum; malformed dynamic JS key can hit prototype |
| woo.exchange.ts:2073 | inverseObj(ORDER_SIDE)[opts.side] | none | caller enum; same |
| woo.exchange.ts:2177 | inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType] | none; `||` default is BEFORE lookup | caller enum; same |
| woo.exchange.ts:2183 | inverseObj(ORDER_SIDE)[opts.side] | none | caller enum; same |
| woo.exchange.ts:2239 | inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType] | none; default BEFORE lookup | caller enum; same |
| woo.exchange.ts:2246 | inverseObj(ORDER_SIDE)[opts.side] | none | caller enum; same |
| woo.exchange.ts:2295 | inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType] | none; default BEFORE lookup | caller enum; same |
| woo.exchange.ts:2309 | inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType] | none; default BEFORE lookup | caller enum; same |
| woo.api.ts:97 | BASE_URL[testnet ? 'testnet' : 'livenet'] | none | closed own-key selection |
| woo.ws-private.ts:31 | BASE_WS_URL.private[testnet ? 'testnet' : 'livenet'] | none | closed own-key selection |
| woo.ws-public.ts:37 | BASE_WS_URL.public[testnet ? 'testnet' : 'livenet'] | none | closed own-key selection |
| woo.exchange.ts:1586 | timeframeToSeconds[opts.interval] | `|| Infinity` | caller interval; prototype function suppresses fallback if malformed JS input reaches this statement; earlier getCandleStartTime may reject first |
| woo.v3.public-parity.ts:598 | WOO_V3_FIXED_KLINE_INTERVAL_MS[fixedInterval] | none | explicitly guarded by Object.hasOwn |
| woo.v3.runtime-selector.ts:207 | runtimeGates[route.gate] | `if (!...)` migration gate | route gate from validated source descriptors |
| woo.v3.websocket.ts:367 | WOO_V3_WS_BLOCKERS['command-request-id-type-undocumented'] | none | literal own key |
| woo.v3.websocket.ts:439 | WOO_V3_WS_BLOCKERS['command-failure-envelope-undocumented'] | none | literal own key |
| woo.v3.websocket.ts:741 | WOO_V3_WS_BLOCKERS['private-account-replacement-semantics-undocumented'] | none | literal own key |
| woo.v3.websocket.ts:742 | WOO_V3_WS_BLOCKERS['private-balance-replacement-semantics-undocumented'] | none | literal own key |
| woo.v3.websocket.ts:743 | WOO_V3_WS_BLOCKERS['private-position-replacement-semantics-undocumented'] | none | literal own key |
| woo.v3.websocket.ts:744 | WOO_V3_WS_BLOCKERS['execution-status-lifecycle-incomplete'] | none | literal own key |
| woo.v3.websocket.ts:745 | WOO_V3_WS_BLOCKERS['algo-shape-and-lifecycle-incomplete'] | none | literal own key |

## Mutable registry reads: 35 sites

These are additional to the static table count. A source-level truthiness site
is not proof that an untrusted key survives every preceding normalization.

| File | Lines (one read per listed line unless specified) | Result use |
|---|---|---|
| woo.exchange.ts | 222 | market.quote `|| 'USDT'`, not market truthiness |
| woo.exchange.ts | 385,1082,1356,1705,1838,1886,2009,2158,2159,2273 | missing market/ticker truthiness guards |
| woo.exchange.ts | 808 | optional mark -> Number -> finite/positive fallback |
| woo.exchange.ts | 1717 | optional market.symbol projection |
| woo.exchange.ts | 2024 | ticker passed to pricing predicates |
| woo.exchange.ts | 2219,2220 | market/ticker properties consumed, no lookup fallback |
| woo.v3.public.ts | 300 (two reads) | normalized lookup `??` raw lookup, then `if (!market)`; inherited hit suppresses fallback and skip |
| woo.ws-public.ts | 87 | messageHandlers[untrusted topic], `!handlers?.length`, then for-of |
| woo.ws-public.ts | 112,129,145 | ticker guard/projection in message callbacks |
| woo.ws-public.ts | 258,264,278,280,404,421 | messageHandlers `|| []` before spread/filter |
| woo.ws-public.ts | 283,288,294,423,442 | messageHandlers.length truthiness controls cleanup/unsubscribe |
| woo.ws-public.ts | 312 | `!orderBookInnerState[innerId]` initialization guard |
| woo.ws-public.ts | 319 | state object retrieval, no fallback |

## Findings and shared representation request

`ts-bug:` the seven raw forward lookup sites above accept inherited functions.
The private fill handler is the direct truthiness/control-flow witness: with
no updatedOrder.side, side='constructor' produces a function and does not return
at the missing-side guard; an unknown key does return. The three uppercase sites
do not have that witness. `inverseObj` constructs ordinary objects too, but its
keys here come from caller enum inputs, not a raw venue response.

The existing ORDER_TYPE/ORDER_SIDE/ORDER_TRIGGER_TYPE declaration rows already
record a CORRECTED typed-enum limitation with live prototype witnesses. That is
not sufficient to claim their live consumers ported. mapOrder, mapAlgoOrder and
both private execution handlers remain pending: they need the shared inherited
value representation, including truthiness, without widening domain enums.
No venue-local prototype map is introduced by this audit.

An existing local adaptation does exist in v3_public::map_woo_v3_futures_stats:
its previously measured prototype-key set retains the stats row with absent
id/symbol. This predates the no-local-representation ruling; it must be considered
when the shared representation lands rather than copied to new consumers.
The source oracle already pins that path. All V3 migration gates remain false.

Reproduction: `SAFE_CEX_TYPESCRIPT_PATH=<isolated TypeScript> node
sdk-rs/safe-cex-venue-woo/scripts/table-lookup-audit.cjs` executes the actual
handleExecutionReport declaration with real get-key/lodash and WOO tables/utils;
only parent/store/timestamp dependencies are controlled test doubles. All seven
prototype names emit a fill whose side is a function; unknown emits nothing.
`--correct-prototype` changes ORDER_SIDE to a null-prototype table and fails the
named inherited-side assertion at exit 1. This is an audit witness, not a newly
CI-wired gate or evidence of an implemented Rust handler.

## Immunity classification (source mechanisms, no behavioral change)

- **Uppercase immunity: 3 sites.** exchange:487 and order-history:92,93
  uppercase before lookup, so none of constructor, toString, valueOf,
  hasOwnProperty, isPrototypeOf, propertyIsEnumerable, toLocaleString or
  __proto__ can survive as that spelling. No bug marker/witness is needed for
  these sites.
- **Namespace immunity: 11 mutable reads.** ws-public:258,264,278,280,283,288,
  294,404,421,423,442 use locally constructed topics containing @kline_, @trades
  or @orderbookupdate. Their keys cannot equal any of the eight inherited names.
  This does NOT protect ws-public:87, which indexes the untrusted incoming topic
  directly rather than reconstructing it from the subscribed namespace.
- **Closed source, not immunity: 11 static sites.** The three testnet/livenet
  endpoint choices, seven literal blocker keys, and runtime-selector:207's
  descriptor gate do not take their lookup key from the wire.
- **Explicit own-property guard: 1 static site.** public-parity:598 follows
  Object.hasOwn; an inherited key never reaches its indexed lookup. This is an
  existing source guard, not an added normalization or a null-prototype claim.
- The eight inverse-table sites consume caller enums; a type annotation alone
  does not make their dynamic JavaScript inputs closed source. Their current
  malformed-input qualification remains in the table above.
- No numeric-conversion or null-prototype immunity was established for the
  remaining listed sites. The two orderBookInnerState reads use a locally
  generated nanoid, not a venue-provided key; no prototype witness is claimed.

The UTF-8 citations are unchanged. Integration f450ab80c3 supplies the explicit
HTML id shared-utf8-string-boundary; no row or witness needs an anchor rewrite.

## Order output boundary capture after shared TableHit landed

`order-output-oracle.cjs` executes the actual mapLimitOrder/mapAlgoOrder members
with real WOO tables, get-key/lodash, symbol utilities and safe-math. Only the
market store and clock are controlled dependencies. 85 cases cover five output
positions, ordinary/unknown keys and all twelve inherited names. The fixture
records function identity and the prototype object BEFORE JSON serialization.
A null-prototype correction fails the named inherited-output assertion (exit 1).
Captured before implementing either Rust mapper; neither row is closed by it.

The concrete remaining seam is production safe-cex-types::Order: type and side
are closed enums, as are its optional trigger fields. A local TableHit preserves
the lookup result but cannot travel through that production Order into the
shared store. This requires a shared output-boundary decision or an explicitly
accepted, witnessed divergence; a venue-local wrapper is not end-to-end parity.
Both mapper bodies also still call shared get-key v, not yet present in this
tree. No local v implementation, enum widening or silent inherited-value drop
was added to work around either seam.

The dispatch audit now includes all twelve inherited names. Private fill output
retains eleven functions and one prototype object. Executing the complete WOO
public onMessage member gives a different result: eight positive-arity functions
pass the length guard, record handler stats, then throw TypeError (handlers is
not iterable); toString/valueOf/toLocaleString and __proto__ return at the length
guard. This records the WOO member's exception boundary, not a claim about the
outer host socket's exception handling. Its null-prototype correction separately
fails `public handler length/iteration constructor` at exit 1. No Rust handler is
claimed implemented by either audit.

Public writer follow-through: ws-public:112,129,145 lookup keys come from the
actual normalizeSymbol uppercase transform, so none of the twelve intrinsic
Object.prototype names can reach these guards. This is source-present immunity;
no own-property guard or table replacement was added. The 87-case production
Store oracle includes constructor/toString/__proto__ spellings through that
transform and observes mutations and batch termination, not just a lookup.
This does not claim immunity to separately poisoned custom prototypes in a
Store dictionary; the shared dictionary-write findings remain their own scope.
