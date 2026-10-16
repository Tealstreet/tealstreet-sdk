# Store notification audit

Audited the worker production tree at `e1a7cfde67`. This records the existing
shared notification hold; it does not request a worker-local Store operation.
The full 26-method notification cutover belongs to the Store owner.

## Census and scope

Five concrete calls in four worker paths are listed below. The two position
refresh branches are separate call sites. This inventory covers direct Store
mutations in worker production code plus the explicit removeOrders base
delegation. It does **not** claim every indirect mutation performed by a venue
operation dispatched through CallMethodHandler, or every socket lifecycle path,
has been audited. Constructor/type rows are not reclassified as writer methods.

| Worker path and call site | Store operation | Observable source failure |
| --- | --- | --- |
| `positions.rs::handle_update_positions`, full refresh | `add_or_update_positions_full` | Positions commit; catch sets failure timestamp and posts `success:false`; request slots release in finally. |
| `positions.rs::handle_update_positions`, symbol refresh | `add_or_update_positions` | Same catch and committed-state behavior. |
| `ui_state.rs::update_positions_with_leverage` | `update_positions` | Leverage commits; the same subscriber error propagates to the caller. |
| `call_method.rs::handle_call_method`, removeOrders arm | `BaseExchange::remove_orders` → `Store::remove_orders` | Orders disappear; handler catches notification failure and posts `success:false`. |
| `subscriptions.rs::send_order_book_update` | `update_ticker` | Orderbook post occurs first; BBO commits; the first subscriber error propagates before later subscribers. |

At this audited tree, shared `Subscriber` is `Box<dyn Fn(&StoreData, bool)>`.
The position insertion/removal methods return unit; numeric patch methods return
`StorePatchError`, which represents patch failures, not subscriber failures.
A native panic cannot stand in for these recoverable source exceptions on wasm.
The numeric ModelPatch repairs remain valid and independent of this finding.

## Maintained witness

`tests/store_notification_oracle.mjs` executes the actual DefaultStore
constructor, source BaseHandler, source worker handlers and source throttle
constructor. Removal additionally uses the extracted actual BaseExchange
removeOrders method; this is not a claim that a complete venue constructor ran.
The host exchange's fetch result, clock and worker post target are explicit
fixtures. No mock Store replaces notification or mutation.

Twenty cases cover success and first-listener failure with Error and TypeError
on all five sites. Assertions
pin committed state observed by the first subscriber, skipped second subscriber,
error identity, caught failure replies, and BBO's already-posted book. Removal
replies retain the original name, message and code; position refresh retains the
Error versus TypeError spelling in its failure reason. The Rust
`store_notification` test invokes this live-source witness; it is deliberately a
source FINDING witness rather than a claim that Rust can already return the
subscriber error. The current callback signature cannot express it.

The no-throw control exits 1 at `NOTIFICATION first throw skips later subscriber`.
Name-flattening and code-dropping source controls exit 1 at
`NOTIFICATION caught error name` and `NOTIFICATION caught error code`.
The Store owner's published follow-up `984390f12a` adds an Rc-backed original
cause to StoreSubscriberError; core's coordinated migration must preserve and
recover the original ExchangeError rather than flattening its name/code into a
message or inventing a response side channel. This witness remains source-only
until the coherent Store/core cutover is consumed.
Missing compiler and lodash imports fail loudly through oracle_dependencies.
All twenty cases also passed in a disposable tracked-source mirror with no ancestor
node_modules, using explicit isolated compiler and pinned lodash paths. Both
missing-package controls failed MODULE_NOT_FOUND; the mirror was deleted.

## Manifest consequences and cutover checklist

Three rows move from ported to FINDING: UpdatePositionsHandler.handleUpdatePositions,
UpdatePositionsHandler.handle, and UiStateHandler#updatePositionsWithLeverage.
Four already-divergent rows gain a separate notification finding: both call-method
handlers and both orderbook send/handle rows. Counts become **96 ported / 47
divergent / 77 pending**, with no production behavior changed by this audit.

When the ABI lands, check all five calls above, preserve each source catch or
propagation boundary, then add native notification-error witnesses alongside the
source cases. Do not erase numeric, output, callback or representation findings
merely because Store notification becomes fallible. Pending handlers may be
implemented now against the current API, declared FINDING across this seam and
added here; the breaking cutover forces every call to be revisited by compilation.

## Conversion audit before the cutover

All worker production `map_err` sites were inspected: they convert JSON decoding,
string coercion or allocation errors, not Store write failures. The equivalent
inference hazard exists in BBO's `if let Err(error)` followed by `to_string()`;
its update result is now explicitly `Result<(), StorePatchError>` so the full
notification cutover cannot silently preserve the flattening conversion.
The leverage helper already returns an explicit StorePatchError and propagates
with `?`. Position insertion and removeOrders still use the unit-returning
contract recorded above; their notification FINDINGs remain open.

## Notification cutover consumption

The merge of `9419b340d8a6` changes all five inventoried calls to the fallible
Store contract. Position refresh catches the converted subscriber error after
the committed write; removal propagates through the existing worker reply catch;
leverage returns StoreMutationError directly. Twenty native cases over Error and TypeError pin these four sites, committed
state, ordered fanout, reply status, and original name/code/stack/context.
Four disposal-during-notification cases supplement the original 20 source cases. These cases do not claim arbitrary venue side effects.

BBO reports the converted error through the runtime exception callback, retaining
metadata. Core's order-book callback still returns unit, so this cannot reproduce
the synchronous source throw to its caller; that residual FINDING remains. The
subscriber-id follow-up was verified in integration `710e663156` and consumed
by `bc8c7f5990`. The leverage consumer checks the original inner ID followed by
the actual nonzero failing subscriber ID. The BBO consumer checks Error and
TypeError, complete retained metadata, and IDs `[401, 2]`: 401 is the incoming
inner-boundary witness; warmup and BaseExchange subscriptions occupy this
Store's IDs 0 and 1. Equal IDs from different Stores do not establish identity.

The BBO native callback observes one post before the committed bid/ask update,
one failing subscriber, zero later-subscriber calls and one visible host report.
Production empty-ID and zero-outer-ID mutations compile and fail
`BBO-IDENTITY inner-to-outer nonzero subscriber`; restoration passes. The shared
structural field is consumed directly, with no worker-owned error box or sidecar.
Every metadata field is initialized explicitly, with distinct optional values;
a compiling request/config swap fails `BBO-IDENTITY complete retained error metadata`.
This closes the subscriber-ID omission, not the infallible callback's inability
to throw synchronously. The source notification oracle remains unchanged.

The cutover also exposes seven fallible non-Store operations: reconnect in
CallMethodHandler and DevHandler, active-symbol publication, chaser log and price
error emission, and command exec-log/exec-result emission. Synchronous failures
propagate; a command's detached rejection reports a secondary listener failure
to the runtime. Chaser log failure precedes stopping, while error-listener failure
follows stopping. No row is promoted solely because its call now compiles.

The merged-state reviewer found that a successful subscriber disposing the
exchange must not suppress the already-started refresh's success reply. The new
native case first failed `NOTIFICATION native disposal preserves success reply`
with zero replies versus one, then passed after retaining only the pre-write
disposal gate. Source fixtures change the disposed flag; native fixtures exercise
BaseExchange::dispose, including its separate Store reset. Their shared assertion
is the reply after the successful write, not equivalence of disposal internals.

## Isolated integration consumer repair

Integration a09ea6489b retains the old worker signatures even though the worker
feature branch has already migrated them. This repair applies only the worker
cutover and its consumer witnesses on a separate branch. No shared error
conversion is introduced and no StoreMutationError becomes StorePatchError.

| Source row | Classification and explicit boundary |
| --- | --- |
| UiStateHandler#updatePositionsWithLeverage | FINDING resolved for notification propagation: return StoreMutationError unchanged, retaining Patch, Notification and InvalidOperation identity. Its caller may use the existing shared ExchangeError conversion. |
| OrderBookSubscriptionsHandler#sendOrderBookUpdate | FINDING retained: the infallible core callback cannot propagate a synchronous source throw. Explicitly convert StoreMutationError through the existing ExchangeError conversion and report it to the runtime, retaining cause, metadata and inner-to-outer Store-local subscriber IDs. |

Compilation iteration 1 after transplanting the existing consumer migration
passes the worker all-target check with zero new diagnostics. This does not
claim the integration workspace compiles. The migration includes fallible
reconnect, active-symbol publication, command emissions and chaser emissions
which previously became visible only after the two type errors were removed.
Successful subscriber disposal during position refresh still permits the reply;
only disposal before the write suppresses it. The original disposal regression
and full notification identity witnesses accompany this extraction.

Core confirms that StoreMutationError must never receive a blanket narrowing
conversion into StorePatchError: Patch is only one variant, and Notification
retains subscriber provenance plus an opaque cause while InvalidOperation is
separate. FINDING retained at the canonical ExchangeError projection for arbitrary
non-ExchangeError causes and variant identity; this consumer repair neither
widens nor silently claims to close that shared projection boundary.

Iteration 2 after merging e7d3497502 passes the worker all-target check with
`--keep-going` and zero new diagnostics. The broader handler test reports an
independent source-inventory mismatch (20 records versus its pinned 19); that
failure remains visible and is not repaired by weakening its count in this slice.

Final extraction check, iteration 4, against ed94b3b2bf: workspace all-target
`--keep-going` exits 101 with 13 distinct errors outside this crate (WOO 7,
OKX 5, Asterdex 1), zero worker diagnostics. Iteration 3 exposed an ignored
Result in the restored subscription fixture; the fixture now asserts success.
Notification, subscription and chaser-price targets pass 7 tests; host and wasm
strict all-target Clippy each exit 0. This is not a successful workspace build
or a wasm workspace build. The broader handler target still exposes the new
source recoverAfterOffline case (20 records versus its old 19-record inventory).

A compiling BBO mutation replacing the canonical conversion with
ExchangeError::new(error.to_string()) fails
`BBO-IDENTITY inner-to-outer nonzero subscriber`; the restored target passes.
The subscription source corpus is unchanged; unrelated field-identity fixture
changes accidentally included during extraction were removed before validation.

## Margin-symbol fetch helper

`ui_state.rs::handle_active_margin_symbol_change` calls Store::update for the
initial placeholder outside its completion catch. It propagates the canonical
ExchangeError before any venue fetch when the first subscriber fails.
`ui_state.rs::clear_fetched_leverage` calls Store::update on the current map;
null-result cleanup runs in the completion path and may be retried by the catch.
The existing update_positions_with_leverage delegation remains one fallible batch.
The live source witness pins commit-before-notify, catch retry and final error
identity. Cancellation performs no Store write because marker ownership is not
represented; it reports to ExchangeRuntime and retains concurrent Store state.
The canonical arbitrary-cause/variant projection finding remains separate.
