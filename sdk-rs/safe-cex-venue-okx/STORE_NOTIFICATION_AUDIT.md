# Store notification audit

## Notification cutover

The five ticker writers now consume `Result<(), StoreMutationError>` and use
core's qualified `ExchangeError::from` conversion with an explicitly typed
StoreMutationError parameter. Incoming `on_message` propagates both
`track_last_received` and `handle_pong_event` with `?`. No signature was widened.

Core's subscriber-ID finding is closed at integration 9af82bfeae. Its shared
conversion retains Store-local IDs in inner-to-outer order in both branches.
The original cause metadata remains intact; IDs are not globally unique.
Native writer witnesses compare the appended ID with the same Store's public
StoreMutationError::Notification accessor, retaining an existing inner ID.

Direct writer tests exercise all five handlers through real DefaultStore, pin
commit before rejection, stop before later subscribers and restore the exact
original ExchangeError including name and response. Health tests inject errors
at the existing WsParent publication/latency boundary and pin exact propagation;
they do not prove production health writes through the fixture parent.
No row moves to ported: the notification finding is closed, while independent receiver/prototype
choices remain. Browser rollback and
LoadedChunks execution remain UNEXECUTED.

| Production site | Contract |
| --- | --- |
| handle_ticker_events → update_ticker | StoreMutationError → core ExchangeError::from |
| handle_open_interest_events → update_ticker | StoreMutationError → core ExchangeError::from |
| handle_funding_rate_events → update_ticker | StoreMutationError → core ExchangeError::from |
| handle_mark_price_events → update_ticker | StoreMutationError → core ExchangeError::from |
| handle_index_ticker_events → update_ticker | StoreMutationError → core ExchangeError::from, stops loop on Err |
| on_message → track_last_received | Result<(), ExchangeError>, propagated before frame parsing |
| on_message → handle_pong_event | Result<(), ExchangeError>, propagated before frame dispatch |

## Earlier forcing-function evidence


The shared notification repair is routed to the Store owner; Binance witness
`097041a8c7` and Bybit's corresponding audit identify the same contract.
`Store::update_ticker` already returns patch errors, but `Subscriber` still returns
unit. This Result does not propagate listener failure: TypeScript commits the
patch, invokes listeners, and a throw stops later listeners and escapes to the
handler. Rust cannot represent that error through its current callback contract.
A panic is not parity, especially on wasm.

| Source row | Rust production call site | Current claim |
| --- | --- | --- |
| OKXPublicWebsocket.handleTickerEvents | public_socket.rs::handle_ticker_events → Store::update_ticker | divergent, notification FINDING |
| OKXPublicWebsocket.handleOpenInterestEvents | public_socket.rs::handle_open_interest_events → Store::update_ticker | divergent, notification FINDING |
| OKXPublicWebsocket.handleFundingRateEvents | public_socket.rs::handle_funding_rate_events → Store::update_ticker | divergent, notification FINDING |
| OKXPublicWebsocket.handleMarkPriceEvents | public_socket.rs::handle_mark_price_events → Store::update_ticker | divergent, notification FINDING |
| OKXPublicWebsocket.handleIndexTickerEvents | public_socket.rs::handle_index_ticker_events → Store::update_ticker (each selected ID) | divergent, notification FINDING |

This is an inventory of the five implemented writers, not a census of all
pending OKX methods. Each uses native ModelPatch numeric fields and propagates
patch validation errors. None swallows a represented error. The direct
breaking notification ABI will force all five call sites to compile against the new
contract; no parallel operation or migration flag is introduced.

The numeric handler oracle executes the actual TypeScript bodies with a recording
Store boundary. Rust's tests also exercise real DefaultStore and an ordinary
nonthrowing listener. Those tests establish field extraction, first-record
selection, numeric bits and ordinary notification count. They do not prove
throwing-listener behavior, global dictionary prototype mutation, or listener
order after a failure. These remain shared findings, not silently approved parity.

All five conversions explicitly bind the current StorePatchError type. An inferred map_err closure could otherwise continue compiling after Store gains a notification-capable error and flatten that error to a string. The explicit type makes the planned return-type cutover a compiler error at every listed writer, forcing deliberate propagation of notification identity rather than silently accepting the new error through Display. This is a pin on the current API, not a parallel fallible Store operation.

## Callee-first forcing-function re-sweep

Audited production at `36ff486702`: all ten `src/*.rs` files reachable from the
crate's nine declared modules plus `lib.rs`, not just occurrences of `map_err`.
Started from the current Store contract's six Result-returning mutation callees:
`mark_orders_terminal`, `update_order`, `update_position`, `update_positions`,
`update_ticker`, and `update_market`. Also checked the other notifying Store
operations, imports/aliases, receiver signatures, captured method references,
wrappers and local conversion implementations so another spelling would not
exclude a call site.

Exactly five direct Store mutation calls exist, all `update_ticker`, at the five
functions in the table above. Each complete result is returned after a conversion
with an explicitly typed StorePatchError parameter; the index handler propagates
that typed result with `?` inside its loop. No direct Store result flows through
an inferred `if let Err`, formatting conversion, blanket-From `?`, `.ok()`,
`unwrap_or_default`, inferred `.into()`, or a discard. No additional unpinned
conversion was found. The two local From implementations convert protocol enums
into private wire enums; neither converts an error. JSON/string builders and
serializer Option defaults do not consume Store mutation results.

The remaining production modules hold pure policy/mappers, protocol constants,
HTTP adapters, immutable Order/Market inputs and explicit callbacks; none holds
a Store receiver or a hidden Store-writing wrapper. Core socket calls propagate
concrete ExchangeError results directly, while arbitrary host callbacks and
core-owned socket health publication are outside this direct-result census.
This is not a claim that indirect notification behavior is already correct:
notification FINDINGs and the core's own cutover inventory still apply.

P3b's candle-cache production crate has no safe-cex-store dependency or receiver.
Its key-value backend results belong to CandleCacheKeyValueStore, not to this
Subscriber/Store mutation ABI. The test fixture named Store implements that
backend trait and is not a production domain-Store writer. This negative finding
is scoped to the notification forcing function, not a general error-path audit.

Indirect writer inventory: public_socket::subscribe calls BaseWebSocket::send. On not-ready non-heartbeat sends with no existing socket, send calls reconnect, whose no-socket branch calls on_close then process_close then WsParent::sync_socket_health_to_store. That outgoing path crosses the routed notification contract; subscribe stays divergent FINDING. Its connected-send oracle does not test the disconnected publication or throwing listeners. No new direct Store result conversion is introduced.

Indirect ping inventory: public_socket::ping invokes BaseWebSocket::send with
literal ping. On a not-ready socket it calls fallible WsParent::log without
reconnecting. The original source send/log witness and real core send now agree
on listener TypeError after clock recording. The previous success witness flips;
the fixture log callback retains the original error and Store-local ID list.
This establishes caller propagation, not a new DefaultStore/logger composition
claim; shared notification fanout has its own independent evidence.
