# Rust socket integration

The behavioral authority remains [the shared exchange guidance](../../packages/safe-cex/src/exchanges/CLAUDE.md) and [BaseWebSocket](../../packages/safe-cex/src/exchanges/base.ws.ts). This document describes the Rust binding surface, rather than maintaining another copy of those rules.

`BaseWebSocket::new` receives a `WsParent`, a `WsRuntime`, and a `WsProtocol`. `WsExchangeParent::new(&venue)` provides the standard parent bridge for an `Rc<dyn Exchange>`: it keeps the venue weak, delegates to the real shared `Store`, and shares the exchange's reopen-resync coordinator across sockets. It also attaches the weak venue reference used by the exchange watchdog. Register sockets through `BaseExchange::set_socket(SocketSlot, ...)`. Host labels (`BaseWebSocket::id`) do not determine membership or coalescing; both use object identity.

`BaseWebSocket::start_websocket(url, protocols, options)` is the synchronous
entrypoint for a venue connect callback. It drives the existing asynchronous
dial in the core-owned task registry and uses the existing socket-error log path.
Disposal cancels pending dials, including a host that polls inline; no
`WsProtocol` disposal hook or venue-held task handle is needed.

When the URL depends on an asynchronous REST token request, call
`start_prepared_websocket(move || async move { ... })` from the synchronous
connect callback. The closure returns `Result<Option<PreparedWebSocket>,
ExchangeError>`: `Some` supplies URL, protocols and optional connection options;
`None` completes without dialing. Capture the venue's REST client in the closure
and await its token request there. The core invokes and drives that closure in
its owned task, so disposal releases an unpolled or pending REST future as well
as a pending dial. No venue-owned task handle or protocol disposal hook is needed.

Each start reserves a new connection generation. A superseded preparation may
finish, but cannot dial or report a stale error. Preparation errors use the same
socket-error diagnostics as dial errors. Dropping a host task releases its slot;
it introduces no persistent re-entry latch and a later start can proceed.
Cancellation of the waiter does not promise cancellation at the remote server:
the host must release its REST future when the owned task is cancelled.

`WsProtocol` supplies venue ping, connect/subscribe, open, and message callbacks, plus recovery/watchdog/resync overrides. Asynchronous open callbacks run independently of the event consumer, so messages can resolve an awaited authentication acknowledgement. The private helper reports the original authentication error through the runtime exception sink before its existing warning/reconnect recovery; it does not replace the cause. The exact Bybit disposal and Paradex/Orderly recovery ACCEPTs are recorded in PRIVATE_OPEN_REVIEW.md. Private open handlers can use `authenticate_and_subscribe(auth_future, subscribe_callback)` to await successful authentication, stop after disposal, and refuse subscription on failure. The authentication future itself must honor disposal around its awaits and sends. Its `Ok(())` must mean the venue's required authentication stage completed.

`WsRuntime` extends the existing local `ExchangeRuntime`, whose required monotonic clock also serves generic exchange timers, with a resolved transport provider, the actual `TunnelHost`, and ordinary/header-capable socket dialing. Its two minimum reconnect delays are separate stable configured values, rather than newly sampled jitter on each getter call: `min_reconnection_delay_ms` represents SDK tuning (1000 + random * 2000), and `library_min_reconnection_delay_ms` represents RWS defaults (1000 + random * 4000). The supplied provider must represent the account's selected transport. Host futures may retain browser handles and need not be `Send`.

Ordinary sockets implement `WsSocket`. `next_event` is a single-consumer stream of open/message/error/close events; events occurring before the returned handle reaches the core must remain queued. The core attaches lifecycle/error handling for every dial path. `is_reconnecting` identifies a host-managed reconnect wrapper; such a wrapper owns its retry loop and must keep its event stream usable across closes. Runtime callbacks may synchronously reenter the core, so handle calls occur after state borrows are released.

The options and header types are re-exported from the transport owner:

```rust
use safe_cex_exchange::websocket::{UpgradeHeaders, WebSocketConnectionOptions};

let options = WebSocketConnectionOptions {
    headers: Some(UpgradeHeaders::Static(
        [("x-auth".to_owned(), "signed-value".to_owned())]
            .into_iter()
            .collect(),
    )),
    headers_required: true,
    ..Default::default()
};
```

Relay connections have the same managed lifecycle as TypeScript's
ReconnectingWebSocket wrapper. Their automatic closes do not enter the native
100-close circuit breaker. The core-owned receiver retries with the stable
resolved minimum delay, grow factor and cap, resetting after the configured
minimum uptime; the outer connection deadline uses the configured timeout.
Absent tuning fields use RWS defaults (factor 1.3, uptime 5 seconds, timeout
4 seconds, unlimited retries). A finite max_retries bounds retries after the initial attempt. Explicit reconnect retains its
separate source counter and waits before closing the current connection.
Force-close clears readiness immediately and cancels the receiver; force-open
closes and cancels the previous attempt before restarting. A discarded task
releases its run slot but retains the connection descriptor for recovery.
`create_websocket` and `start_websocket` take `Option<WebSocketConnectionOptions>`.
`None` is omitted options and supplies SDK minimum and 5-second maximum.
`Some(WebSocketConnectionOptions::default())` is an explicit empty object and
uses the library minimum and 10-second maximum. Explicit tuning fields override
those defaults. `start_closed: Some(true)` retains a present CLOSED wrapper for
managed connections, with no connection task, fresh-header evaluation or inner
construction until reopening. The plain native branch ignores this RWS option.
Outer required-header preflight remains eager. The resolved route and merged
header provider are retained for activation rather than resolved twice.
Cancellation and error-delivery costs cite
MANIFEST-SPEC.md#exchange-runtime-callback-choice; scoped witnesses live in
`src/dormant_socket_tests.rs` and `tests/dormant_socket_oracle.cjs`.
maxEnqueuedMessages, debug and custom WebSocket constructors remain a manifest
FINDING. Make absence a distinct representable state rather than a default
value; never infer object presence from headers. The host dial receives the same
resolved options used by managed relay retries.

Use `UpgradeHeaders::Fresh(FreshHeadersProvider)` for a fresh synchronous header provider. It remains lazy and runs at each inner connection attempt. Transport routing headers merge after caller headers. Relay sockets use `RelayWebSocket::with_connection_options`; the core does not recreate tunnel authentication, relay negotiation, or sealed WAF headers. The relay owner's constructor manifest documents case-conflict handling and unsupported arbitrary `nodeOptions` extensions. Required-header preflight applies before dialing or evaluating a provider. The separate `protocols` argument to `create_websocket` supplies the transport options' protocol list.

The shared handler registry is an `IndexMap`: non-integer handler keys retain
registration order for venue prefix fallback. Use `shift_remove` for deletion;
`swap_remove` changes the order of surviving handlers. Replacing an existing key
keeps its position, while deletion followed by reinsertion moves it to the end.

Handler-map values are `HandlerList` (`Rc<RefCell<Vec<MessageHandler>>>`). `dispatch_message` captures the matched list once and iterates by index without cloning its vector. A successful callback may remove the map key while remaining callbacks on the captured list still run, matching the shared TypeScript dispatch pattern. Each callback returns `Result<(), ExchangeError>`; the first error stops iteration and is returned unchanged. A venue propagates `dispatch_message` through its existing `on_message` result rather than collecting errors or continuing callbacks. Missing and empty keys return `Ok(())`. Stats are recorded once under the matched key before invoking callbacks.

Diagnostic truncation reports an ExchangeError when the capped prefix cannot be materialized. Ordinary boundaries preserve the source output. The manifest cap row cites MANIFEST-SPEC.md#forced-surrogate-conflict and the actual program-consumer witness.

The core owns cancellation handles for event consumers, open/auth futures, and managed reconnect delays. Disposal drains and cancels them before closing host sockets; a host need not wake a pending receive when close is called. Hosts must honor `ExchangeTask::cancel` by releasing the future (immediately or on their next scheduler turn). Inline spawn completion or disposal must not leave a stale handle registered. Handle cancellation and destruction occur outside core state borrows.

`start_message_operation` returns a weak `OwnedOperation`. `start_timeout` and
`start_interval` use the same registry and return an individually cancellable
operation. Cancel a topic handle before replacing it; cancellation removes its
registration and captured callback before the host's next scheduler turn. A
handle's Drop is intentionally inert. Socket disposal closes this owner before
host I/O closes, so no protocol dispose hook or venue registry is required.

JS socket `pingTimeoutId`, `pongMissTimeoutId` and `lastReceivedTimer` read the
actual native timer slots. Their Timeout objects use the same identity profile
as Scope timers; `clearTimeout` cancels the exact owned native deadline through
the socket context. Each slot retains one native handle, separate from its
assignable source field value. Replacement identities prevent stale handles from
cancelling newer work. Source close cleanup therefore stops heartbeat and
receive work even when the venue does not delegate to the base close handler.

An offline-to-online transition runs each registered socket’s offline recovery.
That path clears native retry backoff, failure pauses and close/reconnect counters,
then runs a pending reconnect immediately. Repeated online notifications do nothing.

Typed `BaseWebSocket::reconnect` dispatches through `WsProtocol::reconnect`.
The JS protocol calls the live receiver member, so venue arrow-field overrides
run for watchdogs, send recovery and explicit native reconnects. The inherited
JS member calls the shared base algorithm directly, preserving superclass calls
without recursion. Native protocols inherit that same base algorithm by default.


The base error listener runs before venue listeners and stops immediate
propagation. Plain WebSocket dispatch therefore logs the error without invoking
`WsProtocol::on_error`; grouped ReconnectingWebSocket dispatch still invokes it,
as that dispatcher ignores the DOM propagation flag. Actual close events retain
their normal protocol handling. A failed native network attempt retains a closed
socket handle and delivers Error then Close(1006) through ordinary event dispatch,
so normal diagnostics and recovery run with existing generation/disposal guards.
Constructor/admission failures remain distinct from transport failures. The production Aster and Bitunix host witnesses
are in registry's `native_socket_error_dispatch` target.

Socket preparation and dial entry now use the canonical eager Promise boundary
inside the existing cancellation registry. The prefix runs before the venue
connect callback returns; a ready host installs its socket immediately. Disposal
still drops the actual pending preparation/dial body before the scheduler runs,
and wakes its shared driver for cleanup. See the Bitget EAGER_DIAL_AUDIT.md for
real Bitget/Backpack prefix probes and independent production controls. A native
held handshake still needs a CONNECTING handle; that separate finding remains
open and is not implied fixed by eager startup.

The native plain socket returns its stable host handle in CONNECTING before the
HTTP upgrade settles. Constructor admission (URL, protocol/header parsing and TLS
configuration) precedes that handle; the one transport handshake is then driven
by the canonical eager boundary in the socket's owned task registry. The event
consumer waits for that same handle to open or fail, preserving the existing
network Error then Close(1006) path and constructor-error origin.

Direct send while CONNECTING throws `InvalidStateError: Sent before connected.`;
BaseWebSocket.send instead observes state 0 and uses its existing warning and
reconnect path. Closing during CONNECTING cancels pending physical I/O and emits
one Error then Close(1006), with no Open. The maintained Node WebSocket stays
CLOSING (2) after that pair; the native plain handle preserves that observed
quirk. Its normalized Error message is the source listener's `event.type` fallback.
Dispose/final drop cancels the handshake and suppresses later delivery. Event
waiter drop only removes its own shared registration; it does not cancel a live
socket. `safe-cex-registry/tests/native_connecting_socket.rs` runs actual native
handshakes and the constructed Bitget secondary market callback against
`connecting_socket_source.cjs`, which executes the maintained Bitget/BaseWebSocket
and real WebSocket over a held local upgrade. Core's native connecting tests pin
immediate cancellation and waiter registration release independently.
Event delivery uses the existing queued native consumer under the
[callback choice](../parity/MANIFEST-SPEC.md#exchange-runtime-callback-choice).
The Bitget witness checks handle presence at callback entry and the resulting
lifecycle effects after event delivery; it does not claim Node EventTarget's
synchronous close-callback stack.
