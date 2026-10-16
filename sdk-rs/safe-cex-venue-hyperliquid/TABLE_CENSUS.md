# Hyperliquid table census

Measured against the tracked TypeScript with `scripts/table-census.cjs`:
248 element-access expressions, including ten named static-table lookups.
The script emits every expression, including dictionary reads and writes;
the static-table count is not a claim that all 248 expressions are immune.

| Source symbol / table | Sites | Key and downstream branch |
| --- | ---: | --- |
| createExchangeAPI/createInfoAPI and spot createAPI / BASE_URLS | 3 | Closed testnet ternary, not a wire-controlled key; no fallback. |
| Four connectAndSubscribe implementations / WSS_URLS | 4 | Closed testnet ternary, not a wire-controlled key; no fallback. |
| isKnownHyperliquidBuilder / HYPERLIQUID_BUILDER_TIERS | 1 | External rate, guarded by Object.hasOwn before lookup; inherited members cannot reach the optional method call. This deliberate Object.hasOwn immunity is present in source and preserved. |
| mapHyperliquidHistoricalOrder / SIDE | 1 | Wire side through String; `if (!side)` drops misses but inherited hits are truthy and survive into the returned record. |
| mapHyperliquidHistoricalOrder / ORDER_TYPE | 1 | Wire orderType through String and lowercase; `?? Limit` does not replace inherited hits. Lowercase still admits constructor and __proto__, so this is not immunity. |

Three static sites use external keys; two are unguarded. The historical-order
mapper remains pending. Its implementation needs shared TableHit branch semantics,
not a venue-local prototype list. Closed outputs must cite
[closed-typed-output-boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary):
TypeScript returns a usable record whose callable field is omitted in JSON,
whereas a Rust refusal drops the whole record. There is no serialized convergence.
`__proto__` instead returns an object and serializes a present `{}` field;
it requires a separate witness/FINDING. Capture typeof before serialization.

Dynamic dictionaries are also in the emitted inventory and remain part of the
exchange/WS implementation audit. The 31 messageHandlers accesses and twelve
topics accesses use candle/l2Book/bbo namespaces in the source, preventing any
Object.prototype name from being spelled. The eleven orderBookInnerState accesses
use locally generated nanoid keys, not venue keys. Pre-placement cache keys
include the coin/side separator; pending-order cache keys need their own audit.
Store indexes, response dictionaries and local asset indexes are not declared
immune by the static-table census.

All four connectAndSubscribe methods select WSS_URLS synchronously and then
dial; none awaits a REST token. Use core-owned start_websocket, with no venue
spawn, protocol disposal hook or task handle. Startup's account-mode REST work
is a separate exchange lifecycle concern.
