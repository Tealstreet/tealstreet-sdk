# Store notification reachability audit

Bitget at `fa86d4ef33`: 37 ported rows, split into 19 callable declarations
(9 field functions, 8 accessors, 1 method, 1 const function) and 18 data/type rows.
All 37 source declarations resolved with the TypeScript AST; zero unresolved.
The first direct-write grep was narrower than this audit and is not its proof.

Method follows Bybit's `sdk-rs/safe-cex-venue-bybit/STORE_NOTIFICATION_AUDIT.md`
and sibling `scripts/check-store-notification.mjs`, read at `7180768a0a846142bc46b750b9face4b90ee6e30`
(the audit records source commit `5f36bbf8d3`): follow fixed internal calls and receiver
accesses, including indirect send/reconnect/close/health paths, rather than
counting literal Store calls. Arbitrary effects of injected user callbacks or
replaced built-ins are not fixed internal Store operations.

| Root group | Reachability checked |
| --- | --- |
| Eight profile/routing accessors | Read own scalar fields and BaseExchange.options, a stored field assigned by the constructor. usesUtaRouting reads isUta and supportsUtaRouting, both fields. No health publication or notification operation. |
| Seven side/type predicates | Branches over typed inputs and enum constants, with no calls. |
| shouldSuppressValidationErrorEmission | Reads BitgetExchange.suppressBitgetValidationErrorEmission getter → _suppressBitgetValidationErrorEmission stored field. No notification operation. |
| planTypeComparableAmount, floorToLot | DefaultStore.marketsBySymbol getter returns marketsBySymbolMap directly. Then adjust → afterDecimal/pow10 and built-in Number/Math/string operations. The corresponding Rust safe-cex-num path contains numeric operations only. No Store writer or notification callback. |
| generateUuid | Built-in string replacement, Math.random and numeric toString. Rust consumes a supplied random operation; no fixed internal Store path. |
| Seventeen constants and one type | No executable notification path. |

**Result: zero affected ported rows by reachability.** No ported Bitget socket
send, subscribe, reconnect, close, health-publication or snapshot-application row
exists in this census, so Bybit's indirect path has no ported root here.

At `fa86d4ef33`, before the handler slice below, the separate production-Rust
direct sweep also found no Store mutation calls.
`build_ticker_update` constructs native field pairs; position folds return
values. Neither applies a Store update. Existing DefaultStore regression
witnesses are tests, not production write-row claims. The async plan selector's
supplied resolver implementation remains a separate pending row; its callback
boundary is not a claim that the resolver/cache implementation is complete.

This is a result for these 37 ported roots, **not** a guarantee about future
writers or arbitrary callback implementations. Revisit notification reachability
when socket orchestration, Store application or a currently pending receiver
operation is implemented. The notification cutover at 9419b340d8a6 is now consumed by the handler below.
Future writers must propagate the fallible contract and join this inventory.

## New handler crossings (divergent, not part of the 37 ported roots)

| Source row | Fixed path and current FINDING |
| --- | --- |
| BitgetPublicWebsocket.handleTickerTopic | handleTickerTopic → parent.store.updateTicker → notify → ordered subscribers. State commits before a subscriber throws; later subscriber and later ticker rows do not run. |
| BitgetSecondaryPublicWebsocket.handleTickerTopic | Same path from the separately executed secondary class body. |

Both map to ticker::handle_ticker_topic's single Store::update_ticker call. Its
Result now retains StoreMutationError unchanged. The fallible Subscriber's first
failure surfaces after commit and aborts later listeners and rows. The existing
frozen source witness is unchanged; its native expectation now matches the live
source. It asserts Notification, this Store's subscriber ID, response metadata
and original Rc cause identity before projecting the error for source comparison.
No production panic, local notification loop or ExchangeError conversion is used.

Two source paths, one Rust call site. The earlier zero remains a statement about
ported roots, not these newly implemented divergent rows.

The poisoned ticker-index witness has two stages: seeding a __proto__ ticker
replaces the index prototype, then an ordinary-looking symbol key traverses
that poisoned chain. Its inherited string value has no id, so updateTicker
coerces the absent id to "undefined" and updates a different real row.
The stock-prototype dictionary-write contract at5a9137e650 describes the initial
assignment only. It explicitly excludes already-replaced chains and dynamic
Store reads, and ProtoMember identifies stock members only. It neither classifies
nor repairs the later traversal. The shared owner retains this FINDING; no local
replacement cache is introduced. This is separate from notification reachability.

## Store-result conversion audit after the ticker slice

At d0d4f79783, a callee-based sweep of every production src file finds one
Store mutation: ticker::handle_ticker_topic → Store::update_ticker. The only
other Store operation is its tickers_by_symbol read; no production BaseExchange
or BaseWebSocket owner delegates a notifying operation indirectly. Other modules
return projections or call supplied operations, whose implementations remain
separate rows as described above.

The mutation result propagates through `?` to TickerHandlerError, using an
explicit `From<StoreMutationError>` implementation and a typed Store variant.
There is no blanket From implementation, inferred map_err conversion,
if-let-Err stringification, discarded .ok()/unwrap_or_default, or inferred into()
on this path. The result is now additionally annotated
`Result<(), StoreMutationError>` directly at the call site. The former
StorePatchError annotation produced E0308 at ticker.rs:129 on the cutover before
migration. Result: one production write inspected, zero error-identity-laundering
sites; two divergent source handlers no longer carry the notification FINDING.
Remaining prototype-chain and representation causes keep both rows divergent.
This is a callee/result-flow audit, not a map_err spelling census.

Core structural subscriber-ID retention was absent at 9419b340d8a6, but is
repaired in the merged 9af82bfeae tree: ExchangeErrorMetadata contains the list
and both runtime conversion paths append the current subscriber ID. The native
second-subscriber witness additionally invokes the shared fallback conversion
and asserts [1] plus retained response metadata. No production conversion or
temporary box was introduced in Bitget; the handler still retains the full Store
error. Concrete socket assembly remains pending. IDs are Store-local, not
globally unique; no cross-Store identity is inferred.
