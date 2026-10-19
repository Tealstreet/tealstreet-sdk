# MEXC non-UTF-8 FIX audit

This reviews the nine assigned rows from `non-utf8-fix-by-crate.json` against
constructed `MexcExchange` bindings and maintained TypeScript bodies. Results:
**2 FIX, 7 ACCEPT**. ACCEPT means the claimed non-UTF-8 difference does not occur
on the measured binding path; it does not assert all-input parity or remove
separate UTF-16, typed-output or raw-alias decisions.

| Assigned row | Result | Real-binding observation and reason |
|---|---|---|
| `MexcExchange.updateSymbolMetadata` | ACCEPT | Existing #1071 eager boundary notifies the real DefaultStore before returning its public-only waiter. Private held HTTP survives waiter drop; ready HTTP retains its await checkpoint, committed fallback and original second subscriber error. `exchange_trait::eager_wrappers` and its maintained-source oracle replay these cases. The census's lazy-start claim is obsolete. |
| `MexcPublicWebsocket#addHandler` | ACCEPT | A candle callback registers B through actual `Exchange::listen_ohlcv`; B first runs on the following frame. Venue-generated kline/depth/trade keys have fixed prefixes, so the binding cannot select an inherited stock Object key. Existing seven source handler profiles retain the direct inherited-key refusal and duplicate/throw controls. |
| `MexcPublicWebsocket#removeHandler` | ACCEPT | A removes B during the next real frame; B still runs from that frame's captured array, then disappears. Final cleanup removes A. Actual source and binding traces are `A,A,B,A`. No receiver-timing difference is reproduced. |
| `MexcPublicWebsocket` | FIX | NativeRuntime master returned from the book listener with zero snapshot HTTP calls; maintained TS starts one synchronously. Canonical eager startup now starts the actual request before returning, while callback/subscription work stays after the reaction. Held completion, early cleanup, disposal and owner release retain their source behavior. |
| `MexcPublicWebsocket.constructor` | ACCEPT | The actual constructor installs both sockets and the active-ticker listener. Changing active symbols sends the expected unsubscribe; disposal detaches the listener. The maintained complete public class runs the same registration/reconciliation/disposal path. Native capabilities add no independent source decision here. |
| `MexcPublicWebsocket.onMessage` | ACCEPT | Real registered ticker dispatch commits native price 29 before the first original subscriber error; later subscribers do not run. Candle frames also reach the captured handler arrays above. The maintained parser, class methods and DefaultStore confirm the same effects. Separate ingress/string-domain choices remain separate. |
| `MexcPublicWebsocket#subscribe` | ACCEPT | Send-time registration through the real candle listener adds Min5 while Min1 is being re-subscribed. The live registry visits Min5 again, matching maintained Map iteration. Six exact wire frames are compared; a compiled early-termination fault fails this assertion. |
| `MexcPublicWebsocket#unsubscribe` | ACCEPT | Real candle cleanups send the original symbol/interval unsubscribe and remove tracked subscriptions. The actual active-symbol event also sends ticker unsubscribe; disposal prevents later sends. Source uses the same tracking-before-send and method replacement order. |
| `MexcPrivateWebsocket` | FIX | NativeRuntime master completed the actual debounce timer callback with zero asset HTTP calls; TS async callback starts fetchBalance before returning. Eager request entry repairs this; Store update remains after await, disposed sockets stay guarded, fetch failures remain swallowed. Reconnect also starts its discarded snapshot eagerly before reconnected, with a canonical eager HTTP waiter so a ready response cannot commit Store inline. The already-canonical remote signer starts before reconnected and emits no login while signing is held. Registered private fill/market/Store dispatch remains covered. |

## Executable evidence

`tests/socket_assembly.rs` constructs the real venue and registered sockets with
DefaultStore. `native_book_entry` and `support/native_socket_audit.rs` use actual
NativeRuntime scheduling, with scripted terminal HTTP/WebSocket/signing only.
The deterministic socket fixture now queues spawn bodies untouched instead of
polling them inline; explicit event/reaction turns cannot supply a missing
synchronous request prefix in the NativeRuntime assertions.

`book-entry-source.mjs` executes the maintained book listener plus actual parent
fetch method and normalizers. `socket-audit-source.mjs` executes maintained private
open/login/debounce methods. `public-binding-audit-source.mjs` executes the whole
maintained public class and DefaultStore, terminating only inherited I/O, clock
and diagnostic capabilities. The existing socket target runs all three profiles.
No source body is replaced with an expected-result stub.

`socket-entry-controls.py` independently compiles delayed book startup, delayed
balance startup, prematurely terminated live subscription iteration, delayed
reconnect snapshot entry, and a removed canonical HTTP waiter. Each
must fail its named native assertion (not compilation), then pass after exact
source-byte restoration. `MEXC_BOOK_SOURCE_CONTROL=defer` independently delays
the maintained TypeScript request and must fail its synchronous request-count
assertion. Master produced the named book, balance and reconnect-prefix failures before
the production repairs; an intermediate eager snapshot also failed the independent
ready-response Store assertion until the HTTP await boundary was preserved.

The eager helpers carry the request's `Result` as their successful payload until
the existing catch runs. An immediately rejected request therefore retains the
source catch behavior without creating an intermediate unhandled rejection.
