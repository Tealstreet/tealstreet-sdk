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
Legacy executions-history metadata is retained as metadata and gates no call.

Implementations must preserve the documented TS quirks. In particular, an empty
supplied symbol list differs from no symbol list, market refresh compares IDs,
and a rejected/disposed position lookup retains its shared fetch slot.

## Validated placement example

Venue code implements hooks that receive validated payloads; callers invoke the
base entrypoints with ordinary `PlaceOrderOpts`. No venue constructs the private
validated fields. This example runs as a crate doctest: it constructs the production Store and all
five BaseExchange arguments, places one order and one native batch, and verifies
a rejected batch sends nothing. The recording runtime implements all nine host
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
bypasses and captured base method references, and pins hook call sites plus their
enclosing function token fingerprints in `override-call-sites.json`. This is a
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
fold_position_adapter. Reconstruction is path-dependent: seeds remain incomplete,
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
