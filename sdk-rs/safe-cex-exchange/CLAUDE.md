# Exchange core contract

Behavioral sources are [exchange guidance](../../packages/safe-cex/src/exchanges/CLAUDE.md)
and [SDK guidance](../../packages/safe-cex/CLAUDE.md). This document records the
Rust composition boundary, not a second exchange specification.

## Abstract venue hooks

`Exchange` is the object-safe venue protocol surface. Venue implementations
supply account validation, market/ticker/private snapshots, native placement,
updates, cancellations, leverage and mode changes. Hooks with no implementation
retain the TypeScript `Not implemented` rejection. Local boxed `ExchangeFuture`
values allow browser handles; no `Send` requirement is imposed.

Raw placement belongs to concrete `BaseExchange` orchestration. A venue's
placement hook accepts `ValidatedOrder<Self>` / `ValidatedBatch<Self>`, whose payloads are
private, immutable, non-Clone and not deserializable. A validation token cannot
be duplicated to send the same caller client ID twice. The SDK checks every caller ID,
honorable shape, market-size split and duplicate before invoking a write hook.
Venue formatters still check that a caller ID maps to exactly one native payload.
The venue type is part of each token: an A token cannot be passed to B's typed
hook. Placement hooks require `Self: Sized`; the remaining protocol remains
object safe. `PlaceOrderBinding::implemented::<Venue>` and
`PlaceOrdersBinding::implemented::<Venue>` register concrete stateful hooks.
The private dynamic boundary downcasts the target before constructing a token;
a mismatched registration rejects without invoking its handler. Raw placement
always validates the target's base before dispatch. Native bulk implementations
must register `place_orders`; its default dispatches sequentially to `place_order`
and is branded `BaseCapabilityMethod::PlaceOrders` so an omitted registration
is detectable. A trait override alone is not dynamic registration.
Runtime signing and transport handles stay outside serializable account options.

Every concrete venue declares `type PlaceOpts = PlaceOrderOpts` or its full venue
extension type. The associated type has `where Self: Sized`, keeping the remaining
`dyn Exchange` surface object-safe. It must implement `Clone`,
`Borrow<PlaceOrderOpts>`, `From<PlaceOrderOpts>` and `DeserializeOwned`.
`ValidatedOrder<V>::options()` returns `&V::PlaceOpts`, including extensions.
The shared conversion is explicit: generic operations such as nuke construct
shared opts, then the binding converts them through the venue's `From` impl.
For Bybit-like families that conversion selects Plain in a Plain/Bbo/TickSize
union; it never projects an extended request back into the shared type.

`validate_order_capabilities(&Self::PlaceOpts)` is overridable per concrete venue
type, including variants inside one crate. It is a core dispatch seam for typed
preflight; TS's concrete example is BybitExchange's hook and HyroTrader's override,
not a method on TS BaseExchange. Single placement checks shared caller-ID rules
then the full venue hook. Batch placement decodes all inputs, checks every shared
ID and duplicate, then checks every full venue input before minting tokens or
calling any native handler. Default sequential dispatch preserves each full
venue payload. Native readiness checks still belong in native handlers.

The typed raw entrypoints accept shared or full venue opts without serialization.
Wire callers use `place_order_json` / `place_orders_json`; these decode directly
as the selected venue's type, never through shared options or a fallback decoder.
Typed requests for another options type are rejected, never projected via Borrow.
A venue's deserializer must preserve or reject extension presence/null and reject
invalid exclusive combinations: `DeserializeOwned` itself does not prevent
unknown-field loss. The core tests exercise typed and JSON extension preservation,
second-item batch refusal, family-specific validation and generic nuke defaults.

## Concrete shared behavior

`BaseExchange` owns lifecycle/disposal, listeners, logging, active-symbol filters,
auth backoff, batch and cancellation orchestration, private-snapshot retries,
watchdog aggregation, overlays and worker correlation. Venues reuse those
algorithms instead of copying them. `BaseWebSocket` owns shared lifecycle,
heartbeat, keyed dispatch, diagnostics and reconnect state; venue protocol hooks
supply authentication, subscriptions and frame interpretation. Private auth is
awaited before subscriptions, with disposal checked after asynchronous work.

The store contract is `safe-cex-store`'s port of
[`Store`](../../packages/safe-cex/src/store/store.interface.ts). The core consumes
that trait directly; it never defines a projection or another store shape.
`ExchangeRuntime` supplies clock, local task/timer cancellation, runtime logging
and worker requests. Runtime signing handles stay outside serializable options.
The native wall clock supplies integer epoch milliseconds for Date.now timestamps;
its monotonic clock retains fractional milliseconds. Source virtualClock reads,
including the shared JS facade, use safe-cex-store's VIRTUAL_CLOCK instead so
server offsets and its native/worker policies remain authoritative for signing.
The shared JS Date.now facade rounds down an injected wall clock, while
performance.now keeps the host's fractional precision.
Remote-signer availability is the separate TS `RemoteSigner.isAvailable` /
`onAvailabilityChange` contract; `payloadSigner` presence does not replace it.

See the [socket host contract](WS_CONTRACT.md) for host scheduling and relay options.
Synchronous venue `WsProtocol::connect_and_subscribe` implementations call
`socket.start_websocket(url, protocols, options)`. The core owns the dial task,
its cancellation and error diagnostics; the venue never retains its handle and
needs no protocol disposal hook.

A venue that must do asynchronous work BEFORE it can dial — kucoin's bullet-token
REST fetch is the case — uses `start_prepared_websocket(prepare)` instead, so
preparation and the dial are ONE cancellable core-owned task bound to one socket
generation. The closure runs only inside that task, never on the venue's own
executor, and returning `None` means the venue has already handled a no-dial
outcome rather than that preparation failed. Doing the fetch in the venue and
then calling `start_websocket` reopens exactly the disposal and stale-generation
races this operation exists to own.

JS-backed sockets dispatch typed disposal through the live receiver's `dispose`
member. The facade guards both that dispatch and the base prototype's
`super.dispose()` path against reentry. A source throw is reported through the
runtime; the infallible protocol hook still permits core transport cleanup.
The Weex source-member witness covers timer/listener cleanup, superclass order,
direct and typed calls, repeated disposal and guard reset after errors.

The returned JS websocket handle uses `close_current_websocket()` for `ws.close()`.
This closes only the current transport and preserves the superclass force-closed
flag and retained handles. The raw-close source witness executes both Crypto.com
no-super disposal overrides and contrasts them with the source `forceClose()`.

Socket options use `None` for omitted SDK tuning and `Some(options)` for explicit
RWS options, including an empty object. Make absence a distinct representable
state rather than a default value. Header presence does not select tuning.
Managed `start_closed` dormancy and its scoped source/native witnesses are
documented in WS_CONTRACT.md; its residual choices cite
MANIFEST-SPEC.md#exchange-runtime-callback-choice.

Keyed message handlers return `Result<(), ExchangeError>`. Venue protocols return
`socket.dispatch_message(key, message)` (or use `?`) so the first callback error
stops dispatch and reaches the existing on-message error path. Dispatch retains
the captured list across key removal, indexes the live list, and records matched-key
stats once before callbacks. Do not collect errors in a side channel or continue
after a coercion error: TypeScript stops at the throw.

When a venue needs to coordinate with a lifetime or error path owned by the core,
provide a core operation that owns that coordination. Exposing only the component
types forces venues to duplicate cancellation registries or error enhancement.
A venue compiling through its own replica does not prove the composition works;
test through the public operation and the production Store/adapter boundary.

Socket construction consumes the transport provider, existing relay/fallback
clients and host socket interfaces. It does not construct opaque tunnel dial
options or reproduce HOW framing. An ordinary venue socket needs a host dial
hook; header-required sockets refuse hosts that cannot supply upgrade headers.

## Branded fallback bindings

A capability binding carries the actual callback and its origin together:
base fallback, explicit empty override, or venue implementation. Only the SDK
constructs base-fallback bindings; registering an override removes the brand.
Support is never inferred from a returned empty vector or a separate boolean.
The branded throwing defaults are `start`, `placeOrder` and the three option
reads (`fetchOptionChain`, `fetchOptionExpiries`, `fetchAccountGreeks`), which
reject naming the venue rather than return an empty chain; the branded empty
defaults are fills, income, income range, historical orders and closed PnL.
`fetchOHLCV` is unbranded in the actual TS source despite older guidance saying
otherwise. The journal-position default refuses: it never invents an empty
journal or silently folds fills. Imported folding is an explicit host seam.

Implementations must preserve the documented TS quirks. In particular, an empty
supplied symbol list differs from no symbol list, market refresh compares IDs,
and a rejected/disposed position lookup retains its shared fetch slot.

## Validated placement example

Venue code implements hooks that receive validated payloads; callers invoke the
base entrypoints with ordinary `PlaceOrderOpts`. No venue constructs the private
validated fields. This example runs as a crate doctest: it constructs the production Store and all
five BaseExchange arguments, places one order and one native batch, and verifies
a rejected batch sends nothing. The recording runtime implements all ten host
methods explicitly; its unused I/O paths panic rather than pretending to work.

```rust
use safe_cex_exchange::{
    capabilities::{PlaceOrderBinding, PlaceOrdersBinding, ExchangeCapabilities},
    client_order_id::{pattern_client_order_id_rule, ClientOrderIdRuleOverrides},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedOrder, ValidatedBatch},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{store::DefaultStore, Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{json, Value};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc, task::{Context, Poll, Waker}};

// This recording example performs no I/O. A production host must supply real
// clocks, timers, scheduling/cancellation and worker dispatch (WS_CONTRACT.md).
struct RecordingRuntime;
impl ExchangeRuntime for RecordingRuntime {
    fn monotonic_now_ms(&self) -> f64 { 1_000.0 }
    fn now_ms(&self) -> f64 { 1_000.0 }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> { panic!("unexpected timer") }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected detached task")
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String { "fixture-id".into() }
    fn is_worker(&self) -> bool { false }
    fn random(&self) -> f64 { 0.5 }
    fn report_exception(&self, error: &ExchangeError, _: &Value) { eprintln!("{}: {}", error.name, error.message); }
    fn report_raw_exception(&self, _: &Value, _: &Value) { panic!("unexpected raw exception report") }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker request")
    }
}
struct ToyVenue {
    core: BaseExchange,
    capabilities: ExchangeCapabilities,
    writes: RefCell<Vec<Vec<String>>>,
}
impl ToyVenue {
    fn new() -> Self {
        let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
        store.update(StoreUpdate {
            markets: Some(vec![serde_json::from_value(json!({
                "id":"BTC", "symbol":"BTC", "base":"BTC", "quote":"USDT", "active":true,
                "precision":{"amount":1,"price":1},
                "limits":{"amount":{"min":1,"max":5},"leverage":{"min":1,"max":100}}
            })).unwrap()]),
            ..StoreUpdate::default()
        }, false).unwrap();
        // Without a rule, the base deliberately refuses EVERY caller-supplied ID.
        let settings = ExchangeSettings {
            client_order_id_rule: Some(pattern_client_order_id_rule(
                "Toy".into(), "/^[a-z0-9-]{1,20}$/".into(),
                Rc::new(|id| !id.is_empty() && id.len() <= 20 &&
                    id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')),
                ClientOrderIdRuleOverrides::default(),
            )),
            ..ExchangeSettings::default()
        };
        let core = BaseExchange::new(
            ExchangeName::Binance,
            serde_json::from_value(json!({"key":"fixture", "secret":"fixture"})).unwrap(),
            store, Rc::new(RecordingRuntime), settings,
        );
        let capabilities = ExchangeCapabilities {
            place_order: PlaceOrderBinding::implemented::<Self>(|venue, order| venue.place_order(order)),
            place_orders: PlaceOrdersBinding::implemented::<Self>(|venue, batch| venue.place_orders(batch)),
            ..ExchangeCapabilities::default()
        };
        Self { core, capabilities, writes: RefCell::new(vec![]) }
    }
}
impl Exchange for ToyVenue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange { &self.core }
    fn capabilities(&self) -> &ExchangeCapabilities { &self.capabilities }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move {
            order.assert_single_native_client_order_id(&self.core, 1)?;
            let ids = vec![order.options().client_order_id.clone().unwrap()];
            self.writes.borrow_mut().push(ids.clone());
            Ok(ids)
        })
    }
    fn place_orders(&self, batch: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move {
            for order in batch.orders() {
                order.assert_single_native_client_order_id(&self.core, 1)?;
            }
            let ids: Vec<_> = batch.orders().iter()
                .map(|order| order.options().client_order_id.clone().unwrap()).collect();
            self.writes.borrow_mut().push(ids.clone()); // One recorded native bulk write.
            Ok(ids)
        })
    }
}
fn order(id: &str) -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTC", "type":"limit", "side":"buy",
        "amount":1, "clientOrderId":id})).unwrap()
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("recording placement unexpectedly waited for I/O"),
    }
}
let venue = ToyVenue::new();
assert_eq!(ready(venue.core.place_order(&venue, order("one"))).unwrap(), vec!["one"]);
assert_eq!(ready(venue.core.place_orders(&venue, vec![order("two"), order("three")])).unwrap(),
    vec!["two", "three"]);
assert_eq!(*venue.writes.borrow(), vec![vec!["one"], vec!["two", "three"]]);
assert!(ready(venue.core.place_orders(&venue, vec![order("four"), order("INVALID")])).is_err());
assert_eq!(venue.writes.borrow().len(), 2, "invalid batch must send nothing");
```

A token from one concrete venue cannot enter another venue's typed hook. This
second example is also pinned by the order/batch trybuild diagnostic snapshots
in `tests/ui`; the test checks the type mismatch, not just compilation failure.

```compile_fail
use safe_cex_exchange::{exchange::{BaseExchange, Exchange, ValidatedOrder}, capabilities::ExchangeCapabilities};
struct VenueA;
struct VenueB;
# impl Exchange for VenueA {
#     type PlaceOpts = safe_cex_types::PlaceOrderOpts;
#     fn base(&self) -> &BaseExchange { unimplemented!() }
#     fn capabilities(&self) -> &ExchangeCapabilities { unimplemented!() }
# }
# impl Exchange for VenueB {
#     type PlaceOpts = safe_cex_types::PlaceOrderOpts;
#     fn base(&self) -> &BaseExchange { unimplemented!() }
#     fn capabilities(&self) -> &ExchangeCapabilities { unimplemented!() }
# }
fn place_on_b(_: ValidatedOrder<VenueB>) {}
fn wrong_venue(order: ValidatedOrder<VenueA>) {
    place_on_b(order);
}
```

Native bulk venues must register `PlaceOrdersBinding::implemented::<Self>` AND
implement `Exchange::place_orders` by iterating
`ValidatedBatch::orders()` to build their single request. They must perform the
native payload count assertion before sending. `empty_override` represents an
entirely empty implementation; `is_base_capability_fallback` answers whether an
override exists and `is_inert_capability` also rejects those empty overrides.
Conditional early-empty returns belong to `implemented`, never `empty_override`.

Required venue follow-up: cross-check these declarations against each real venue
under recorder transports and controlled response fixtures, following the TS
capability and fail-closed suites. Until that registry-driven gate exists, an
incorrectly declared `implemented` callback whose body is always empty is caught
only by review. Registration is not self-validating. A legitimate empty response
alone must not be used to infer lack of support.

The hook already exists: tests take `&dyn Exchange`, inspect
`Exchange::capabilities()` with both brand predicates, and exercise its public
history/start methods or `BaseExchange::place_order` / `place_orders` with raw
requests. The latter validate before typed dispatch; the harness must not bypass
that boundary or manufacture tokens. Instantiate venues with recording host and
transport implementations so requests are captured and fixture responses are
returned. No production callback execution is used to infer a brand. The
`core_contract` two-venue tests demonstrate this dynamic entrypoint; the future
registry test must replace toy venues with every registered venue.

## Host-only health classification

The exchange-core default has no regex dependency. Its `health` feature enables
`ExchangeHealthMonitor` for hosts; the `safe-cex` facade enables that feature by
default. HTTP interceptors classify status counters without running the health
message sweep. CI tests the default core and all features separately, including
both wasm builds. The source patterns are generated and checked against live TS.

Client-ID rules are shared in `client_order_id`; caller IDs remain verbatim and
map to exactly one native payload or emit an error before any write. Rule pattern
validation is an explicit predicate, not an automatic Rust regex conversion:
Orderly's negative lookahead, JS UTF-16 dot length, and JS whitespace require
source-compatible tests. See [pattern evidence](CLIENT_ID_PATTERN_EVIDENCE.md).
RemoteSigner availability extends its transport signer with both source methods;
payload-signer presence is an independent capability. Journal folding accepts an
owner-provided callback and never imports private venue derivations.

Branded capabilities bind a callback explicitly. A stateful venue can register
`|venue, args| venue.place_order(args)` and implement the typed `Exchange` hook,
as the example does; it need not downcast a trait object or capture a strong
reference to itself. Register the analogous history/start hook only when it is
implemented. Empty overrides use `empty_override` so the inert predicate remains
true. Callers send raw order options through `BaseExchange` entrypoints, while
the venue hook receives only validated payloads.

Venue websocket signing is an asynchronous `Exchange::sign_ws_auth` hook; call it
through the venue trait to preserve Kraken and Coinbase overrides. The base
helper retains the crypto crate's documented WS envelope projection. Subscription
hooks also dispatch through `Exchange`; multi-symbol subscriptions emit one
`SubscribedCandle` carrying `candle`, `symbol`, and `interval` (flattened on wire).
The committed health-feature policy test checks Cargo's actual default graphs:
regex stays out of the core graph and remains present through the host facade.

Proxy JWT expiration lives in `proxy_token::is_token_expired(token, &VirtualClock)`. Hosts return a clone of their existing service clock through `ProxyTokenHost::virtual_clock`; both proxy token client and future access-token adapters must use this shared helper.

## Cancellation and host scheduling

Position-fetch work is detached: cancelling its initiating waiter does not
cancel the shared fetch or its store update. The host must still drive the task.
If the host discards an unpolled or pending task, a guard created before spawn
settles all waiters with an error; it cannot promise a store commit without
executing the fetch. Rejected/disposed shared slots retain the source's behavior.
See the [candle-cache guard contract](../safe-cex-candle-cache/CLAUDE.md).

Reopen resync constructs its latch guard before creating or spawning the snapshot
future. Discarding an unpolled or pending task releases running and queued-rerun
state across sibling sockets; it cannot complete the discarded snapshot. Normal
completion disarms the guard after draining the queued follow-up.

Watchdog tasks carry a pre-spawn generation guard: cancellation and rejected
sleeps release the retained handle, inline completion cannot install a dead
handle, and cleanup from an older generation cannot remove its successor.

Remote-signer waits release both their subscription and waiter slot on completion
or cancellation. The waiting future owns that guard directly; no detached task
is involved, and late availability callbacks cannot emit a connected status.

Native reconnects arm the account reconnect event before invoking the venue
dial operation, bind it to the replacement socket generation, and emit on the
actual open event. Both public/private paths and inline hosts exercise that
contract; direct calls to the event emitter only test its separate throttle.

Socket timer and owned-task slots carry guards created before pinning. Each
release checks its unique slot identity, so a superseded task cannot remove a
replacement. The owning reconnect timer also releases `reconnecting` on drop or
sleep error: a live-account task discard is not the TS disposal-path exception.
Inline completion never installs a dead handle.

Health recovery retains its original deadline if the host discards the task or
rejects its sleep. The next status observation or recorded error reconciles an
expired orphan; it never recovers early or restarts the window on another warning.
Live timers keep normal host scheduling. Recovery timestamps describe the actual
observation, not a timer execution that never happened. Reset, critical errors
and disposal invalidate the orphan deadline before cancelling old work.

Nuke operations use an armed cleanup guard. Completion, error and cancellation
reset the latch; repeated cleanup followed by Drop cannot clear a newer
operation's latch. Worker reply cancellation unregisters its listener and waker,
so late replies cannot revive a dropped caller. Host callbacks and task drops
run after RefCell guards have been released.

Health regex generation translates an unescaped dot outside character classes
to `[^\n\r\u{2028}\u{2029}]`: JavaScript dot excludes all four line
terminators. Do not insert raw Rust regex dots for source patterns. ASCII source
case folding, word boundaries and JavaScript whitespace also require the
existing translator; new patterns must pass the live TypeScript oracle.

Venue crates depend on this crate. Registry constructors belong in
`safe-cex-registry`, whose dedicated layer is directly above venues. The
`safe-cex` SDK facade sits above registry. This core must not depend upward on
venues, registry or facade.

Health alone enables the optional regex engine; default features are empty, so
venue defaults exclude regex. Workspace pins =1.13.1 explicitly; upgrading for a
security patch requires changing that exact requirement. Client-ID predicates
use JavaScript character semantics, not the health regex engine.

JavaScript whitespace and numeric text delegate to safe-cex-num shared shims.
The websocket scanner's character predicate calls js_trim on one scalar; it
holds no independent whitespace set. js_number_string supplies numeric spelling
for JSON strings, client IDs and JWT coercion; JSON null/non-finite policy stays
with each caller. The core has no direct ryu-js dependency.

HTTP hosts must forward the original adapter `TransportError` in
`HttpFailure::transport_error`. HTTP-status failures and failures outside transport
use None. Missing response is not a network classification: it also covers
signing failures and timeouts. The core preserves this provenance through error
enhancement. Venue retries must consume the transport crate's classified read
token, re-enter signing on each attempt and return the last attempt's full error;
they must not replay the first signed request or infer retryability from text.


HTTP factory codecs (`HttpClientOptions::params_serializer`) obey **sign, then
serialize, then transmit**. Installing a tunnel adapter wraps the new inner
transport with the same serializer before replacing the remote signer's inner
adapter. This preserves the ordering protected by the TypeScript API-factory
regression audit against direct `defaults.adapter` assignments.

The following precondition is inherited from Axios: a signer signing a specific
query encoding must bake it into `signed.url` and omit params. Returning params
asserts that the signer's encoding agrees with the factory codec; otherwise array
format, space spelling or key order can change the signed bytes on transmission.
The reported Kraken regression makes this operationally important: a query-order
mismatch returned HTTP 200 with status `unauthorized`, looking like a dead credential;
it succeeded only when the caller happened to supply parameters alphabetically.
The core cannot verify that assertion. It never reparses a returned URL query or
restores a body omitted by the signer. Recorder tests prove preservation of the
signer's output bytes, not validity of a signature.

For a nonempty serialized string, Axios URL assembly removes the first `#` and
everything after it, then appends `?` when no question mark exists, otherwise `&`.
An empty serialized string leaves the URL, including any fragment, unchanged.
Params are consumed so the host cannot serialize them again. Serializer errors
refuse before transmission with zero sends; no unserialized fallback is allowed.
All codec errors become failures with absent network provenance, even if a callback returns a
Network variant, so the classified network-read retry policy cannot retry them.

`HttpHost::execute` must drive the supplied adapter exactly once. Building a wire
request directly from RestRequest bypasses both remote signing and serialization.
The remote-signer/tunnel recorder tests exercise the host-to-adapter path, including
signer-produced query bytes and authoritative body omission; success-only host
stubs do not prove that host contract.

Reconnect-hook failures go to `ExchangeRuntime::report_exception`, separately
from the transport-event error dedup window. This CHOSEN host reporting boundary
does not claim to equal a JavaScript uncaught exception. The source throw skips
post-hook listener, reconnect-latch and timestamp updates; Rust preserves that
short-circuit while clearing its internal pre-armed listener intent.

UI and worker leverage requests call `Exchange::fetch_leverage_for_symbol` on
the venue, not the base helper directly. Its default delegates to the existing
base fetch/store-update behavior; venue overrides can return asymmetric leverage
or their own errors without invoking the base max-leverage request.

Per-socket postprocessors use `BaseExchange::get_socket(SocketSlot)` to retain
public, inverse and secondary identity, including missing slots. The grouped
getters deliberately flatten absent sockets and cannot identify a named slot.
The read-only getter clones the Rc and releases the borrow before returning.

Order refresh and private snapshot callers dispatch through the matching
`Exchange` hooks. `WsExchangeParent` uses that same dynamic snapshot hook, so
a reconnect preserves the venue's snapshot concurrency and reconciliation policy.
The base implementations remain callable explicitly by overrides delegating to
source `super` behavior; they are not worker dispatch entrypoints.

Destructive orchestration (`nuke`, `nuke_positions`), teardown (`dispose`),
`set_all_leverage`, client-ID lookup and market refresh dispatch through `Exchange`.
Their defaults delegate to the base implementation; concrete venues override the
trait hook to retain their source policy. The blanket `ExchangeView` supplies
an object-safe self reference for default orchestration without per-venue glue.

The override contract has two independent gates in `tests/override_contract.rs`:
`override-inventory.cjs` resolves TypeScript class ancestry and inventories every
callable override of a BaseExchange member; each must have a dynamic hook (or the
explicit typed-placement binding). The Rust source audit rejects direct base
bypasses and captured base method references, and pins each function's hook call
edges (kind, hook, receiver base/dynamic/path, count) in `override-call-sites.json`;
unrelated body edits keep the pin, edge changes fail. This is a
syntax/alias audit with review-required inventory changes, not Rust type inference.
It sees local receiver/alias syntax, not ambient type-alias or field-type changes
outside the function; macros are unexpanded. Those still need semantic review.
The audit excludes inline cfg(test) modules and direct #[test] functions, even
when their module is stored in a separate file. It does not exempt a file merely
because it is named tests.rs; unannotated helpers are conservatively audited.
A test fixture and production sibling witness keeps that distinction pinned.
Placement and the worker post-hook base wrappers are allowed only while their
bodies actually dispatch to the venue. Deliberate source `super` delegation needs
an explicit reviewed classification; do not bypass the gate by renaming a receiver.

To regenerate the source inventory, run
`node scripts/override-inventory.cjs > tests/venue-overrides.json` from this crate
with `SAFE_CEX_TYPESCRIPT_PATH` pointing at the existing isolated compiler.
After reviewing a changed Rust call site, run the `production_callers` test with
`UPDATE_EXCHANGE_CALL_INVENTORY=1`; ordinary local tests and CI never set it.
The live source test runs in the existing mandatory core `--include-ignored` step.

`HttpFailure` is a transitional compatibility alias of transport `RestFailure`;
new adapter code should use the latter spelling. `HttpClient` implements
`RestAdapter` directly, preserving structured errors through secondary clients.
Signer/preflight refusal calls `HttpClient::refuse_before_transmit`; it sends
nothing and precedes common request stats. Serializer refusal uses the same
non-network failure constructor inside the adapter, after one request was counted.
These counts deliberately differ because Axios rejects at different pipeline
points. Neither refusal carries network provenance or becomes retry eligible.

HTTP request bodies explicitly carry `RestBody::Untransformed(Value)` or
`RestBody::Encoded(Vec<u8>)`; the body type deliberately has no `Default`.
Secondary client selection forwards that representation unchanged. Local signing
hosts/interceptors must use `untransformed()` before signing: encoded bytes refuse
instead of being reparsed to guess origin. Binance's string-body index enumeration
is a source bug that this distinction must remain able to represent. An object
and a JSON-looking string can have identical wire bytes and different signatures.
Only after signing does the HTTP adapter transform data. With an actual content
type containing `application/json`, strings first undergo JSON syntax validation:
valid JSON text is JS-trimmed; invalid text (including BOM-prefixed JSON) is
encoded by canonical `js_json_stringify`. Without that content type, strings
remain raw. Other JSON uses the same canonical encoder. Signing retains the
original whitespace and raw/empty string, while transmission may trim or quote
it. Header names are case-insensitive; the content-type value substring is
case-sensitive, matching Axios. Encoded bytes are never transformed again. Installing a tunnel
adapter retains this transform with the factory serializer. The remote signer's
source-mandated `tryParseJson` remains a separate, explicit source operation; it
is not permission to infer the origin of an `Encoded` body.

Remote hosts implement the single transport `RemoteSigner::sign` operation using
an exhaustive `RemoteSignerRequest::{Http, WsAuth}` match. WS uses the canonical
`safe_cex_types::ws_auth::WsAuthRequest` also re-exported by crypto: retain
`extraData`, including Bybit's `expires`, and reply with `wsAuthSignature`.
A host without WS signing must return an explicit error, never project WS into
HTTP config. The core availability trait still extends this same operation.
Host endpoints dispatch through `dyn Exchange::sign_remote_request` to preserve
both concrete HTTP and WS venue overrides. The HTTP-only hook takes
`HttpSigningRequest` and returns `HttpSigningResponse`; it cannot receive WS.
Wire responses have no kind discriminator. Use the originating request's
`decode_response` method so an error-only response retains its operation kind.

Retry policies require the host `RateLimitClock` at `execute(adapter, clock)`.
`with_delay` selects only the delay function; the same execute clock measures
elapsed request time and performs sleeps. The original request timeout is the
only input budget. Remaining time is derived inside the engine by subtracting
elapsed time and the next delay, and exhaustion returns the terminal failure
without sleeping or sending again. No caller supplies a remaining-timeout value.
The installed Axios truthy guards retain timeout zero as unlimited and skip the
first subtraction for a clock starting exactly at epoch zero.

Transport JavaScript coercion failures retain their name and unprefixed message
when converted into ExchangeError. The original RestFailure remains the opaque
origin. Direct JsCoercionError conversion retains its typed native cause too;
Binance and BingX signature predicates use these conversions instead of rebuilding
an Error from text. The native order/account witnesses cover resolved envelopes
and HTTP rejection, and assert that coercion failures never trigger a resend.

The JS REST adapter keeps string error-code UTF-16 units in the existing owned
callback origin; `RestFailure.code` remains its explicit UTF-8 diagnostic view.
The shared ExchangeError conversion applies response and context views to both
retained callback causes and ordinary failures, preserving the complete original
RestFailure for downcasts. The constructed Aster rejection regression compares
normal and lone-surrogate codes to the maintained source; the core origin test
also checks response phase, own undefined, signed zero, keys and request metadata.

`RestFailure.context` is boxed to keep the response-bearing error small; the
metadata is unchanged. `RestFailure.code` preserves general host/status error codes as well as the code
copied from opaque NetworkFailure. The library network-or-idempotent helper is
separate from network-only provenance classification: it consumes actual error
code, response absence/status and Axios's normalized lowercase config method.
A venue's explicit 503/504 OR belongs in its classified-read condition, never in
the library helper. The helper's input request represents Axios error.config;
hosts must retain that originating configuration when projecting an Axios error.


Retry request data has a third explicit `RestBody::Transformed` state. Its
`TransformedData` payload has private construction and no `Default`; the shared
transform mints it while the JS data origin is known. It is distinct from opaque
`Encoded` bytes. `untransformed()` supplies signing input for the current attempt,
including the actual transformed string on a retry. A local signer replacing
`data` must use `with_signed_data` to retain the transform phase. Retries use the
Axios identity transform, even if that signer replaces the string again.

Retain failure config after signing/data transformation but before query URL
serialization. This preserves first-attempt timestamp headers and avoids
appending params twice. ts-bug: JSON-stringifying venue signers can sign raw JSON
first and quoted JSON later while the wire bytes remain identical. The maintained
retry-body oracle captures both views, including raw text, empty strings and
whitespace-valid JSON, rather than asserting parity from wire bytes alone.

The retry engine REVALIDATES every returned failure config against the ORIGINAL
read classifier before replay. A config changed into a write is refused with the
terminal error; it is never sanitized into a read. Timeout remains the engine's
derived remaining budget rather than a value adopted from error config.

Getter overrides are part of the venue dispatch contract. The live override
inventory includes get accessors as well as callable methods/properties.
`Exchange::can_sign`, `is_public_data_only` and `name` dispatch per concrete
venue. Base private snapshots and account-market-overlay decisions consult the
venue privacy hook; the default privacy hook consults the venue signing hook.
This matters for account-identity authentication independent of local secrets.
The base privacy/overlay helpers require the venue argument rather than silently
reverting to key-based classification. WS bridge construction reads the venue name.


Options identity has one shared owner in BaseExchange and every WsExchangeParent.
Read `base.options()` or `WsParent::options()` for an owned snapshot; no RefCell
borrow escapes. Resolve account identity through `patch_account_identity` with
explicit Unchanged, Set(String), or Remove per applicationId/vault. Empty strings
remain present values. A bridge created before resolution sees the new identity
on its next read. Retaining a prior snapshot does not reproduce JavaScript's live
options alias; the constructor row declares this CHOSEN trade and a live source
constructor witness records both views.

Rc is intentional: existing browser main-thread, webworker-local and native
local-executor hosts already use Rc Store/runtime/listeners and non-Send futures.
This does not introduce a new Send/Sync restriction. Cross-thread Exchange sharing
would require a broader core migration, not replacing this one Rc with Arc.

Cancellation discovery and classifications live in
[scripts/CANCELLABLE-FUTURES.md](scripts/CANCELLABLE-FUTURES.md). The gate fingerprints
production creators and named Drop implementations, requires task-drop witnesses,
and fails on open findings rather than treating a routed defect as covered.
Both waiter-retention repairs are included and their original public witnesses
now assert release at waiter drop. Discovery counts are not counts of runtime
drop paths proved safe.
Axios header assembly is a pipeline operation, not a property of IndexMap or
serialize_headers. Merge factory defaults and request headers case-insensitively
before the signer, then normalize again after signer output. The first spelling
survives while the last value wins. Plain-object serialize_headers deliberately
retains its entries, including case variants.

The JS client gives source request interceptors an AxiosHeaders facade with
inherited `set(name, value, rewrite?)`, `set(record, rewrite?)`, `get(name)` and
`has(name)` methods. Header casing, value normalization, fluent receiver identity
and own-key order are checked against installed Axios; methods never become wire
entries. Parser/matcher overloads of get/has fail visibly until bound. This does
not widen the transport's string-valued header ABI to Axios's false sentinel.

After signing, the shared request transform generates application/json for
object/array data when Content-Type is absent, and dispatch supplies
application/x-www-form-urlencoded for POST/PUT/PATCH when still absent. Explicit
Content-Type remains authoritative. Null without JSON content type remains null
in the retry configuration and produces no wire body. Generated headers must not
be moved before signing: the live header oracle compares both interceptor input
and adapter output. The string-valued header ABI cannot represent Axios's false
sentinel that disables a header; this operation does not widen that ABI.
The factory also retains Axios's default Accept value, with case-insensitive
caller overrides. The byte-oriented transport projects non-null primitive adapter
data to bytes; the oracle observes Axios adapter configuration, not a claim that
all browser and Node network adapters accept the same primitive body types.


Message callbacks can launch non-dial work with `start_message_operation`.
Execute the source synchronous prefix in the callback, then pass only the async
continuation. JS async functions execute through their first await immediately;
Rust futures are lazy, so the owned start does not promise eager execution on a
deferred host. Capture venue auth cleanup guards before constructing the future
so even an unpolled discard clears them. The operation returns immediately and
shares socket disposal without changing connection generation or dialing.

Failures reach `ExchangeRuntime::report_exception`, independently of transport
error deduplication. Production hosts MUST make that callback observable through
console, telemetry or an error event. Actual host delivery is a host-integration
acceptance obligation: this tree currently contains only injected runtime
contracts and test implementations, so recording-host tests prove routing only.

Venue request interceptors belong inside `HttpClient::execute_with_interceptor`:
core first assembles factory defaults, then awaits the venue callback, then runs
the existing request accounting, enhancement and transport pipeline. Header
removal by the callback is authoritative; no defaults are merged again. A venue
that prepares a request before calling plain execute cannot express deletion of
an absent factory header. Interceptor errors return through shared enhancement
before common request statistics. There is no public bypass flag or prepared
request escape hatch.

The post-merge cancellation inventory discovers 115 production creators; it does not prove 115 creators cancellation-safe. At aa632b5578 both existing public Weak-sentinel waiter witnesses flip: WAITER Drop immediately releases the sentinel and subsequent TASK Drop performs no cancelled-caller wake. Their classifications now name the per-future Registration guard. Four AST fixtures and 11 classifier controls cover discovery and classification mechanics, not semantic proof of the whole graph. New interceptor and non-dial message wrappers are classified by their existing lifetime owners, not by adding a venue-owned registry.

Cancellation witnesses cannot be ignored or hidden behind an unverified cfg/cfg_attr. AST test presence alone is not execution coverage; the gate accepts cfg(test) and the explicitly enabled health feature in its mandatory all-features native lane and refuses conditional witnesses whose execution is unproved.
Remote signing has two Axios stages. The hosted server transforms its request
before entering the remote adapter; that adapter applies source tryParseJson,
which restores only objects and arrays. JSON primitives remain transformed
strings when they reach the browser's signing interceptors. The browser then
runs its own signing and post-sign transform. Do not move the local signing
boundary to imitate the server stage, and do not parse primitive JSON strings
back into booleans, numbers or null. The whole-source remote oracle executes
both Axios clients and the production remoteSigningAdapter.ts with real nanoid.

Shared position/token fetches give each waiting future its own waker registration.
Dropping a waiter unregisters its waker without cancelling or settling the shared
operation. Waker equality is not ownership: independent futures may share one
executor waker. Re-poll replaces only that registration. Registration guards are
captured before pinning, and wake/replacement/drop release RefCell borrows before
invoking or destroying host wakers.

Handler statistics read missing own handler keys through canonical TableHit /
ProtoMember arity: an inherited function contributes its length and __proto__
contributes undefined, distinct from a missing key's 1 (record) or 0 (reconcile).
Own handler lists shadow inherited members, including empty lists. The stats
object has a second prototype read: recording an inherited name does not create
an own stats record. Rust retains that absence, but does not model mutation of
the shared function/Object.prototype referent. The three stats rows remain
FINDING for those live aliases and inherited enumerable properties, including
source reconciliation throws caused by __proto__ writes. The 78-case live oracle
captures typeof and referent fields before JSON serialization; it is not a claim
that the global dictionary-write boundary has been implemented.

`record_message_handler_stats` returns Result and dispatch propagates its error
before invoking callbacks. The immediate __proto__ reconciliation TypeError is
projected without writing Object.prototype; no panic substitutes for it. Later
cross-socket effects remain CHOSEN FINDING: a global side table is representable,
but reproducing process-wide prototype corruption is deliberately declined.

Remote signer output goes directly to the selected inner adapter: a returned
object remains an object, null remains null and omission remains absent. The
client response may come from an echo signer rather than a browser Axios adapter;
there is no additional Axios transform after that response. Transport records it
as Transformed adapter data using a crate-private constructor, retaining origin
without parsing encoded bytes. Physical network adapters own byte projection.

The HTTP params wrapper selects its phase internally from the installed remote
adapter, including replacement transports. Local requests still transform after
signing; remote output only serializes query params and preserves returned data
and header deletion. There is no public caller flag for skipping the transform.
The maintained oracle runs both actual browser Axios signing and an echo signer,
and compares exact inner-adapter data types as well as complete headers.

The syntax-only override scanner distinguishes a locally declared BaseHandler
(or Rc<BaseHandler>) field from BaseExchange for direct self.field calls. Unknown
field types remain conservatively audited. This is not compiler receiver or
import-alias resolution; a control retains exchange base/core calls in the same
struct and a following impl. The call inventory still records the worker handler
call rather than deleting it.

Response enhancement reads `RestResponseBody::json_value` as a model view and
falls back to current adapter data for non-JSON bytes. It does not apply a venue
response parser. Factories that install one use the shared current-data transform,
which preserves the distinction between raw quoted HTTP text and a HOW parser's
already-decoded string. Constructors must state the response representation.


Listener callbacks and `BaseExchange::emit`, `log`, and `emit_and_log_error`
use one variadic dispatch operation. `emit_args`/`on_args` retain the argument
list; `emit` sends one value and `on`/`once` observe its first argument. Array
values are never interpreted as argument lists. The JS facade and worker log
bridge use the full list, while native fill listeners receive the fill object.
Callbacks and emission methods
return `Result<(), ExchangeError>`. TinyEmitter snapshots the current listener
list, removes a once listener before invoking it, and stops on the first throw;
Rust retains that order and invokes callbacks outside all listener-map borrows.
Store subscriptions use this same emitter operation. A failing update listener
is attached as StoreSubscriberError's opaque original cause, and converting the
Store notification back into ExchangeError recovers its name and full metadata.
A Store write commits before notifying; no rollback or successful-unit bridge is
introduced. Later subscribers and listeners do not run after a failure.

Callers retain their own catch boundaries. Private snapshot retries catch both
fetch and Store-notification failures; an error-emitter failure escapes before
the next retry sleep. Leverage's catch includes its Store update and can itself
fail while logging. Position fetch waiters share the fetch result only: a Store
notification failure belongs to the originating operation, while an existing
waiter still receives the fetched positions. The owned task still performs the
write if the originating Rust future is dropped, as before.

Synchronous notification callers now expose Result, including disposal and
socket-parent hooks. Runtime-owned websocket/timer callbacks report uncaught
listener errors through the required observable host contract and stop the
remaining operations in that callback. This is a CHOSEN host projection, not a
claim that this repository contains production host implementations.
The remote-signer availability callback retains the source settled-before-emit,
resolve-after-emit order: a connected listener throw is reported and leaves the
source promise unresolved even through subsequent disposal. An initial status
throw instead rejects the awaited operation before callback registration.

The maintained TinyEmitter oracle also executes the actual worker findBestPrice
and emitComplete bodies. These pin a consumer contract: log failure precedes
markComplete; error failure follows it; completion catches its listener error
but a failure in the recovery log escapes. Core tests demonstrate the fallible
surface against that order; worker orchestration must still consume these
results in its own migration, and these tests do not claim that migration done.

Resync completion has a source `finally` boundary: even if its catch logger
fails, a queued follow-up is started unless the parent is disposed. It must not
be treated as ordinary callback short-circuiting. Core performs that bookkeeping
and creates the follow-up before reporting the original logger rejection. The
reporting delivery/timing is a CHOSEN host projection, not a native Promise
unhandled-rejection scheduler. When a Store subscriber has no opaque
ExchangeError cause, its canonical response still survives conversion so catch
sites retain response.data.msg precedence.

Source-oracle comparisons must assert native field identity before serializing
typed records. A paired serde-name swap can cancel during a roundtrip; distinct
fixture values alone do not catch it. Reporting flags and placement precision
also need differing sibling values. Health's existing single-error corpus does
not establish multi-entry ordering or exercise the Network category.
Live source oracles return a whole corpus from one Node invocation; compare
individual cases in Rust rather than spawning Node inside a case loop.

Store-notification conversion preserves subscriber IDs structurally in
ExchangeError.store_subscriber_ids, ordered inner to outer. Recovering an opaque
ExchangeError appends the current notification ID without changing its original
metadata or mutating the cause. Fallback projection retains both ID and response.
IDs remain local to their Store; this list is notification provenance, not a
global subscriber identifier. Non-notification conversions add no IDs.


Individually cancellable operations share one host-handle registry per exchange
or socket. `OwnedOperation` is weak; dropping it does not cancel siblings after
an outer call returns. Explicit cancellation removes the registration before
host cancellation, releases captured work immediately unless it is currently
being polled, and finishes that poll before releasing its body in the latter
case. Host wrappers may remain queued but hold no cancelled work. Owner disposal
closes the registry permanently before fallible notifications. Socket timer and
relay selectors retain exact identities; old cleanup cannot remove replacements.

`ExchangeRuntime::monotonic_now_ms` is required, with no wall-time fallback;
it replaces the socket-only monotonic getter. Sleep must use compatible elapsed
time and yield a scheduler turn even for zero delay. New timeout/interval APIs
accept already-normalized delays from 1 through 2147483647 milliseconds. They
reject invalid delays explicitly rather than implementing JavaScript argument
coercion. Intervals capture time before their synchronous callback and schedule
captured time plus the period, yielding even when overdue. Reinsertion occurs before a callback error is
reported, matching Node finally order. Core polls the next sleep once before
reporting so lazy hosts register it too; a reentrant reporter may cancel that
registration. An explicit scheduler yield prevents an already-ready sleep from
running the next callback in the same poll; completed futures are not repolled. A sleep error is reported
and ends the operation. Async source prefixes remain in the calling callback.

`BaseExchange::start_owned_operation` owns a unit-output async tail. A caller
must handle its error policy explicitly: KuCoin's Promise.all late transport
rejection belongs at a named consumed-discard site with its ts-bug note. Core
owns cancellation, not a universal post-return error-emission policy. The core
owner-lifetime tests do not claim to implement KuCoin's aggregation or its seven
actual-source scheduling and returned-array witnesses.

## Shared income range paging

`income_paging::paginate_income_range` owns no executor or transport. Its awaited
fetch callback returns the shared typed ledger; its awaited delay operation
receives the original f64, leaving host timer conversion explicit. Only fetch
failures are caught: the branch becomes incomplete, while delay failures escape.
Caller drop cancels its owned pending callback future.

The pending-window stack is LIFO. Direction reads use the original global range,
not the current subwindow; page length includes unusable rows. Identity retains
both PnL and commission from the same bill, keeps the first duplicate, and stable
newest-first sorting preserves insertion order on timestamp ties. No termination
guard repairs source floating-point nonprogress for non-advancing chunks.

Typed `IncomeRecord` excludes nullish/coercible/open source records and UTF-16
lone surrogates. Returned records and options are owned rather than live aliases.
These are declared choices, not source-equivalence claims for arbitrary objects.
The six existing income-paging manifest rows own the shared implementation;
binance, bitget, kraken and weex each have one separate wrapper call to adopt.

The unchanged 438-line TypeScript feeder is run by real Vitest through a labeled
supplement, once per Node process. Rust compares every request and decoded native
record field before serialization. See `INCOME_PAGING_EVIDENCE.md` for its byte
hash, scope and mutation controls; supplements claim no historical independence.

The HTTP serializer consumes QueryValue. A request's explicit query profile wins
over the factory callback and survives signing/replay. Retry restoration compares
retained number bits, so a NaN clone matches and changing a zero sign does not.
Factory profiles are copied before the venue interceptor; no global array format
is assumed. Query JSON diagnostic projection is separate from URL rendering.

Shared journal intervals and fold projection live in position_intervals and
fold_position_adapter. PositionInterval's camelCase Serde bridge lets production
venue bindings pass the canonical interval through native JS values; optional
fields omit absence and the shared native serializer retains floating-point values.
Its private remote decoder requires an object and uses the existing numeric ingress
pipeline; raw number spelling and native nonfinite/signed-zero bits must survive.
No second interval model or fold implementation is introduced.
Reconstruction is path-dependent: seeds remain incomplete,
partial missing reducing PnL refuses a total, and crossing fills split fees by
size while assigning all realized PnL to the closing interval. Funding includes
both endpoints and remains unattributed for hedges/incomplete/open intervals.
Folding preserves reported zero PnL but refuses absence; gross PnL passes through
unchanged. Native callbacks are infallible and leg values are closed; malformed
surviving seed legs refuse at the final model boundary. Bare valid leg keys retain
source slice(-1) symbols. Native stable sort does not claim V8 comparator schedules
for non-transitive mixed-ID ties. No journal grading or venue fetch composition
is implied. The scoped live test is `--test journal -- --include-ignored`.

build_records ports the standalone journal reader over a callable FoldableExchange
projection. Income failures are fatal when their symbols drive scoped fills reads;
account-wide reads continue with unavailable funding. Errors retain a wrapped
native cause and source name. Typed shared read rows/errors exclude malformed
optional income rows and raw falsy JS rejections; native futures are lazy.
Read symbols in first-seen order, request the 14-day straddle lookback, fold
account-wide fills per symbol, then omit pre-window/open intervals. The existing
JournalFolder host seam is not automatically bound to this implementation.


Venue callbacks that test `this.ws` must use BaseWebSocket::has_websocket, not
is_connected or diagnostics.initialized. Presence includes connecting and closed
instances; initialized records first received traffic. The source synchronous
constructor and core-owned asynchronous dial can still differ before installation;
venue adapters must declare that timing boundary rather than invent socket state.

HTTP hosts now return RestExecution and forward the supplied adapter's receipt.
Common response hooks consume returned effective configuration; no input clone
can stand in for response.config after signing or middleware. Codec projection
reverses only its own unchanged wire fields, retaining actual inner replacements
and omissions. HttpClientOptions.response_transform, when selected by a venue,
runs before common success hooks and before failure hooks on rejected responses.
Adapter installation preserves that callback. Factories without a selected
callback retain their existing model-view behavior.

start_shared_operation publishes a shared handle through its builder before
spawning, then delegates task ownership to OwnedTasks. Each wait has its own
Waiters registration; dropping a waiter releases its observer without cancelling
shared work. Owner cancellation and host task discard settle retained observers
with a visible error. Closed owners skip the builder. Result cloning and waking
occur outside state borrows, and the first settlement remains authoritative.

Structured WebSocket redaction drops the source fresh-object __proto__ assignment
from own output keys at processed depths. Both the shared utility and core socket
path preserve the depth>12 passthrough; native own-key assertions precede wire checks.

Worker offline recovery dispatches through the object-safe Exchange hook. Each
socket enforces its protocol recovery flag after disposed/force-closed guards;
the base visits every socket before reducing the boolean results.

A source global timeout that survives exchange disposal uses
`start_detached_timeout` with a normalized delay. It reuses the existing timer
and Driver lifecycle in an independent registry; dropping its returned handle
does not cancel it, while explicit cancellation releases its sleep and callback.
Ordinary exchange/socket registries retain their disposal ownership.

The exchanges-level audit and retained options gap are tracked in
[the resolution](../parity/EXCHANGES_DIVERGENCE_RESOLUTION.md). Signer and income
entrypoints execute their initial source calls before returning their futures;
caller-drop and reporting choices cite MANIFEST-SPEC.md#exchange-runtime-callback-choice.

The shared function `call` member invokes the original target with the supplied
strict receiver and remaining arguments. It preserves native values, returned
references/promises and arbitrary thrown values without awaiting or JSON.
Non-callable receivers reject before reading conversion hooks. Own function
`call` properties/getters retain precedence. The native/Node witness
is `tests/js_function_call.rs`; function construction and prototype metadata
remain separate capabilities in the Hyperliquid shared-runtime consolidation.

The shared JS runtime supplies `Function.prototype.bind` for venue callbacks,
retaining the target receiver and prepending bound arguments even when the result
is rebound or called by an array helper. Own function properties take precedence.
`JSON.stringify` supports callable replacers: `toJSON` runs first with the actual
property key, then the replacer receives the live holder, key and value, including
the root and array holes. Traversal captures keys/array length before reading
individual values, so earlier callbacks can change later siblings. Unreplaced
BigInt and circular structures throw; replacers can convert or omit them.
An array replacer becomes the spec PropertyList (string/number items, first
occurrence kept) and selects object keys even when non-enumerable; indentation
remains outside this runtime surface.

The JS HTTP client's retry loop resubmits the failed response's effective config,
including interceptor changes and retry-predicate stamps. Rebuilding from the
initial input loses one-shot signature-retry guards and can resend more often
than the venue allows. Request interceptors still run again to refresh signing.

## Native host (`native` feature)

The JS socket constructor forwards protocols and connection tuning. Required
upgrade headers refuse unsupported hosts before a dial. A retained header
provider runs in each core-owned dial preparation; a thrown auth error prevents
that attempt. Native reconnects call the venue's constructor path again, so the
next attempt regenerates headers. The native host accepts the source's header
wrapper selection as a plain authenticated socket; the core owns reconnects.
Header-less requests for a host-managed reconnecting wrapper still refuse.
`ws_options` tests compare actual source constructor inputs and verify two fresh
upgrade headers at a real loopback server, failed preparation and host refusal.

`native::native_runtime()` is the production `ExchangeRuntime` + `WsRuntime` on a
tokio `LocalSet` over the direct transport; `NativeHttpHost` drives the adapter
once. Sockets are plain (`enable_reconnecting` false): the core owns
reconnection, and a header-less `reconnecting` dial is refused. The
feature's dependencies are target-gated, so `--all-features` wasm builds stay
I/O-free. Inherent `impl` blocks inside cfg-gated modules fail the parity
indexer closed; native modules use free functions and trait impls only.

`native_tealstreet_runtime(token)` is registerTealstreetTransport for a native host:
`TealstreetTransport` over HOW adapters (bigInt = raw format) on a tunnel host that
authenticates with the account's proxy token and checks expiry with the canonical
`is_token_expired`. Proof: `safe-cex-registry/tests/native_host_tunnel.rs`.

The JS HTTP terminal honors `responseType: 'text'` for encoded response bodies,
including non-2xx responses. It preserves raw JSON tokens and whitespace until a
venue's bigint-safe parser runs. The default continues ordinary JSON projection.

Hosted JS venues receive the signer through JsHttpHost, shared by Scope and BaseExchange assembly. Structural exchange_options decoding omits capability handles without altering the JS options object; never serialize hosted signers into the uninhabited JSON markers. The native host binds the exchange name before sending, wraps the existing RemoteSigningAdapter and propagates its effective configuration to source response hooks. Query serialization uses the shared HTTP codec after signing; response/error configs retain signed params and source URLs for retries. Native registry tests exercise secretless validation and private websocket auth over real local sockets.

The js_runtime typed-value bridge uses the shared native serialization capture and native Serde visitors, preserving NaN, infinities and signed zero in scalar and nested model fields. `to_json` remains the explicit wire projection; do not route typed values through it. Native nonfinite values aimed at opaque serde_json::Value are refused rather than silently projected to null. Typed input retains the shared UTF-8 and bounded-depth choices. `tests/typed_js_values.rs` checks both directions independently, actual order-option fields, enum shapes and optional values; venue E2E tests own their receiver-level effects.

JS socket properties are the source of truth for typed lifecycle grouping. BaseExchange installs weak, lazy lookups for the eight source fields; these create socket cores after venue assembly and observe property replacement/removal on each lookup. The JS runtime regression checks source order, public/private grouping and disposal.

The js_runtime socket terminal preserves Binary as the host binary object: native
Buffer and wasm Blob, with exact bytes behind ArrayBuffer. Buffer.toString and
Blob.text perform UTF-8 replacement only when the venue explicitly asks for text;
compressed-frame consumers must read the bytes and use their source decoder.
`ws_lifecycle::js_sockets::binary_delivery_preserves_bytes_until_explicit_text_conversion`
checks non-UTF-8 bytes, object identity, explicit conversion and unchanged text frames.

Venue overrides that assign `this.isDisposed` before socket cleanup use the shared
JS setter or `BaseExchange::set_disposed_flag`. This writes only the core flag;
`dispose()` still owns superclass cleanup. Six actual GRVT/Crypto.com source
cases cover successful cleanup and each throwing socket, shared-cell visibility,
retained listeners, and resetting the writable flag in `tests/js_runtime.rs`.

The js_runtime Object.defineProperty binding retains ordinary-object/function data
attributes, following ToPropertyDescriptor and ValidateAndApplyPropertyDescriptor
in ECMA-262. New fields default to undefined/false; omitted fields on an existing
property retain their value. Read-only writes reject, non-enumerable fields stay
out of keys/spread/JSON, and non-configurable fields survive deletion and reject
incompatible redefinition (SameValue preserves NaN and the sign of zero).
Arrays and accessor descriptors explicitly refuse until a source caller requires
that representation. Existing host define_accessor remains the host accessor seam.
The engine oracle in tests/js_data_descriptors_oracle.cjs checks the data-flag and
redefinition matrix; the native error test covers Polymarket's enhanced message.

The JS BaseExchange facade's `callMethodInWorker` delegates to the existing core
worker RPC owner. Its eager Promise preserves the core reply listener, deadline,
completion hook and reconstructed error properties. Arguments/results use the
existing JSON worker boundary; it does not create a parallel worker protocol.

JS venue imports of safeCexAxiosCreate use `js_runtime::client::factory::create`
with the venue runtime's TransportProvider. Raw `client::create` only supplies
Axios mechanics and does not select proxy routing. The factory resolves provider
REST defaults eagerly, forces the source 20-second timeout, and authorizes a
proxied attempt after venue request interceptors, before hosted signing. It
applies only provider changes, preserving unchanged JS values and AxiosHeaders.
Venues that explicitly call source installTransportAdapter use the matching
factory helper; no implicit HOW installation is inferred from the tunnel flag.
Installed adapters retain the same hosted signer, and HOW keeps frame params
rather than calling a URL serializer. `http_factory` tests compare actual source
Axios behavior and observe proxy-auth/sign/query/send ordering through the native
terminal bridge. Existing source bindings continue to own telemetry callbacks.

Venue overrides that assign `this.isDisposed` before socket cleanup use the shared
JS setter or `BaseExchange::set_disposed_flag`. This writes only the core flag;
`dispose()` still owns superclass cleanup. Six actual GRVT/Crypto.com source
cases cover successful cleanup and each throwing socket, shared-cell visibility,
retained listeners, and resetting the writable flag in `tests/js_runtime.rs`.

The JS HTTP bridge retains JSON-data adapter config in RestRequest.config_extensions,
including GRVT's browser cookie/account fields. It leaves executable Axios hooks
on the source config. Hosted effective config still spreads that source object,
then overlays actual adapter fields; terminal extension updates survive too.
Cookie promotion belongs exclusively to HOW, not the direct HTTP bridge.


The common `Js::Builtin` variant retains an existing native `PrototypeRealm`
intrinsic referent when decoded-data venue bodies interact with core socket
statistics. `Js::from_prototype` preserves reference/realm identity and live
primitive fields instead of copying a snapshot. This is the native data-reference
profile: type/text/arity, scalar field access, enumerable assigned fields and
identity; it does not supply intrinsic function execution or full descriptors.
Unsupported intrinsic field writes are refused. Original-receiver prototype
access and native error identity are covered by `tests/prototype_js_values.rs`.
Binance's `EventValue` is now an alias to this graph, with its existing decoded
coercion profile expressed as operations rather than another heap.
`to_typed_preserving_undefined` is the declared-DTO mode of the same native
visitor: own undefined fields enter as unit values, while callable fields are
refused. Binance uses this existing DTO policy for nested `info`; the ordinary
`to_typed` JSON-shaped omission policy is unchanged. The two modes have independent
presence/null, native numeric-bit and callable-refusal tests in `typed_js_values`.

The shared lodash import table supplies `times` for source order splitting.
It truncates the coerced count, returns an empty array for invalid/nonpositive
counts, and treats non-callable iteratees as identity (not a property shorthand).
Callbacks receive only their index with undefined `this`; results retain their
values, and a thrown callback stops further calls without replacing its error.
`js_runtime::library::tests::lodash_times_*` compares 68 installed-lodash cases
and independently checks native callback/error behavior. The callback-arity
mutation compiles and fails both tests, then passes after restoration.

Configured js_runtime retries retain `config["axios-retry"]` through the effective
failed-config replay. The source condition sees the current retryCount; retryDelay
sees its incremented value. The shared runtime owns the delay, and request start
time resets in the installed request interceptor. Unless shouldResetTimeout is
truthy, elapsed wall time and the chosen delay consume the remaining timeout;
an exhausted budget rejects the original error before sleeping or sending.
`tests/client_retry.rs` compares native traces against installed Axios/axios-retry,
including request overrides, async predicate results, timer failure and loss of
an observer during a pending delay. This covers configured callbacks within the
runtime's existing HTTP and timer profiles.

Dispose delegation reviews name the actual TypeScript class and method span in
`tests/override-base-delegations.json`. The live `dispose-source.cjs` gate resolves
inheritance and AST super calls: a socket receiver type alone cannot justify base
cleanup, and an own no-super override cannot use chosen-base to pass this gate.
The shared BaseExchange bridge cites its actual base implementation separately.
See `../parity/DISPOSE_AUDIT.md` for the complete venue sweep and routed mismatches.

Venue overrides whose source calls only `this.ws.close()` use
`BaseWebSocket::close_current_websocket`. It forwards current native/relay close
without superclass disposal or force-close metadata/task changes, and releases
state borrows before invoking host close callbacks. Native and managed-relay
state-retention witnesses cover this primitive; Backpack's bound disposal tests
compare the source-specific cleanup and order independently.

The shared utility import table also installs Lodash `throttle`. Its scheduler
uses the existing Scope clock and cancellable timers, preserving leading and
trailing options, the latest receiver/arguments, maxWait, cached returns and
callback errors. The returned function exposes cancel and flush. Polymarket
consumes this shared factory directly; it has no venue-local scheduler.
`tests/js_throttle.rs` compares 294 installed-Lodash traces in one Node capture,
including sustained bursts, stalled timers and clock rollback, then checks
native error identity separately. Host timer precision remains Scope's existing
millisecond profile. The sequential `tests/js_throttle_controls.py` faults each
of leading, trailing and maxWait; all three must compile and fail the source
comparison, then pass after exact restoration. A fourth fault removes the shared
import and must fail Polymarket construction, then restore all 15 concrete tests.

The hosted HTTP bridge retains a JavaScript string as typed data until the
remote signing adapter applies Axios transformation and tryParseJson. A form
string cannot become opaque Encoded bytes before signing: that loses its origin
and the transport correctly refuses it. Direct terminal requests keep their
existing byte representation. `hosted_signing_accepts_source_string_data_before_wire_encoding`
compares four form/empty/JSON-looking/scalar strings against the actual source
remote adapter with installed Axios on both sides of the relay, including
signer inputs and final transmitted text.

Shared `Js::to_js_string` converts ordinary live objects with a string hint:
`toString` precedes `valueOf`, each method is looked up when reached, and a
primitive result ends conversion. Non-callable properties are skipped; failure
to produce a primitive throws. The 31-case Node witness compares native UTF-16
units and getter/callback order, including inherited methods and null prototypes.
Stock intrinsic string forms remain the value model's default prototype methods.

The single `js_runtime::base_exchange::create` factory takes a synchronous source
field initializer as its sixth argument. It initializes the JS receiver, reads
`isSpot` and `isDemo` using shared property access/truthiness, then publishes the
immutable typed base and installs its callbacks. Venue adapters own subclass
field values; core contains no venue-name classification. Empty initializers keep
the source base defaults. The hook must only initialize fields, without starting
work or retaining the unpublished receiver. Typed flags are construction snapshots;
later JS field changes do not rewrite them.
Kraken/KuCoin Spot and Bybit/OKX Demo construction tests exercise actual facades.
`tests/source_identity_controls.py` removes each projection, requires compiled
assertion failures in both affected families, restores exact factory bytes and
requires all four witnesses to pass. The runtime test also covers source getters,
truthiness, inherited false defaults and original getter-error identity.

The shared BigInt `toString` member honors radix 2 through 36, defaults omitted
or undefined radix to decimal, truncates numeric radix arguments, and rejects
out-of-range values with RangeError. Radix conversion uses the existing shared
ToNumber profile; BigInt radix arguments retain its TypeError. Extracted methods
read the actual call receiver and refuse non-BigInt values. Native arbitrary
precision conversion supplies lowercase digits without losing integer bits.
`tests/js_bigint_radix.rs` compares 472 engine cases against Node across every
valid base, signed/large integers, defaults and scalar/array coercion boundaries.
`tests/js_bigint_radix_controls.py` sequentially injects decimal-only output,
incorrect rounding and base-two refusal; each must compile, fail the named source
comparison and pass again after exact restoration. No venue-local radix shim is
required.

The JS Store facade constructs scalar model patches with the existing native
number carrier and typed-value bridge. All declared numeric fields of Position,
Order, Ticker and Market retain NaN, infinities and signed zero through partial
updates. Opaque `info` retains its explicit JSON policy; nested option terms keep
their separate typed decoder. Other field admission, omission and atomic patch
validation retain [the Store patch choice](../parity/MANIFEST-SPEC.md#store-typed-patch-choice).
`tests/js_store_patch_numbers.rs` exercises all five model patch methods, with
compiled projection and signed-zero controls in the adjacent Python runner.

`BaseWebSocket::last_received` and `set_last_received` expose the raw source
`lastReceived` scalar without normalization or side effects. Subclasses that
assign it must synchronize health explicitly in source order. The existing
`last_message_timestamp` and diagnostics normalize nonpositive/NaN values to
absence. Native websocket tests check exact numeric bits and the connected
health/timestamp transition through assignment and reset. The adjacent
`tests/raw_last_received_controls.py` requires compiled failures when assignment
is removed or signed zero is erased, with exact restoration after each fault.

Map/Set `values`, `keys`, `entries` and the well-known iterator method now return
live iterator objects. Native computed access uses `Js::get_property` with
`Js::SymbolIterator`, keeping that key distinct from every string. `Js::iterator`
implements GetIterator and `iterable_values` consumes it for spread adapters.
The shared `Array.from`, Map and Set constructors consume the same protocol;
Array.from mapping runs between iterator steps and closes on mapper failure.
Iterator next methods use the actual branded receiver, keep source values and
references, and release the collection on permanent exhaustion. Set keys/values
and each collection's default iterator retain the source method aliases.

`Collection<T>` keeps dense native access with stable insertion positions:
deleting/clearing cannot shift a suspended cursor, and reinsertion appends a new
position. Native host adapters must use its mutation methods rather than replace
the backing Vec; the OKX state and Bitget lifecycle adapters preserve surviving
positions when updating typed views. Kraken, WEEX, OKX, Lighter, Paradex and
Polymarket spread/iteration consumers use the shared protocol instead of array
indexing. Kraken disposal and Lighter iteration advance lazily; Lighter retains
iterator throws, and OKX historical fills spreads the live Map values iterator.
`tests/js_collection_iterators.rs` compares 80 Node traces and checks native
numeric bits, references, aliases, actual receivers, ownership and constructors.
Bybit's `hyro_native_iterator` executes the existing source reset body over these
values, including an ACK rejection that appends another pending ACK; its trace
comes from the current TypeScript method. The adjacent compiled control runner
faults both snapshot-array factories, frozen iteration, terminal exhaustion and
IteratorClose cleanup,
then restores exact source bytes. Algorithms follow
[CreateMapIterator](https://tc39.es/ecma262/multipage/keyed-collections.html#sec-createmapiterator)
and [CreateSetIterator](https://tc39.es/ecma262/multipage/keyed-collections.html#sec-createsetiterator)
within the runtime's existing value/property profile.

The five iterator-related accepted owners have separate reviews in
[ITERATOR_CONTRACT_EVIDENCE.md](ITERATOR_CONTRACT_EVIDENCE.md). Three private
callable factories retain specific non-decoder classifications; `Collection`'s
native `Deref` view and the added `Js` protocol APIs have executed output
observations. Independently authored Node fixtures and per-owner compiled
controls supplement these bounded native/default receipts.

The JS Store facade exposes `markOrdersTerminal` by delegating to the existing
native terminal-memory operation. It remembers IDs even when the Store has no
orders, and preserves committed-state subscriber errors through the existing
mutation bridge. IDs cross the declared typed string boundary without lossy
surrogate replacement. `tests/js_store_terminal.rs` covers absent IDs, stale
snapshots, unrelated IDs, native error metadata and that string boundary; OKX's
real private-frame witnesses cover the production caller and final fill ordering.

Shared builtins for source callers: `Array#pop`/`reverse` (holes mirror),
`Object.getOwnPropertyNames` (own keys incl. non-enumerable; functions and
intrinsics refuse) and `console.log` (runtime console, severity `log`). The Dayjs
facade models dayjs 1.11 `plugin/utc` in the runtime's UTC zone: non-enumerable
`$d`/`$offset`/`$u`/`$localOffset` fields drive `valueOf`, `utcOffset(input, keep)`
and `startOf(unit, start)`, and `dayjs(instance)` clones them. Offset strings use
the installed plugin parser; non-string arguments retain JS coercion and offset
identity. Falsy start flags select the last millisecond of the unit. Native
Date wall values use shared TimeClip.
`tests/js_builtins.rs` compares 16 array, 420 dayjs-chain, 5 key-order and 7
property-list cases against Node with installed dayjs (TZ=UTC), plus the
Lighter, Orderly, Bitrue and Phemex production paths that call them.

`JsProtocol::resyncs_private_state_on_reopen` reads the receiver's
`resyncsPrivateStateOnReopen` member; undefined keeps the private-socket default.
OKX's business socket defines it, so a business-only re-open runs the private
snapshot (`okx/tests/exchange_binding.rs::business_socket_reopen_alone_resyncs_the_private_snapshot`).

The shared Promise global is a native constructor with its existing static
methods. It runs the executor synchronously with resolve/reject functions that
share a first-call lock, including while adopting a pending value. Resolution
reads a thenable's `then` synchronously and queues its invocation in the same
runtime FIFO as Promise reactions;
self-resolution rejects and original thrown values survive. The native scheduler
retains the queued job after observer drop. `tests/js_promise_constructor.rs`
uses NativeRuntime/native HTTP hosting without replacement hosts and compares
executor/thenable traces with Node; Bitrue's production binding witnesses
connected candle-request resolution, cache reuse and timeout rejection.

`__configureRateLimit` registers a request interceptor at the source registration point. Axios LIFO ordering therefore waits before the earlier signing interceptor, while later request hooks still run before the wait. Each registration owns its window. The queued nonce regression and installed Axios oracle are `tests/js_runtime.rs::rate_limit_dequeues_before_signing_and_keeps_interceptor_registration_order` and `tests/rate-limit-signing-source.cjs`.

JS runtime StringToNumber delegates directly to `safe_cex_num::number_from_string`.
It rejects signs inside radix digits (for example `0x+1`) and preserves the shared
full-string, whitespace and arbitrary-length binary64 rounding rules. The native
host `js_string_number` target compares 93 Node results through installed Number,
including signed radix refusals, large values, overflow and signed zero.

Array `sort` and Lodash `orderBy` use one private fallible stable merge sort.
JavaScript callbacks need not define a Rust total order: NaN comparison results
mean equality, and inconsistent callbacks must not panic. The first source throw
stops comparisons immediately. Default Array sorting compares original UTF-16
units, puts undefined before holes and rejects a present non-callable comparator.
The native-host `js_sort` target checks the compiled master panic and error count,
plus Node ordering across merge boundaries. Ordering under an inconsistent
comparator remains implementation-dependent under ECMAScript.

Ordinary numeric conversion of live objects/functions calls `valueOf` before
`toString`, reading each member only when reached and retaining the original
receiver and thrown value. String conversion uses the same private primitive
conversion algorithm with its string-hint ordering. Object results and
non-callable members advance to the next method; no primitive result throws.
The production native-host `js_number_coercion` witness compares Node traces,
negative-zero/NaN results, inheritance, live lookup and conversion failures.

The JS Number `toFixed` builtin delegates to `safe_cex_num::js_to_fixed` after
ordinary digits coercion. Exact binary ties round away from zero; negative zero,
nonfinite values, the 1e21 exponent threshold and RangeError use ECMAScript rules.
`tests/js_number_to_fixed.rs` compares the production native dispatcher with Node;
Polymarket's concrete binding witness checks the resulting signed order amounts.

JS socket listener throws are host exceptions, never transport errors. Plain
EventTarget dispatch continues after each throw; source ReconnectingWebSocket
(header/relay/configured wrapper) dispatch stops its grouped callback at the
throw. The resolved connection records that choice before delivery. Returned
promises do not enter the socket-error path; general unhandled-rejection
reporting is a separate runtime concern. `tests/js_socket_listener_errors.rs`
compares eight real native loopback open/message cases with Node EventTarget
and the maintained RWS implementation, and checks the native exception output.

The shared `qs.stringify` binding consumes arrayFormat (indices/repeat/comma/
brackets), callable sort, RFC3986/RFC1738 format and the encode boolean. It
sorts keys at every object/array level before reading child values and keeps
undefined omissions. Comparator failures propagate; native sorting does not
claim V8's comparator call schedule. `tests/js_qs_options.rs` executes the actual
installed qs package over 96 option/input combinations. The BingX registry
regression additionally observes final signed GET query ordering through the
production HTTP client and verifies the signature against the source preimage.
Other qs options remain outside this imported runtime surface.

The JS socket's lastReceived accessor shares the native core's Number field;
pre-assembly writes are retained until lazy core creation. Frames and JS session
resets therefore update the same health timestamp, preserving native f64 bits.
Receive timers dispatch the instance checkLastReceived method through WsProtocol;
the inherited facade method calls the base check directly, avoiding recursion.
Scoped js_runtime witnesses cover lazy initialization, bidirectional reads/writes,
raw bits, zero reset/reinitialization and a derived watchdog receiver. A real GRVT
private socket witnesses its 30-second ping / 90-second Warning reconnect behavior;
all three regressions compile and fail before this bridge/dispatch repair.

Installed Promise.all/allSettled consume the shared synchronous iterator protocol,
including Map/Set iterators and Unicode strings. Iterator acquisition/step errors
reject the returned Promise. Inputs remain native Js values until existing
combinator settlement; pending members, object identity, negative zero and NaN
payloads survive. `js_runtime`'s two promise_combinators tests compare eighteen
Node outcomes and exercise native pending members; both compile/fail on the old
array-only globals dispatch. Promise scheduling remains in the shared promise core.

Map/Set `forEach` uses stable insertion positions, like the live iterators, and
releases its collection borrow before each callback. Deletions, clear/reinsert,
updates and new entries are observed during traversal. Callbacks receive the
source thisArg and native value/key/owner references; invalid callbacks reject
even on empty collections. `js_collection_foreach` checks fourteen Node traces,
nested traversal, abrupt completion, native signed-zero/reference identity and
thisArg. Both named regressions compiled and failed on the snapshot implementation.

Both JS `virtualClock` getters delegate to the shared synced singleton.
`getCurrentTime` wraps `VirtualClock::get_current_time` in the existing dayjs
object, retaining date clipping and arithmetic; `getCurrentTimeMs` remains the
numeric hot path. Source `Date.now()` still reads the runtime wall clock.
Registry `native_host_phemex_clock` exercises the real dayjs consumer through a
signed native HTTP request, verifying its HMAC and expiry after a 90-second
server correction. The core virtual-clock regression checks both getters and
the independent raw Date clock.

Shared JS promises run on a runtime-owned FIFO job queue. Their async bodies
start synchronously through the first suspension; settled awaits and reactions
resume in queued jobs. Returned promises receive a separate resolution job.
Nonempty all/allSettled register each input reaction synchronously, retaining
input order while observing the first rejection in settlement order.

NativeRuntime drains that queue before and after each host future poll. All
callbacks within a poll remain one batch; the checkpoint does not split socket
frames that the host delivers together. Scope timers use the same queue and
drain reactions before the next timer. Other runtime embedders still control
their host callback boundaries; the scoped native witnesses cover this native
checkpoint, timer cancellation and host task discard.

The shared JS Promise sink reports unhandled rejections through the runtime
exception boundary at a quiescent microtask checkpoint. Registering an await or
reaction handles its source immediately; an unhandled propagated rejection
belongs to the derived promise. Reading result() alone does not handle it.
Checks retain discarded rejection values until their reporting turn, and host
driver discard releases those pending captures without reporting them.

Bitrue's constructed public socket regression drives malformed raw depth frames
through the real socket dispatcher. Its discarded async listener promise reports
the source TypeError exactly once; the next valid snapshot still reaches its
callback. Native host controls cover catches before rejection, during the same
microtask checkpoint, and after the first report, plus host-discard cleanup.

String indexOf/lastIndexOf/includes/startsWith/endsWith search UTF-16 code
units and honor their position argument after search-string coercion. Positions
truncate and clamp; lastIndexOf treats NaN/undefined as the string length, while
endsWith defaults to the full length only for undefined. The native Node corpus
covers surrogate halves, empty needles, all position boundaries and thrown
position conversion identity.

The receive loop polls the open callback's synchronous prefix before dispatching
a buffered first message, for direct and relay transports. A pending callback
continues in the existing socket-owned task registry; the receive loop does not
wait for its authentication or other asynchronous completion. The native TCP
`js_socket_open_order` witness coalesces the upgrade and first frame and compares
synchronous and asynchronous listener traces with Node.

The shared JS facade's async snapshot/order/position methods return rejected
Promises for preparation errors, including errors before their first await.
Snapshot privacy/disposal guards precede retry-count coercion. The native
`js_async_facade` target compares return/catch behavior with the maintained TS
async field bodies executed by Node, preserving original conversion exceptions.

Emitter callbacks and `BaseExchange::emit`/`emit_args` carry shared `Js` directly.
Scalar listeners read a missing first argument as Undefined; variadic listeners
retain the original argument count. Producers use `from_typed` before any JSON
serialization; already-JSON inputs opt into `from_json` explicitly. Listener
errors stop dispatch, and once rows are removed before invoking their callback.

`ExchangeError.code` is `Option<Js>`: absent differs from own Undefined, and
thrown-error roundtrips retain native number bits and object references. Sentry
formatter results are also `Js`, including own Undefined keys. The runtime's
exception-report callbacks remain explicit JSON host boundaries. Worker event
posts use the required native DATA capability; main-thread worker RPC arguments
and successful results keep their separately scoped JSON ABI.

Adapter config capture/restore reuses the canonical RetainedJsonInput and existing
native typed codec through two thin value helpers. Shared HTTP adapters retain
own undefined, raw NaN payloads, infinities and signed zero at the terminal and
effective success/error config; they do not JSON-copy extension data. Transport
HOW owns the eventual protocol projection. This retains the typed codec
limitations and does not claim executable callbacks or live JS alias identity.

SubscribedCandle.interval uses ModelField<AnyTimeframe> at the callback boundary.
Its native record codec preserves absent, own undefined and concrete intervals,
including 1M, and omits undefined intervals from JSON. Candle number bits remain
native through that codec; strict public OHLCV(S) request options are unchanged.
The scoped subscribed_candle_fields witness exercises the actual native bridge.

The JS emitter facade follows the installed tiny-emitter removal algorithm:
omitted or falsy callbacks clear the event, and callback removal filters every
matching function or live `_` alias. Alias getters run without a listener borrow;
a successful removal commits the original filtered snapshot, preserving source
reentrant removal/addition behavior. Getter errors preserve their original value
and side effects. Core host listeners share that snapshot, and an in-flight emit
keeps its own snapshot. `tests/js_emitter_off.rs` compares 20 cases through the
production native runtime and BaseExchange facade against installed tiny-emitter.

Singular JS Store order/position/market mutators read the selector's `id` and
compare it strictly with native string IDs. Non-string IDs and bare primitives
do not identify those rows; nullish selectors and throwing getters propagate
when the source actually reads them. Empty order/position arrays retain their
findIndex short circuit. Ticker mutation follows its distinct object-key lookup
with property-key coercion. `tests/js_store_id_matching.rs` compares132 actual
maintained Store outcomes through DefaultStore's production facade. Bulk order
lookup and the adaptive multi-position index are separate paths.

JS `updatePositions` keeps the maintained adaptive lookup: strict IDs for a
single update or stored position, property-key coercion only when both counts
exceed one. It resolves every selector before patches, preserves unmatched rows
as `None` in the Store batch, and avoids reading patches for unmatched IDs.
`tests/js_store_position_batch_ids.rs` compares168 maintained Store cases with
real DefaultStore, including empty, single, distinct and duplicate-ID states.


The JS Axios dispatcher lets installed callable adapters own response settlement,
as Axios does. HOW fulfills non-2xx and zero-status responses, so their configured
transforms and fulfilled response interceptors run; default native HTTP responses
still undergo the existing status rejection. `tests/js_tunnel_status.rs` drives
the actual native runtime, factory, transport and HOW protocol against scripted
socket I/O and compares four statuses with the maintained HOW adapter and Axios.

Array `slice` applies ToIntegerOrInfinity's NaN-to-zero rule to both bounds,
including malformed numeric strings. `tests/js_array_slice.rs` compares422 Node
cases through the installed native builtins, observing each output slot's value
and presence so copied holes stay distinct from own undefined elements.

The runtime JSON builtin and default encoded-HTTP response parser decode through the canonical retained carrier
into existing Js values. JSON strings/keys preserve escaped and raw UTF-16 units,
including lone surrogates; object members use own data-property definition, and
numbers retain binary64 bits. Both consumers delegate to the canonical lower retained JSON parser. Invalid
HTTP JSON retains Axios's existing raw-text result, while the builtin throws
SyntaxError. tests/js_json_parse.rs compares 200 valid/invalid syntax cases and
all 65,536 escaped units with Node and checks a real loopback HTTP response
against actual Axios. scripts/json-parse-controls.py restores the original
builtin parser, raw-unit replacement, and HTTP parser independently; each fault
must compile, fail its named real-boundary assertion, and pass after exact
restoration. This does not widen HOW response data or typed Store/model strings;
JSON reviver behavior and exact engine diagnostic text remain outside this fix.


Plain JS socket dispatch follows the DOM listener snapshot and per-registration
removed flag: removal before a callback's turn skips it, and readding the same
function cannot revive its old registration. ReconnectingWebSocket retains its
vendored captured-array removal behavior. The native TCP
`js_socket_listener_removal` witness compares 16 open/message cases with Node
EventTarget and the actual vendored class, using only shared Node/DOM outcomes.
The browser rule is https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke;
Node's additions-during-dispatch quirk is outside this removal comparison.

For array inputs, JS Store `removeOrders` reads each element's `id` and applies
source object-key coercion; it never treats a bare element as the ID itself.
Property access and string-hint conversion failures propagate after the source's
empty-input/empty-Store early return. `tests/js_store_bulk_id_matching.rs` compares
30 maintained Store cases through the production facade, including numeric keys,
primitive elements, nullish elements and throwing getters/conversions. Direct
Symbols and object IDs converting to Symbols stay unmatched to native string IDs;
conversion uses the shared string-hint property-key operation. The object-to-Symbol
case compiles and fails on master before this repair.

The numeric patch witnesses pass entity selectors (`{id}`), matching the source
Store signatures, while retaining all numeric-bit, null and omission assertions.
Bare primitive selectors remain negative cases in the source ID corpus.

Standard Error instances inherit stock names and define supplied messages with
writable/configurable, non-enumerable descriptors. Omitted or undefined constructor
messages stay inherited; string coercion preserves UTF-16 and propagates failures.
The existing native string diagnostic factory shares this creation body; custom
Axios diagnostic name/message fields retain their distinct enumerable profile.
Stock prototypes contain no runtime captures or constructor backreferences.
`tests/js_error_enumerability.rs` compares156 Node cases through actual installed
constructors, Object.keys/assign, JSON and native spread, including assignments,
message deletion, coercion failures and lone surrogates. It inspects real native
Prop descriptors directly because Object.getOwnPropertyDescriptor is not bound;
constructor.prototype/stack reflection is outside this instance-property repair.
An actual installed AxiosError supplies the separate custom descriptor witness.

Shared console.info/debug route to the native runtime console sink and return
undefined. `tests/js_console_info_debug.rs` captures the actual NativeRuntime
output in a child process and compares its payloads with Node, including exact
info/debug severity dispatch. The original shared globals compile but fail at
console.info before either host output can be observed. Console.log is handled
by its separate source-binding change.

Number.toString applies radix coercion and range validation before formatting,
including for NaN, infinities and zero. Radix 10 uses the existing number formatter;
other radices use the private V8-derived precision/rounding algorithm documented
in THIRD_PARTY_NOTICES.md. The installed-native `js_number_radix` witness compares
5,320 Node outputs across all radices 2..36, fixed boundaries and deterministic
binary64 samples, plus 60 coercion/error cases. Its test-only commit compiles and
fails on the original binding at `(255).toString(2)`. This does not change the separate
BigInt formatter or the existing Number method receiver profile.

BigNumber's private value representation distinguishes finite decimals, signed
infinities and NaN; finite zero retains its sign. Arithmetic and comparisons use
that representation directly, including zero division and infinity operations.
Rounding a negative nonzero value to zero retains the minus in toFixed, while an
actual negative zero formats unsigned. valueOf/toJSON and toNumber preserve its
sign, and cloning uses the same value representation. The installed native
`js_bignumber_special_values` target compares 22 scalar inputs, 49 operand pairs
across six bound arithmetic and six comparison methods, and five min/max argument
sets against the actual npm dependency. Positive finite precision stays decimal;
the public shared model types are unchanged.

Error prototype thread-local storage stays inside its sole private accessor.
Module-level item macros are rejected by the parity indexer. The focused
`safe-cex-parity/tests/runtime_error_prototypes.rs` test indexes the actual SDK
module graph and requires the public Error factory to remain discoverable.

Orderbook utility bindings preserve raw price/amount comparisons and the source
binary-search read order. They read only visited prices, defer amount reads until
lookup completes, and retain the inserted update object. Totals capture both sides
before looping, re-read live lengths/rows and use JS addition; missing/null sides
throw instead of becoming empty arrays. `tests/js_orderbook_source.rs` executes
147 update and10 total cases against the maintained TS utility modules through
installed NativeRuntime bindings, observing getter order, holes, duplicate amount
reads, inserted identity and reentrant row/length changes. Both regressions compile
and fail on the prior implementation; their inputs do not replace utility code.

String and RegExp split share the source ToUint32 limit conversion, including
large finite values reduced modulo 2^32 before an integer cast. Plain splitting
coerces limit before separator even when limit is zero, retains empty fields,
and searches/slices UTF-16 units without replacement decoding. Existing RegExp
symbol dispatch and dense own-undefined captures are preserved. The installed
native `js_split_limit` target compares 1,440 Node cases across five strings,
12 separators and 24 limits, including lone surrogates, regex captures, wrapping
limits and throwing coercions. Regex literals use the actual production parser;
this host does not install a global RegExp constructor.

String.localeCompare uses the existing pinned JsRootCollator for the imported
runtime's default English/root collation, including case, accents, canonical
combining forms and lexicographic digits. Argument conversion uses the ordinary
string hint and preserves thrown identity. No new collation dependency or data
is added. tests/js_locale_compare.rs compares 2,116 Node pairs through the
installed String and native host, plus primitive/default coercion and abrupt
conversion. Locale/options selection is outside this default-profile binding.
The overseer explicitly permits ICU replacement of unpaired UTF-16 surrogates;
a separate native test pins both Node's distinct weights and ICU's equal weights.

The imported get-key `v` helper rejects non-null primitive right-hand operands
with TypeError before key conversion can execute user code. Null/undefined
short-circuit without coercing the key. Property traversal stops at an explicit
null prototype; existing own/inherited keys and alias precedence remain intact.
The internal `in` helper is fallible, and lodash throttle checks whether options
are objects before using it, as the maintained implementation does.
`tests/js_get_key_in.rs` executes the complete maintained get-key module and
installed lodash through 126 real native-host scenes, including poisonous keys,
null prototypes and primitive/default throttle options. String-key diagnostics
match Node; malformed non-string keys use an engine-neutral TypeError diagnostic.

BigNumber.isFinite reads the private decimal/Infinity/NaN variant directly; it
never converts through f64. The Node oracle includes finite values at +/-1e1000
and +/-1e-1000, which overflow or underflow a Number conversion, as well as
NaN, both infinities, signed zero, clones and arithmetic results.


Raw error reporter inputs use `ExchangeErrorInput::Raw(Js)`. Pass the original
shared-runtime value; already-JSON readers use explicit `from_json` at that boundary.
The reporter retains lazy reads, native formatted messages and worker error identity.
Optional message/extra-data retain their stated UTF-8/JSON profiles; console and
main-thread raw exception hosts receive original local Js references; `EMITTER_CONTRACT_EVIDENCE.md` records the scoped witnesses.

Worker/native delivery services live in exchange-core so constructor hosts never
depend upward on safe-cex-webworker. `delivery_timers` owns the existing timer
capability; `store_delivery` owns its single leading/trailing Store throttle;
`store_payload` owns snapshot preparation; `message_stats` owns socket-stat
projection; `throttle` owns the distinct keyed metadata throttle. Worker and native
hosts import these implementations directly. `ExchangeWorkerHost` inherits the
minimal StoreUpdateThrottleHost capability; each host supplies its existing timer
error reporter. The deterministic timer fixture and keyed cancellation witness
also live with the lower core owner.

GetIterator rejects missing or non-callable iterator methods with the shared V8
non-iterable diagnostic, before invoking input coercion hooks. Nullish inputs use
that diagnostic as well. `tests/js_iterator_errors.rs` compares 30 live Node
Promise.all/allSettled errors through NativeRuntime and the shared iterator API.


Number.isSafeInteger accepts only native Number values that are finite, integral
and at most 2^53-1 in magnitude; it never coerces arguments. The installed-native
js_number_safe_integer witness compares239 raw-bit inputs and13 argument types
with Node, including poisonous objects. WOO’s constructed client-ID rule compares
26 actual maintained-source cases through this shared builtin. Both regressions
compile and fail when the static binding is absent.

String uppercase/lowercase conversion maps whole valid Unicode runs and preserves
unpaired UTF-16 units verbatim. Context-sensitive lowercase mappings keep their
valid-run context. Trim slices the original units using the shared RegExp
whitespace predicate. `tests/js_string_utf16.rs` compares 4,109 Node cases per
method through the installed String builtin on NativeRuntime, including every
surrogate unit, paired astral letters, Greek sigma and whitespace boundaries.
All three regressions compile and fail against the original production bodies.

Shared nuke closes use the canonical nativePositionSize port and cancel orders
before closing; cancel failure logs and continues. NUKE_NATIVE_CLOSES.md records
the real registry/OKX HTTP regression and pending-close guard witness.
