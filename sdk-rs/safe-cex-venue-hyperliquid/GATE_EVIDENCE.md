# Production emitter argument lists

The Realm BaseExchange facade forwards TinyEmitter arguments through the shared
BaseExchange::emit_args API. Empty calls retain an empty list, one array remains
one argument, and separate message/metadata arguments remain separate. The
existing per-value JSON boundary and native callback error propagation remain.

The production-facade regression constructs the actual Realm superclass over a
real BaseExchange and observes native on_args listeners. The old bridge compiles
and fails: empty calls produce [null], and the warning pair produces one nested
array. The single-array payload and inherited native log already match. After
the bridge change, all 40 library, 24 production Perpetual binding and 21 Spot
binding tests pass (85 total). Scoped formatting and strict native all-target
Clippy pass. Independent review reports no findings; CodeRabbit is rate limited.
This test scripts only the runtime boundary; the facade, native emitter and JSON conversions are real.

The three existing enclosing-function pins for base_exchange_class are reviewed
manually. Only b.emit becomes b.emit_args; the existing b.dispose, b.can_sign and
b.name edges are unchanged. The token hash changes from d0b3260d4e9cb7dc to
87e8a021dad29e72. No other call-site inventory or shared runtime code is authored
in this slice. The required shared emitter API is inherited from PR425.
Initial global parity reported five missing mappings in that shared dependency: emit_args,
on_args, once_args, ArgsListener and install_exchange_event_bridge. No Hyperliquid
finding is reported; the shared owner has the exact list. This is not a global
gate clearance claim. The supported scoped Hyperliquid check passes using the
unchanged existing exemption subset: 554 rows, 1243 public Rust items, zero
invention/status findings. After inheriting the shared mapping repair at
795d8691d6 through PR425 head ee665ced69, ordinary global parity passes with
7459 rows and 17219 public Rust items, zero findings. The merged source/history
and emitter slices retain all production test cases independently.

# Production history, listeners and private snapshot dispatch

The production perpetual Exchange now forwards candles, historical fills/orders,
income history/range, client-ID lookup and private reconnect snapshots to the
maintained Realm methods. Both public listeners use the existing Realm callback
and cleanup boundary, matching the spot binding. Lookup preserves the source
orderStatus request by cloid and its non-mutating behavior; reconnect runs the
source parallel orders/core-state snapshot instead of the generic order-only
fallback. Terminal-cancel classification, history collapse/mapping and derived
closed-position imports now delegate to their existing maintained helpers.
Derived closed positions remain distinct from fetchClosedPnl.

Nine actual production-binding baselines fail before these overrides/imports:
the read defaults return empty results, lookup takes the generic open-order path,
reconnect skips core state and failed cancels replace the venue message with a
ReferenceError. Both separately authored public listener baselines also fail
against the defaults. After wiring, all fourteen new binding regressions pass.
They observe real native candle/execution/history/income values, signed cancel
cleanup, original nonterminal error preservation, precise REST queries, listener
subscription/delivery/cleanup, and Store updates from actual private order,
position and balance frames. The merged fill regression checks actual private
userFills notifications, Store positions and emitted fill events; non-fill topic
witnesses assert that those frames do not manufacture fill events. Only external
HTTP, socket transport and clocks are scripted; imported names and mappers are
production implementations. Startup order reads are joined before assertions so
background startup completion cannot erase seeded orders or contaminate counts.

Wiring exposed an additional native candle panic: the destructuring host returns
only selected bindings, but fetchOHLCV indexed as if the leading skipped element
occupied a slot. The compiled production candle case panics before this fix;
reading selected slots zero and one matches the maintained TS destructuring and
returns the authored candle/window values. Regex and arithmetic implementations
are unchanged. Income-range expectations preserve the source's non-funding
selection of both PnL and fee rows; balance total excludes unrealized PnL, and
execution timestamps remain seconds while income timestamps remain milliseconds.

Independent review caught an error bridge that inspected a resolver's thrown
value before returning it. Its compiled Realm regression observes unwanted
ordinary-object toString/code/response reads. The corrected bridge retains the
original Value and uses an inert native sentinel only to unwind the DTO mapper.
The restored test checks original identity and zero extra reads for both ordinary
objects and Error values.

Four additional production Realm tests execute the history imports, strict
terminal classifier and derived native fields; retain callback throw identity;
read all five actual class timeout properties; and exercise the four guarded
browser network/approval flows without any window/ethers/browser stand-ins.
Those guarded calls return their explicit missing-wallet refusal. This does not
claim that every generic browser helper is directly callable on an unconfigured
Realm. Browser workflow exports are not installed as Exchange class methods;
the native class's builder-fee checker receives an explicit fourth fee argument.
The production spot constructor aliases perp.createAPI to the native HTTP client,
so its alternate generic spot_client factory and interceptor-only names are not
assembled into the native path. These source-call graph distinctions retain the
separate generic-helper/accepted-set obligations; no globals or exemptions were
invented to suppress static leads.

Seven independently compiled controls remove each of the four new import
registrations, disconnect the orderUpdates and clearinghouseState dispatch
branches, and invert the missing-window guard. They produce eight named native
failures, including both clearinghouse positions and balance assertions. Every
mutation is restored byte-for-byte. A first browser-control attempt failed to
compile and is excluded; the corrected control compiles and fails its native
missing-wallet assertion. The restored library, binding and placement targets
pass 43 + 38 + 1 tests. Fourteen of the binding tests and four library tests are
new in this slice. All 21 existing production Spot binding tests also pass after
the shared import additions. Scoped formatting and strict native all-target Clippy
pass. Global parity:check passes with 7459 rows and 17214 public Rust items,
zero invention/status findings. Independent final review reports no remaining findings.

The shared updateOrder result still returns Vec<String> and discards the named
replacement clientOrderId property. The existing ReplacementOrderIds type and
required common-trait/caller/dispatch migration are recorded for the overseer's
shared API cutover; this venue slice does not claim to resolve that boundary.

# Production TP/SL, fills and serial placement imports

The production Realm now registers `hasSeenFill`, `markFillSeen`,
`isFinitePositiveNumber`, `mapSeries` and
`hyperliquidOrderResponseHasDeferredOrderStatus`. The fill wrappers share the
same existing per-Realm cross-cache module as `getCrossCache`. The numeric
predicate delegates only actual finite numbers to the existing native helper;
non-number objects are never serialized or coerced. The deferred-status wrapper
uses the ordinary native REST response DTO boundary.

`mapSeries` keeps Realm values, receiver/error identity and promises intact.
It reads live length and present slots, resolves `callback.call` before reading
and awaiting an item, invokes that member with the callback as receiver, then
awaits the result before moving on. Its default receiver is the retained module
object, with a weak function capture. The installed p-iteration implementation
independently agrees on the accessor/own-call order and serial live-length cases.

Six `production_import_` regressions in `tests/binding.rs` construct the real
production Exchange and Realm. Only external HTTP, sockets and runtime clocks
are scripted. They observe signed zero-size position and partial TP/SL requests,
a posted waiting trigger returning no IDs and scheduling a refresh without a
duplicate POST, two ordered signed batch requests, first-error short-circuit,
and real private userFills notifications. The fill case checks all three emitter
events, the later BTC position update, replay deduplication and actual HIP-3
clearinghouse HTTP after debounce and shared limiter pacing. Four additional
Realm tests check the shared cache, strict numeric inputs, serial input/callback
awaits, live length, module/custom receivers, own-call access order and original
rejection identity. Unknown imports remain ReferenceErrors. The finite placement
semantics host now refuses unreviewed names; its source-level stubs provide no
production registration evidence.

The compiled baseline fails all six production regressions. Five separate
registration-name removals preserve helper bodies but fail the named production
binding test for each missing import. The fill removals yield zero events; the
numeric and mapping removals yield their named ReferenceErrors; the deferred
removal fails after the trigger has already been posted. Each source is restored
byte-for-byte before the next control. A restored replay runs the complete Realm,
binding and placement-dispatch test targets: 39 library tests, 24 production
binding tests and one native/source placement oracle, all passing.

This slice covers these five imports and production paths. Cancellation,
history and remaining static-name reachability, browser-only boundaries and
missing perpetual trait wiring remain separate follow-up work. The current
accepted-set preflight/refusals and contract backlog are unchanged; this is no
whole-venue parity claim.

# Request-weight nested helper ownership

The local-functions policy previously delegated `rate_limit.rs::LOCAL_FUNCTION:text`
to private `raw_number`, which is absent from the actual compiler inventory. The
policy now names the inventoried `get_hyperliquid_rest_request_weight` decoder.
Its actual call chain reaches `raw_request_weight`, `raw_number` and the unchanged
nested `text` coercer. The category and whole-file source pin are unchanged;
only the reviewed parent and its explanation change. No production body changes.

The new exact compiler function contract has 51 independently authored raw and
typed JSON cases. They observe the actual returned weight's decimal text and
binary64 bits, or thrown error name/message. Cases cover Number coercion, array
and nested-array joins, late own-toString failures after a surrogate, signed and
overflow inputs, endpoint suffixes, single JSON parsing and batch boundaries.
The maintained TS source plus its actual endpoint/type constants independently
match all 51 expectations. The literal native adapter passes all 51 cases, and
the normal fresh compiler/artifact/inventory `runProbes` path returns one actual
runtime result, zero classifications and zero failures. All 534 owners and 36
aliases remain indexed before selecting that one function.

A compiled mutation changes only nested `text`'s comma separator to `0`. The
public caller's named comma-join assertion fails with native weight 71 versus
expected 20, proving the delegated helper executes. Exact source restoration
passes all 51 native cases. Restoring the old policy parent independently
reproduces the original named ordinary refusal; the correct policy bytes are
then restored. The source remains byte-identical to its reviewed SHA
`b147ca525ffac1439ed6d128d5d2779b4be6a964540f3e5c592bec461a3a103a`.

Strict native all-target clippy, formatting and parity pass; parity indexes 7459
rows and 17214 public Rust items. After inheriting merged #389, all 102 master
contracts are unchanged and this PR adds one actual owner contract. Current
coverage is 94 owners, nine aliases and two construction reviews: 105 of 572
reviewed obligations, 467 remaining.

Ordinary preflight advances past text and retains
`browser_approval_flows.rs::LOCAL_FUNCTION:attempt_switch: unclassified nested free function`.
That helper, live getter/prototype behavior, limiter scheduling, dependencies and
other target/feature configurations retain their own reviews. No whole-package
acceptance is claimed.

# Twenty parent and collection record reviews

This cohort reviews twenty concrete maintained records against the actual TS
members, native field types and private remote rename/presence attributes.
`tests/fixtures/accepted_nested_record_schema.json` is the manually reviewed
mapping table and body-pin ledger; it grants no scanner or policy permission.
The production decoder bodies and all private schema attributes are unchanged.

| Native owner | Reviewed structure |
| --- | --- |
| `UserFills` | Both maintained merged interface declarations; all 13 native fill members |
| `WsFill` | Required fill strings, bool and exact numeric bits |
| `Candle` | Required OHLCV strings, distinct `t`/`T` native members and counters |
| `HyperliquidCandle` | Its own concrete candle members and decoder |
| `WsUserFill` | Its own concrete user-fill members and decoder |
| `LimitOrderType` | Required Tif string tag plus actual native enum variant identity |
| `TriggerOrderType` | Trigger numeric bits, boolean and native Tpsl identity |
| `CancelByCloidRequest` | Distinct native coin and Cloid string |
| `ClearinghouseStateAssetPositionsItemPosition` | Every scalar and nested funding/leverage member |
| `ClearinghouseStateAssetPositionsItem` | Native position object and type string |
| `ClearinghouseState` | Ordered position collection, both margin records, time and raw strings |
| `WsOrder` | Native order object, status, timestamp bits and user |
| `WsUserFills` | Snapshot bool, ordered fill collection and user |
| `WsUserFundings` | Snapshot bool, ordered funding collection and user |
| `WsUserNonFundingLedgerUpdate` | Time bits, coin/usdc and all four native ledger variants |
| `WsUserNonFundingLedgerUpdates` | Snapshot bool, ordered ledger collection and user |
| `WsBook` | Coin/time and independently observed bid/ask collections |
| `L2Book` | Actual distinct bid/ask tuple members with element order/cardinality |
| `WsBbo` | Both independently nullable book slots, coin and time bits |
| `SpotClearinghouseBalance` | Required strings and optional token/entryNtl presence; present null refused |

All 1,519 expected cases were authored from the reviewed declarations and values,
without candidate decoder outputs. Each actual maintained adapter reads typed
native members recursively before owner serialization. It records binary64 bits,
native enum variants, Option presence and every sequence element in order. Values
vary individually at each primitive leaf as well as together; collection witnesses
use distinct elements, multiple lengths, empty collections and reversed order.
The corpus independently checks every object member's omission/null/wrong shape,
string versus tagged enum shapes, valid complete declaration-order arrays at
nested map boundaries, wrong tuple lengths, scalar roots, negative zero, positive
overflow and negative underflow. Both nullable bid/ask slots vary independently;
optional spot members admit omission and reject present null.

The installed twenty literal contract adapters pass twenty actual native tests,
1,519 cases through six public Serde/shared ingress routes: 9,114 observations.
The single-field LimitOrderType projection reads the actual tag and Rust variant;
L2Book reads the actual two tuple members. These are native values, not echoed
inputs or success/cardinality placeholders.

Each public map guard was removed independently while preserving JsNumberInput capture and all private wire attributes. All twenty compiled mutations failed the named complete-valid-array native assertion. Separate mutations removed only token or entryNtl present-value admission; both compiled and failed their named present-null assertion. The entire production source was restored byte-for-byte after every mutation. The final restored replay passes all twenty native tests / 1,519 cases / 9,114 observations.

After inheriting current master and merged #382, strict native all-target clippy, formatting and parity pass. parity:check indexes 7459 rows and 17214 public Rust items. Static compiler checks authenticate all reviewed native field identities (including the twenty roots and nine reused child records), exact owner APIs, unchanged public decoder and private wire bodies, and every original census key.

All 82 master contracts are preserved unchanged, with only these twenty new rows appended. The actual inventory remains 534 owners, 36 aliases and two construction obligations (572 total). This branch has 93 reviewed owners, nine aliases and two construction reviews: 104 reviewed, 468 remaining.

The ordinary package gate retains `rate_limit.rs::LOCAL_FUNCTION:text: decoder helper requires its inventoried production decoder`. That existing policy points at non-indexed raw_number; its caller/native-evidence review is separate. This batch does not change local-helper policies or grant a preflight exception.

This reviews only the twenty concrete native/default owners. Child owners,
private companions, local helpers, aliases, dependencies, serializers and the
remaining target/feature matrix retain their own obligations. No whole-package
acceptance is claimed.

# Three fixed future and callback alias reviews

The ordinary package preflight previously stopped at
`spot_account_reads::AccountFuture` with `fixed local future alias probe:
lifetime then output parameter required`. These three reviewed declarations use
two type parameters, so their exact identities belong in the existing finite
`generic-aliases` registry. No scanner, alias helper or validator is widened.
All six existing fixed Realm projection records remain unchanged.

| Actual alias | Fixed reviewed instantiation | Independent expected container |
| --- | --- | --- |
| `spot_account_reads::AccountFuture<V,E>` | production `Value, Value` | `Pin<Box<dyn Future<Output=Result<Value,Value>>>>` |
| `income::IncomeFuture<'a,V,E>` | `'static`, production `Value, Value` | `Pin<Box<dyn Future<Output=Result<Value,Value>> + 'static>>` |
| `startup_timeout::TimeoutCallback<V,E>` | production `Value, Value` | `Rc<dyn Fn(Vec<Value>)->Result<Value,Value>>` |

The registry fixes the actual source file, compiler generic bounds, raw and
expanded targets, parameter order, canonical Pin/Rc roots, sizedness and expected
native type. Fresh ordinary compiler discovery retains every 534 owner and 36
alias before selecting the three. Artifact authentication and all ordinary Serde
and conversion detectors run through the maintained `runProbes` implementation.

`tests/support/accepted_future_aliases.rs` additionally runs three actual native
witnesses. Compile-time Realm host bounds authenticate Value/Error bindings;
invariant mutable references and TypeId pin the independently authored expected
container. Both futures preserve Pending, a real waker call, Ready success/error,
exact numeric bits, lone UTF-16 units, undefined and object/error identity. Poll,
wake and drop counts include unpolled cancellation. The callback preserves shared
Rc allocation/clone identity, ordered native arguments and returned/thrown object
identity. These are distinct runtime witnesses, not classification-only receipts.
They do not exercise the host bodies that supply those executable containers.

The three actual alias contracts pass ordinary native and target-specific Wasm capability classifications (three classifications, zero failures on each target). Separate native ownership/result tests pass 3/3. Strict all-target native clippy, formatting and parity pass; parity indexes 7459 rows and 17214 public Rust items.

Removing each of the three exact finite registry entries independently restores its original ordinary preflight refusal in a fresh process using actual compiler/artifact context. Each policy file is restored byte-for-byte in finally, and all three native capability probes pass after restoration. No validator guard is removed or widened.

After integrating merged #378, all 79 master contracts and six master projection records remain unchanged. The branch has 73 reviewed owners, nine aliases and two construction obligations: 84 of 572 reviewed, 488 remaining. Ordinary preflight advances past the three aliases and retains `rate_limit.rs::LOCAL_FUNCTION:text: decoder helper requires its inventoried production decoder`; its existing policy points to non-indexed `rate_limit::raw_number`. The inventoried request-weight caller and its native observations require a separate review.

Only these three fixed Value/Value monomorphizations are classified; other
arguments, IncomeFuture lifetimes, host bodies, local helpers, dependencies and
the remaining feature/target matrix retain their separate obligations. No whole
package clearance is claimed.

# Twenty-two required-field record reviews

This cohort reviews 22 concrete records individually against their maintained TS
field declarations, native fields, private remote renames and required-field
attributes. Every selected member is a required, non-nullable String, f64 or bool.
The existing JsNumberInput capture and OnlyMap admission remain unchanged.
`tests/fixtures/accepted_flat_record_schema.json` records the manually reviewed
field mappings and exact public decoder body pins; it grants no policy permission.

| Native owner | Maintained TS declaration | Required wire fields |
| --- | --- | --- |
| `ClearinghouseStateAssetPositionsItemPositionCumFunding` | `ClearinghouseState.assetPositions[].position.cumFunding` | `allTime`, `sinceChange`, `sinceOpen` |
| `ClearinghouseStateAssetPositionsItemPositionLeverage` | `ClearinghouseState.assetPositions[].position.leverage` | `rawUsd`, `type`, `value` |
| `ClearinghouseStateCrossMarginSummary` | `ClearinghouseState.crossMarginSummary` | `accountValue`, `totalMarginUsed`, `totalNtlPos`, `totalRawUsd` |
| `ClearinghouseStateMarginSummary` | `ClearinghouseState.marginSummary` | `accountValue`, `totalMarginUsed`, `totalNtlPos`, `totalRawUsd` |
| `OrderResponseStatusCase2` | `OrderResponseStatus error branch` | `error` |
| `WsTrade` | `WsTrade` | `coin`, `side`, `px`, `sz`, `hash`, `time`, `tid` |
| `WsLevel` | `WsLevel` | `px`, `sz`, `n` |
| `WsOrderOrder` | `WsOrder.order` | `coin`, `side`, `limitPx`, `sz`, `oid`, `timestamp`, `origSz` |
| `WsUserFunding` | `WsUserFunding` | `time`, `coin`, `usdc`, `szi`, `fundingRate` |
| `WsLiquidation` | `WsLiquidation` | `lid`, `liquidator`, `liquidated_user`, `liquidated_ntl_pos`, `liquidated_account_value` |
| `WsNonUserCancel` | `WsNonUserCancel` | `coin`, `oid` |
| `L2BookLevelsItem0Item` | `L2Book.levels[0][]` | `px`, `sz`, `n` |
| `L2BookLevelsItem1Item` | `L2Book.levels[1][]` | `px`, `sz`, `n` |
| `FundingHistoryEntry` | `FundingHistoryEntry` | `coin`, `fundingRate`, `premium`, `time` |
| `SpotTokenEvmContract` | `SpotToken.evmContract concrete object branch` | `address`, `evm_extra_wei_decimals` |
| `UserOpenOrder` | `UserOpenOrder` | `coin`, `limitPx`, `oid`, `side`, `sz`, `timestamp` |
| `CancelOrderRequest` | `CancelOrderRequest` | `coin`, `o` |
| `CancelOrderRequestsItem` | `CancelOrderRequests[]` | `a`, `o` |
| `Signature` | `Signature` | `r`, `s`, `v` |
| `Notification` | `Notification` | `notification`, `user` |
| `BuilderInfo` | `BuilderInfo` | `b`, `f` |
| `BuilderFeeApprovalResult` | `BuilderFeeApprovalResult` | `success`, `message` |

The permanent typed projections read every native member before owner serialization.
Distinct values catch swapped field mappings. The 549 independently authored cases
include changed Unicode/boolean/number values, reversed object keys, ignored extra
keys, omission/null/wrong primitive for every required member, negative-zero,
overflow and negative-underflow bits for every numeric member, complete valid
Serde-declaration-order arrays and invalid scalar roots. All six native ingress
routes are checked for each case. The typed fixture passes 22 tests, and the actual
installed contract expressions independently pass 22 tests / 549 cases / 3294
native route observations. `OrderResponseStatusCase2` additionally reads the actual
error string's UTF-16 units; native fields are never success/count placeholders.

Each of the 22 public map-only guards was removed independently while retaining numeric capture and private wire metadata. Every compiled mutation failed its named complete-valid-array assertion; exact source restoration then passed all 22 native tests.

Integrated native tests pass 22/22; strict native all-target clippy and formatting pass. parity:check passes with 7459 rows and 17214 public Rust items. Static compiler checks confirm exact scalar field identities, reviewed decoder bodies and all existing master rows.

The current compiler inventory contains 534 owners, 36 aliases and two construction obligations (572 total). This branch reviews 73 owners, six aliases and two construction obligations: 81 reviewed, 491 remaining. Every original census key is retained.

The ordinary package gate retains the AccountFuture refusal (`fixed local future alias probe: lifetime then output parameter required`). This cohort changes only the
22 reviewed contracts and their evidence; private helpers, other records, aliases,
local scopes, dependencies and the feature/target matrix retain separate reviews.
No whole-package acceptance is claimed.


# Six fixed Realm associated-projection contracts

Six accepted contracts classify only `PrivateValue<Realm>`, `PrivateError<Realm>`,
`PublicValue<Realm>`, `PublicError<Realm>`, `ExchangeValue<Realm>` and
`ExchangeError<Realm>`. Their actual raw and expanded targets remain the exact
`<H as WsPingHost>::Value` or `::Error` projection. The shared finite mechanism in
merged #362 authenticates the same-crate trait, concrete Realm, native Value enum,
unique Realm implementation and associated bindings from fresh compiler metadata.
This consumer preserves all six policy records and installs only its six contracts.

`tests/support/accepted_projection_aliases.rs`: **6/6 native runtime identity tests
pass** against the refreshed master. Each checks invariant mutable-reference type
equality in both directions, TypeId, the original native slot, undefined,
negative-zero/infinity bits, unpaired UTF-16 units and object/thrown identity. A
write through the alias remains visible through the original object.

`scripts/accepted-projection-native.mjs` reads the installed contracts and runs fresh
ordinary rustdoc/check discovery, source claims and compiler-root authentication
before selecting the finite cohort. It discovers all 534 owners and 36 aliases and
requires new compiler-derived `projectionIdentity` evidence for every selected alias.
The maintained `runProbes` executes the ordinary Serde and conversion detectors:
**six native capability classifications pass with zero failures**. These are
classification receipts, separate from the six runtime identity tests above.
The target-specific Wasm run also passes all six capability classifications with
zero failures, retaining all 534 owners and 36 aliases. Reproduce it with
`--target=wasm32-unknown-unknown`.

The shared mechanism's five executed provenance/host/source guard removals and its
compiled concrete-owner rename control remain unchanged in #362. The normal native
consumer probes grant only one fixed Realm identity per alias. Other generic hosts,
remaining alias families, runtime decoder owners, local helpers, dependency and
feature/target obligations retain their separate reviews. No package acceptance is
claimed by these six contracts.

The refreshed ordinary package gate now advances beyond all six projections to
`spot_account_reads::AccountFuture`, where the unreviewed local-future shape is
refused. Every original census key remains indexed. All 51 master contract rows
were retained exactly; the shard now contains 51 owner contracts and six reviewed
aliases, alongside two existing Clone reviews: **59 reviewed of 572 entries,
513 unreviewed**. Earlier cohort sections below record their original snapshots.

Strict native all-targets clippy and formatting pass. Parity passes with
7459 rows, 17214 public items and zero findings. The final independent review
found no issues after the evidence status was updated.

# Two waiting-status owner reviews

`OrderResponseStatusCase3` admits only `waitingForFill`; `OrderResponseStatusCase4`
admits only `waitingForTrigger`. Each concrete native variant, private wire rename
and String admission was reviewed against the separate maintained TS union branch
at hyperliquid.types.ts lines 190–191. Production source is unchanged. The outer
status union and each private companion retain their own review obligations.

The existing literal fixture now also checks both status variants, their exact wire
tags, JSON whitespace, rejection of the other status, every valid tagged-unit object
and seven other invalid values through six native ingress routes. With merged #359,
**14/14 integrated literal tests pass**. The two direct contract expressions pass
**22 independently authored cases / 132 native route observations**, comparing
variants and fixed ordinals before serialization. The original four-owner adapter
runner still passes its unchanged 50 cases / 300 observations.

Run `scripts/accepted-literal-native.py --cohort=statuses` for the two contract
adapters and `scripts/accepted-literal-controls.py --cohort=statuses` for the two
String admission controls. The default cohort remains the original four order
literals. Both controls **compile and fail their named tagged-object assertions**;
exact source restoration passes each test and the complete six-test fixture.
No compiler failure is counted as a control; temporary adapter files are removed.

The master merge retained all 49 existing contract rows and appended only these
two rows. Every row from both merge stages was verified with deep equality,
including signed zero. Fresh compiler API/eligibility checks pass for both owners
and retain every original census key. Current scope is **51 owner contracts plus
two Clone reviews: 53 reviewed of 572 entries, 519 unreviewed**. This includes the
534 actual owners, 36 aliases and two construction obligations; the two additional
owners absent from the old 570 census remain unreviewed.

Strict native all-targets clippy, formatting and parity pass (7459 rows, 17206
public items, zero findings). The ordinary accepted-set gate still refuses the
actual `private_class::PrivateValue` associated projection before native package
receipts. Its finite mechanism and six-alias consumer are separate PRs. No package,
dependency, local-helper, feature or Wasm acceptance is claimed by this two-row batch.

# Eight account, market and ledger string-enum reviews

Individually reviewed contracts cover `HyperliquidUserAbstractionMode`, both
`MetaUniverseItem` margin/growth enums, `UserFundingDeltaType`,
`UserNonFundingLedgerDeltaType`, `WsUserNonFundingLedgerUpdateType` and both
`Hip3MarketInfo` margin/growth enums. Maintained TS lines 79-80, 104-105, 396,
410, 633 and 666-667 define their exact tag sets. Each native declaration and
private remote rename table matches its own TS declaration. The production
String admissions and bodies remain unchanged.

Native tests pass **8/8** for all 23 valid tags, exact native variants and wire
spelling, JSON whitespace, every valid externally tagged map rejection and seven
other forbidden values. Each input traverses text, qualified owned/borrowed Value
and all three shared numeric ingress functions. Direct contract expressions pass
**8/8**, with **110 independently authored cases / 660 native route observations**;
variant/ordinal fields are read before serialization. Temporary probe files are
removed after execution.

`scripts/accepted-mode-controls.py`: **8 compiled named tagged-shape failures /
8 restored passes; final 8/8 pass**. Each existing String admission is removed
independently, forwarding directly to that owner's private remote decoder. Its
named test fails on the admitted tagged object, and exact source restoration
passes. No compiler error counts as negative evidence; `types.rs` is restored
byte-for-byte. These are owner-specific native/default reviews, with no alias,
private helper, local scope, dependency or target-matrix exemption.

Master #349's four string owners are preserved alongside these eight: 49 owner
contracts and two Clone reviews, **51 reviewed of 572 current obligations**, with
**521 unreviewed**. Fresh discovery's additional `host_spot::class_pair` and
`spot_binding::HyperliquidSpotExchange` remain explicit and unreviewed. Integrated native literal tests pass **12/12** after the master merge. The
associated-projection policy review is separate #346; its ordinary refusal is
not waived by this batch. Strict native all-targets clippy and formatting pass
after integration. Fresh compiler API/static eligibility passes for all eight
owners, retaining every original census key and both additional owners. The
ordinary gate still stops at PrivateValue's exact associated projection; it
emits no acceptance receipts. `parity:check` passes with 7459 rows, 17206 public
items and zero findings.

# Four string-enum contract reviews

The contract shard adds individual reviews for `Tif`, `Tpsl`, `Grouping` and
`ScheduleCancelActionType`, matching the maintained TS literal declarations and
concrete native enum variants. Each decoder retains its String admission before
its private remote metadata; no production source changes are proposed.

`tests/accepted_literal_contracts.rs`: **4/4 native tests pass**, checking all nine
valid tags, exact native enum identity and serialization, JSON whitespace, every
valid externally tagged map rejection, and seven other forbidden values. Each
input runs through direct text, qualified owned and borrowed Value deserialization
and the three shared numeric ingress functions. The contract expressions also
execute directly: **4/4 pass, 50 authored cases across six native routes (300
observations)**, comparing native variant/ordinal fields before serialization.
The temporary direct-witness file is removed after execution.

`scripts/accepted-literal-controls.py`: **4 compiled named tagged-shape failures /
4 restored passes; final 4/4 pass**. Each control independently removes only one
public decoder's String admission and calls its existing remote decoder directly.
The named native test fails on the newly admitted tagged object. Every control
restores `types.rs` byte-for-byte; no compiler error counts as negative evidence.

The original 37-owner witness script now names its exact cohort, so new contract
rows cannot accidentally expand or break that historical reproduction: its
37/37 native tests pass again, covering the unchanged 248 cases and formatter. All
existing master contracts are retained. This batch reviews four more obligations:
41 owner contracts plus two Clone reviews, 43 of the original 570 census keys.
Fresh compiler discovery retains every original key and additionally indexes
`realm::host_spot::class_pair` and `spot_binding::HyperliquidSpotExchange`.
The current total is 572 (534 owners, 36 aliases, two construction obligations):
529 remain unreviewed, including both additional owners. The six associated-type
projections remain a separate batch.
Strict native all-targets clippy and formatting pass. `parity:check` passes with
7459 rows, 17206 public items and zero findings. All four contracts match
fresh compiler API inventory and static eligibility. The ordinary package gate
still stops at the exact unreviewed PrivateValue associated projection; it emits
no native acceptance receipts. No package, dependency, local-scope or target-matrix
acceptance is claimed.

# First Hyperliquid accepted-contract cohort

The per-crate contract shard classifies 37 selected owners: cloid and raw-JSON
helpers, response interpretation, cancellation records, raw/retained JSON values,
and the placement record. Direct native tests execute actual production-module
receivers, including private helpers: **37/37 owner tests pass**, covering 248
independently authored cases plus the typed formatter witness. Temporary test
modules are restored byte-for-byte. An initial adapter shadowing compile error
was corrected before execution and is not counted as native failure evidence.
The field observations read native variants, values, presence, UTF-16 units and
float bits before serialization. JsonObject's witness fixes T to JsString; it
makes no claim about arbitrary generic arguments.

Fresh native/default compiler discovery retains 532 owners, 36 aliases and two
construction obligations. The 37 installed contracts match the actual compiler
API lists and pass static contract eligibility. Two additional construction reviews classify only the derived Clone of
`HelperObserver<V,E>` and `SigningPayload`; their owner APIs remain independent.
The other 531 census obligations remain unreviewed. No package, dependency, feature or wasm acceptance is claimed.

The two construction witnesses pass against actual native owners. HelperObserver
clones share pending state and preserve fulfillment/rejection object identity,
registered wakers and late-clone completion. SigningPayload clones retain both
independent wire and encoding snapshots, including native infinity, omitted JSON
fields and signing nil members. Changing either clone implementation independently
compiles and fails its named assertion; exact restoration passes both witnesses.
Run `scripts/accepted-construction-controls.py` for the two mutation controls, or
`scripts/accepted-leaf-native.py` for the 37 direct owner witnesses. These scripts
restore every temporarily edited source byte in `finally`. Neither script grants
compiler, local, namespace, alias, dependency or target-matrix acceptance. Strict
native all-targets clippy passes with both permanent Clone witnesses installed.

Production imports in 25 files now name their actual dependencies explicitly.
After removing use declarations, each file is byte-identical to its prior body;
module/API declarations are unchanged. Strict native all-targets clippy passes.
Existing native tests pass 63/63: 24 library tests, 18 production binding E2Es
and 21 Spot binding E2Es. No extra tests were added for the import-only change.
A fresh source scan removes all import/attribute-scope refusals, while retaining
305 events: both actual core namespace refusals, 146 local OnlyMap types, 146
visitor implementations, ten nested functions and one Step type. Both core
modules remain intact. Compiler artifacts, verified macro origins, source hashes
and the complete scan were preserved for the shared namespace review. The focused
preflight still refuses those unresolved scopes. The ordinary package gate stops
earlier at `private_class::PrivateValue<H>`: the actual associated-type projection
`<H as WsPingHost>::Value` is unsupported by the direct-identity generic alias
probe. It produces no native acceptance receipts. The exact alias remains public
and unclassified; no scanner or alias-policy exemption was introduced.

# Placement-record decoder boundary proof

The actual `HyperliquidPlacementRecord` decoder initially failed three named
native tests: it admitted a complete eight-field positional array, admitted a
present null `timeInForce`, and rejected raw `1e400` before binary64 capture.
The two initial native-field/required-field tests passed. The repair retains the
public serialization schema, uses a private remote decoder behind the existing
object guard, and captures through `JsNumberInput` with exhaustive output checks.

`tests/placement_record_ingress.rs`: **5/5 pass**, including native enum/field
identity, optional presence, overflow, negative zero and binary64 integer rounding
through public Serde and shared numeric entrypoints. No serialized roundtrip is
used to establish native field identity. The shared capture-depth and scalar
Unicode boundaries remain declared in crate guidance.

`python3 sdk-rs/safe-cex-venue-hyperliquid/scripts/placement-record-controls.py`:
**3 compiled named failures / 3 restored passes; final 5/5 pass**. Removing only
the object guard admits the otherwise valid declaration-order array; removing
only the present-field adapter admits null; introducing a Value-first buffer
trips the shared numeric pipeline's explicit buffer refusal. Each control restores
the production source bytes exactly. Logs remain outside the repository.

Integrated checks after merging master: **18/18 scoped native tests pass**
(cache_runtime 11, cloid 2 with the actual maintained TS source oracle, and
placement_record_ingress 5). Strict native all-targets clippy and wasm32 library
clippy pass with warnings denied. `parity:check` exits zero: 7459 manifest rows,
17204 public Rust items, zero invention/status findings. The independent review
found no issues; CodeRabbit's manual review was rate limited, and no unresolved
GitHub review conversations remain.

This is decoder-boundary evidence. The 570-key accepted-contract review remains
open: the selected 37-draft formal runner still refuses unresolved local helper
and import/macro scopes, and no package/dependency/matrix clearance is claimed.

# Hyperliquid pure-phase verification

Scope: types/tables, signing composition, order-response helpers, weighted limiter
and REST API processing. No exchange class, placement dispatch, store reconciliation
or WebSocket implementation is claimed. No live venue calls were made.

Positive gates on this branch:

| Gate | Actual outcome | Negative control and observed refusal |
| --- | --- | --- |
| `cargo fmt -p safe-cex-venue-hyperliquid --check` | pass | Unformatted private function: exit 1, rustfmt diff. |
| `cargo clippy -p safe-cex-venue-hyperliquid --all-targets --locked` | pass under inherited warnings/clippy denies | Unused private function: exit 101, dead-code error from workspace warnings denial. |
| `cargo test -p safe-cex-venue-hyperliquid --all-targets --locked -- --include-ignored` | 24 pass: 20 API + 4 type/table tests, none ignored | Flip vault flag: exit 101 at exact TypeScript action-hash mismatch. Further controls below. |
| Warnings-denied wasm32 build, scoped/locked | pass | wasm-only compile_error: exit 101 at `wasm control`. |
| `yarn parity:extract --check` | pass, 6979 symbols, zero changes/additions/orphans | Rename a manifest TS symbol: exit 1, one missing original plus one orphan; no source changes needed. |
| `yarn parity:check` | pass | Replace order_type_to_wire target with nonexistent item: exit 1, rs resolution error. |
| `yarn parity:report` | pass | Malformed manifest table: exit 1, explicit TOML parse error rather than a progress figure. |
| `yarn parity:progress` | pass | Same malformed manifest: exit 1, explicit parse refusal. |
| `node sdk-rs/scripts/check-layers.mjs` | pass | Temporary venue -> safe-cex-ffi dependency: exit 1, upward-dependency assertion. Policy untouched. |
| Live API/signing oracle in normal native tests | pass; separate isolated-crypto-dependency replay also passes | Change one weight golden 60 -> 61: exit 1, complete-corpus live-source drift. Explicit capture on existing file exits 1 with EEXIST. |

Every control was restored, followed by passing gates. Yarn was invoked directly
through the installed 1.22.22 executable, bypassing the recursive Volta shim;
actual tool output and exit status were inspected. Initial extract check correctly
refused five noncanonical manifest serializations; the extractor normalized only
this venue's files, with zero symbol changes. Types' array item order/notes survived.

The API oracle transpiles the existing TypeScript functions, exposing local helpers,
and uses real ethers/MessagePack with the P5 published-package fingerprints. Axios
registration and host adapter plumbing are modeled; no network or real credential
is used. `qs` is deliberately an invocation trap because query serialization is
an acknowledged host-boundary omission, not a tested claim. CI resolves TypeScript
through SAFE_CEX_TYPESCRIPT_PATH and primitives through SAFE_CEX_CRYPTO_ORACLE_DEPS;
normal cargo tests invoke both live venue oracles. No new workflow step was added;
this branch's push name is not in rust.yml's push filter, so an actual branch CI run
is not claimed. PR/integration native tests execute these unignored tests.

Raw fixture captures were committed before their new implementation fixes:
user actions/numeric edges, diagnostic JSON, sign-only nonce refusals, nonnumeric
order refusals and relay guards. The original L1 corpus is unchanged from the WIP.
All Rust vector entry points parse JSON text, including `1e400`. The diagnostic
fixture retains the original lone-surrogate TS golden: the Rust test explicitly
maps that unrepresentable surrogate to U+FFFD, rather than changing the oracle.

Behavior controls:

- Removing existing-signature guard: exit 101, signed output replaced expected
  `already-signed`. Corpus requires string/object/array plus false/zero/null cases.
- Removing follower wake: exit 101, observed wake count 0 instead of 1. The first
  manual-poll/noop-waker version let this mutation pass; counting-waker assertions
  now pin both cancellation and normal completion, without double wake on Drop.
- Removing ticket removal: reviewer exit 101 at reservation-budget assertion.
  Tests cancel unpolled/front-sleeping/queued work, then drive surviving waiters.
- Removing scheduleCancel corpus entries: exit 101 at required relay action/mode
  property. Request coverage enumerates all eight allowed and three denied actions
  across client/public/network modes; no fixture count alone discharges coverage.
- Signed-body two-key order and appended signature are checked at the recording
  adapter, and the signature is compared against the original action preimage.
- Before fixes, raw `+`/`-` nonces incorrectly signed as zero and raw string/null/bool
  order fields incorrectly reached signing: reviewer regressions failed 101.
  Live TS refusal fixtures now bind every numeric field and blank-nonce controls.
- Independent live oracle comparisons bind mainnet/testnet phantom domains, uint64
  nonce wrap, full/short/null vaults, user action mutation, response error precedence,
  capture bypass, terminal-cancel silence, JS BOM/NEL whitespace and ASCII digits.
- Relative exchange lost-response writes make one attempt, info reads make four;
  TS's absolute-URL misclassification deliberately makes four and is marked ts-bug.

Isolated plan and implementation audits used separate detached checkouts. The
implementation reviewer returned MERGE after nonce/type/wake/signature findings
were fixed, with vault corruption, ticket cleanup, wake deletion and signature
bypass all failing. Their last full baseline had 19 API tests; the final 20-test
baseline adds the broader existing-signature truthiness corpus without changing
production code after that review.

Manifest measurement on this branch: Hyperliquid has 531 rows: 53 ported,
86 divergent, 392 pending. The five pure files have no pending rows; divergent
rows name host-owned adapters/runtime clock/shared limiter, typed numeric bounds,
error representation, wallet ownership and the acknowledged query/general
Signature.from omissions. Repository report is 329/6979 on this parent, not an
integration progress claim. Pending exchange/WS and other helper files remain
pending by phase scope. See README for the forced/chosen bounds and preserved bugs.

Subsequent classification audit: see CLAIM_AUDIT.md. Both isolated reviewers returned
DO NOT MERGE unchanged claims. The earlier 53/86 figures and MERGE verdict are historical,
not a claim that the newly identified wire/type/diagnostic findings are closed.

## Integration merge and shared formatter

Merged c2a96059ca at 4b840ee48f. Shared conflicting files initially took canonical
integration content. Hyperliquid production/test ryu wrappers now call public
safe_cex_num::js_number_string; the direct ryu-js venue dependency is removed.
The implementation body is identical. No audit finding was declared fixed.

Merge preservation controls found and repaired two losses: incoming workflow lacked
P5's mandatory live oracle steps/setup/controls, and incoming exempt metadata lacked
P3b's already-existing local_runtime host-glue entry for source carried by this
branch. The latter was restored byte-for-byte from 08409644f3, not invented anew.
The canonical integration wildcard/gate repairs remain present.

Named P5 live crypto oracle steps: before merge **2**, incoming c2a96059ca **0**,
restored **2** (`Run Lighter Go-WASM vector oracle`, `Run all crypto signing oracles`).
Both isolated dependency/native-SDK setup steps are also retained. A reviewer caught
an inert branch-removal control caused by wildcard position; token removal now fails
regardless of branch-list position. The restored P5 runner/step controls and canonical
wildcard controls pass: 36 focused tests, including removal/narrowing/masking cases.

Scoped verification after migration: fmt pass; 24 venue tests pass; inherited strict
all-targets clippy pass; warnings-denied wasm pass; extract/check/report/progress pass
at 6982 manifest rows. Independent formatter review compared against live Node and
replaced formatting with Rust Display: exit101 at an exponent boundary (Rust decimal
spelling versus Node exponential spelling), also exposing signed-zero mismatch.
Scratch mutation restored. Code-specific review MERGE; whole branch DO NOT MERGE.

Full CI wiring suite: 79 tests, 55 pass, 24 fail. These failures are the canonical
layers policy not classifying the safe-cex-exchange scaffold inherited from venue-deps,
so every venue's dependency points at an unclassified crate. Shared policy repair
was reported to the overseer; no layer policy or expected test set was widened.
The venue wildcard now matches this branch. That enables a real push CI run; it
does not establish a CI-green claim while this policy failure and the 15 claim
finding rows remain open. All earlier no-push-filter statements are historical.

## Missing-field payload priority repair

Oracle capture 742f88195a wraps only ethers.keccak256 to observe the bytes emitted by
existing TS actionHash; no expected signing expression is re-derived. The committed
missing-fields corpus contains complete/missing buy/missing reduce/both cases, exact
wire/action JSON, preimage, hash/signature and serialized/reparsed control. Normal
native tests replay the full live source oracle including this corpus.

After repair: 25 scoped tests pass (21 API plus 4 types), strict clippy/wasm pass,
parity invention/status pass. Independent controls: encoding_view->wire fails101 at
actual keccak bytes; Serialize->encoding fails101 at JSON omission. Reverse retained
payload replay pins no per-process cross-order state. Complete old vectors unchanged.
This closes the missing b/r representation mismatch, not the remaining root/order_type
errors or prepared metadata across serialized RestRequest boundaries. TypeScript's
nil-signed/omitted-JSON discrepancy is marked and deliberately preserved.

Wire diagnostics: disabling the null order-type guard failed cargo test with Error versus TypeError (exit 101); changing the TypeError constructor name to Error independently failed the name assertion (101). Restored source passes. The corpus requires null root, invalid object, missing order_type and null order_type cases explicitly, not only a count.

Integration 311678087e merge: named step occurrences 56 (Hyperliquid), 52 (integration), 56 (result), with zero missing names from either parent; unique names 48/44/48 because setup names recur across jobs. Crypto setup/run occurrences remain four, including both live oracle gates. Canonical layer policy is unchanged. Backend merge takes LoadedChunks and adapts inherited P3b delegates and IndexedDB; only canonical parser visibility changes to pub(crate), avoiding a second parser. Existing workflow-removal/masking controls all pass (132 tests).

Core 6922766a6c merge (includes production Store759bd52c8b): named steps56/54/58, zero missing from either parent. Adds core push trigger alongside venue wildcard and P5 trigger; crypto oracle steps remain. Cargo adds only the core pinned test support target-triple1.0.1 and trybuild1.0.116, leaving crypto primitives unchanged. 26 venue tests, plain workspace-strict clippy, warnings-denied wasm, parity and132 wiring controls pass.

## Accepted-set sweep (literal unions and object records)

The audit found all 14 derived literal enums accepted externally tagged object
forms (34 names), although their string metadata matched TypeScript. All 77
records now have a map-only gate: 74 generated wire records and three cancel
response records. The source-derived corpus supplies 351 literal rejections
and object/positional probes for every record. Concrete qualified public calls
are tested in addition to generic trait calls; private remote companions hold
the canonical serde names once. Explicit checked-in impls are necessary because
the parity extractor intentionally refuses item macro expansion.

Independent checkout controls each fail 101 for their actual defect: adding an
unprobed alias changes actual serde metadata; restoring permissive enum decode
accepts the tagged Tif object; restoring permissive record decode accepts its
positional array; adding a public inherent decoder bypass fails the qualified
call; deleting a record corpus row and duplicating a literal corpus row each
fail exact coverage/uniqueness. The generator plus rustfmt reproduces the strict
source byte-for-byte. Removing only the checked-in Tif string front gate
fails the generation check at strict wire source regeneration drift (exit 1).
Mandatory venue-oracle:hyperliquid runs generation drift,
source corpora and Rust comparisons using the existing isolated TS install.
Removal/masking of its step or deletion of any child fails the CI wiring gate.

Native 30 tests, strict clippy, warnings-denied wasm, layering and parity pass.
This scoped review does not close the separate CLAIM_AUDIT findings or mark the
exchange/WS implementations complete. Optional nonnull fields still require
their own repair; map-only entry is an independent input-shape property.

## Shared coercion utility merge

Merged utility revision 5cf6b4d8d8. Named CI step occurrences: 59 on this
parent, 55 on utilities, 60 after resolution; no names missing from either
parent. Both parents' exemption target sets are retained with no duplicates.
CI control groups and candle evidence were combined per hunk. The shared
formatter delegates to the utility owner's single lower-layer implementation.
Cargo reconciled merged manifests from the existing exact-pin lock seed;
Poseidon2 0.1.4, ECgFp5 0.1.2, alloy-primitives 1.7.3, franklin revision
6ba7708b and bellman revision 04e846ca remain unchanged. The utility parent
requires num-bigint 0.4.6, replacing the previously unpinned 0.4.8 in the shared
graph; this is a transitive crypto dependency change, checked by live scheme
oracles and the scoped crypto suite before committing the merge.

Scoped Hyperliquid 30 native tests, strict clippy and warnings-denied wasm pass
with --jobs 2, sequentially. All 138 wiring controls pass; parity resolves 1272
public items. No HOW tsx test was modified or made optional. The new shared
coercions are available for venue callers; this merge does not assert that
every existing local coercion call has already been migrated.

## Integration merge at 7fcffd57e8

Named CI steps: 60 on this parent, 52 on integration, 60 after merge; no name
missing from either parent. Source-root conflict resolved as an additive union:
retain the three authored candle roots and two narrow P5 roots, add integration's
json-parse root. No duplicate roots. The generated lock reconciles the transport
formatter dependency change without changing crypto pins. The new optional
RestResponse.reason field required five recorder fixtures to explicitly supply
None; production Hyperliquid remains unchanged by that compile repair.

After repair: 30 native tests, strict clippy and warnings-denied wasm pass with
--jobs 2. Extraction proposes zero updates; parity resolves 6984 rows/1276
public items; 138 CI wiring controls pass. Integration's known HOW tsx oracle
environment failure is not modified, skipped or treated as a venue regression.
The own-oracle as-const sweep finds runtime JSON/byte comparisons and direct TS
AST shape inventories, rather than TS assignability tests over inferred tuples;
no expected as-const tuple is widened through a broad annotation. Existing
wrong-value, alias and decoder mutations fail at named runtime assertions.

## Response-bearing REST and retry ABI merge

Core 86aae2c4e1 is retained with both remote-signer arms. The venue API now
uses RestBody and RestFuture/RestFailure. Response-interceptor refusals retain
the terminal status/reason/headers/body and the signed request; adapter errors
propagate unchanged. Retry uses the existing injected limiter clock, with the
Hyperliquid all-POST URL classifier unchanged. No classified method or URL is
replaced by a captured failure request.

`venue_refusal_retains_response_and_signed_request`: replacing the production
response Some with None compiled, then exited 101 at `venue refusal must retain
terminal response`. `retry_budget_uses_the_injected_limiter_clock`: replacing
the classified request with a timeout-free copy compiled, then exited 101 at
`expired read budget must not retry`. Both restored tests pass; 24 API tests
and 32 total venue tests pass, with strict clippy and wasm checks.

Merge CI steps: 71 venue-parent / 59 core-parent -> 71, none missing.
The venue Cargo dependencies survived core scaffold cleanup and compile.

## Exchange numeric helpers

Actual source bodies were captured first at 344d5c2da8 and 54c37ca4ee; the
committed expected values were not changed to fit Rust. Tests parse raw strings
and compare Number-to-String BEFORE serialization, including positive/negative
Infinity from overflowing arithmetic. The shared parseFloat, fallible String
and Number primitives supply coercion; no venue parser or get-key clone exists.
Lone-surrogate valid-JSON witnesses preserve Node outputs alongside the chosen
shared UTF-8 ingress refusal, with scalar Unicode, astral and BOM/NEL positives.

Native CI discovers the numeric_helpers test; the existing mandatory named
venue oracle also invokes both generator modes and the Rust test. Its exact
command is wiring-pinned. The child executes actual TS and requires a
parseFloat-to-Number mutation to fail at actual exchange numeric helper drift,
so merely running a no-op oracle cannot pass.

Four production controls each compiled then failed the raw helper comparison at
runtime 101: parseFloat replaced with Number (pre-serialization numeric value),
String error erased (source refusal must remain an error), number-type guard
removed (isFinitePositiveNumber boolean case), and nonpositive denominator guard
removed (pre-serialization numeric value). Restored numeric tests pass.

Host strict clippy passes separately. Wasm strict clippy was actually run with
--target wasm32-unknown-unknown --all-targets -- -D warnings and fails in the
canonical inherited candle-cache handle_upgrade_error on two collapsible-if
lints. That dependency is byte-identical to d8bcccd413; this is routed to its
owner, not hidden by claiming the wasm build covers linting.

C1 review found that deleting the positive-number helper's two surrogate rows
and its generator arm together escaped the first coverage assertion. That
assertion was repaired to require BOTH raw witnesses for EACH divergent helper.
The same self-consistent deletion now exits 101 at `every divergent helper
requires both lone-surrogate witnesses: isFinitePositiveNumber`; restoring
fixture and generator passes both numeric tests. Scoped repair review: MERGE.

## Canonical body-state and privacy merge, 935fc8f578

Merge 1d760225ee retains 75 named CI steps against 74 on each parent;
no step name or multiplicity is missing. Real tomllib parsing finds 284 exemption
blocks / 284 distinct targets / zero blank reasons. Privacy dispatch is present;
the applicationId/vault in-place options patch remains a separate core seam.

Hyperliquid now handles RestBody::Transformed explicitly. Its stored string stays
a string at the source signing guard; fresh requests start Untransformed and
signing retains the original phase through with_signed_data. Unsigned Encoded
candidates refuse before transmission. Re-parsing the transformed string into
an object compiles, then fails the replay signing-guard assertion at runtime 101.
Restored API suite: 25 tests pass. Strict host clippy and strict wasm32 clippy
both pass separately (all-targets, warnings denied, jobs 2). Scoped source review
returned MERGE for this repair, without claiming the unfinished exchange/WS.

## Historical-order pure helpers

The unmodified real TypeScript module supplied fixtures in 86c3d9e908, before
Rust implementation. 27436b4fd9 added independent live-alias, scalar Unicode and
valid-JSON lone-surrogate witnesses; source outputs stay double escaped so the
fixture itself remains readable at the chosen UTF-8 boundary.

Five restored history tests pass. Four production controls compile then fail
runtime 101 in `actual_history_source_pins_transitions_and_pre_serialization_hits`:
filled rank 2 -> 0 changes equal-time terminal selection; Number in place of
parseFloat changes trailing-junk amounts; own-only SIDE changes an inherited
truthy hit into a dropped row; treating inherited ORDER_TYPE as missing wrongly
applies Limit. The live runner's rank mutation fails regeneration drift. Its
copy-at-Map-insertion mutation fails the before/after alias witness drift. CI's
existing mandatory Hyperliquid step now executes both generators and the Rust
history comparisons; removing either child/comparison fails the wiring audit.

Historical output refusal preserves source field-coercion timing: an inherited
side plus a throwing status still returns the source TypeError before a typed
output refusal. Eleven callable prototype members are witnessed before JSON
omits their field; __proto__ has its own object witness that serializes as {}.
Rust refuses the entire typed record, whereas TypeScript returns a usable record.
No serialized-convergence claim is made. Collapse returns owned snapshots where
source winners remain live input aliases; the caller-mutation witness records
both directions, independently of closed-field and UTF-8 losses.

The reviewer found a public-helper error-order mismatch: incumbent null had been
checked before candidate timestamp coercion. Actual private TS helpers were
captured in 1568196cb2 before fixing that order. Restoring the early incumbent
check compiles and fails runtime 101 at the direct-helper error-order assertion.
The symbol resolver is explicitly fallible; source callback refusal was captured
in e640e3430c before propagating it through Rust. Swallowing that callback error
compiles and fails the same direct-helper regression on the source refusal.
Scoped review accepted the order correction; class/WS remain outside this slice.

The per-helper surrogate inventory now requires both escaped extremes separately
for toNumber, toOptionalNumber and isLaterState, and for both map/collapse rows.
Collapse positives retain ordinary Unicode, an astral pair and BOM in the key.
These are input/output witnesses, not an assertion that the Rust representation
can store those refused code units.

## Consolidation merge of integration 8cbfb8b5f9

Named CI step multisets: 75 venue / 80 integration -> 81 merged, zero missing
from either parent. The wiring conflict combined relocated shared assertions
with the venue assertions: retained one copy of identical shared checks and all
Hyperliquid children. 215 wiring tests pass. Real tomllib parsing: 303 exemption
blocks / 303 distinct targets / zero blank reasons; all 490 manifest files parse.
Cargo.lock package triples exactly equal integration (519), with no missing or
extra triple. All eight venue dependencies survive. Scoped native all-targets
venue tests pass, host strict clippy and wasm strict clippy pass separately,
78 layer tests pass, extractor is 0 added / 0 orphaned / 0 updates, and parity
passes on 6984 rows. Numeric candidate files were restored before these gates.
The labelled .rs.pending harness is explicitly not a passing parity assertion.

## Existing decoder audit, independent of the rejected Value-first candidate

The Node source corpus has 238 signed-zero witness contexts across all 53 numeric
types. Existing production code was restored before the four-form sweep. Raw
lexical -0 refuses nine contexts through untagged unions; already-Value lexical
-0 decodes those same nine as positive zero. The four affected roots are
OidOrCloid, OrderResponseStatus, OrderResponse and ModifyRequest (six distinct
name/path pairs). Lexical -0.0 and natively constructed Value(-0.0) retain every
bit on all 238 contexts. All four roots were already F-NUMERIC-INGRESS FINDING,
so no ported-row demotion is hidden by this measurement. The permanent witness
pins the known refusal/loss and clean forms and must flip when shared ingress
lands; it is not a claim of parity. Full 2,380-case matching remains explicitly
pending. Numeric corpus regeneration executes before the existing-decoder witness.

## Deep numeric adoption and current integration merge

The 53 numeric roots compare 2,380 oracle-first Node cases on seven bare/shared
entry routes (16,660 native bit comparisons before serialization). Raw overflow
and signed zero are tested independently. Null accepted-set fixtures were also
captured before repair: 150 compiler-rejected optional-null/missing-nullable cases.
Production record, untagged union, optional-null and required-nullable bypasses
all compiled and failed named runtime assertions; restoring them passed.
Source contextual annotation widening fails its named assertion and a permanent
child-process test requires this failure. Isolated compiler execution in a
tracked-source mirror with no ancestor node_modules passes; a missing-compiler
control fails MODULE_NOT_FOUND. The temporary mirror was deleted.

At integration merge 1ad6029e32 (parents 434a32064a and 1f6698344c), 47 tests
across 10 binaries pass, zero ignored. Host and wasm strict clippy passed
separately, jobs 2. Wiring 247, layer tests 78, extractor 0 added / 0 orphaned,
parity 7,334 rows / 2,717 public items. Parity was run with an explicit workspace
path to avoid an old build's embedded default root. Real TOML parse: 333 blocks,
333 distinct targets, zero duplicates and blank reasons, no parent target loss.
CI multiset: rust workflow 88/88 to 89; all workflows 348/348 to 349, no missing
occurrences. Eight venue dependencies and crypto/lock/layer-container pins survive.
An independent merged-state review returned MERGE for integrity; this is not a
whole-venue verdict, and the 13 remaining findings are not discharged by gates.

Readiness refresh: 86639d3df5 merges current integration b0e0b9c68b, adding shared
container eligibility and GRVT/worker ingress without conflicts. The full scoped
47-test suite, separate host/wasm strict clippy, 247 wiring tests, 78 layer tests,
extractor 0/0 and explicit-workspace parity (7,334/2,728) all pass again. Rust CI
89/88 to 89, with zero missing; exemptions remain 333/333 with no blank reasons.

## Exchange state (constructor still incomplete)

Actual-source captures preceded implementation: 533 typed identity/TIF/cancel
cases, separate valid-JSON code-unit string witnesses, one host-clock reentry
witness and an options-alias witness. Three production mutations compile and
fail runtime assertions: >= expiry (cancel expiry), base.can_sign privacy
(venue privacy dispatch), omitted delete-before-reinsert (TIF insertion order).
The original suppression implementation held a RefCell borrow during the host
clock call and panicked under the source-agreeing reentry witness; clock sampling
now precedes the borrow. The permanent reentry test pins this failure mode.
The source age/privacy controls and their expected failures are executable too.
Source oracles reproduce through the isolated compiler in a tracked mirror with
no ancestor node_modules; missing compiler fails MODULE_NOT_FOUND. Mirror removed.
Scoped adversarial review returns MERGE for the state and alias claims, excluding
client/socket composition and original findings. Only own-package lock edge to
the test's real Store is added; no package/version/feature resolution changes.

Merge refresh e4d3801f9f takes integration 968da80231 after the state slice.
Scoped venue tests pass with zero ignored; wiring 252, layers 78, extractor
0 added / 0 orphaned and explicit-workspace parity 7334 / 3242 pass.
Real tomllib parse: 338 exemption blocks / 338 distinct targets, zero duplicates,
blank reasons or missing targets from either parent. Rust CI named steps
89 / 90 to 90 with zero missing. Eight production dependencies and the own
Store test dependency remain; the scoped locked offline all-target check passes.
Merged-state adversarial review returned MERGE for integrity, independently
confirming CI occurrences, dependency/layer retention, lock identities and the
encoded-body guard. Refreshed host and separate wasm strict clippy both pass.
The next nonce implementation has passing six-test state coverage but its row
remains pending until its production mutation and complete scoped lint finish.

Heritage census audit: six type-source inventories plus the signing cancel-record path ignored InterfaceDeclaration.heritageClauses. Current sources have none, so no row behavior or count changes. Read-only inherited-required-number mutation failed before the fix; all seven now fail specifically at Unsupported interface heritage. A normal cargo literal-union test executes the seven controls; source and committed fixtures remain unchanged.

Nonce row: source-first ebb2bfe191 captured 15 pre-serialization bit cases.
Replacing shared math_max with f64::max compiles and fails runtime 101 at
source nonce bits for NaN. Restored shared operation; source 2^53 increment
plateau is preserved and marked ts-bug on the row. Counts become 63/97/371.

Nonce closure verification: six state tests pass after restoration; separate
host and wasm strict clippy pass, parity 7334/3243 passes, extractor reports
0 manifests changed / 0 added / 0 orphaned. Heritage literal suite passes 3/3.

Resume merge cc0a109bcd takes integration a3d514bec8. Full scoped suite passes
54 tests, zero ignored; separate host and wasm strict clippy pass with jobs 1.
Wiring 252, layers 78, explicit-workspace parity 7334/3244 and extractor
0 changed/0 added/0 orphaned pass. Real tomllib: 338 blocks / 338 distinct,
zero duplicates, blank reasons or missing parent targets. CI 90/90 to 90,
zero missing occurrences; eight production dependencies retained. Independent
merged-state review returns MERGE integrity, excluding whole-venue completeness.

API/history/crypto identity disposition: source-first supplements e67ec09630
and028143068c precede consumer tests. API waiting/deferred substitution compiles
and fails101 at API deferred identity; mapper timestamp swap compiles and fails
HistoricalOrder native field identity createdTime; public Stark domain version/
revision swap compiles and fails Stark declared domain version identity. All
restore to passing. API26/26 and history6/6 pass; crypto focused identity1/1
passes; host and wasm strict clippy separately pass for the two changed crates
with jobs1. Wiring256 passes with SDK/native identity omission and masked-failure
controls, retaining every older runner call. All34 Hyper and50 crypto original
fixture/support artifacts remain byte-identical to5bbac197df. Counts remain
63/97/371; the thirteen findings are individually listed in the identity audit.
Extractor rerun through the exact real Yarn1.22.21 entrypoint remains0 changed/
0 added/0 orphaned, exit0; prior real-entrypoint1.22.22 result did not change.
Subprocess inspection finds no per-case Node loop in the owned Hyper/crypto
oracles: each child processes a whole corpus; heritage loops over seven distinct
source-inventory controls, not cases. No invocation count reduction claimed.

Follow-on integration merge e53f05751d retains the pinned PR7267 head
a670ccc8d1. Read-only merged-state integrity review: CI90/90 to90, none
missing; exemptions363 blocks/363 distinct, zero duplicates or blank reasons;
all eight production dependencies and two test dependencies retained, lock
and layer containers preserved. Wiring257 passes. This is integrity evidence,
not a whole cardinality disposition or merge-readiness sheet.

Subscription cardinality: removing all source corpus/category count guards
and supplying an empty source compiled and failed runtime101 at
SUBSCRIPTION-CARDINALITY native failure index coverage. The initial mutant
left an unused closure parameter and failed compilation; it supplies no
runtime proof. Renaming that mutant-only parameter exposed the named runtime
failure. Corpus bytes were not edited. The larger API/history/enum/nonce/parser
census still has open cardinality controls; inspection is not proof.

A consuming-empty Meta universe mutation decodes the complete vector before
clearing it, compiles, and fails runtime101 at per-field identity Meta.
This is an injected mutation testing assertion sensitivity, not a production
decoder defect. No trailing-element rejection supplies the failure.
Keeping the consuming-empty mutant while bypassing the array length guard
restores a green target: the fixed outer931 count alone cannot detect a lost
inner collection. The native array guard is therefore load-bearing; this
control does not prove every other collection path is covered.

API identity cardinality: empty and shortened source corpora each compile
and fail runtime101 at API-IDENTITY source total. Bypassing that source guard
with the same two mutations instead fails API-IDENTITY native index coverage.
Restored focused API target passes1/1. No source artifact was rewritten.

Numeric cardinality supplement pins53 roots,2380 cases and16660 completed
native entry comparisons. Initial empty/subset controls compile and fail
NUMERIC-CARDINALITY source cases; bypassing source guards fails
NUMERIC-CARDINALITY native index coverage. Separately pinned per-root totals
are an added expectation, not a new original-capture independence claim.
The broad census also identifies open API signature/relay/diagnostic, history,
exchange state/placement/nonce, raw types, ingress audit/string/null/literal/
record and scalar helper targets. Counts/shape witnesses and regeneration are
not automatically native comparison coverage. No whole-sweep closure claimed.
Numeric subcorpus redistribution preserving2380 cases compiles and fails
NUMERIC-CARDINALITY source subcorpus TriggerOrderType. Bypassing every
source case/subcorpus/edge guard and emptying cases still fails native index
coverage. The restored numeric target passes1/1 with the root totals and
7980 source field /55860 native field totals enabled.

API output controls mutate production after it consumes every status: clear,
pop and extra ID each compile and fail API-IDENTITY independent native order
ID count. Keeping the consuming-empty mutation while removing both source
and native per-case ID count guards still fails API native resting and filled
order IDs, establishing a second comparison layer rather than trusting counts.
These are assertion-sensitivity controls, not claims of production defects.
Restored focused API and numeric targets pass1/1 each on the supplemented
working tree. Numeric assertions verify7980 source numeric fields and55860
native numeric field comparisons across the seven fixed entry paths.
Scoped strict Clippy passes separately on host and wasm32-unknown-unknown
for the two changed test targets api and typed_numbers, with locked/offline
and jobs1. These results do not cover KuCoin or the workspace build.

Nonce cardinality: the scalar native target now fixes15 source cases and30
returned/stored bit comparisons. Empty/subset decoded source mutations each
compile and fail101 at NONCE-CARDINALITY source total; bypassing that guard
with the same mutations fails NONCE-CARDINALITY native index coverage.
The restored focused lib target passes1/1. The source fixture remains unchanged.
Native empty/subset/extra collection-output mutations are not applicable to
this f64-returning operation; no collection-decoder immunity is claimed.
No fresh host/wasm Clippy or whole-sweep readiness sheet is attached to this
nonce-only assertion change.

State cardinality: fixed533 source/native cases, seven category totals and563
completed native field comparisons. Empty/subset source mutations compile
and fail101 at STATE-CARDINALITY source total; bypassing all source guards
with those mutations fails STATE-CARDINALITY native index coverage. A case
redistribution preserving533 rows fails source category privacy/None.
Production TIF cache clear, prefix shortening and extra-entry mutations each
compile and fail101 at tif insertion-order and eviction. Keeping the clear
mutation with all source-count/category guards removed still fails that same
full native key-vector assertion. Original source artifact stays unchanged.
Restored focused state target passes1/1 including533 case /563 field counters.
This is a state target disposition, not whole-sweep readiness; no new host/wasm
Clippy claim attaches to this assertion-only change.

Placement cardinality: fixed30 cases (10 price/20 TPSL) and60 native value/
read-effect comparisons; price input vectors independently require4 entries.
Empty/subset source mutations compile and fail101 at PLACEMENT-CARDINALITY
source total; bypassing source guards with each mutation fails native index
coverage. A same-total category redistribution fails source category price.
Zeroing the production selected price compiles and fails source price precedence
bits; skipping the first production position compiles and fails source first
position and TPSL guards. Restored focused target passes1/1. Artifacts unchanged.
These outputs are scalar f64/bool, not collections. This is no market-step
precision claim: neither helper reads separate price/amount precision.
Broader cardinality audit remains OPEN; no fresh host/wasm lint sheet claimed.


## Accepted-set and inventory receipt at 10712b8eea

[GATE_RECEIPT_10712b8eea.md](GATE_RECEIPT_10712b8eea.md) binds the scoped
Cargo check, exact real-entrypoint extractor, seven boundary baseline tests,
eight record / ten literal controls and six inventory controls to the measured
code/tooling SHA. Seventeen runtime controls fail by name; the literal
content-guard-bypass survivor is retained as FINDING. Both restored focused
targets pass. The 63/104/364 census and all 13 original findings remain unchanged.
CARDINALITY_AUDIT.md records residual targets; ROW_DISPOSITIONS.json is a
source-file evidence/callee inventory, not per-row mutation proof. No host/wasm
lint, workspace build, crypto suite or complete sweep claim attaches here.


## Five pending inference rows moved at d09515e030

Moved3 pending→ported and2 pending→CHOSEN divergent against integration
2bc83d78d4, independently measured with tomllib:63/104/364 becomes66/106/359.
[GATE_RECEIPT_d09515e030.md](GATE_RECEIPT_d09515e030.md) binds scoped compilation,
extractor, five compiling production mutations and restoration to the measured
code SHA. The old literal survivor remains exit0, now classified by the actual
strict compiler:empty string/null both reject2322; values/messages differ. The
separate accepted unifiedAccount replacement fails the named native assertion
with the content guard bypassed. Source widening fails too. New-row native
fields/bits/enums/errors are directly asserted; full old-row matrices are paused
and the broad sweep remains OPEN. No workspace/lint/wasm/crypto/CI claim.


## Nineteen builder/constants rows moved at 760e602850

Moved19 pending→ported,0→divergent against direct integration parent dab5ce9827;
tomllib measures66/106/359→85/106/340.
[GATE_RECEIPT_760e602850.md](GATE_RECEIPT_760e602850.md) binds scoped check,
extractor check,13 named constant assertion failures, six function/model
controls and restored three-test pass to the measured SHA. The UTF-16 native
configuration retains arbitrary source strings and configured reference
identity. A review-found typed getter gap was fixed with fallible property reads
before commit; five source/native getter cases cover ordering/errors/early
return. No new survivor. Sweep remains OPEN; no workspace/lint/wasm/crypto/CI
claim, and no approval/network/constructor workflow row promoted.

## Fourteen spot table/order-schema rows moved at 730dc7c5ea

Independent tomllib comparison with requested integration parent 7fcfce7950
moves14 pending->ported,0->divergent; census85/106/340->99/106/326.
[GATE_RECEIPT_730dc7c5ea.md](GATE_RECEIPT_730dc7c5ea.md) binds scoped check and
extractor exits0 to the measured code SHA, plus one combined production control
Cargo101 with14named native failures and restored7tests/Cargo0. Source and native
assertions cover8table roots and6closed schema roots/47strict-TypeScript cases,
stopping at zero-based case46. Historical sweep staysOPEN and original findings
remain individually named. Full workspace/host-clippy/wasm/CI were not run.

## Ten approval/cancellation leaves moved at650a576d55

Independent tomllib comparison with requested parentd3e8141626 moves10pending->ported,
0->divergent; census99/106/326->109/106/316.
[GATE_RECEIPT_650a576d55.md](GATE_RECEIPT_650a576d55.md) and
[GATE_EXITS_650a576d55.json](GATE_EXITS_650a576d55.json) bind scopedcheck/extractor0,
one combined production mutationCargo101/10named native failures, and restored
5tests/Cargo0 to the measured code SHA. New floors6constants/50names/18wallets/
18cancellations stop at case49/17/17. Spot schedule preserves null separately from
absence; shared perpetual behavior is unchanged. SweepOPEN; original findings
remain individual. Full workspace/hostclippy/wasm/browser/CI were not run.

Post-merge correction: shared runtime required monotonic_now_ms, so the first
merged scoped check at91d8bd9717 exited101. The test-clock adapter was fixed at
f9080e6afb; scoped check0, five new native tests0, eight exchange-state tests0.
[GATE_MERGE_f9080e6afb.json](GATE_MERGE_f9080e6afb.json) records exact SHAs and
exits. Extractor0 is measured at91d8; original mutation101/restored0 at650a;
explicit unchanged-input diffs both0. No mutation rerun is claimed atf908.

Final measured merge8c887d8268 keeps upstream test timer panic guard and
balance_handle forwarding. Scoped check0, extractor0, approval native5tests0,
exchange-state8tests0. Exact receipt: [GATE_FINAL_8c887d8268.json](GATE_FINAL_8c887d8268.json).
Mutation remains measured at650a; unchanged new-leaf inputs diff0; no rerun claimed.


## Fifty spot raw-model rows — parent ed7ac407c7

Moved50pending->ported,0->divergent; independently measured TOML census159/106/266.
Code SHA db2c7f8a709472a62014e4b5fa5f52a87323e513; code gate/controls:
GATE_RECEIPT_db2c7f8a70.json and GATE_CONTROLS_db2c7f8a70.json.
The grouped UTF-16 mutation hit50named decoded-native root assertions (Cargo101,
restore0). Concrete FrontendOpenOrders isPositionTpsl=true/reduceOnly=false serde
pair swap hit the manual native flag and source native witness (101,restore0).
Both in-memory inherited-field controls hit the named guard (1/1).
No mutation survivor was dropped; this control enumeration ends at those cases.

Merged integration3788c309f1: own src/tests preservation exit0. Shared inputs
changed, so fresh gates at9e930c25b92055983add50c2761994cb3f64a0e0 are attached
in GATE_FINAL_9e930c25b9.json: scoped all-targets check0, native spot_models six
passed/zero failed exit0, extractor0 with zero manifest updates, inventory0.
Native floor50roots/400cases plus5witnesses ends at404. Read-only sub-agent
review found no remaining blocker. Historical cardinality sweep remains OPEN;
13original findings remain individually inventoried. No workspace build,
monorepo suites, host/wasm clippy, CI rerun or live venue requests were run.


## JSON boundary slice — parent6ee57c1e6a

Moved15pending->ported,0->divergent; independent TOML census174/106/251.
Measured code SHA a7a3be959cffb673e18f9c048ddb9af6c1cb7c16. Tracked receipts:
GATE_RECEIPT_a7a3be959c.json and GATE_CONTROLS_a7a3be959c.json.
Scoped all-targets cargo check0, extractor0 with zero manifest updates, generated
models check0 and row inventory0. Native json_boundaries gate3passed/0failed
exit0 after restored mutation. One grouped production mutation hit all8model
and7function named native assertions (101), restored3tests0; both model heritage
entrypoints failed the named guard (1/1). Control enumeration stops there.

Model floor8roots/71cases ends70; actualsource function floor7leaves/215cases
ends214. Assertions read native units, ordered keys, f64 bits, fields/tags,
receiver values and opaque object/error identity before any wire assertion.
Revoked-proxy direct versus parse-try classifier catch boundaries are preserved.
Read-only sub-agent review found no remaining blocker. Snapshot models do not
establish general non-JSON any/prototype/live alias support; JSON/String/Array
intrinsics stay host bindings. Historical sweep stays OPEN, original findings
remain individually named in both receipt and ROW_DISPOSITIONS.json. No workspace
build, monorepo suites, host/wasm clippy, CI rerun or live venue requests run.


JSON boundary post-merge gates at79c6c899bf79011ecb25626cb742e3772ee198fa:
merged integration1273115ab9, own src/tests diff againsta7a3be959c exit0.
Shared inputs changed, so scoped check0, native json_boundaries3passed/0failed
exit0, extractor0/zero updates, generatorcheck0 and inventory0 were freshly run.
Tracked binding: GATE_FINAL_79c6c899bf.json. Independent TOML comparison with the
merged integration retains15pending->ported,0->divergent and174/106/251census.
Unchanged production mutation inputs preserve the original15named-failure
control receipt at its actual measureda7a3be959c SHA, without claiming a rerun.


Error and market helper slice:12 pending to ported,0 divergent;186/106/239 census independently measured against852b13d6c0. CodeSHA c322e6ff15e7aeb6ed568b0a225f8035dd9fe7c9; GATE_RECEIPT_c322e6ff15.json and GATE_CONTROLS_c322e6ff15.json bind scopedcheck0, native2tests0 with275fixed cases through274, extractor0 and inventory0. One grouped12production control hit six native receiver/name arguments and six decoded native values, exit101, restored2tests exit0 with exact bytes. Concrete pairs are retained in receipt; no survivors in this control floor.

After merging reviewed integration93ddd6b77b9ce509b8fa8ee77247a9fbf4dfc472, measured mergeSHA03d198b864399964164135273e8411a0c56ae67f passed scopedcheck0/native2tests0/extractor0/inventory0. GATE_FINAL_03d198b864.json records exits and source/test/script/manifest equality to codeSHA. Final readonly sub-agent review found no remaining blocker and independently confirmed the12row delta. External intrinsic bindings, asyncfetches/live network and exhaustive cardinality remain outside proof; sweep OPEN; original13findings stay named individually. Workspace/monorepo/hostclippy/wasmclippy/CI were not run. This appended evidence child is not itself claimed compiled.


Cross-cache method slice:10pending to ported,0divergent; independent196/106/229 census against4cbcc49350. ProductionSHA788a26b045224a56925f4edd941b75032b54692b; measuredSHAb6e468d17ed432cec69bca952534f5bcda41a9f1. GATE_RECEIPT_b6e468d17e.json records scopedcheck0, native1test0, extractor0 and inventory0. GATE_CONTROLS_b6e468d17e.json retains actual named native arguments/results for ten grouped production controls, exit101, exact-byte restored1test0. Native source corpus stops at80cases through79; no roundtrip/wire gate. FIFO seen state, own payload properties, fresh global/property reads, catch scopes, shallow array copies, pending entry identity and iterator binding failure close are asserted. Class/constructor/singleton and other cache lifecycle remain pending; browser/JS primitives remain supplied bindings. Original findings remain individual; cardinality sweep OPEN. Readonly sub-agent review found no blocker. Workspace/fullvenue/fullmonorepo/hostclippy/wasmclippy/CI/livebrowser/venue not run. First compile-invalid control attempt is retained separately and is not a discrimination gate. Evidence child itself is not claimed compiled.


Cache merged-build verification: initialparent4cbcc49350, integrationmerged65e086a3c9039a79e4c7e9f354aba31d8f83009e; actualmeasuredmergeSHAa89df4fadd848606bed3b9ba65e39525ca84a7fc. GATE_FINAL_a89df4fadd.json binds scopedcheck0/native1test0/extractor0/inventory0 after upstream Cargo.lock/sharedtransport changes. Own src/tests/scripts/manifests equal b6e468d17e (gitdiffexit0); namedcontrols remain measured atb6e468d17e, not rerun onmerge. Native floor80through79; census196/106/229,10pending->ported,0divergent. Evidence child not claimed compiled.


Pending-cache lifecycle/delegation slice:10pending->ported,0divergent,206/106/219 independently measured againstf0cf136efbc17b93fe52369e71342e620dd925ff. Actualcode/gateSHA284a21631d8ef9602f177e530be821177364430a. GATE_RECEIPT_284a21631d.json binds scopedcheck0, extractor0, inventory0, native2tests0. New source floor95cases through94, existing80through79; exact native values/arguments/record states, >60000stale capture/alias, strict assignment/delete and optional undefined caller parameters. GATE_CONTROLS_284a21631d.json retains ten named production failures, groupedmutation101 (oldtestpasses/newtestfails), exact bytesrestored2tests0, concrete pairs and rawoutput; no survivors at this control floor. Readonly sub-agent review found no blocker and independently confirmed census/delta. Browser/JS primitives, existing cache callees/singleton construction and full class composition remain outside this proof. Original findings remain individual, sweep OPEN. Fullvenue/workspace/fullmonorepo/hostclippy/wasmclippy/CI/livebrowser/venue not run. Evidence-only child is not claimed compiled.


Private position-state slice:11pending->ported,0divergent; independent217/106/208census against fresh integrationparentd47e29496d501fa64d798557bf6e2e41902c884a. Code/gateSHAa841b383768af79176f6fb50716866928835ccf3. GATE_RECEIPT_a841b38376.json binds scopedcheck0/extractor0/inventory0 and native1test0; GATE_CONTROLS_a841b38376.json retains groupedproduction101 with eleven named native assertions/concrete pairs, exact-byte restored0. Source floor133through132; no wire/roundtrip gate. Native UTF16/f64/summaryfields/queue/Set/PnLstate and source-order primitive/callee arguments read directly. Full class/handlers remainpending; JS host contracts, standard intrinsics and snapshot limitations explicit. Historical sweepOPEN; original13findings remain individually named. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun. Evidence-only child not claimed compiled.

Private-position merge verification: measuredSHAf67d9e2371fc18e35d90bc0333ba3dd8db34d01b, integrationmerged845cd310a963e53fc50dd9180a152e034262b6b4. Own src/tests/scripts/manifests equal originala841 (diff0); sourceoracle/types unchanged. Resolved481package closure equal, but shared safe-cex-exchange source changed, so fresh scopedcheck0/native1test0/extractor0/inventory0 were run. GATE_FINAL_f67d9e2371.json binds actual exits. Independent531row TOML comparison to actual merged integration still11pending->ported,0divergent,217/106/208. Final readonly sub-agent audit ofa841 found no blocker; reviewed parent-run controls101/restored0, did not run Cargo. Native floor133through132; original controls remain at actuala841SHA, not rerun onmerge. Evidence-only child not claimed compiled; sweepOPEN and originalfindings remainindividual.

Final private-position mergea7d3e632c5a485c53edfbc7a700b9b8f9937a909 includes integration70fa531e1c4a2952ecc651183dab58b6806eb834. Own source/tests/script/manifests diff0 vs originala841; all local resolved compile/oracle inputs and Cargo.lock diff0 vs fresh gateSHAf67. Cargo/native/inventory exits remain measured atf67, not rerun here. Global extractor freshly passed0/zerodrift ata7d after Paradexmanifest changed. Independent TOML delta against final integration remains11pending->ported/0divergent,217/106/208,531rowfloor. GATE_PRESERVATION_a7d3e632c5.json records exact measuredSHAs and no compiled evidence-child claim. SweepOPEN.

Websocket ping/opening slice:10pending->ported,0divergent; independent227/106/198 census against freshparentb5ac6229e2502e48106b0a549da0524583b04627. Code/gateSHAb7d6091802f81fb93e268047d6e2aa3ccf4d04e1. GATE_RECEIPT_b7d6091802.json binds scopedcheck0/extractor0/inventory0/native1test0; GATE_CONTROLS_b7d6091802.json retains groupedproduction101 with all ten named native assertions, exact-byte restored0 and concrete value pairs. Sourcefloor123through122; public12/4/16/16/15 and private9/4/16/16/15 independentcounts. Lexical timer callback body lives in Rust; native fields/closure receiver/timer registration/Set clear/source-order effects and payload one-member method checked directly. Stop fixtures prove clearTimeout call/ID/field effects, not pre-existing timer removal or cancellation primitive implementation. Fullclass/virtualcallees/globaltimerclockJSON primitives/liveoperation remain outside proof. Readonly sub-agent found no blocker. Historical sweepOPEN; originalfindings remain individual. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun. Evidence-only child not claimed compiled.

Ping/opening final merge98b804c4949747a623754be00ce2ca6b2756bf70 includes integrationb5d642aea03ced39a3cc27bb95767b8fd242c3b7. Own src/tests/scripts/manifests diff0 vs gateSHAb7d6091802; all resolved local compileinputs, Cargo.lock and oracleinputs unchanged. Resolved closure481packages/8localcrates independently equal. Cargo/native/control exits remain measured atb7d, not rerun onmerge. Fresh merged extractor0/zerodrift and inventory0 measured at98b8. Independent531row TOML comparison to mergedintegration still10pending->ported/0divergent,227/106/198. GATE_PRESERVATION_98b804c494.json binds exactmeasuredSHAs. Final readonly sub-agent found no blocker, independently confirmed freshparent/census/tenmoves and reviewedrawcontrols; no reviewerCargo. Evidence-only child notclaimedcompiled; historicalsweepOPEN and13originalfindings remainindividuallynamed.

Websocket transport slice:10pending->ported,0divergent; independent237/106/188 census against freshparent3042a36afe21890c7cb5eaa5ba78333f35e5bd40. Production/alltargetscheck/extractor/inventorySHAccc648edc35c1fc53e9608b40ef1eaf1006b4442 (all exits0); exact-name controlSHA15e726d8595f938bdcbc558192f09e04b9eab8fc, groupedproduction101/tennamednativefailures and exact-byte restorednative1test0. Sourcefloor202through201; explicitWsVenue preservesperpapi-ui versusspotapi URLtable selection, freshsocket/callee/handler order, nativefields/listenerrecords/strictassignment/errors/immediaterefreshboolean. Payload'snative method and one-membercount precede stringify/sender checks; no decode/reencode gate. GATE_RECEIPT_15e726d859.json binds exactper-gateSHAs; GATE_CONTROLS_15e726d859.json retains rawnewcontroloutputs. FirstGATE_CONTROLS_ccc648edc3.json retained: checker suffixmatching could conflate public withspot_public/private withspot_private; oldrawoutput independentlycontainedallten directfailures, correctedprefixboundary at15e7 and actualcontrol rerun101/restored0. Source/tests/oracle diff0 betweencccand15; checker-only change, oldcheck exits not relabeled. Readonlysub-agent no blocker; fullclasses/virtualcallees/browser/EventTarget/clock/globalJSON primitives/liveevents/network/exhaustivecardinality unproved. Originalfindings stayindividual; sweepOPEN. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun. Evidence-only child notclaimedcompiled.

Transport final merge5add9039b853418505ff586f81f0c6571dd5ce62 includes integration7703721ff41de7009f91cfe827a003a8b9967e93. Own src/tests/scripts/manifests diff0 vs controlSHA15e7; all compiled/oracleinputs equalcodeccc, resolved481package/8localcrate closure equal. Scopedcheck remains measuredccc; exact-name controls/native remain measured15e7; no compiledgates relabeled asmergemeasured. Freshmerged extractor0/zerodrift andinventory0 measured5add. Independent531rowTOMLcomparison toactualmergparent still10pending->ported/0divergent,237/106/188. GATE_PRESERVATION_5add9039b8.json binds exactmeasurementSHAs. Finalreadonly sub-agent no blocker; exactprefixchecker verified with3negative suffixwitnesses andallten directnative failures inbothrawruns. Checkerfinding/oldrawoutput retained; no survivors at executedcontrolfloor. Evidence-only child notclaimedcompiled; original13findings individual; cardinalitysweepOPEN.

Websocket emission slice:10pending->ported,0divergent; independent247/106/178 census against freshparent71eabb7cb603983d0ae3979f39ab22e1672a2369. Production/alltargetscheck/extractor/inventory/protocolSHAac44ab621b966b8ab93f5e32e7aacfcd641efc04(all exits0); controlSHAcccc3aa3af9e1a690b1e71b880d6cb9e4b470e3e groupedproduction101/tennamednativefailures, exact-byte restorednative1test0. GATE_RECEIPT_cccc3aa3af.json binds eachactualSHA; GATE_CONTROLS_cccc3aa3af.json retains rawoutput and sixnamedoracle controls1/restored0. Sourcefloor118through117; protocolfloor3namedcases. Nativeorderedliteralfieldtrees beforewire/formattercallbackscope/IteratorClose/capturedcoreuser versusfreshspotuser and6/7sourceconstantarrays checked. Sixneworacle findings corrected and retained individually: GetVboxing, unconfiguredthrowtag, valuegetterdouble-read, nextmethodcache, primitive-resultvalidation, returnmethodcache. Independentcore1BTC/spotuser2candle+l2Book/getter1BTCexpectations prevent artificialall-error paths. Firstreturn-cache control invokedmethodinstead of2ndgetter, producedNode1TypeErrorbefore namedassertion; GATE_CONTROL_ATTEMPT_ac44ab621b.json retainedasinvalidcontrol, correctedandfullrerunpassed. Production/test/oracle/protocolbytes unchangedbetweenac44andcccc; oldcheckexits notrelabeled. Fullclasses/productionhostprimitives/virtualcallee/library/intrinsic/liveoperation/exhaustivecardinality unproved. Readonlyreviewno remainingblocker. Original13findings remainindividual; sweepOPEN. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun. Evidence-only child notclaimedcompiled.

Emission final merge1f2804b4f5bb51b818d403b2feb01e8e8541995b includes integration671cdadf56e6ec1c1955795f0976a8cfd0d28d09. Own src/tests/scripts/manifests diff0 vscontrolcccc; all local compiled/oracleinputs equalcodeac44. WholeCargo.lock changedbut resolved481package/8localcrate closure equal; packagejsononlyadds unrelatedKucoin testtarget. Scopedcheck remains measuredac44; production/native/sixoracle controls remain measuredcccc; no relabelasmergemeasured. Freshmerged extractor0/zerodrift/inventory0/protocol0 measured1f2804. Independent531rowTOMLcomparison toactualmergparent still10pending->ported/0divergent,247/106/178. GATE_PRESERVATION_1f2804b4f5.json binds exactmeasurementSHAs. Finalreadonly sub-agent no blocker, independentlyconfirmed realtargets/census/tenmoves and all tenexactnativefailures/sixnamedoracleregressions. Invalidfirstreturncontrol andsixoraclefindings retained individually; no survivors at executedcontrolfloor. Evidence-only child notclaimedcompiled; original13findings individual; cardinalitysweepOPEN.


Account/position leaves slice:10pending->ported,0divergent; independent257/106/168 census against freshparent6182276a697448c400531f8b0ed144be093db509. CodeSHA3f72809c6d43ed02dea8956a42271c1b63062ac9 scopedcheck0/extractor0/zero drift/inventory0. CorrectedcontrolSHA4607e8c49a1a3aefd30cf8fe5a71afbdf2e49934 groupedproduction101/allten exactnamed nativeassertions, bytesrestorednative1test0/script0. GATE_RECEIPT_4607e8c49a.json binds actualSHAs; GATE_CONTROLS_4607e8c49a.json preservesrawoutput. Sourcefloor129through128; independentcounts13/12/20/20/16/3/23/15/3/4. Nativebool/null/UTF16/f64/defaultconfig20-vs1/alias/cache and owned91.25>=17.5predicate asserted before any wire; no roundtrip. FirstGATE_CONTROLS_3f72809c6d.json retained: collateral literal mutation catches unnamed Host::frame exhaustion plus aggregate, onlynine directnamedassertions, scriptexit1; not ten-named gate. Correctedbranchfalse control gives concreteunifiedAccount truevsfalse withnamedresult; source/test/oraclebytesunchanged, oldchecknotrelabeled. Fullclasses/virtualgenerator/intrinsicbindings/general sparse-mutable-reentrant collections/liveoperation/exhaustivebehavior unproved. Historical sweepOPEN; original13findings retainedindividually. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun. Evidence-only childnotclaimedcompiled.

Account/position final merge5476148b6ae983f79bc87b684f91efc1cd337ebd includes integrationd5c0e7d58a7f03c8163680335e7b632ebb43ec86. Ownsrc/tests/scripts/manifests diff0 vscontrol4607; all scopedcompiled/oracleinputs inclCargo.lock unchangedvscode3f7280, independentlyequal481package/8localcrate closure. Scopedcheck staysmeasured3f7280; controls/native staymeasured4607; not rerunorrelabelledonmerge. Freshmergedextractor0/zerodrift andinventory0 at5476148b6ae983f79bc87b684f91efc1cd337ebd. ExactTOML531rowcomparison toactualmergeparent still10pending->ported/0divergent,257/106/168. GATE_PRESERVATION_5476148b6a.json binds actualexits andrawreceipts. Finalreadonlysubagent no blocker, alltenexactdirectnativefailures and91.25/17.5swapbeforeaggregate verified; firstunnamedattemptretained/excluded. Nativefloor129through128; evidence-onlychildnotclaimedcompiled; historicalsweepOPEN, original13findings stayindividuallynamed.

Finaladditionalaccount/positionmerge03d76e637e13fc27fb11e519b201bd4cc018a190 includes reviewedintegration536f757500a6eb348752e23387c098e3e9d9e3b0. Owncompiled/oracle/src/test/script/manifestinputs remainbyteequal toactual3f/4607gateSHAs, diff0;481packageclosure remainsidentical. Freshglobalextractor0/zerodrift andinventory0 measured03d76e637e13fc27fb11e519b201bd4cc018a190; independentactualintegrationparentcomparison remains10pending->ported/0divergent,257/106/168. GATE_PRESERVATION_03d76e637e.json retainsactualSHA bindings, originalcontrolattempt andnativefloor129through128; evidence-onlychildnotclaimedcompiled. SweepOPEN.

Lastaccount/positionmerge0a80b641549a4986ad6cd2e976cfff0bf386f1e9 includes integrationc64ee16769cb5795a9ea4daf8495805d8298c722. All scopedcompiled/oracle/ownsrc/tests/scripts/manifests unchanged fromactualcode3f/control4607 (diff0),481packageclosureequal. Freshglobalextractor0/zerodrift andinventory0 measured0a80b641549a4986ad6cd2e976cfff0bf386f1e9; independent531rowcomparison toactualintegrationparent still10pending->ported/0divergent,257/106/168. GATE_PRESERVATION_0a80b64154.json binds actualexits. Nativefloor129through128; originalunnamedcontrolattempt retained; evidence-onlychildnotclaimedcompiled; sweepOPEN.


Lifecycle/delegates slice:10pending->ported,0divergent; independent267/106/158 census against freshdirectparent1674d852f7687b8d427cd4b77c3aee067688979f. CodeSHA116392b74963c7fc88a84957ccc3ca2e688fa00a scopedalltargetscheck0/extractor0/zerodrift. ControlSHA56dfb2294208d5f29ed8364097a77c1b41671c0b groupedproduction101/allten exactnamednativeassertions, exactbytesrestorednative1test0/script0 andcorrectedinventory0. GATE_RECEIPT_56dfb22942.json andGATE_CONTROLS_56dfb22942.json bindactualSHAs/rawoutputs/concretepairs. Nativefloor111through110 withindependent6/6/6/6/21/7/4/15/5/35counts. Listener2arg/cleanupalias, disposal strictfields/threecollections/fiveeffects/optionalreceiverandcallee guards/superorder, retrycleanup and owneddebounce/rejection callbacks checkeddirectly. Scheduler mutation flips ownedtimeoutpendingfalse->true, detectsinsidecallback ratherthanregistration. Nativebeforewire; no roundtrip. Initialinventoryconsistency0at1163 printed oldliteral257/106/168 despitecorrectJSON/assertions267/106/158; findingatmainprint retainedinreceipt, dynamiccomputedprint fixedat56df andrerun0. No production/test/oracle/control/manifestchanges1163->56df (diff0), originalcompile/extractornotrelabeled. Fullclasses/superimplementation/timer-cancellation-primitives/realPromise asynchronouscomposition/virtualcallees/live/universalbehavior outsideproof. HistoricalsweepOPEN; original13findings stayindividual. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun; evidence-onlychildnotclaimedcompiled.

Lifecycle/delegates finalmergee9dcec16d64570b1f3c76855c5f4cc83fa7de9b2 includes reviewedintegrationa739087a0791b49fb0d6d34a5ac0d631f3ce6130. Ownsrc/tests/scripts/manifests diff0 vscontrol56df; all scopedcompiled/oracleinputs andCargo.lock unchangedvscode1163, independent481resolvedpackage/8localcrate closureequal. Scopedcheck staysmeasured1163; native/controls staymeasured56df; notrerunorrelabelledasmergemeasured. Freshmergedextractor0/zerodrift andinventory0 ate9dcec16d64570b1f3c76855c5f4cc83fa7de9b2. Independent531rowTOMLcomparison toactualintegrationparent still10pending->ported/0divergent,267/106/158. GATE_PRESERVATION_e9dcec16d6.json binds exactactualexits andrawreceipt. Finalreadonlysubagent noblocker, confirmedrealpubfntargets/alltennamedcontrols/ownedtimerassignment; no reviewerCargo. Initialstale inventoryprintedliteral retainedascorrectedreportingfinding; actualcountsassert/JSONwerecorrect. Nativefloor111through110, no fullclasses/realPromiseasynccomposition/live/timerprimitive/universalproof; evidence-onlychildnotclaimedcompiled; historicalsweepOPEN andoriginal13findings remainindividuallynamed.


Market/data selectors slice:10pending->ported,0divergent; independent277/106/148 census against freshdirectparentc0567b4ed712109f72436698411e8b35ed782b33. Actualcode/gateSHA0cc2713d99d61e8871aa9df7260c5d42a557d131 scopedalltargetscheck0/extractor0/zerodrift/inventory0; groupedproduction101/alltenexactnamednativeassertions, newoptional-chainoracleregression101/namedprivatecollector, exactsourceandoraclebytesrestorednative1test0/driver0. GATE_RECEIPT_0cc2713d99.json andGATE_CONTROLS_0cc2713d99.json bindactualexits/rawoutputs. Nativefloor161through160; independentcounts15/17/12/17/16/8/12/20/22/22. NativeMap/Set/memberalias/order andsummary91.25/17.5bits/precisioncore0.0001-vs-spot0.000123 inspectedbeforewire; no roundtrip. Newlyfoundoracle lostsecondoptionalguard market?.info?.hip3Dex corrected; continuousmarket?.precision.amount stillguardsrootonly. GATE_DEVELOPMENT_SELECTORS.json retainsfirstuncommitted101/parent/digest asnon-gate; exactcommittedregression101/restored0 provesnativeassertion. Source regexliteral directlypassedfromAST; nativeprojection avoids.namegetter observation. Global/importedCCXT/responsehelper/coercion/regex/property/iterator/collection implementations/fullclass/live/universalbehavior outsideproof. HistoricalsweepOPEN, originalfindings stayindividual. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/livebrowser/venue notrun; evidence-onlychildnotclaimedcompiled.

Market/data selectors finalmerge941484ea45733410b2865c106d90c2438b5caaf9 includes reviewedintegrationacf29148c3f26a07b72f93d1d964b49e81016ec6. Ownsrc/tests/scripts/manifests diff0 vsgate0cc; all scopedcompiled/oracleinputs (includingactualCCXTnumeric/LogSeverity), Cargo.lock and481resolvedpackage/8localcrate closureunchanged. Scopedcheck/native/tenproductioncontrols/neworacle-regression exits staymeasuredat0cc; notrerunorrelabelledasmergemeasured. Freshmergedextractor0/zerodrift andinventory0 at941484ea45733410b2865c106d90c2438b5caaf9. Independent531rowTOMLcomparison toactualintegrationparent still10pending->ported/0divergent,277/106/148. GATE_PRESERVATION_941484ea45.json bindsactualexits andrawreceipts. Finalreadonlysubagent noblocker, confirmedalltenrealpubfntargets/directnativefailures andoptionalregression/runtime101/restored1test0; no reviewerCargo/extractor. Originaldevelopmentfinding retainedwithuncommittedinputdigest; nativefloor161through160, primitive/library/fullclass/live/universalproof excluded. Evidence-onlychildnotclaimedcompiled; historicalsweepOPEN andoriginalfindings remainindividuallynamed.


Balance-state leaves: **10 pending->ported, 0->divergent**, independently measured against fresh parent a9268fab057cad3e028622f1817d0f834980b07d; census287/106/138. Actual code/gate SHA 62dbfcb7c20f574aec193e04728e990667b237df. GATE_RECEIPT_62dbfcb7c2.json binds scoped all-targets check0, extractor0/zero drift, inventory0, grouped production101 with all ten exact named native assertions, both concrete harness regressions101, and exact-byte restored native1test0/driver0. Raw outputs/pairs in GATE_CONTROLS_62dbfcb7c2.json. Source floor152cases through151, independent counts22/16/17/15/12/9/28/16/8/9. Native scalar/field/prototype/handle/state, iterator completion and owned market predicate assertions precede wire; no roundtrip. GATE_DEVELOPMENT_BALANCE.json retains initial uncommitted101, parent/digests and both named harness findings: omitted ForOf tuple binding and callback execution after pre-callback find rejection. Production unchanged by fixes; both committed regression controls discriminate. Read-only sub-agent no production blocker. Host/intrinsic/virtual-callee implementations, async refresh/full class composition, general sparse/reentrant/prototype/live/universal behavior remain outside proof. Historical sweep OPEN; original13findings remain individually inventoried. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/live not run. Evidence-only child not claimed compiled.


Balance-state final merge 16436a57836d86704a2241d8c9027d1f97e05ac3 includes integration 9fdb2a05ffe14554db00996c29ab70cb5a12db1c. Own src/tests/scripts/manifests and listed compiled/oracle inputs match actual code/gate62dbfcb7c2 (diff0). Whole Cargo.lock changed only outside the independently identical481-package/8localcrate venue closure. Compile/native/production/two-harness control exits remain measured at62db, not rerun or relabelled on merge. Fresh merged extractor0/zero drift and inventory0 measured at16436a57836d86704a2241d8c9027d1f97e05ac3; independent531-row TOML comparison to actual integrationparent still10pending->ported/0divergent,287/106/138. GATE_PRESERVATION_16436a5783.json binds actual SHAs/exits. Final read-only sub-agent found no blocker and independently checked named runtime controls/input and closure equality. Reviewer reporting gap resolved: rawcontrolJSON contains inner Cargo exits; actual outerdriver0 separately recorded in GATE_RECEIPT_62dbfcb7c2.json, evidence child not claimed compiled. Two initial harness findings retained individually with committed regression failures/restored native pass. Source floor152through151; host/intrinsic/virtual-callee/async/fullclass/live/universal limitations explicit. Original13findings remain individually named; historical sweep OPEN.


Order-request leaves: **10 pending->ported,0->divergent**, independently counted vs fresh parentc0f4ba828cc10f574e34896c955784ca383909aa;297/106/128. Compile codeSHAaba23b95e4e96e0ad16cc5189a45b3e29a6216e4 scopedcheck0/extractor0zero drift/inventory0. CorrectedcontrolSHA3671d5c4c4501e61ee39b26e1d7a8c61d58ec0e3 groupedproduction101/allten exactnamednativeassertions; isolatedvault-order regression101/3named; groupedtypeof/counter regressions101/4disjointnamed; exactsource/testbytesrestored/oracleunchanged/native1test0/outerdriver0. GATE_RECEIPT_3671d5c4c4.json and GATE_CONTROLS_3671d5c4c4.json bindactualSHAs/rawoutputs. Original GATE_CONTROLS_aba23b95e4.json retained: groupedmutation compile101unusedfee/no nativeassertion, driver1, genuine vault/harness101s andrestored0; not a ten-root gate. GATE_DEVELOPMENT_ORDER_REQUEST.json retains2compile failures andrealproduction vaultguard allocation-order101 beforefix. Sourcefloor143through142, independentrootcounts12/25/9/12/8/17/15/15/15/15 and94manualawait expectations. Native fields/handles/errors/callarity/state and94pendingonce sourcecutpoints precedewire, no roundtrip. Reportinglabel corrected to7semanticmutations/8anchor replacements only; actual367mutator logic unchanged. First-poll/hostawait body scope: embedding immediately first-polls, arbitrary eagerJS-call vs lazyRustfuture scheduling/Promiseassimilation/intrinsics/imported signing/callee/fullclass/live/universal behavior unproved. Originalfindings remain individuallynamed; historicalsweepOPEN. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/live notrun; evidence-onlychildnotclaimedcompiled.


Order-request final merge 09f70af738a062c853c75f8ddacf656fb5f33905 includes integration f6c865ddd8331c32516f063d90d045761bd2c73b. Ownsrc/tests/manifests diff0 vscontrol3671, listedcompiled/oracleinputs diff0 vscodeABA. Controlscript differs only inunique correctedreportlabel; normalizedequality independentlyverified, allmutator/assertion/runtime logic identical to tested367. All other scripts diff0. WholeCargo.lock changed outside independentlyidentical481package/8localcrate venueclosure. Compilecheck staysmeasuredABA; tenproduction/vault/twoharness/native exits staymeasured367, notrerunorrelabelledasmergemeasured. Freshmerged extractor0/zero drift andinventory0 at09f70af738a062c853c75f8ddacf656fb5f33905; independent531-row comparison toactualintegrationparent still10pending->ported/0divergent,297/106/128. GATE_PRESERVATION_09f70af738.json binds actualSHAs/exits. Finalreadonlysubagent no blocker; actualouterdriver0 separatelyrecorded fromrawinner exits, originalcompile-invalidABA control driver1 retained. Nativefloor143through142/94manualawaits, firstpoll/hostawaitscope andfullasync/signing/callee/class/live/universal limitations explicit. Originalfindings remain individuallynamed; historicalsweepOPEN. Evidence-onlychildnotclaimedcompiled.


Browser approval flows: **11pending->ported,0->divergent**, independently counted vsfresh parentfc23eb289f39c11f1bf30b56f1848254b2a520ac;308/106/117. Actualcode/gateSHA769550d4f2407d0c50f8f57029a19cf3a5134910 scopedalltargetscheck0/extractor0zero drift/inventory0. Groupedproduction101/all11exactnamednativeassertions; four correctedallocation-order regressions101/eightdistinctmodule+caseassertions; exactsrcbytesrestored/test+oracleunchanged/native1test0/outerdriver0. GATE_RECEIPT_769550d4f2.json andGATE_CONTROLS_769550d4f2.json bindactualSHAs/rawexits/pairs. GATE_DEVELOPMENT_BROWSER_APPROVAL.json retains compile101before native, thenproductionnative101 atswitch finalfailure/submitdenial/browsernetworkfailure/walletmismatch; fixedliteralallocation/false statusbefore dynamicfield expressions. Branchlabels prevent onebrowserfailure maskinganother. Sourcefloor134through133, fixedrootcounts10/10/11/18/10/10/16/10/11/10/18 and227manually expectedhostawaits. Native fields/handles/modulechainidentity/r-s-v/type/default/scalar/error/alias/receiver/argument assertions beforeJSONwire; no roundtrip. Body firstpoll/hostawait only: localhelper ordinaryouterJSawait microtask scheduling, eagerJS vslazyRust entry, browser/provider/ethers/fetch/JSON/primitive/sibling/helper implementations, live signing andfullclass/generalbehavior outsideproof. Finalreadonlyreview no blocker; originalfindings individually retained, sweepOPEN. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/live notrun. Evidence-onlychildnotclaimedcompiled.


Browser approval final merge f91de82a32b19b14dfab5f2aa350fe3096a38366 includes integration 3cc94529835dde67ba3e6af513a233baa307c3f0. Ownsrc/tests/scripts/manifests diff0 vsactual769. Resolved481package/8localcrate closureequal, butsharedcrypto sourcechanged (poseidonloader/example), so freshscopedalltargetscheck0/native1test0/extractor0zero drift/inventory0 were measured atf91de82a32b19b14dfab5f2aa350fe3096a38366. Originalelevenproduction/eightallocationcase mutation101s remainmeasured769, notrerunorrelabelledasmergemeasured; source/control/test/oraclebytepreservation verified. Independent531rowcomparison toactualintegrationparent remains11pending->ported/0divergent,308/106/117. GATE_PRESERVATION_f91de82a32.json bindsactualSHAs/exits. Finalreadonlysubagent noownslice blocker, independentlyverified rawcontrol nativefailures/pairs andpreservation/sharedsourcechange; reviewer ran no Cargo/extractor. Sourcefloor134through133/227manualhostawaits, localhelper JSouterawait/microtask andfirstpoll/primitive/provider/ethers/sibling/fullclass/live/signing/general limitations explicit. Developmentcompile101 andfourcorrectedproduction sites retainedindividually. Originalfindings individuallynamed, historicalsweepOPEN. Evidence-onlychildnotclaimedcompiled.


Core websocket dispatch/ticker slice: **10 pending->ported, 0->divergent**, independent Python3.12 tomllib against fresh parent645f6fda62f3fab6787db45ab45688376008b739;318/106/107. CodeSHAed7d6630595cff8592cab0a473a806cf452b9291 scopedalltargetscheck0/extractor0zero drift/inventory0. CorrectedcontrolSHAf4b2ae487c82368f98469765775fe3a0966755ee groupedproduction101/allten directnamed nativefailures, exactbyte restored native1test0/driver0. GATE_RECEIPT_f4b2ae487c.json and GATE_CONTROLS_f4b2ae487c.json bind actualSHAs/rawexits; compiled src/tests/oracle/manifests unchanged between thoseSHAs. Fixed140case floor through139, counts20/24/24/3/8/16/8/14/11/12; all native decoded scalars/UTF16/identity/fields/prototypes/maps/sets/reusedbuffers/ownedfilterpredicate/effects before any supplemental wire. No roundtrip. Original GATE_CONTROLS_ed7d663059.json retained: actualmutation101 caught allten roots, restored0, outerdriver1 because numericassertion messageprefix missed two direct17.5vs91.25 failures; corrected parser and rerun. GATE_DEVELOPMENT_CORE_DISPATCH.json separately retains compile101/lifetime, native101/five keycoercion-order cases fixed, and both individually named reportparser sites. Partition coreonly:8private+2public, five spotprivate WIP rows skipped. Primitive/importeddecoder/virtualcallee/fullclass/live/general/reentrant/exhaustive implementations outsideproof. Read-onlysubagent no remaining source/test blocker; original13findings individually named inreceipt/inventory, historical sweepOPEN. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/live notrun. Evidence-only child not claimedcompiled.


Core dispatch final merge22f9a4d7fcd2aabf1cc2d7ba29f5ef0a87d53e5e includes actualintegration775a019fe06014b8b858b3f8f6cbdabac552abaf; earliermergec69d6dd564249c97680df1d426e3a833fef0166c actualsecondparent0643f8d26dbecd42875c88561dddf96d045874f1. Actual second-parent identity used rather than sharedorigin ref. Ownsrc/tests/scripts/manifests diff0 vsf4; every listed compiled/oracle input inclCargo.lock diff0 vsed7,481package/8localcrate closure unchanged. Compile0 staysmeasureded7; mutation101/allten directnames/restorednative0/driver0 staysmeasuredf4, not rerun or relabelled on merge. Freshmergedextractor0/zero drift andinventory0 measured22f (earlierc69 extractor0/inventory0 also retained). Independent531-row TOML comparison to actualmergedintegration still10pending->ported0divergent,318/106/107. GATE_PRESERVATION_22f9a4d7fc.json recordsactualinputs/exits/rawoutputs and namedfindings; readonlyreview no ownblocker.140case floor through139, intrinsic/importeddecoder/virtualcallee/fullclass/live/general scope limitations explicit; historicalsweepOPEN. Evidence-onlychildnotclaimedcompiled.


Core account snapshot/private-query slice: **10 pending->ported,0->divergent**, independently Python3.12tomllib vsfreshparentd8bc6fc533787b072aa5aaad1ee3b0ee09f8681e;328/106/97. ActualcodeSHAf6adcdc71e89a538f9a449f552d82d5af74723a4 scopedalltargetscheck0/extractor0zero drift/inventory0. ActualcontrolSHA7291921b1db4ad1e534e3704badf0a6e37b60d24 groupedproduction101/allten directnamed nativefailures, isolatedcapturedstore receiver regression101/namedfresh-storecase, exactbyte restorednative1test0/driver0. GATE_RECEIPT_7291921b1d.json and GATE_CONTROLS_7291921b1d.json attachSHAs/exits/rawoutputs; src/tests/oracle/manifests unchanged code->control. Fixed100case floor through99, counts6/12/8/13/10/8/11/12/10/10,50manualawait/adoptionpoints counts4/0/0/11/7/0/8/10/0/10. Ownedspot-entry/symbolpredicates/logsummary, primitive/error/handle/array/prototype/field/state/effect assertions beforeallwire; awaitPendingOnce+no continuation+nativecheckpoint; explicitfoldreturnadoption, no roundtrip. GATE_DEVELOPMENT_ACCOUNT_SNAPSHOTS.json retains generator/oraclefixture/Hostunusedfield/anchor failures; GATE_CONTROL_ATTEMPT_2886dd2c1e.json retains groupedcompile101/no native, actualisolatedreceiver101/restored0/driver1. Controls correctedwithout weakening lint. Source-reviewcapturedreceiver correction has realdifferentstore witness. Firstpoll body/runtimeprimitive/Promiseall/microtask/otherimplicitadoption/imported/virtual/fullclass/live/general limitations explicit. Read-onlyreview no remaining source/test blocker; originalfindings individuallynamed, historicalsweepOPEN. Fullvenue/workspace/monorepo/hostclippy/wasmclippy/CI/live notrun. Coreexchangeonlypartition; evidence-onlychildnotclaimedcompiled.


Account snapshot finalmerge5528c6aa4effe1469d950ae086478235af67f354 includes actualintegrationa07e971bb08f867925ff18ca8b13d2f288d0deba. Wholecrate src/tests diff1 vsf6: separatecachelane8rows/module/tests plussharedexchange/transport/Cargo inputs changed. Ownnewbody/test/oracle/control/coremanifestdiff0 vs729. Therefore freshscopedcheck0/native1test0/extractor0zero drift/inventory0 measured18a5bff59f559a61ce19d79e33cbabc8309e6600. Originaltenproductionmutation101/isolatedreceiver101/native25vs20/restored0/outerdriver0 remainmeasured729, notrerunorrelabelledonmerge. Independently531TOMLrows vsactualmergedintegration still10pending->ported0divergent,combined336/106/89; incomingeightcacheports notours. GATE_PRESERVATION_18a5bff59f.json bindsactualSHAs/rawoutputs/inputdiff/limits/namedfindings. Inventorycache-source evidence sentence refreshedafter18a tocorrectobsoletepending wording, independentinventorycheck0 beforeevidencechild; no compiledinputchange. Fixed100case floor through99/50manualawaits; firstpoll/runtime/primitive/Promiseall/otherimplicitadoption/imported/virtual/fullclass/live/general limitations retained. Read-onlyreview no blocker; historicalsweepOPEN, originalfindings individuallynamed. Evidence-onlychildnotclaimedcompiled.


Finalaccount pushmergeab74cc0148b907d55b0e7c38f8f884dfe82ad1f6 includes actualintegration7d73334d51aab93a006d732997ec203cec61ffbc. Exactscoped compiled/oracleinputs inclCargo.lock diff0 vsfresh18a, independentlyidentical481package/8localcrate closure; onlyBitrue files/manifestSpec added. Scopedcheck0/native1test0 remainmeasured18a, controls101/allten+receiver101/native25vs20/restored0/driver0 remainmeasured729. Freshextractor0/zero drift/inventory0 measuredab74. Independent531rowTOML comparison toactualintegration still10pending->ported0divergent,combined336/106/89 (eightcacheports incoming, notours). GATE_PRESERVATION_ab74cc0148.json bindsactualoutputs/inputproof/SHAs/limits/namedfindings. HistoricalsweepOPEN,100case floor through99/50awaits; explicitfirstpoll/runtime/primitive/otherimplicitadoption/fullclass/live limitations. Evidence-onlychildnotclaimedcompiled.


Additionalfinalaccount mergeb6795501cb7e062a3355045c001fe323b9bfe281 includes actualintegration970c8a3a41e3ff7e0da31cbf524f7d6b0e064849. Exactlisted scopedcompiled/oracle/Cargo.lock inputs diff0 vsfresh18a, preserving481package/8localcrate closure; newchanges onlyotherBitget/Kucoin/Polymarket andmanifest/exemption/package scriptmetadata. Check0/native1test0 remainmeasured18a; tenmutation101/receiver101/25vs20/restored0/driver0 remainmeasured729. Freshextractor0zero drift/inventory0 measuredb679. Independent531rowTOML comparison toactualintegration still10pending->ported0divergent,combined336/106/89. GATE_PRESERVATION_b6795501cb.json recordsactualSHA/exits/rawoutputs/inputproof. 100case floor through99/50manualawaits, limitations/namedfindings retained; historicalsweepOPEN; evidence-onlychildnotclaimedcompiled.

Position/order ingestion slice: fresh parent61d416782d630e0058f93a515e81fafc9be87187; ten pending->ported, zero->divergent from independent TOML comparison. Four core exchange roots and six core private websocket roots; no spot/income/cache/startup rows claimed. Code/check/extractor/inventory SHA7aa44dd2bd08c09de03b788cd34853615ad8688d; scoped all-targets check0, extractor0 with zero drift, row inventory0 (346/106/79;452implemented/531).

Native and corrected grouped controls measured at9c92e9796e31be925f13861a10b907822df6e026: mutation runtime101, all ten direct named native assertion families, restored1test0, exact source bytes restored. Guard regression case handleHip3ClearinghouseState normalized distinguishes Undefined from Handle57(map,object). Retained compile-only e78 control attempt (unused label) separately; it is not a native failure gate. Check/oracle compiled inputs unchanged from7aa across the control-script/evidence children (git diff exit0).

Floor:106actualsource cases through105, counts10/10/10/12/16/8/8/12/10/10, nine manually expected await suspensions;13owned callbacks. Native values, fields, binary64 bits, UTF16, aliases, receivers, state and callback outcomes precede wire checks. Library primitives/importedhelpers/virtualcallee fixtures are contracts, not intrinsic implementations. Prototype/reentrant/sparse/implicitthenable/generalasync/fullclass/live behavior unenumerated. Rust late initialization skips three dead primitive source initial values in order inference; no effect is lost. Original cardinality sweep stays OPEN, and each originally named finding remains individually inventoried in ROW_DISPOSITIONS.json.

Read-only subagent review verified source semantics, optional-call correction and ten real targets/TOMLmoves. Not run: fullvenue/workspace/monorepo suites/builds, hostclippy, wasmclippy, CI reruns or live requests. Merge preservation and any fresh post-merge gates are recorded separately.

Fresh post-merge gates at faa1d6bdf1984e60914ce7abd635d137507d62b6: scoped cargo check all-targets0, native core_position_order_state1test0, extractor0 with zero drift, inventory0. Whole Hyperliquid354/106/71, of531; own ten rows versus actual merge second-parentf364cee096ff92584c157cad8b96f12b92cf3a09. Own inputs/controlsource unchanged from9c, while incomingotherlane cratechanges prompted fresh positives. Otherlane rest_helpers runtime suite not run by this lane. Async body proof starts on firstpoll; caller task/eager scheduling and cancellation ownership are outside this isolated body witness.

Final merge19268d2838f3be9642f96698be09f89c7629fb22 includes actualintegration8a98237aa1ea496db3b0e16bafedcce0bfd06fc4. Own moves remain10pending->ported/0->divergent against that exact secondparent; three owned files have37pending rows. All scoped Cargo/native/oracle inputs, Cargo.lock/workspace/package inputs and8localdependency trees diff byte-unchanged fromfaa (exit0). Conservative locked graph481packages/8locals stopped when worklist emptied at pop1390 (no feature-pruning proof). Fresh finalmerge extractor0/zero drift and rowinventory0. No Cargo/control rerun at this finalmerge: positivecheck/native stay measuredfaa, controlsstay9c. Evidence-only child/push SHA changes none of those inputs.

REST discovery/history slice: ten pending->ported, zero->divergent, all core exchange roots. Fresh parent51e9d74873a1c863a42569e7dfeb95cbc2da40bd; no unreviewed slice ancestor. Code/check/extractor/inventory measured df2244165d6f137d6b56b5200286659e0e036a7d: scoped all-targets check0, extractor0/zero drift, inventory0 (364/106/61). GATE_RECEIPT_df2244165d.json records raw output and independent TOML transitions.

Native/control SHA25b2069e91fa9e296ff9286d78019bca3c2a4669 fixes a vacuous literal await equality into measured native total108. Ten grouped production mutations compile then runtime101 with ten direct named native assertions; isolated cancellation-after-observer-drop runtime101 with three direct missing settled-result assertions; exact-byte restored native1test0, driver0. GATE_CONTROLS_25b2069e91.json retains all outputs. No surviving new control. Development build/oracle/adapter failures retained separately in GATE_DEVELOPMENT_CORE_REST_READS.json, not relabeled gates.

Floor94source cases through93/101 independently expected awaits;97native replays including three dropped-root witnesses through2/108 measured awaits;27owned callbacks. All decoded native field/value/handle/prototype/receiver/callback/task/settlement assertions precede wire. New asynchronous entry and local async callbacks start eagerly with owned tasks independent of observer lifetime; three finite drop witnesses do not prove a universal scheduler. Intrinsic/library/imported income/startup/history/dayjs/precision and virtual callee fixtures are contracts. General Promise/proxy/reentrant/sparse/fullclass/live behavior remains outside finite proof. Original cardinality sweep OPEN; thirteen original FINDINGs retained individually in ROW_DISPOSITIONS.json.

Read-only subagent reviewed all ten bodies and direct failure families. Not run: otherlane spot/rest_helpers targets, fullvenue/workspace/monorepo suites/builds, hostclippy, wasmclippy, CI reruns, live requests. Fresh merged positives and integration preservation will be recorded separately. Evidence-only children are not claimed compiled.

Fresh merged check0/native1test0/extractor0 zero drift/inventory0 measured c667de615d3c46f8ea942b2e5524011ad25ca87a, incorporating actualintegration65cdf874589742c3ce300e77c5081a1f9242c6b5 via2117604aa95976a84baf78209db3be5c758eb59a. Both moduleexports retained. Own ten moves unchanged; wholeHyperliquid382/106/43, own27pending. Original controls remain measured25b, ownsource/test/scripts/sourceTS diff0 across merge. Outer controlsdriver exit0 explicitly measured. Incoming spot suite receipts cited only, not run by this lane. GATE_RECEIPT_c667de615d.json holds exactSHA/output/floors/limits/notruns.

Final merge c8d7dc3260d6bc666a7583e5bebd3c80973873fd includes actualintegration a553ce92e78a0559194e953f569bf4fe42e6db8c. Exact listed scoped Cargo/native/oracle/Cargo.lock inputs byte-unchanged from c667 (diff0), conservatively481lockedpackages/8locals, worklist stopped empty at1390pops. Fresh final extractor0/zero drift and inventory0. TOML own10pending->ported/0->divergent;27pending across three owned files, combined382/106/43 of531. No fresh Cargo/control at final merge: positives remain measured c667, controls25b. GATE_PRESERVATION_c8d7dc3260.json records SHA/inputproof/counts/exits. Documentation-only child preserves inputs and is not claimed compiled. Original sweep OPEN; finite proof limitations/named findings/notruns retained.


## Startup timeout body — code d2b383d734238a02e3f4256901dbd60d4a8833af

Fresh parent actually built: ac0f5fccf24207b26c279e67da2a88f2c72a7aae.
No unreviewed Bitget or Bybit ancestor. Tomllib census over venue manifests:
382 ported /106 divergent /43 pending -> 383 /106 /42; one pending-to-ported,
zero pending-to-divergent. This is body parity with explicit native runtime
bindings, not concrete exchange/runtime assembly. Canonical declarations remain
in [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy) and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Measured at that exact code SHA: native/source startup_timeout target 0;
scoped Hyperliquid cargo check --all-targets --jobs 2 0; strict Clippy for
--test startup_timeout --jobs 2 0; extractor check 0 with zero changed, added
or orphaned symbols; scoped production/test rustfmt check 0. Extractor writer
at the initial implementation checkpoint also reported zero changes.

Three production controls compile and fail at 101: race-input swap names
`timeout native race first original operation`; denominator 1000-to-999 names
`timeout native divide denominator bits`; cleanup omission names
`timeout independent final effect order operation`. Restored target exits 0.
Three source controls exit 1: empty/subset loops name
`independent completed timeout source count`; actual TypeScript race-input swap
fails earlier at `source eager prefix order`, since the input assertion throws
inside the source try/finally and synchronous cleanup changes that trace.
Restored source exits 0. All source bytes restored exactly.

SURVIVOR FINDING, fixed: before the actual-completion counter was introduced,
emptying the source scene loop still exited 0 at the oracle completion JSON
call site because that JSON used literal counts. This was reproduced, retained
in STARTUP_TIMEOUT_MUTATION_EVIDENCE.json and repaired; the corrected empty-loop
control exits 1. Sub-agent review also found an inspection-only race-order gap
at 01c706673a; d2b383d734 adds native ordered input pointers and the compiling
swap control above proves discrimination. Eight native and actual-source scenes
stop at index7/drop-fallback; three Rust and three source mutations are floors,
not complete temporal or coercion enumeration. Native values, error/return
identities, arguments and effects are asserted before source wire counts.

Additional parity-check exits 1 before correspondence checks, naming existing
safe-cex-webworker/src/chaser_manager.rs thread_local expansion. That input blob
95fc38286934e5a249f2da9d791ef35d954647f4 and the checker have scoped diff exit 0
against ac0f5fccf2. No baseline checker execution or completed correspondence
proof is claimed. This lane does not alter that other crate or weaken the gate.

Adversarial review by /root/account_read_review is posted on PR7869, with the
exact reviewed code SHA and the repaired proof gap. Reviewer ran no Cargo or
mutations; root ran the controls above. Not run: full Hyperliquid suite,
workspace compilation/tests, wasm checks/Clippy, CI or live venue calls.

### Timeout manifest registration follow-up

Metadata-only c01f7b11e8f84eb8ead0bff2a8f6c4c9a3e877f4 reconciles the mapped
Rust-file inventory through the existing manifest parser/serializer. All symbol
annotations compare equal before/after; each mapped source exists. This also
retains the earlier venue's existing targets instead of replacing its rows.
Extractor writer0 and check0, zero changed/added/orphaned symbols. Source/test/TS
and exemption diff against measured code d2b383d734 exits0. No Cargo gate was
rerun for this metadata-only change, and no new correspondence pass is claimed.
## Primary account bootstrap — code f4153d643fa712d517fdea271a6e73c31a8b827a

Fresh integration parent actually built: ac0f5fccf24207b26c279e67da2a88f2c72a7aae.
No unmerged timeout, Bitget or Bybit slice is an ancestor. Three source bodies
move pending-to-ported: resolveUserAbstractionMode, refreshUnifiedSpotCollateralBalance,
resolveApplicationIdFromKey. Independent tomllib census: 382 ported /106 divergent
/43 pending -> 385 /106 /40; zero pending-to-divergent. The timeout slice is
separate PR7869 and is not counted as landed here. Canonical declarations:
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Measured at the exact code SHA: native/source startup_account_resolution target0;
scoped cargo check --all-targets --jobs 2 exits0; strict Clippy for that target
exits0; extractor check exits0 with zero changed/added/orphaned symbols;
scoped production/test/support rustfmt check exits0. Forty-one completed native
and actual-source cases stop at index40, refresh/sync-throw. Native values,
original error/options/data/flight handles, timestamps, diagnostic channels,
arguments and state effects precede source wire counts. Mode/role/refresh case
floors are 12/15/14; no complete coercion or temporal enumeration is claimed.
Methods return void, so collection-output extra/empty controls are inapplicable;
corpus empty/subset controls instead check actual executed scene counts.

Four production Rust controls compiled and exited101:
allowed-mode removal -> BOOT native abstraction mode assignment;
wrong role message -> BOOT native exact diagnostic message;
cleanup omission -> BOOT native finally clears collateral flight;
applicationId fallback suppression -> BOOT native eager post before observer
poll mode application-only. Restored target exits0. Five source controls exit1:
actual-source allowed-mode removal, role-message corruption and cleanup omission
fail their named source value/effect assertions; empty and subset loops fail
independent completed source bootstrap floor. Restored source exits0.
No surviving control or compiler rejection within these four/five-control floors;
see STARTUP_ACCOUNT_MUTATION_EVIDENCE.json. Source bytes restored exactly.
An initial preservation read overlapped a deliberate subset source mutation;
the post-restoration preservation check separately exited0.

Metadata-only checkpoint f9a687412f1e3c3f471c60564568a5ea2c7ea66b reconciles
[file].rust_files from existing mapped targets using the official serializer,
asserting every symbol row annotation is preserved. Source/test/TypeScript diff
against f4153d643f exits0. Extractor check at f9a687412f exits0 with zero changes,
additions or orphans. This registration repair does not transfer compilation
measurements to a different code snapshot.

Sub-agent /root/bitget_findings_audit reviewed f4153d643f; findings and root controls
are posted on PR7873. The review repaired the type-getter/key-read order, Clone
bounds and missing diagnostic/master-wallet discriminators. No further production
defect found. Bounded residual: native standard/empty unexpected-mode first
warning asserts text; the source oracle asserts its full channel/severity, while
native catch/final diagnostics assert severity elsewhere. Reviewer ran no Cargo
or mutations. Concrete class/runtime assembly and exhaustive schedules remain
unproved. The existing SpotAccountHost is reused, not a new native value model.

Not run: full Hyperliquid suite, crate-wide Clippy, workspace compilation/tests,
wasm checks/Clippy, CI or live venue calls. Global parity-check was not rerun:
the separate timeout slice measured exit1 before correspondence checks on the
unchanged webworker thread_local expansion. No correspondence pass is claimed.
## Primary startup and tick — 2026-10-15

Code and gate SHA: `9d1f8250a73a3f57acc9e5607db18044da1f0510`. Actual integration parent: `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`; fresh branch with no unmerged owner-slice ancestor. Pushed before gates. The other startup/account-bootstrap PRs are separate and are not counted as landed on this branch.

Tomllib census over all Hyperliquid manifests: parent 382 ported / 106 divergent / 43 pending; this slice 384 / 106 / 41. Exactly two pending-to-ported changes: primary `start` and `tick`. Existing annotations reconciled with the official manifest serializer; extractor writer preserved zero added/orphaned symbols. Rust-file inventory derives from actual mapped targets, preserving every row annotation. Callback bodies are mapped in both rows; only native capture/allocation plumbing is exempted. Canonical contracts: [owned snapshots and live aliases](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).

Measured exits at the code SHA:

- Scoped `cargo check -p safe-cex-venue-hyperliquid --all-targets --jobs 2`: **0**.
- `cargo test -p safe-cex-venue-hyperliquid --test startup_lifecycle --jobs 2 -- --nocapture`: restored **0**, one test, zero ignored; 29 source-paired cases =17 start +12 tick, stopped at index28 tick/fire-replacement. Seven additional direct native callback invocations are distinct from that source floor. All native assertions precede source comparisons; no native encoding roundtrip is used.
- `cargo clippy -p safe-cex-venue-hyperliquid --test startup_lifecycle --jobs 2 -- -D warnings`: **0**, targeted test only.
- `parity:extract:check`: **0**, output reports zero changes/additions/orphans. Writer output likewise zero changes.
- Scoped rustfmt check for the new source/test/support and changed lib root: **0**. Source/test/assigned TypeScript restoration diff: **0**.

`STARTUP_LIFECYCLE_MUTATION_EVIDENCE.json` records four actual compiled Rust controls, each exit **101** at a named native assertion: unconditional private-loaded flag; reversed order comparison; null/undefined callback return swap; lost core callback registration. Four source controls exited **1** at named assertions: unconditional private-loaded flag; inclusive order boundary; empty source iteration; start-only subset. Restored native and source runs exited **0**. No compiler rejection was counted as a control, and no survivor was dropped. The legacy native control run label says inclusive, but its recorded literal mutation is `>` to `<`; the source inclusive control is `>` to `>=`.

Initial fixture compilation at 806fa8 failed (unused imports and unnamed constructor forwarding); fixed at24f0a20. An initial independent expectation incorrectly included store update after synchronous mode disposal; actual native failure101 led to correction at4a031b9. These were implementation iterations, not passing controls. Review then found fabricated dropped-observer return, conditional core-count proof and missing independent warning-value assertions. At the measured SHA, actual adopted outer settlement replaces the fabricated result; callback count/arity/complete flag/native handles and all seven rejection warning prefixes are independently checked.

Adversarial sub-agent review: `/root/account_read_review`, exact code SHA above; no remaining concrete blocker within the stated host scope after repairs. Reviewer ran static read/diff checks only; Cargo, oracle, extractor and controls were run by root.

Not run: full Hyperliquid suite, workspace build, workspace/host-wide clippy, wasm checks/clippy, CI reruns, or live exchange calls. Global parity-check was not rerun: the previously reported shared webworker `thread_local` indexing failure remains an unproved global gate. Native primitive/Promise adoption and virtual callee implementations are explicit host bindings. Concrete exchange assembly and exhaustive getter/coercion/scheduling behavior remain outside this 29-case floor.

Parent-label correction: the initial evidence commit copied the earlier ac0 baseline label. Direct `git rev-parse 806fa8bf46^` measures the actual parent as1a43f3a8; it descends fromac0 through merged CI work. Scoped crate/manifest/assigned TypeScript diff between those integration commits exits0; no gate exit was transferred between code SHAs. The code gates above remain attached to9d1f8250a7.
## Primary market bootstrap — 2026-10-15

Measured code SHA: `06c7fdcea56f18204b4787416ad8f261326462db`. Direct parent measurement of initial slice commit64460e0c90: `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`, the actual integration parent. Fresh branch, no unmerged owner-slice ancestor. Pushed before gates.

Tomllib census: parent382 ported /106 divergent /43 pending; this branch383 /106 /42. Exactly one pending-to-ported row: `HyperliquidExchange#fetchMarketsAndTickers`. Other prepared startup/account/timeout PRs are siblings and are not counted as merged here. Official manifest serializer reconciled this row and derived14 mapped Rust files without regenerating annotations. Parent body and owned callback dispatcher are mapped; only native callback allocation, array binding/spread and iterator plumbing are exempted. Canonical contracts are cited by [owned snapshot/live alias anchor](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and [divergence taxonomy anchor](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

At the measured code SHA, after restoring mutations and checking source/test/assigned-TS bytes (diff0), the sequential final gate run measured:

- Required scoped cargo check, `-p safe-cex-venue-hyperliquid --all-targets --jobs 2`: **0**.
- Scoped `cargo test --test market_bootstrap --jobs 2 -- --nocapture`: **0**, one test, zero ignored. Native/source floor24 stops at index23 `body-and-close-error`; five direct native callbacks are additional. Distinct market/ticker values and handles, helper clients/meta/assets/fees, loaded names, budget/cap/floor, completion counts, original opaque error object/full UTF16 fields, DEX iterator closing and dropped-observer actual adopted settlement precede source comparison. No native roundtrip is used.
- Strict targeted clippy `--test market_bootstrap --jobs 2 -- -D warnings`: **0**, not full-crate/workspace clippy.
- Extractor check: **0**, output zero manifest updates/additions/orphans; writer likewise zero updates/additions/orphans. The final check was rerun after source controls restored bytes.
- Scoped rustfmt check (new source/test/support and changed lib root): **0**.

`MARKET_BOOTSTRAP_MUTATION_EVIDENCE.json` attaches exact exits and named assertions. Five actual compiled Rust mutations each exited **101**: markets/tickers swap; failed names published as loaded; uncapped budget; missing iterator close; wrong normal-close protocol replacing original-throw preservation. The last fails the native original-object identity assertion when a distinct iterator-return failure would replace the body failure. Three source controls each exited **1**: actual source market/ticker swap; empty iteration; one-case subset. Restored native/source **0**. No compiler rejection was counted as a control, no mutation survivor was omitted.

Adversarial sub-agent review `/root/bitget_findings_audit` at exact measured SHA: no remaining blocker within declared scope. Review exposed string-only identity overclaim, then a normal-close fixture entrypoint that swallowed the distinct return error. Both were repaired, and the actual wrong-protocol control now fails the named native assertion. Reviewer ran static reads only; root ran runtime gates and mutations. Early fixture iterations separately failed compilation for unnamed constructor forwarding, Node syntax for a missing brace, a RefCell logging borrow, and a stale independent count22 after adding two scenes; these are implementation failures, not passing controls.

Scope limits: native fixture Promise.all awaits supplied immediately settled roots sequentially, so this floor does not establish fail-fast/concurrent-rejection scheduling. DEX for-of close paths are exercised; destructuring and argument-spread iterator failure domains remain unproved. Native intrinsics, Promise primitives and virtual helpers remain explicit bindings; concrete class assembly and broader getters/coercion/cardinality schedules remain outside proof.

Not run: full Hyperliquid suite, workspace compilation/clippy, wasm checks/clippy, CI reruns or live exchange calls. Global parity-check was not rerun; the previously reported shared webworker thread_local indexing failure is not represented as passing.
Primary opt-in spot-market bootstrap: 1 pending->ported, 0->divergent, independently measured TOML census 382/106/43 -> 383/106/42 against actual parent 1a43f3a8f25b7e5fe1c238794e79bad11e4c4788. No unmerged sibling slice is an ancestor. Code/gate SHA 1fee5377e6ffc175487d81fc5ac9698fb53ac796: scoped all-targets check0, targeted spot_market_bootstrap native/source test0 (one test), targeted strict clippy0, extractor check0/zero updates, scoped source/test/support rustfmt0 and lib-root rustfmt0. Writer exit0/zero updates also observed; final restored source/test/assigned-TS diff0. SPOT_MARKET_BOOTSTRAP_MUTATION_EVIDENCE.json binds the actual exits to their SHAs.

Native production controls remain measured at cc82de8a9c8dce4acd2585595d8c8e0b837d9d5f: six compiled runtime101 failures at named native assertions, restored target0. The three Rust source/test/support files are byte-identical through 1fee (scoped diff0); the CJS oracle changed, so no full-test-tree preservation or transferred control exit is claimed. Final source controls measured at 1fee: five exit1 failures (early token/asset key conversion, wrong token-name member, empty/subset source), restored0. The original name->index production mutation SURVIVED with exit0 at 7dc46b1d01825b550d439185639dcde2e8fd3b04: Set cardinality ignored required members at tokenNames.add(meta.tokens[i].name). Retained as a resolved finding, repaired at 2c0529426c26de7d53717352bda8cfceb15e4711 with exact native Set members. Source wrong-member assertion throws inside the parent's catch; the independent ordinary market floor catches the resulting empty output.

Floor24 source-paired scenes through index23 late-helper-error. Native independent IDs/counts/fields, selected helper tuples, UTF16 keys, exact Set members, original context handles, RHS-before-key competing errors and partial-result discard precede supplemental source comparison. Native helpers and runtime operations are explicit bindings, not implementation proofs. Concrete class assembly, general prototype/getter/coercion domains, broad Promise scheduling and live behavior remain outside this finite floor. Canonical contracts: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). Read-only sub-agent reviewed exact 1fee, independently ran the committed source oracle exit0/24 cases, and found no blocker within that scope; no reviewer Cargo/clippy/extractor/mutation run claimed. Not run: full crate/workspace tests or builds, all-targets clippy, wasm, CI reruns, live requests or global parity-check. Evidence-only leaf not claimed compiled.
HIP-3 position refresh triggers: **2 pending->ported,0->divergent**, independent tomllib census382/106/43 ->384/106/41 against actual integration parent0445a753756f197e4363222eab9e787868e4f700; no unmerged sibling slice ancestor. Exact code/gate/control SHA cb5fd32f47cf59637d109936ae73d2c4894a1ab4: scoped all-targets check0, targeted hip3_position_refresh native/source test0 (one test), targeted strict clippy0, extractor check0/zero updates, scoped rustfmt0 and lib-root rustfmt0; source/test/assigned-TS restoration diff0. Writer exit0/zero updates observed. HIP3_POSITION_REFRESH_MUTATION_EVIDENCE.json records actual SHAs/exits and independent row moves.

Seven actual compiled production controls each failed101 at named native assertions: timestamp, disposed refresh, clear-after-refresh, returned child error, omitted timer reset, cancelling zero timer and omitted iterator close. Exact restored target exit0. Four source controls failed1 at named assertions: returned logger value, omitted timer reset, empty/subset source; restored0. No survivor in this matrix. Development compile/harness failures and review findings retained separately: unused helper/imports/missing binding/statement error; wrapper map lost owned callback dispatch (observed runtime101 at4c1b5916fc); reject-one expected array while logger throw yields handled undefined. Opaque logger and refresh return values now discriminate both implicit-void callbacks.

Floor22 source-paired scenes through21 set-error. Native Set discriminants/exact member sequence/identity, original errors, full UTF16 warnings, timestamp/timer values, receiver and clear/reset observations, independent effect counts and callback returns all precede source launch/comparison. Output collection-extra/empty controls are inapplicable to these void methods; input Set and source-loop floors remain tested. The test Set iterator snapshots entries and Promise.all fixture sequentially awaits immediate inputs; no live-addition or fail-fast scheduling proof. Native intrinsic/virtual-callee bindings, arbitrary getter replacements and concrete class assembly remain outside this floor. Canonical contracts: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). Final sub-agent reviewed exactcb5, independently ran pinned committed source oracle0/22cases/last21 and tomllib census0 with exacttwo moves/six native-binding exemptions; no remaining blocker within stated scope. Reviewer ran no Cargo/native/clippy/extractor/Rust mutations. Not run: full crate/workspace runtime suites/builds, all-targets clippy, wasm, CI reruns, live requests or global parity-check. Evidence-only leaf not claimed compiled.

HIP-3 final sub-agent review citation: [exact cb5 review](https://github.com/Tealstreet/tealstreet-next/pull/7901#issuecomment-6185582047).
Primary open-order reads: **3 pending->ported,0->divergent**, independent tomllib382/106/43 ->385/106/40 against actual parent0445a753756f197e4363222eab9e787868e4f700; no unmerged sibling ancestor. Exact code/gate/control SHA7b112f2ae3d5de31347cf0f9b471612c4ddd39a0: scoped all-targets check0, targeted order_reads native/source test0 (one test), targeted strict clippy0, extractor check0/zero updates, scoped source/test/support rustfmt0 and lib-root rustfmt0. Restored source/test/assigned-TS diff0; writer0/zero updates observed. ORDER_READ_MUTATION_EVIDENCE.json binds exact SHAs, actual exits, independent moves, control changes and original findings.

One compiled production control group failed101 at four disjoint named native assertion families: raw-versus-parsed ID cleanup, snapshot moved before REST awaits, unconditional replacement-flight cleanup, and refresh returning its resolved value. The last change fails BOTH fulfilled and actual drop-observer native scenes. Four simultaneous changes are one run, not four isolated mutation runs. Exact restored test0; outer control driver0. Five isolated source controls1 (raw IDs, replacement guard, void return, empty/subset loops); restored0. No survivor in this matrix. Original static review finding atdaa446915c tests/order_reads.rs::native_order_reads_before_source(refresh/drop-observer) fabricated Ok(undefined), and fetch/drop lacked independent completion count. Repaired to actual native result/count1 before positive gate; no old measured survival claimed. Original fixture compile101, borrow/missing-string-binding101 and iterator-handle-comparison101 retained separately, not production failures or controls.

Floor38 source-paired scenes through37 refresh/drop-observer, independent24/10/4 root counts. Native exact returned/action/raw-ID/request/log floors, original first-wins and update handles, unchanged store alias, full UTF16 log values, distinct finally errors, original replacement response, iterator-close boundaries and actual retained task settlements precede source launch. Fresh per-scene failures are collected and asserted empty before source comparisons; failed scenes cannot silently disappear. Fixtures bind intrinsic/virtual-callee contracts; live Map/Set additions, general getter/proxy/coercion domains, GC/fullPromise scheduling and concrete exchange assembly remain outside this floor. Promise handles retained for inspection provide no GC proof. Canonical contracts: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). Final sub-agent review of exact7b112 found no blocker within scope; independently pinned oracle0/38cases/last37, production diff0 vs7380a17482, tomllib census0/exactthree moves/nine real bindings. Reviewer ran no Cargo/native/clippy/extractor/mutations. Not run: isolated native changes, full crate/workspace runtime suites/builds, all-targets clippy, wasm, CI reruns, live requests or global parity-check. Evidence-only leaf not claimed compiled.

Final sub-agent review citation: [exact 7b112 review](https://github.com/Tealstreet/tealstreet-next/pull/7905#issuecomment-6185845331).
Coalesced HIP-3 order retries: **2 pending->ported,0->divergent**, independent tomllib382/106/43 ->384/106/41 against actual parent0445a753756f197e4363222eab9e787868e4f700; no unmerged sibling ancestor. Exact code/gate/control SHA1e20d8f7f08bfd48f073c2e189965531c4459edf: scoped all-targets check0, targeted order_retry native/source test0 (one test), targeted strict clippy0, extractor check0/zero updates, scoped source/test/support rustfmt0 and lib-root rustfmt0. Restoration source/test/assigned-TS diff0; writer0/zero updates observed. Initial all-targets check0 at9a037f65ff predates the new test and is not substituted for the final check. ORDER_RETRY_MUTATION_EVIDENCE.json records actual SHAs/exits and independent moves.

One compiled group of three production changes failed101 at three disjoint native families: active guard moved before ID merge, third delay1500->1501, timer returning captured owner instead of undefined. The last fails the actual dropped-HelperObserver settlement assertion. Exact restored test0/outerdriver0; isolated native changes not run. Five isolated source controls failed1: active-before-merge, numeric string-attempt addition, terminal boundary, empty/subset loops; restored0. Terminal control first raises its independent full-chain timer-ceiling assertion inside the fixture catch, then fails the winning outer original-error-identity assertion. Initial report matched only nested text; both original report and corrected detector are retained, with isolated rerun1/restored0 at the same SHA. No production/oracle change or survivor. Initial unused-import fixture compilation101 retained separately, not a production failure or control.

Floor46 source-paired scenes through45 timer/disposed-get-error, independent10/11/25 root counts. Full-chain witness uses both new roots and actual existing Rust lifecycle_delegates::end_retry_chain: five callbacks through index4, five exact progressive delays/fetches and one end. Native exact Set/delete sequences, pending/active/timer and call counts, distinct original/secondary error pointers/code/UTF16, lexical receivers, string/stateful coercion hints and actual dropped observer settlement/count1 precede source launch. Native scene failures are accumulated and asserted empty first. Intrinsic/operator/iterator and virtual-callee implementations remain explicit bindings; general live mutation/getter/proxy/coercion domains, GC/fullPromise scheduling and concrete exchange assembly remain outside this floor. Canonical contracts: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). Final sub-agent review of exact1e20 found no blocker within scope; independently pinned oracle0/46cases/last45, reviewed Rust preservation diff0, tomllib census0/exacttwo moves/seven actual runtime bindings/derived14-file inventory. Reviewer ran no Cargo/native/clippy/extractor/mutations. Not run: isolated native changes, full crate/workspace runtime suites/builds, all-targets clippy, wasm, CI reruns, live requests or global parity-check. Evidence-only leaf not claimed compiled.

Final sub-agent review citation: [exact 1e20 review](https://github.com/Tealstreet/tealstreet-next/pull/7910#issuecomment-6186199162).
## Limit requests and signed placement: two rows

Exact code SHA `41c79bfa91705b23bc9ec3a2e7e7f84d4ad77cc0`, actual integration parent `f853d56fd227c2106dba609141b1fdd44bcab125`; no unreviewed sibling ancestor. Independent tomllib: 382 ported / 106 divergent / 43 pending -> 384 / 106 / 41 over531 rows; exactly2pending->ported. Sibling PRs are not counted as merged.

Measured at that SHA: scoped all-targets check0; targeted `limit_orders` native/source test0 (one test,35cases through34 place/wrong-type plus actual dropped-observer settlement/count1/original IDs/orders2); targeted strict clippy0; extractor check0/zero updates; scoped/lib-root rustfmt0. Writer0/zero updates was run after reconciliation; final extractor is independently measured. All native assertions run before source comparisons.

One compiled group of3production mutations fails101 at separate native trigger-order, fresh-grouping and independent-precache-count assertions. One isolated empty-return mutation fails101 at original-ID return and actual dropped-observer ID assertions. Restored native/source0; driver0. Four isolated source controls1: empty/subset independent completion floor, omitted SL independent wire count, bypassed type guards independent rejection floor; restored0/driver0. No survivor; grouped native changes were not measured individually.

Static review found the earlier Option-position assertion accepted missing precache; fixed with independent count1 and required positions. Missing fixture Array.push was also found by inspection and bound. Initial test compiler failure101 (unused items) and terminated loader-startup attempt101/SIGTERM15 are retained separately in `LIMIT_ORDER_MUTATION_EVIDENCE.json`; neither is a runtime control. A mixed-development-state test0 is not transferred to this SHA.

Final adversarial sub-agent reviews: [account_read_review and bitget_findings_audit](https://github.com/Tealstreet/tealstreet-next/pull/7911#issuecomment-6186549629). No final blocker within the finite floor; reviewers did not run Cargo/native/clippy/extractor/mutations. Canonical references: sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias and sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy. Primitive/helper implementations, general getter/proxy/coercion/live iterator domains, first-binding IteratorClose implementation, GC/full scheduling and concrete assembly remain outside this floor.

Not run: full crate/workspace runtime suites/builds, all-targets clippy, wasm, CI reruns, live exchange or global parity checker. Evidence-only leaf preserves measured source/tests/manifests/exemptions/assigned TS by scoped diff0 and is not claimed compiled.


LIMIT native substring binding repair: code/gate SHA 031ba1ff10b530aee8b5092adcaa9b11c2b7866a, actual integration parent f853d56fd227c2106dba609141b1fdd44bcab125 (merge already up to date; no unreviewed sibling ancestor). Scoped all-targets check0, target native/source test0, target clippy0, extractor0/zero drift, scopedfmt0. Isolated compiled wrong-needle mutation101 at LIMIT native HIP3 includes needle; exact bytes restored test0/scoped diff0. LIMIT_NEEDLE_FIX_EVIDENCE.json binds exact exits. Static predicted gap from order-update review, not a measured pre-repair survivor. Production/source/mappings unchanged; zero new rows moved. Earlier gates/controls retain earlier SHA attribution; previous mutation matrix not rerun here. Review: https://github.com/Tealstreet/tealstreet-next/pull/7911#issuecomment-6187777211. Native argument assertions precede source projection; supplied needle drives lookup. Full venue/workspace suites/builds, all-targets clippy, wasm, global checker, CI and live requests not run. Evidence-only leaf not claimed compiled; general binding/full assembly bounds and older findings unchanged.
## Single and serial placement dispatch: two rows

Exact code SHA `b75388272e60d6b73045db2bbc292d9ad5c4c8a6`; actual integration parent `f853d56fd227c2106dba609141b1fdd44bcab125`, no unreviewed sibling ancestor. Independent tomllib 382ported/106divergent/43pending ->384/106/41 over531 rows; exactly2pending->ported. Unmerged siblings are not counted as landed.

At exact code SHA: scoped all-targets check0; targeted `placement_dispatch` native/source test0 (one test,33cases through32 serial-drop plus2direct actual dropped observers); targeted strict clippy0; extractor check0/zero updates; scoped/lib rustfmt0. Writer0/zero updates after reconciliation, final extractor separately measured. All native checks precede source launch. Actual Rust dispatcher composes serial limit rows and existing Rust reduce-only composes wire bodies; source uses installed p-iteration.mapSeries.

One compiled group of3production changes fails101 at disjoint native price-band, full-position-size and serial-flatten assertions; actual dropped-serial flatten assertion also fails. Restored0/driver0. Isolated constant-true mainnet mutation fails101 at named native mainnet-vs-testnet assertion in market-zero; restored0/driver0. Five source controls1 at independent completion, original market IDs, serial order/one-level flat and invalid-trigger rejection assertions; restored0/driver0. No survivor; grouped native changes were not measured individually.

Earlier static review gaps (replacement row identity, ordering labels backed only by counts, no testnet=true witness) were repaired at this SHA. The old constant-true survivor was predicted, not executed; final compiled detector is measured. `PLACEMENT_DISPATCH_MUTATION_EVIDENCE.json` retains earlier4a77 gates/controls as separately measured, with no transfer to final SHA. Production preservation1358->final diff0.

Final [adversarial sub-agent reviews](https://github.com/Tealstreet/tealstreet-next/pull/7915#issuecomment-6186846138) found no blocker within scope; reviewers did not run Cargo/native/clippy/extractor/controls. Canonical references: sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias and sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy. Dense-array native sequencing, primitive/helper implementations, general getters/proxies/coercions/live iteration, GC/full scheduling and concrete assembly remain outside this floor.

Not run: full crate/workspace suites/builds, all-targets clippy, wasm, CI reruns, live exchange or global parity checker. Evidence-only leaf preserves measured source/tests/manifests/exemptions/assigned TS by scoped diff0 and is not claimed compiled.


PLACEMENT native substring binding repair: code/gate SHA 486147d71d97c5eb1f43827aea5e2f93c81cf49e, actual integration parent f853d56fd227c2106dba609141b1fdd44bcab125 (merge already up to date; no unreviewed sibling ancestor). Scoped all-targets check0, target native/source test0, target clippy0, extractor0/zero drift, scopedfmt0. Isolated compiled wrong-needle mutation101 at PLACE native HIP3 includes needle; exact bytes restored test0/scoped diff0. PLACEMENT_NEEDLE_FIX_EVIDENCE.json binds exact exits. Static predicted gap from order-update review, not a measured pre-repair survivor. Production/source/mappings unchanged; zero new rows moved. Earlier gates/controls retain earlier SHA attribution; previous mutation matrix not rerun here. Review: https://github.com/Tealstreet/tealstreet-next/pull/7915#issuecomment-6187817116. Native argument assertions precede source projection; supplied needle drives lookup. Full venue/workspace suites/builds, all-targets clippy, wasm, global checker, CI and live requests not run. Evidence-only leaf not claimed compiled; general binding/full assembly bounds and older findings unchanged.
## Signed cancellation and optimistic cleanup: one row

Exact code SHA `52e92dc72d1877fc97b4b3a3f29f6707b8d4ff72`, actual integration parent `f853d56fd227c2106dba609141b1fdd44bcab125`, no unreviewed sibling ancestor. Independent tomllib382ported/106divergent/43pending ->383/106/42 over531rows; exactly1pending->ported. Unmerged siblings are not counted as landed.

Measured at exact code SHA: scoped all-targets check0; targeted `cancellation` native/source test0 (one test,32cases through31 drop-observer plus direct actual drop settlement/count1/void/removal3); targeted strict clippy0; extractor check0/zero updates; scoped/lib rustfmt0. Writer0/zero updates after reconciliation. All native assertions precede source launch.

Compiled group of3production changes fails101 at separate native asset-value, Error-selector and actual dropped-void assertions; grouped changes were not measured individually. Isolated late-Set.add change fails101 at native Set.add-before-cache-callee assertion. Restored native/source0/driver0. Four source controls1 at independent completion, void and rejection assertions; restored0/driver0. No survivor.

Prior static review gap: instance_of fixture ignored constructor argument. Fixed exactErrorselector and measured wrong-constructor control; the prior predicted survivor was not executed. Initial check090a0 and test05600 remain separate receipts, not transferred. [Final adversarial sub-agent reviews](https://github.com/Tealstreet/tealstreet-next/pull/7917#issuecomment-6187155506) found no blocker; reviewers did not run Cargo/native/clippy/extractor/controls. `CANCELLATION_MUTATION_EVIDENCE.json` carries exact-SHA receipts.

Native action bits/NaN/UTF16/handles/internal counts, raw IDs, dedup keys and cleanup prefixes are checked directly. Source Error realm is aligned; actual source terminal/coin helpers and existing Rust scalar terminal helper are composed in stated fixture domains. Void-output collection extra/empty controls are INAPPLICABLE; internal counts are independent. Canonical references: sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias and sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy. Native no-pipe coin binding, general primitive/helper/prototype/getter/coercion/live iteration, dense map, GC/full scheduling and concrete assembly remain outside this floor.

Not run: full crate/workspace suites/builds, all-targets clippy, wasm, CI reruns, live exchange or global parity checker. Evidence-only leaf preserves measured source/tests/manifests/exemptions/assigned TS with scoped diff0 and is not claimed compiled.


CANCEL native substring binding repair: code/gate SHA 78423ecf800a2355c0bc6001ecf23fe183a56356, actual integration parent f853d56fd227c2106dba609141b1fdd44bcab125 (merge already up to date; no unreviewed sibling ancestor). Scoped all-targets check0, target native/source test0, target clippy0, extractor0/zero drift, scopedfmt0. Isolated compiled wrong-needle mutation101 at CANCEL native HIP3 includes needle; exact bytes restored test0/scoped diff0. CANCEL_NEEDLE_FIX_EVIDENCE.json binds exact exits. Static predicted gap from order-update review, not a measured pre-repair survivor. Production/source/mappings unchanged; zero new rows moved. Earlier gates/controls retain earlier SHA attribution; previous mutation matrix not rerun here. Review: https://github.com/Tealstreet/tealstreet-next/pull/7917#issuecomment-6187855405. Native argument assertions precede source projection; supplied needle drives lookup. Full venue/workspace suites/builds, all-targets clippy, wasm, global checker, CI and live requests not run. Evidence-only leaf not claimed compiled; general binding/full assembly bounds and older findings unchanged.
Order update: one pending→ported, zero→divergent against actual parent f853d56fd227c2106dba609141b1fdd44bcab125; independent TOML census383/106/42. Code/gate SHA 8849c5408581d58fcc4117e1ad948e7df8c684a1: scoped all-targets check0, order_update1test0, target clippy0, extractor0/zero drift, scopedfmt0. Four simultaneous compiled changes runtime101 at four named independent native families (not isolated-per-change); isolated threshold101 and supplied-needle101, exact-byte restored tests0. Six isolated source controls1 at named floor/policy/ID assertions, restored0. ORDER_UPDATE_MUTATION_EVIDENCE.json binds SHAs and retains pre-repair4a receipts separately. Native floor41 through40 utf16-symbol plus actual dropped-observer settlement precedes wire; no roundtrip. Review: https://github.com/Tealstreet/tealstreet-next/pull/7923#issuecomment-6187714423. Native includes binding now validates arity/raw needle and uses it; prior static gap was not executed as a pre-repair survivor. Same static gap in three sibling PRs remains separate fix-forward work. Parameter binding correction and development compile/fixture failures retained. Intrinsic/imported/virtual operations, general coercion/proxy/live iteration/full scheduler/concrete assembly remain unproved; conditional numeric recording checks supplement explicit branch assertions. See [canonical string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and [canonical alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias). Full venue/workspace suites/builds, all-targets clippy, wasm, CI, live and global checker not run. Evidence-only leaf not claimed compiled; older audit/findings remain open.
## Core/spot public constructors — measured d9d9d57318

Measured pushed code `d9d9d57318e1157d3956e2acfb5f7fb0005a75a5`, directly based on
integration333fd419fdbf70f081039610f22da9ee653bc96f; no unreviewed feature branch
ancestor. Scope: exactly2constructor rows pending→ported, tomllib census
382ported/106divergent/43pending→384ported/106divergent/41pending. Core class remains
divergent FINDING; spot class remains pending with empty target. These are derived
construction/field bodies over native bindings, not complete class assembly.

At that SHA: scoped Hyperliquid all-targets check0 (`--jobs 2`); public_constructor
target0 (1passed/0ignored); strict host Clippy for that test target0; extractor
check0 with Would update0/added0/orphaned0; scoped source/test fmt0 and lib fmt0.
61native scenes (38core/23spot) through60 spot define-error/listenOrderBook finish
before Node. Buffers are read as native binary64, object/Set/function identities
and same-instance/cross-instance freshness are direct checks, errors retain
original allocation, partial field prefixes are independently fixed, detached
onOpen executes actual already-mapped Rust bodies before comparison with full
original TypeScript class construction.

Adversarial sub-agent reviews found same-shaped _midsUpdateBuf/_fastMidUpdateBuf
identity was unchecked at3463d945383594e58b1192981327d644b4a574c7. Root reusing the
first allocation at new_core_public_socket/_fastMidUpdateBuf initializer COMPILED
and SURVIVED0. Retained finding, not a parity pass. Pairwise native/source checks
were added. Same mutation at measured d9 compiled101 at PUBLIC-CTOR distinct
same-instance fields, restored0. Four other isolated compiled controls failed101
with named native assertions and restored0: original newTarget and lexical receiver
at3463; positive-zero and empty-native-loop completion floor at d9. Two source
collection-discard controls empty/subset failed1 at independently fixed61 and
restored0. The first runner later exited1 on its own8-versus10 mutation-site
precondition BEFORE applying that control; repaired runner used actual10sites.
No compiler failure is counted as a discriminating control.

Final read-only reviews account_read_review and bitget_findings_audit pinned d9
found no remaining blocker within the constructor-only boundary. The first
independently ran final full-class source oracle0 (61through60), prior census0 and
final source/manifest preservationdiff0; the second ran only Git reads. Neither
ran Cargo/nativeRust/clippy/extractor/mutations. Root measurements are separately
attached in PUBLIC_CONSTRUCTOR_EVIDENCE.json, including the original survivor.
Supplied super/prototype/function/property primitive contracts and full class/static/
dispatch composition remain outside the floor, as do general proxies or nested
allocation failures not enumerated here. Canonical declaration referenced at
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), not duplicated.

Not run: full venue/workspace suites, workspace compilation, all-target host
Clippy, wasm, global parity checker, CI or live socket/network gates. Later
evidence-only leaf is checked by scoped preservation diff, not claimed compiled.
## Core public frame body — measured b169b611ee

Measured pushed code `b169b611eeb0df1951a92df0754d556b9107962a`; actual direct
integration parent d4e20845a85cc4e2711c99648ef7547f8aa806df, no unreviewed feature
branch ancestor. Tomllib382ported/106divergent/43pending→383/106/42: exactly1pending
onMessage→ported. Complete class FINDING stays unchanged. Other prepared PR row
moves are not counted as landed or added to this branch's census.

At measured SHA: scoped Hyperliquid all-targets check0 (`--jobs 2`);
core_public_message target0 (1passed/0ignored); strict host Clippy for this test
target0; extractor check0 (Would update0/added0/orphaned0); scoped source/test fmt0
and lib fmt0.52native scenes through51channel-reentry precede Node. Original decoded
values/handles, exact Store keys and raw UTF16 stats/topic arguments, signedzero/
overflow/latency bits, log text and independent effects/counts are asserted before
projection. No full class/heap/intrinsic/proxy/coercion coverage inferred.

Adversarial review found two documentation/test-proof gaps in otherwise correct
production: pong Store update could gain balance:7 and BBO stats raw D800 could
become D801 after correct handler lookup. GROUPED2 mutations compiled and SURVIVED0
at24cf9843f99f0803c24eb988b601bab2e01c6576. Both sites are retained in receipt.
Exact same group at8a5545567116082bcf0d7a73a05a6a6b63423901 compiled101 with BOTH
native named argument assertions observed; restored0. These are not two isolated
exits. Initial invalid group hit compilerE0502 from an incorrect mutation closure
borrow; retained, not a gate. Earlier e2657f418d compiled101 in json-array because
a reused cache fixture supplied a virtual method for a missing field; fixed in24cf,
restored0. Neither development issue is represented as a production repair.

At final measured SHA four ISOLATED compiling Rust controls each failed101 and
restored0: reversed handler catch policy, wrong latency divisor, swallowed outside
receive-marker error, empty native case loop. Three source controls failed1 and
restored0: empty/subset result collections and an assertion deliberately thrown
inside a real swallowed source callback. Instrumented assertion recording rethrows
that fixture error outside the actual production catch, preventing a false Node0.

Sub-agent reviews account_read_review and bitget_findings_audit pinned final b169
found no blocker. First independently ran actual source oracle0 (52through51),
census0 (one body promotion/13exemptions/classF unchanged); second independently
read source and producer effect/catch/order. Neither ran Cargo/nativeRust/clippy/
extractor/mutations; root receipts are distinct. Native operation contracts and
full class assembly remain separate. Canonical scope reference only:
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

Requested finding-type ledger records17canonical open Hyperliquid notes and named
Bitget closures at their actual evidence level. Reporter PR7865 is documentation/
proof closure of EXISTING fallible behavior, not a production behavior change.
actionHash inherits already-known addressToBytes diagnostics. Classification alone
changes no manifest status. No checker artifact is alleged or checker pass claimed.

Exact receipts in CORE_PUBLIC_MESSAGE_EVIDENCE.json. Not run: full venue/workspace
suites, workspace compilation, all-target host Clippy, wasm, global parity checker,
CI or live sockets/network. Later evidence-only leaf has scoped preservationdiff0;
its docs are not claimed compiled.
Spot limiter bodies, PR7971: measured code/gate SHA **99b5f5a5e78a79a654e17cefe9375e2c3917b41a**, direct integration parent694e07f7e049a5a7047c41e994a0fc854dc78a59. Integration merge-down was already current; pushed before gates. No unreviewed lane ancestor. Independent tomllib census: **382 ported / 106 divergent / 43 pending -> 385 / 106 / 40**, **3pending->ported,0->divergent**; leading FINDING rows remain17. Native base-class/prototype allocation/newTarget/method installation remains pending with no invented class symbol. The three actual body mappings include the anonymous callback execution body; seven exact exemptions cover Rust capture/primitive bindings only.

Exact code exits: scoped Hyperliquid all-targets cargo check **0**; spot_rate_limiter native/source target **0** (1passed,0ignored); strict target clippy **0**; extractor check **0**, output Would update0 manifests/0added/0orphans; scoped rustfmt **0** and lib skip_children rustfmt **0**. These are scoped gates, not workspace compilation. SPOT_LIMITER_BODIES_EVIDENCE.json binds exact SHAs/exits/controls/scope. Evidence-only child is not claimed compiled.

Independent native floor34 through33 fractional-second-rounding:1constructor/7refill/26wait scenes. Actual native own keys/tokens/lastRefill/capacity, raw binary64 timer delay, default/subtraction effects, captured weight and original error identities, result settlements and retained task completion precede all wire projection. Immediate tokens are checked before pumping. Dropped observer proof observes actual task completion after timer release, without peeking a native outer result; source separately observes actual outer fulfillment. Native Promise operations/timers are explicit bindings, manual firing/32pump/adoption floors finite. NaN/Infinity/oversized retry scenes stop after firing timer0 with timer1 pending; no infinite or universal scheduler/thenable/proxy/coercion-error proof.

Two adversarial subagent reviews found proof gaps, classification **documentation**, not production behaviour defects: constructor accepted an extra own field, and whole-second fixtures missed waitTime reassociation. Atbf66a26e4b74a253c1948afd6736d0df1f710752 the exact two-change group compiled and **SURVIVED0**, restored0. After exact constructor keys and an independent literal0.9999999999976694 delay witness, the SAME group at34a63609bb959b2a125bbc9b60d49fe8eb3a0a0b compiled101 at BOTH named native assertions, restored0. These are grouped measurements, not two isolated exits. Production bytes did not change. Five other isolated compiled Rust controls at34a each101/restored0 (field order, minute threshold, cached token read, replaced captured weight, empty native floor); two isolated source controls each1/restored0 (empty/subset result collection). Initialbf66 minute-control101 hit unsupported fixture > instead of intended assertion and is retained as an invalid control, not credited; corrected threshold control is separately measured. No survivor was dropped. Scoped src/native/support/source/input diff34a..99b5 exit0 preserves actual control inputs; mutation exits stay bound34a.

[Subagent findings and closure](https://github.com/Tealstreet/tealstreet-next/pull/7971#issuecomment-6190646902): final registration reviews at99b5 found no blocker; account_read_review independently measured source0 at34a,34through33, and final census/preservation0. Other reviewer used Git reads and an earlier standalone arithmetic comparison. Neither ran Cargo/native Rust/clippy/extractor/mutations; their earlier source exits are not transferred. No human gate. No full venue/workspace suite/build, all-target host clippy, wasm, global parity checker, CI or live run. Native class assembly and older63-row audit remain unfinished outside this body slice.
Candle listener bodies, PR7977: exact code/gate SHA **47963b3045e9b3636a22870481da266477d94b4f**, actual merged integration parent **8fcf323edfc6747fcdf4427ce19267a7147d018b**; own first parent b031ed76871aee26f43e32c698972e61d61c1936, original branch point694e07f7e049a5a7047c41e994a0fc854dc78a59. Pushed before gates, no unreviewed external lane ancestor. Independent tomllib: **382 ported/106 divergent/43 pending ->384/106/41**, exactly **2pending->ported,0->divergent**,17 leading FINDING rows unchanged. Both actual listenOHLCV wrappers plus their run_candle_callback execution targets map;17 exact native binding/capture exemptions. Full socket class assembly remains outside this slice.

Final479 exits: scoped Hyperliquid all-targets cargo check **0**; candle_listeners **0**(1passed,0ignored); strict target clippy **0**; extractor check **0**, output Would update0 manifests/0added/0orphans; scoped rustfmt **0**, lib skip_children rustfmt **0**. CANDLE_LISTENERS_EVIDENCE.json attaches exact SHAs, commands, exits, census, controls and limits. Evidence-only child is not claimed compiled.

Independent native/source floor58 through57 spot/parse-callee-capture,29core/29spot. Exact six-field candle values/binary64 bits/allocation identity, original callbacks/error identity, four complete raw UTF-16 dictionary key vectors, live field read floors, full pre-JSON native payload and actual send attempts/arity/native string, original/zero/fired/reentrant timer handles and discarded returns precede final wire comparisons. Spot production JSON.stringify executes as source requires, with direct native payload assertions in its binding. Captured opts/frame references and shared waitingTimeout cell preserve fresh fields, live frame length/entries, assignment target order, timer/clear getter/call reentry, repeated cleanup and native string-count ++/ToNumeric --. Manual timer firing and dense native array/filter/spread/JSON bindings are finite contracts; no BigInt, sparse/proxy/general iterator, native class, GC, generic intrinsic or real scheduling proof.

[Two adversarial subagent reviews and closure](https://github.com/Tealstreet/tealstreet-next/pull/7977#issuecomment-6191516270) at479 found no remaining blocker within scope. Their three findings are **documentation/proof**, not production behaviour: actual send attempts were inferred from JSON construction, full native dictionary keys were incomplete, and early mismatched entry.i access lacked a throwing getter/literal read floors. All strengthened without production changes. Original send-error already caught unconditional omitted send, so no old whole-gate survivor claim. Both reviewers measured scoped preservation/census0; account_read_review independently ran source0 at earlier edb2dfa98d and d0b3862c03,58through57, not at merged479. Neither reviewer ran final Cargo/clippy/extractor/mutations.

Controls remain bound to **d0b3862c03a157564827d54a78afb2ba58311677**: eight isolated compiled Rust mutations each runtime101/restored0 (field swap, stale clear argument, cleanup wrong key, snapshot frame length, omitted spot send, extra handler key, eager mismatched interval, empty native floor); isolated source empty/subset each1/restored0. Named assertions and exact modifications retained in JSON. No survivor in this ten-control set. Initial edb fixture compiler101 under denied warnings, precommit formatting syntax refusal with no separately measured exit, and d0 control-runner precondition1 before mutation/Cargo are retained as development failures, never credited as controls. Official writer0 at d0 plus three dirty parity files subsequently committed b031 reported Updated0/0added/0orphans; final committed extractor479 confirms canonical output. Owned code/fixtures/source unchanged d0..479 (diff0), shared dependencies changed during integration; fresh six positive gates cover479, while mutation exits remain d0 without full-input preservation or rerun claims. No full venue/workspace suite/build, all-target host clippy, wasm, global parity checker, CI or live run.
Private socket constructor and mutable fill-limit repair: actual parent d74a1c01f77439f9df61fa330063be194aab7576, one pending→ported/zero→divergent; independent TOML383/106/42. Class frame/static and complete dispatcher remain pending, empty mapping; constructor witness does not claim full class assembly. Code/gate SHA 83f5a21fa863dab9063238262358627c162a4c3c: scoped all-targets check0, private_constructor1test0, private_position_helpers1test0, both-target clippy0, extractor0/zero drift, scopedfmt0. Constructor41cases through40 define-error/handleOrderUpdates; helper136through135. All constructor decoded native assertions complete before source comparison, including independent member/capture/class/parent/receiver/error identities, positive-zero bits, exact prefixes and partial allocation counts, actual detached onOpen body and per-instance freshness. Valid compiled controls101: symmetric Map member swap,19-site arrow-capture family, frozen lexical limit, corrected isolated newTarget discard; restored targets0. Initial newTarget control compile101unusedparameter is retained and is not a discrimination gate. Two source empty/subset controls1 at independent completed floor; restored0. PRIVATE_CONSTRUCTOR_MUTATION_EVIDENCE.json binds exact SHAs and retains development compile/oracle failures and pre-fix static-limit probe (native101 TS1vsRust20000, restored0) separately. Reviewer citation https://github.com/Tealstreet/tealstreet-next/pull/7929#issuecomment-6188186704. Misleading alternate-super scene renamed base-marker; source checks prototype contract, Rust forwarding fixture does not prove superclass/prototype primitive implementation. Native class/property/descriptor/collection/function operations remain bindings; nested allocation/global-get/arrow failures, generic proxies/coercions/GC/scheduling and fullclass/live assembly unproved. See [canonical alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and [canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy). Full venue/workspace suites/builds, all-targets clippy, wasm, global checker, CI and live requests not run. Fresh integration fetch/merge already up to date, no unreviewed sibling ancestor. Evidence-only leaf not claimed compiled; scoped preservation diff0.

## Weight ingress repair/audit — code 71541ba3f8d0fc09ca7f2630edbc8f15d8bfff39

Fresh parent actually built: ac0f5fccf24207b26c279e67da2a88f2c72a7aae;
no unmerged timeout/bootstrap/Bitget/Bybit slice is an ancestor. Two production
bugs are repaired: shared Number failures now escape before pacing, and unknown
action types return zero batch length instead of reading the empty-string key.
Six helper bodies now have their own source-symbol mappings; the parent composes
them. Native errors retain their name/message in the numeric helper. The shared
API Host-string error projection remains separately CHOSEN. Canonical rules:
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Baseline native probe at production ac0f5fccf2 exits0, retaining the actual bad
values: four poisoned Number limits yield20; three unknown/missing/null action
types with a forty-element empty-key array yield2; valid raw userRole JSON with
an ignored lone-surrogate field or depth140 yields20. Actual source respectively
throws TypeError, yields1, and yields60. Five positives distinguish those failures.
Fourteen native probe cases stop at13/literal-backslash-u positive. The baseline
test fixture was uncommitted, not a gate at that commit; its complete source,
SHA256, production SHA and native outputs are retained in WEIGHT_PRE_REPAIR_EVIDENCE.json.
The maintained source oracle separately executes all14 cases with an actual
completion counter. This is not a claim that an old test existed at the parent.

Measured at code71541ba3f8: scoped cargo check --all-targets --jobs 2 exits0;
API test target27 passed plus weight_ingress target4 passed, zero ignored, exit0;
strict Clippy for weight_ingress exits0; extractor check0, zero changed/added/
orphaned symbols; scoped source/API/new-test rustfmt check0. Every native value,
error field, identity and effect assertion precedes source wire counts. Source
counterexamples retain native callbacks/getters before JSON, including their
original thrown value. Main ingress floor14 stops at13; URL floors2 native/4
source stop at low-surrogate/scalar respectively; array floor5 stops at two
members. Action parser floor3 stops at paired-surrogate positive. These are
floors, not a complete input/prototype/coercion enumeration.

Four actual production controls compiled and exited101:
- swallowed Number -> native poisoned limit must retain Number failure;
- unknown-action empty-key count -> native weight at case6;
- swallowed API weight error -> native API Number failure before pacing;
- pacing before propagation -> native weight failure before limiter clock.
Restored four-test target exits0. Four actual-source/oracle controls exit1:
swallowed Number -> source Number error name; unknown batch default -> source
native expected weight at6; empty/subset loops -> independent completed source
weight floor. Restored source exits0. No survivor or compiler rejection within
these four/four-control floors; WEIGHT_MUTATION_EVIDENCE.json records each.
All production and TypeScript source bytes were restored exactly.

The direct prepare_request witness isolates limiter-clock calls at zero;
execute separately proves no adapter call, with capture reaching the adapter
once. Read retry budgeting legitimately reads the shared clock before an attempt,
so no zero-total-clock-after-execute claim is made. No signing-path proof beyond
the exercised info-client path is claimed. Scalar outputs/errors make collection
output extra/empty controls inapplicable; corpus empty/subset controls remain.

Remaining parser findings are explicit: native20/source60 for the two valid
JSON inputs, and native None/source cancel at the independent action-type entry.
Encoded-body preparse is additionally identified statically at api::request_data;
its own runtime pacing counterexample was not executed. Constructor and omitted-
weight defaults remain unfixed. The omitted-weight finding is a signature audit,
not an executable native missing-argument call. Source executable coercions/live
accessors and UTF16 URL prefixes remain separately priced binding exclusions.

Metadata-only c8656c446d8c17bf485697222579311bad100ea5 adds the concrete shared
adapter-error-ABI reason and the missing-weight-default finding. Source/test/TS
diff versus measured code71541 exits0. Extractor check atc8656c exits0 with zero
changes/additions/orphans; no Cargo measurements are transferred to that SHA.
Tomllib census:382 ported/106 divergent/43 pending ->376/112/43,531rows:
0 pending-to-ported,0 pending-to-divergent; six inherited ported claims become
divergent. FINDING-bearing notes20->25 is a lexical row-note census, not unique
causes. This audit stops after the seven weight roots and action-type root;
the older63-root audit remains incomplete. No claim of venue clearance.

Sub-agent /root/bitget_findings_audit reviewed code71541ba3f8: no blocking finding
in the repaired paths; own review and root exits are posted on PR7881. Reviewer
ran no Cargo or mutations. Not run: full Hyperliquid suite, workspace compilation/
tests, crate-wide or wasm Clippy, wasm compilation, CI or live venue calls.
Global parity-check was not rerun; the separate timeout slice measured unchanged
webworker thread_local indexing failure before correspondence checks.

Core dictionary ordering repair, PR 7965: measured code/gate SHA **4a48e78eff5e5ff12a54abeb00b0c891fad2549f**, direct integration parent d4e20845a85cc4e2711c99648ef7547f8aa806df, no unreviewed lane ancestor. Tomllib over committed manifests: **382 ported / 106 divergent / 43 pending -> 383 / 105 / 43**. Zero pending rows move; AllMids D->P, UserRateLimit/OrderStatus D->CHOSEN. Three leading FINDING rows close (17->14), classification **behaviour**, one shared native key-order cause. Remaining any-domain rationale is explanation, with explicit pre-JSON costs and why the shared carrier is retained. See [snapshot declaration](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and [divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

Exact-code exits: scoped all-targets cargo check **0**; core_map_models **0** (1 passed,0 ignored); legacy types **0** (4 passed,0 ignored, including207 positive wire-shape cases, not207 new native proofs); filtered per_field_identity_survives_public_decode **0** (1 passed,2 filtered,931 scalar-Unicode cases through930); strict clippy for core_map_models/types **0**; generator read-only --check **0**; extractor check **0**, output Would update0 manifests/0added/0orphans; scoped rustfmt **0**. Pushed before gates.

All native assertions precede wire assertions:25 independent inputs through24 OrderStatus/nested-utf16,24 decoded DTOs plus one expected typed rejection,10 actual-interface compiler probes,7 native excluded-domain witnesses,3 actual in-memory numeric values. Whole raw keys/values retain UTF-16, duplicate replacement precedes typed decoding, index keys precede stable ordinary keys, own prototype names stay own fields, native f64 bits retain signed zero/overflow before JS numeric wire conversion. JSON wire comparison is semantic, not escape spelling. Any2 cannot retain own undefined/function/BigInt/Symbol, callable roots, sparse arrays or cycles; no arbitrary JS heap, generic Serde input, or manually normalized public-Vec claim.

Baseline6e98e6ccf22db16de1886e035a9faa69ce17fb33 compiled then failed101 at all three named native order assertions. At0f697a5b85ec176710baf64b727017cae11c6269 three isolated compiled Rust controls (first duplicate wins, lossy UTF-16 key, lost negative zero) each failed101 at the named native assertion and restored0. Three isolated source controls (empty/subset results, absent own undefined) each failed1 at independent source/native assertions and restored0. No survivors in these six controls. Production/scripts/core native/source/input bytes are identical0f697..measured4a48 (scoped git diff --exit-code0); controls stay attributed0f697. Development compiler101 at70638777 unused fixture import and scoped check101 at66ae4019 missing older NativeIdentity adapter are retained separately, not mutation gates. Fix-forward4a48 adds strict direct-native test projections; the final all-targets check is green.

Two adversarial subagent reviews, [posted review and correction](https://github.com/Tealstreet/tealstreet-next/pull/7965#issuecomment-6190306196), no remaining blocker. One inapplicable closed-output citation was removed after review; no human gate. CORE_MAP_MODELS_EVIDENCE.json binds exact exits, reviewed SHAs, failures, control sites and bounds. No full venue/workspace suite/build, all-target host clippy, field-identity clippy, wasm, global parity checker, CI or live run. Evidence-only child is not claimed compiled; its preservation is checked separately.

Core order-book listener: actual integration parent **539ac8165b8e36ef009dc467123e69fe14e002e2**, reviewed own candle-listener dependency **fdb850cb92611b68ebdf8b02fe128d7b73087f68 / PR7977**, no unreviewed external lane ancestor. Independent tomllib29-manifest floor ending hyperliquidspot.ws-public.ts.toml,531identities: **382ported/106divergent/43pending ->385/106/40**. Exactly **1 new pending->ported**, plus2 reviewed dependency pending->ported;0->divergent, leading FINDING17 unchanged. The source listener and retained depth/BBO/map/filter/poll/cleanup bodies execute in Rust; class assembly remains its separate row. Two actual entry targets,9 native plumbing exemptions and exact8-file union independently reviewed. See [canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), without copying declarations.

Exact final code/gate SHA **10154a7363c4110dde72a01f3111a3d2e0fd39ee**: scoped all-target check0; core_book_listener0(1passed,0ignored); candle_listeners dependency0(1passed,0ignored); strict Hyperliquid all-target host clippy0; locked Hyperliquid wasm32-unknown-unknown build0; locked workspace build0; extractor0 with Would update0/0added/0orphans; scoped rustfmt0 and lib skip_children rustfmt0. All completed, one Cargo at a time. CORE_BOOK_LISTENER_EVIDENCE.json binds every command/exit to its actual SHA; evidence-only leaf is not claimed compiled. Latest sprint protocol prohibits all pushes: this deliverable is committed locally for filesystem handoff, no own merge.

Load-bearing proof: at **54f781628a9e11f64683bb8e776a92bf10d84780**, grouped omission of cleanup filter-result spread plus initial isSnapshotLoaded=true compiled and **SURVIVED0/restored0**, retained as documentation/proof finding. At **427a9f0bb8da59032e10a8bc8477073cb4c9271f**, the SAME group fails runtime101 at BOOK independent initial snapshot false/restores0; group alone establishes its first assertion only. Separate isolated initial-flag and filter-spread controls each101 at their own named native assertions/restored0. Four additional compiled controls (19-level cap, symmetric px/sz input swap, raw-time stale guard, empty native enumeration) each101/restored0, making7Rust runs there including one group. Two source empty/subset result controls each1/restored0. At final10154, isolated clone-before-callback fails101 at BOOK callback original total object identity/restores0; isolated21-level cap fails101 at independent side length/restores0. Total9 compiled Rust runs and2source runs in the final control sets, no survivor there; original survivor is not erased. The symmetric field swap changes two expressions as one semantic control, not two independent controls. Original427 controls remain measured427; later source delta is exactly two from_ref substitutions plus strengthened fixture identity assertions, not relabelled final-SHA control runs.

Native/actual-source floor **34through33 utf16-keys**: decoded native price/amount/total bits, exact keys/UTF16 dictionaries, pre-total zero resets and original accumulator-input/callback-object identities, independent callbacks/send attempts/handler/timer/clear counts and original errors precede wire. Cases exercise BBO before depth, freshness including raw string-time ordering, nested best alias, invalid/missing sides, nonfinite/-0 values,20-level cap, live depth-loop growth, callee capture, retained foreign handlers, disconnected/queued/zero-handle/reentrant polls and repeated cleanup. Native properties, function allocation, operators, arrays/iterators, imported nanoid/total and cyclic reclamation are embedding contracts; dense finite fixtures do not establish universal proxy/coercion/sparse/BigInt/symbol-spread/GC/scheduler/class or complete-function parity. No63-earlier-row audit claim.

Two adversarial subagents reviewed pinned commits: account_read_review final10154 sourceoracle0,34through33, mapping/exempt/source preservation0, no remaining blocker within floor; bitget_findings_audit427 sourceoracle0,34through33 and closure of first two proof gaps. The third original-total identity gap is closed at10154 with independent counts; final reviewer ran no Cargo/native/clippy/extractor/mutations. Three findings are **documentation/proof**, not production behaviour defects or manifest FINDING closures. Initial a4c7 unused-helper check101 and source fixture instrumentation failure1 (early cleanup fallback literals bypassed table-only instrumentation, observed0filters expected2) are retained separately from gates; corrected VM-prototype instrumentation and actual native filter-result capture have independent0/2/4 counts. Not run despite CI enforcement: workspace fmt/clippy/tests, full wasm policy build/clippy, parity:check, binding regeneration/TypeScript regressions, remaining venue/crypto/transport/FFI/primitive gates and Linux/Node observations. Full venue tests, CI reruns, wasm runtime and live requests also not run.

Final order-book integration: code/control SHA **96587d82f9110b4b7497047c5f0f33a25a781c13**, actual integration parent **b2d71ee1d37d299534f4dce25045455548acc944**, reviewed own dependency0c04426b9d87; no unreviewed external lane ancestor. Independent tomllib29-manifest/531-row floor through hyperliquidspot.ws-public.ts.toml: **419ported/111divergent/1pending ->420/111/0**, exactly **1pending->ported**,0->divergent;16leading FINDINGs unchanged. Reconciliation preserves every other row annotation, all integration exemption records and whole evidence paragraphs; adds9 reviewed core-book binding records and one module. FINAL_BOOK_INTEGRATION_EVIDENCE.json records source/native preservation and two clean pinned sub-agent reviews. Canonical declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Fresh code-SHA baseline native/source1test0,34scenes through33utf16-keys; clone-before-callback and21-level-cap compiled controls each runtime101 at original-total identity/independent side length respectively, exact-byte restored0 after each. Source empty and33-result subset each1 at independent source result collection floor/restored0. No survivor in this fresh four-control set; original historical grouped survivor remains reported. Native assertions precede wire; finite primitive/manual-timer limits remain, no complete-function proof or old-row census audit claim. Existing proof repairs classify as documentation, no manifest FINDING closure. Three workspace gates run after this provenance commit and their exits attach to the actual handed-off HEAD in the local receipt; no evidence child inherits gate credit. No push.


Selected sixteen Hyperliquid findings, local delivery: integration parent **7bac54167ca1344621ff98cfc492c2838926bffb** is merged, including the fleet formatting sweep. Classification is **11 behaviour repairs /5 rationale rows (3 explanation,2 already-known)**, with no scheduling/documentation-class row among this selected16. Class/constructor rows share a root cause, so11/16 is a row ratio, not distinct defect frequency. Independent tomllib census across29 preserved manifests/531 rows, stopping at hyperliquidspot.ws-public.ts.toml: **420ported/111divergent/0pending ->425/106/0**, exactly5D->P and0pending moved. The ledger preserves the exact original16 source identities and per-row results. Prior evidence paragraphs remain historical measurements; the old ignored-surrogate/action cancellation, constructor-default and class-assembly findings described above are repaired by this unit. The unselected waitForToken omitted-weight finding and older mixed notes remain outside its closure.

FINDINGS_16_EVIDENCE.json binds controls to their actual SHAs and named assertions. Fresh base **78d315731fc7a7d41e7fb6ef6f3b799ba03445f8**:16 isolated Rust controls, indices0..15 through wallet-nested-inheritance, each compiled then failed101 at its intended native assertion and restored0 with exactly one native test. No survivor in this fresh16-control floor. Empty/subset source corpus controls each failed1 at independent completed13 floor, restored0 and exact source bytes. Earlier stale-site and compiler-rejected controls are retained as protocol failures, never counted as runtime discrimination. No gate credit transfers from these controls to the delivery commit.

The native repairs cover ignored raw surrogate fields at both action selector and encoded request cancellation, wallet-key-before-vault failure order, error message/name/display and limiter defaults, signing truthiness/BytesLike/primary-type/struct-property/safe-integer projection and signature recovery/compact inputs, four core WsUserEvent alternatives, native exchange/public class assembly and public fixed/user subscription composition. Signing native r/s/v and ingress bits precede source comparison:38 signing cases through37 uint-number-overflow,20 split through19 low-high-s,8 keys through7 order-minus-one. Core/public metadata counts are census, not proof that all94/17 bodies executed; immutable lexical class scope, imported primitive contracts, finite mock hosts and retained task observations do not establish a concrete VM or complete-function parity.

Pinned adversarial sub-agent review receipts are in FINDINGS_16_REVIEWS.md. Identified production gaps (predicate identity/falsy return, nested inherited wallet properties) and native-before-wire proof gaps were fixed forward; targeted follow-ups identify their exact scope and pins, with no human review gate or inherited reviewer Cargo exits. Canonical declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). Rationale notes price the concrete native callsite costs and retain their divergence buckets.

The local handoff receipt records the exact delivered commit and measured scoped tests, locked workspace build, host workspace clippy and workspace fmt --check exits. The overseer owns canonical wasm clippy centrally under the latest gate policy; it is not run locally. It is written after those gates without creating an evidence-only child commit. No push. NOT RUN scopes include full workspace tests/doc/include-ignored variants, standalone canonical wasm build, remaining CI/static/extractor/report/layer/routing suites, binding regeneration, full TypeScript regressions, other venue/runtime/live calls and CI reruns; the receipt lists them explicitly. The sprint waiver does not turn these into passing claims.


Encoded pacing/telemetry and optional weight audit: actual fetched integration parent **1810a76a9764020538782a82b29780b98c9778d1** merged locally at3dc992039fb8ab001a34dda93d12fc6961a3379a; reviewed own ancestor0753b7ed8dcc85ce972d72153a4a201ee9dcc061 was already accepted in user batch53ee502232, whose gates had not yet advanced origin. No unreviewed external lane ancestor is introduced. RATE_INGRESS_AUDIT.json selects exactly4 new rows through index3 RateLimiter.waitForToken: **3 behaviour repairs /1 explanation**. Pacing/telemetry share a cause, so3/4 is a row ratio, not distinct bugs or whole-venue frequency. The separately clarified existing signing refusal is excluded from this denominator. Independent tomllib29-manifest/531-row floor through hyperliquidspot.ws-public.ts.toml remains **425ported/106divergent/0pending**:0status moves,0pending moves; fixing a defect does not erase remaining chosen runtime/binding divergence.

At baseline574392f965cd576f04ebf91e47af9e2f2fb00ad7, both native tests compile and fail101: actual prepare_request reserves3000ms rather than1000ms for a nested JSON String; telemetry receives its decoded inner String rather than original outer String. The actual source oracle exits0 there. Fixed encoded-data conversion now passes original valid-UTF8 String to pacing and telemetry. The classifier parses once. Opaque encoded signing provenance refusal remains deliberately unchanged; known Transformed String replay still skips signing. None supplies omitted/undefined weight1 while explicit numeric normalization and existing lazy/drop queue ownership stay intact.

At389e1ee5b149a4af3c81dabcb1bc02fa15e98cd3, three isolated compiling controls through index2 omitted-default each fail101 at their own native reserved-weight/original-data/default-reservation assertion; each restores exact bytes and passes0 with one test. Two source empty/subset-loop controls fail1 at independent completed9 floor and restore0. No survivor in this3+2 floor. Native9encoded/5telemetry/9optional-weight groups all finish before source receipt parsing; native numeric/clock/sleep variants, provider/emitter values and context fields precede wire. Source stops: capture, malformed raw text, fractional1.5 respectively. Exact retained body bytes, callback allocation identity, arbitrary heap/proxies, live network/full Axios and complete Promise/function parity are not established. Invalid-UTF8 physical bytes have no claimed source-string counterpart.

The remaining whole-Value parser choice is explicitly priced at its parent: ignored materialized D800 or depth140 remains source60/native20 tokens and3000ms/1000ms next reservation. It is not called repaired or forced at the float-output parent. The shared full-value parser is retained rather than maintaining a selective UTF-16 decoder with separate coercions/depth decisions. See canonical [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). RATE_INGRESS_REVIEWS.md holds pinned adversarial sub-agent source/code and metadata reviews; no human gate or reviewer-measured Cargo claim.

Final scoped tests, fmt check, locked workspace build and host workspace all-target clippy are measured at the handoff SHA in the local receipt, without creating an evidence-only child afterward. Canonical wasm clippy, wiring, layers and routing are centrally owned and not run locally. Full workspace tests/doc/include-ignored variants, standalone wasm build, remaining static/extractor/report suites, binding regeneration, full TypeScript regressions and live/runtime calls are not run. No push or self-merge.


Historical eleven-row behaviour revalidation: actual integration parent **5fd278411cdce527ed977e8c98066dfbc8f5b9ae**, no unreviewed external lane ancestor. The eleven behaviour rows in the original selected sixteen were already repaired in accepted **0753b7ed8dcc85ce972d72153a4a201ee9dcc061**. The historical split is11 behaviour/5 rationale (3 explanation,2 already-known), not eleven current unresolved defects. Five of those eleven already became ported; six retain separately priced CHOSEN/MIXED rationale. Dividing that historical repaired numerator by the current106 divergent rows would mix populations and cannot establish current defect prevalence. This unit revalidates11 previously repaired rows, leaves0 known original defects unrepaired, makes0 new production repairs and claims0 new ported/status credit. Independent tomllib29-manifest/531-row floor through hyperliquidspot.ws-public.ts.toml remains425ported/106divergent/0pending; all manifest annotations are byte-preserved.

FINDINGS_11_REVALIDATION.json maps each original behaviour row to named native controls and preserves exact measurement SHAs. Sixteen controls at ee0e0562034338c5d2504a2bdae03cd613c4f076, eleven at4cc1de5013e31e1ff553b49fba6de99feadb65ae and three at e5e8a9801c744cf218f85b0efe64806727bfd6c9 each compiled, failed101 at the intended native assertion, then restored0 with exactly one native test:30 executions of23 unique recipes. Enumeration stops respectively at15 wallet-nested-inheritance,21 public-dispose-composition and22 public-interval-own-undefined. The diagnostic-unit control is an additional retained rationale-row guard, not an extra behaviour repair. These are mutation floors, not exhaustive function proofs or final delivery gate exits.

Three actual historical SURVIVOR0/restored0 controls remain recorded with callsites and clean base SHAs: omitting installed fixed-to-user delegation, omitting installed prototype-dispose delegation, and adding an enumerable own interval:undefined to depth/BBO input objects. These are documentation/proof findings: production already contains the correct edges and shape, but the earlier native target did not observe them. The strengthened test invokes the installed fixed arrow with nonempty topics and a foreign call-time receiver; observes actual native subscription arguments, exact own keys/descriptors and values before wire assertions; and invokes actual prototype dispose while asserting original receiver and stop-before-super effects. All three now fail101/restored0. The original7-case source feeder remains byte-unchanged; a later2-case composition supplement stops at installed-dispose-dynamic-receiver, and is explicitly not retroactive independent evidence.

FINDINGS_11_REVIEWS.md preserves pinned adversarial sub-agent reviews of the accounting, strict runtime-control criteria, native public composition and exact-key qualification. No reviewer Cargo or mutation exit is inherited. Production code is preserved; the signing src delta is only a cfg(test) case-name diagnostic. Complete-function parity, execution of every installed class body, general proxies/coercions/GC and asynchronous scheduling remain unestablished; this is not the other lane's whole-divergence census. Canonical declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Final handoff gates are measured only after committing this evidence: workspace fmt --check, locked workspace build, host workspace clippy, plus the selected native/source regression targets. Their exact delivered SHA, exits and logs are written to the local handoff receipt without creating an evidence-only child commit. NOT RUN: centrally owned canonical wasm clippy; full venue/workspace/doc/ignored tests; extractor/static/report/wiring/layer/routing gates; dedicated crypto oracle commands for this Hyperliquid-only proof unit; full TypeScript regressions, other venues, live network/browser/runtime integration and CI reruns. No push.


Hyperliquid divergence continuation: the preceding rate-ingress source60/native20
measurement is historical. Raw field projection now yields60 at those two direct
and encoded callers; current expectations use3000ms. The original SHA receipts,
controls and annotations above remain unchanged; the continuation outcome is
sdk-rs/parity/HYPERLIQUID_DIVERGENCE_RESOLUTION.md. No earlier gate exit transfers.


Delivery update: integration **0286f63437e4320e8c12886d5fef8fc6dfa2eac3** merged after the preceding proof measurements. Both evidence histories are retained. The latest local gate bar is only workspace fmt --check and locked workspace build, measured at the merged delivery SHA in the local handoff receipt. Host clippy, wasm, wiring, layers and routing are centrally owned and are not rerun locally. Earlier control/test/gate exits remain attached to their original SHAs; they do not transfer to this merge. No discarded-duplicate capture files are edited by this unit. No push.


Historical survivor status made explicit: **3 resolved /0 current survivors** within the three retained cases. Their original SURVIVOR0/restored0 observations remain preserved, rather than being erased or treated as still open. The earlier delivery had already caught each with the strengthened native assertions. Fresh repeat at **df8f7434dffd005ca500d9a4c72ea10821b11d72**, after fetching and merging origin0286f63437 (already contained), independently requires recipes20,21,22 through public-interval-own-undefined, each compiled native runtime101 at its own assertion and restored0 with exactly one native test. This unit adds explicit current_status/resolution links and measured repeat receipts; it makes0 new assertion/production repairs and0 manifest status moves.

The earlier tests missed, respectively, the fixed-to-user composition edge because topics were empty on fixed calls; the native dispose edge because only its descriptor was inspected; and own interval:undefined because Get and JSON text cannot distinguish it from absence. The current native test invokes installed fixed with populated topics, invokes installed prototype dispose with original receiver/effect observations, and compares exact own key/descriptors before wire. FINDINGS_11_REVALIDATION.json records each cause, named assertion and broken/restored exit. This is a finite three-recipe repeat; the other twenty unique recipes are not re-run in this unit. No discarded-duplicate capture file is edited.

Pinned read-only sub-agent closure review is appended to FINDINGS_11_REVIEWS.md. Final local gates remain only workspace fmt --check and locked workspace build at the delivered commit, with exits bound in the local receipt. Clippy, wasm, wiring, layers and routing are centrally owned and not run locally; full suites/static/extractor/live/CI checks also are not run. No push.


### Production journal fold binding (ddi2il-20)

`Exchange::fetch_historical_positions` now dispatches the existing source arrow,
whose superclass helper uses the shared `build_records` engine. The engine reads
the same realm instance's source income-range and historical-fill methods; income
mapper/pager imports bind to `income.rs`, and asynchronous REST callbacks use the
existing owned task dispatcher. The shared typed output boundary still applies.

Compiled negative controls used the new round-trip production test: the original
trait default failed with `no journal records path declared`; enabling only trait
dispatch failed with `fetchHistoricalPositions is not bound by this realm`. Both
are runtime failures after successful compilation. The restored implementation
returns a real closed round trip (PnL1000, fees3) with requested account, bounds
and market settlement currency. A separate default-window test pins integer
Date.now, the source30-day range and the shared14-day straddle lookback. A native
registry test obtains the same closed record through real reqwest HTTP without
starting sockets. Empty results or bypassing symbol-scoped execution reads fail
the nonempty-record assertion. The source `deriveClosedPositions` axis remains
separate from this shared journal fold.

Scoped gates: production binding16/16; native Hyperliquid HTTP2/2; registry
construction13/13; actual-source core snapshots/core REST/rest helpers11/11;
Hyperliquid venue-oracle39/39; parity:check0; registry48 names and control1/1;
layers88/88. Native all-target and wasm library strict clippy plus fmt pass for
the venue and registry. No fixture or manifest regeneration.
### Native external public data binding (ddi2il-20)

`HyperliquidDependencies.external_public_data` accepts the shared provider factory.
The realm superclass delegates initialization to the existing native BaseExchange:
provider creation receives the same Store owner, startup waits for both loaded
flags, and BaseExchange disposes the retained provider. The registry supplies its
existing gateway factory. An embedding with configured external data and no
factory gets the shared explicit diagnostic before network activity.

A new production registry test,
`native_host_external_data::hyperliquid_starts_on_a_public_data_gateway_over_websocket`,
was compiled against the old binding and failed at startup with the old explicit
Hyperliquid refusal (0 passed, 1 failed). After wiring the factory it passes over
real native websocket IO, receives gateway markets/tickers and makes no venue
REST request. `binding::external_startup_waits_for_both_loaded_flags_and_disposes_its_provider`
keeps startup pending with markets alone, completes after ticker hydration,
checks the exact Store owner, makes no direct public request/socket and observes
provider disposal once. The companion missing-factory test refuses visibly.

Scoped validation: production binding 16/16; native gateway suite 4/4; native
Hyperliquid signed HTTP 1/1; registry construction 13/13; Hyperliquid venue-oracle
39 passing tests across its 11 selected binaries; registry source equivalence
48 names plus mutation-control test 1/1; layers 88/88; parity:check zero findings.
Native all-target and wasm library strict clippy cover the venue and registry.
No manifests or fixture corpora were regenerated. The inherited seed oracle
hash failure was resolved by merging landed #198, including its one-row
provenance check, before the passing oracle run.

### Override inventory review after external-data and journal assembly

Reviewed the two `realm/facade_exchange.rs::base_exchange_class` inventory rows.
`b.dispose()` is the inherited BaseExchange implementation reached by the source
class's explicit super-dispose path; source override dispatch happens in the
realm first. `b.name()` serves the inherited name getter over the native account
name. Neither call newly bypasses a venue override. The external-data hydration/
disposal test observes the provider released exactly once, and construction/
native registry tests retain the venue name.

The stale inventory compiled and failed
`production_callers_do_not_bypass_exchange_override_hooks` before the repair.
After reviewing the final combined journal/external-data body, only those two
fingerprints changed to `f16535160061e222`; the same test passes (1/1). No inventory
regeneration or blanket delegation was used. This final body differs from the
external-data-only master body's fingerprint because it also binds journal folding.

## 2026-10-17 — raw socket close preserves forceClosed

The production family BaseWebSocket facade now forwards source `ws.close()` to
shared `BaseWebSocket::close_current_websocket()` from merged #227. It previously
called `force_close()`, incorrectly changing the superclass forceClosed flag.
The source private perpetual socket's `dispose` calls stopPingLoop and raw close,
without super.dispose or forceClose (`hyperliquid.ws-private.ts`).

`realm::socket_tests::private_no_super_dispose_closes_wire_without_force_closing_base`
constructs the production Hyperliquid exchange and invokes its actual private
socket methods through the family realm. The compiled old bridge closed the wire
but failed the direct native forceClosed assertion. After the transport-only
cutover, the wire closes once and native forceClosed/parent-disposal stay false.
The independent `tests/private-close-source.cjs` executes the actual source class,
checks those same observations, and rejects a source forceClose mutation. Its
control runs inside the maintained native test. No test transport flags stand in
for the production BaseWebSocket state.

The existing `base_websocket_class` inventory pin is reviewed and updated from
98b2e6c011ff88a4 to4a6e54ae7b9b544a. The inventoried h.socket.dispose call is
unchanged and remains the explicit superclass socket path; only raw close changes.
The inventory is reconciled manually, with other lanes' rows retained.

Measured close-fix validation: production realm test1/1 (with actual-source
mutation control), core binding18/18, native/private-class source target1/1 with
60 source cases, strict native all-target Clippy and wasm32 library Clippy pass,
scoped formatting passes, parity0 (7459 rows,17146 public items). The full global
override guard's inherited eight Binance pins are repaired by author PR#235;
that repair is inherited separately before marking this change ready.

## 2026-10-17 — Hyperliquid Spot production binding

`spot_binding::HyperliquidSpotExchange::new` constructs the existing Spot class
inside the production family realm, with the shared BaseExchange, Store, socket,
HTTP, signer and caller-owned limiter inputs. The native BaseExchange also records
`is_spot = true`. `RustVenueHost::hyperliquid` now takes the selected exchange name;
all callers are updated. The native registry constructs both names independently.
The source constructor's imported perpetual API factory and default-mainnet local
signing calls are preserved.

Observed gates on the integrated branch:

- `cargo +1.99.0 test --locked --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-hyperliquid --test binding --test spot_binding --test
  spot_runtime --test spot_account_reads --test spot_ws_dispatch`: 18 + 21 + 11 +
  10 + 9 passing tests. Every required Spot method is invoked; seven absent
  source overrides retain the exact BaseExchange rejection. Additional tests
  exercise real source OHLCV, book/candle message dispatch and cleanup, preflight
  caller/batch IDs, the native spot flag, and source numeric-key result handling.
- `python3 scripts/spot-binding-controls.py`: 19/19 independent method mutations
  compiled and failed their named test; restored 21/21 pass. Sixteen required
  methods and the three candle/book operations are controlled individually.
- `scripts/spot-boundary-controls.py`: the three old-bridge/oracle controls and
  separately selected `native-spot-flag` each compiled and failed their named
  assertion; exact byte restoration passed Spot21 + source-runtime11. The wrong
  omitUndefined stub still reads the actual source utility, keeping its source
  count at9; it fails the native result assertion, not a hash/count guard.
- Registry tests `native_host_external_data`, `native_host_hyperliquid`,
  `native_host_hyperliquidspot`, `rust_venues`: 5 + 2 + 1 + 14 pass. The new Spot
  test constructs through the real table, boots via loopback reqwest/tungstenite,
  verifies source action/nonce signatures server-side, preserves caller cloid,
  uses asset10000 and cancels oid777. The gateway test proves configured Spot
  external-data startup without direct venue public REST/socket. Earlier table
  removal compiled and failed that gateway construction witness. Hyperliquid's
  native host test also proves the same limiter Rc across Spot/perpetual hosts
  and selected Spot exchange-name metadata at the remote HTTP adapter.
- Scoped strict Clippy passes for native all-targets and wasm32 library targets;
  scoped formatting passes. `layers:check`:88/88. Registry equivalence:48 ordered
  names plus its corrupt-claim control pass. `parity:check`:0 invention/status
  findings (7459 rows,17127 public items on this observed tree).

The real utility `utils/omitUndefined.ts` delegates to lodash/omitBy: source Spot
placement returns a numeric-key object. The old source-runtime stub incorrectly
used Array.filter. The corrected oracle executes the actual utility; the test host
now reproduces its object result. The production realm already retained the right
shape. Native order IDs use the explicit conversion documented on the place/update
manifest rows and README under MANIFEST-SPEC#closed-typed-output-boundary; source
objects are not rewritten to arrays. Batch preflight/mapSeries ordering is retained
while combining lowered IDs at the native ABI.

Realistic Spot asset contexts omit oraclePx/funding, producing source NaN. The
Store/DTO bridge now uses the shared js_runtime typed codec instead of JSON and
preserves f64 values through native snapshots/lookups; wire JSON is unchanged.
The candle body also exposed an old production host error: object fields t/o/h/l/c/v
were destructured as an array. The actual object pattern is now bound, with a
compiling old-code control. Calendar arithmetic is the existing shared dayjs
implementation, exposed through the two operations this source consumes.

Override inventory updates are reviewed manually, without regeneration: the common
Binding extraction preserves core dynamic dispose and its unbound fallback; the
family base getter binds inherited BaseExchange isPublicDataOnly. Neither source
Hyperliquid class overrides canSign. The new Spot start capability calls the
concrete venue hook. Only affected existing pins and these two new calls are
reconciled; other lanes' rows are retained.

Spot integration follow-up: inherited merged #229's GRVT delegate review and
current master, retaining both Spot and ApexOmni construction docs. Core18 and
Spot21 production tests pass again; live TS override inventory passes1/1. The
full guard reached the final inventory comparison: manually reviewed Spot/base
class/core-dispose rows match. A further Store facade enclosing-body pin is
updated for the typed Store.update bridge; its native Store.update_order call
is unchanged. Remaining stale Binance rows on that master tree were reported
to their author lane for review; they are not regenerated or edited here.
