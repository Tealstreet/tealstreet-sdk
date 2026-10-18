# Native async call boundaries

The production Exchange dispatches through the existing owner Rc. Its 27 typed
REST/order/history/session wrappers now use shared `eager::start_eager`, retaining
native results and errors without serializing them. The runtime owns continuation;
dropping a caller waiter cannot cancel a request or its later Store mutation.
Private callback and pager helpers remain composed inside these owned bodies.

The common native `Client::request` is also an async boundary: Axios supplies a
Promise even when terminal HTTP is already ready. Its shared eager waiter retains
that await fence, so ready leverage/metadata responses cannot update Store inside
the caller's synchronous prefix. Held HTTP responses register a real waker in the
terminal fixture; releasing them resumes both long and short position updates.

Private `send_login` uses the same boundary at its actual socket owner. The real
`BaseWebSocket::on_open` must capture its timestamp/nonce and start clock sync
before scheduling the login waiter, while the existing 25ms sleep still suspends
frame transmission. `native_clock` retains actual login-frame and retry checks.

`native_query_tests` exercises all assigned reads, both order-read endpoints,
normal and standalone TP/SL amendments, ready/held Store continuations, and the
actual private socket open path. `native_identity` retains the discarded-ticker
regression and executes the maintained TypeScript arrows with terminal HTTP and
Store primitives. `eager-operation-controls.py` independently restores lazy
Exchange, HTTP and login boundaries, requiring compiled failures and exact passing
restorations. These are native binding observations, not a blanket classification
of every generic helper accepting arbitrary user futures.

The journal dependency row is ACCEPT: its real constructed history/Store probe
observes account identity, quote lookup/fallback, fees and the source's omitted
funding option. The supplied native resolver has no external executable callback.
