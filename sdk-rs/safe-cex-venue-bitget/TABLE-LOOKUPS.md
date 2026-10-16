# Bitget plain-object lookup inventory

26 protocol/category table element-access expressions in the venue family (TypeScript AST
ElementAccessExpression inventory; type annotations are excluded).

| Source member | Table | Sites | Result control flow |
| --- | --- | ---: | --- |
| BitgetExchange.fetchOHLCV | INTERVAL, timeframeToSeconds | 2 | INTERVAL passed to request; timeframeToSeconds result uses `|| 0` |
| BitgetPublicWebsocket.listenOHLCV | INTERVAL | 1 | Passed to channel; no truthiness fallback |
| BitgetExchange.mapOrder | ORDER_STATUS | 1 | Output field only |
| BitgetExchange.mapOrder | ORDER_TYPE | 2 | Initial type feeds `!order.type` strategy recovery; plan override feeds enum equality |
| BitgetExchange.mapOrder | ORDER_SIDE | 2 | Both feed `||` fallback; inherited members suppress it |
| BitgetExchange.mapOrder | TIME_IN_FORCE | 1 | `|| GoodTillCancel` fallback suppressed |
| BitgetInverseExchange.fetchOpenPositions | POSITION_SIDE | 1 | Position ID and output side; no direct truthiness fallback |
| BitgetInverseExchange.mapOrder | ORDER_STATUS | 1 | Output field only |
| BitgetInverseExchange.mapOrder | ORDER_TYPE | 2 | Enum equality controls attached strategies; no unknown-type recovery |
| BitgetInverseExchange.mapOrder | ORDER_SIDE | 1 | `|| o.side` fallback suppressed |
| BitgetInverseExchange.mapOrder | TIME_IN_FORCE | 1 | `|| GoodTillCancel` fallback suppressed |
| BitgetPrivateWebsocket.handleOrderTopic | ORDER_STATUS | 1 | Enum equality; inherited members match no status |
| BitgetPrivateWebsocket.handleAlgoOrdersTopic | ORDER_STATUS | 1 | Enum equality; inherited members match no status |
| mapBitgetBill | CATEGORY_BY_TYPE | 1 | `?? other` fallback suppressed by inherited values |
| BitgetExchange.getCancelPlanType | planTypeMap | 2 | Truthy guard then return; inherited member bypasses typed-order fallback |
| BitgetExchange.formatCreateOrder | inverseObj(TIME_IN_FORCE), inverseObj(ORDER_TYPE) | 3 | Two caller-enum keys and one constant GTC key; enum comparison, no result truthiness |
| BitgetExchange.formatUtaCreateOrder | inverseObj(TIME_IN_FORCE), inverseObj(ORDER_TYPE) | 3 | Same as Classic builder |

The initial uppercase-only count17 omitted the local planTypeMap (2), imported
timeframeToSeconds (1) and computed inverse tables (6). The maintained AST
inventory includes all26. Two inverse-table keys are constants, four are typed
caller enums; interval keys are caller enums too. No uppercasing protects any
wire-keyed protocol lookup here.

Six direct `||` result fallbacks, one local truthy guard, one downstream `!order.type` recovery test,
and one `??` result fallback. Input alias `||` expressions are separate from
truthiness of the lookup result. Store indexes and computed configuration
properties are separate maps, not counted as named protocol tables.

## mapOrder projection boundary

The maintained `mapOrderReference` and `mapOrderCases` corpora execute the actual
class member and protocol tables, preserving own undefined, function identity,
Object.prototype identity, numeric tags and clock timing across 1291 cases.
`orders::map_order` uses shared TableHit for every protocol read. Inherited hits
retain their source truthiness through plan/hedge overwrites and strategy
recovery. Typed projection occurs after source computations and attached legs,
so a later TypeError is not hidden by an earlier representation refusal.

The shared Order schema refuses remaining inherited enum outputs, unknown or
undefined status/type, non-enum raw sides and non-string IDs. This refuses the
whole result, unlike TypeScript returning usable records with open fields. Eleven
callables disappear only on source serialization; __proto__ remains `{}` and is
a separate FINDING. These are declared CHOSEN boundaries, not lookup fixes.
The source's inherited-member bug remains marked; no shared enum was widened.

`info` is an owned snapshot here. A maintained live-source witness records gateway
to info and info to gateway mutation, alongside Rust's unchanged snapshots.
The source also retains a later NaN assignment in info, a separate numeric-domain
FINDING. Both axes stay distinct from the scalar UTF-8 boundary.

## Dynamic object-map inventory

The same maintained AST sweep records86 dynamic-map element expressions:
60 reads,17 writes,9 deletes (nested object expressions counted separately).
Exchange27, inverse15, public WS42, secondary WS2. This is separate from the26
protocol expressions, and the committed fixture lists each exact expression.

- All messageHandlers topics start `candle_` or `orderBook_`: namespacing makes
  inherited names unreachable, including truthy guards and `|| []` fallbacks.
- Outer topics keys are `candle${interval}` or the fixed books channel: namespace
  or closed-source keys prevent inherited names. Inner symbol keys have no such
  protection; `|| 0` can use an inherited member instead of zero.
- Ticker subscription symbol reads feed truthiness guards without key protection.
- Store market/ticker indexes have raw symbol keys; results feed truthy guards,
  optional property accesses and numeric fallbacks. Existing resolve-market and
  PnL witnesses cover the corresponding already-implemented functions; other
  consumers remain pending. No immunity is inferred from TypeScript annotations.
- Positioning cache and inflight records have raw symbol keys. The cached-value
  guard also checks timestamp arithmetic; inherited missing `at` gives NaN and
  fails that comparison. Inflight uses a direct truthy guard and returns the
  inherited member instead of starting work. Write/delete occurrences are counted
  separately and are not falsely described as value lookups.
- Inverse grouping accumulators have raw symbol keys. `if (!acc[key])` suppresses
  initialization on inherited members, then `.push` attempts to call a missing
  method. Both cancel grouping and placement grouping have this shape.
- Inverse coins[marginCoin] is a write, not an inherited-value read. It remains
  in the mutation inventory rather than being counted as a fallback hazard.

No local prototype map, shared enum widening or production correction accompanies
this inventory. Pending owners must preserve these mechanisms through shared
operations when implemented.

The inverse mapper now also consumes shared TableHit through final typed
projection. Its source corpus includes tradeSide/tS inherited hits paired with a
valid raw side, so an own-only lookup changes an observable fallback rather than
merely refusing the same closed field. Unshadowed inherited marketsBySymbol
values have no isInverse property and take the same false branch as a missing
typed market; the source Store is constructed and populated by production code.
