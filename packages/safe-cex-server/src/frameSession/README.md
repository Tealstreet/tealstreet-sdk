# Native account sessions

`ServerSessionFactory` selects an explicitly requested engine. For `engine:
'rust'`, it connects to an already provisioned native account and forwards SCXF
frames to a client sink. It never constructs a TS exchange object, starts a
second engine, or falls back after a connection failure. `engine: 'typescript'`
delegates directly to the caller's `PublicExchangeFactory.get` and returns its
original handle. Existing callers of `PublicExchangeFactory` are unchanged.
Await `open`/`get` for the factory's readiness contract. After
`handle.forceRefreshMarkets`, re-read `handle.exchange`: the consumer-blockers
factory may replace its Rust metadata engine. The adapter never snapshots that
exchange reference or takes ownership of its lifecycle.

The application authorizes the client before opening a session. Its trusted
`resolveNativeAccount` must verify **both** the account and exchange and return
the configured `/accounts/<id>` WebSocket URL and account token. These are
application configuration, never client-selected URLs or venue credentials.
Native account tokens grant full state and command access; sharing an account
does not make book subscriptions an authorization boundary. Use WSS outside a
trusted local network. TLS, token provisioning and process supervision remain
application responsibilities. This library does not deploy anything.

```ts
import { ServerSessionFactory } from '@tealstreet/safe-cex-server/src';

const sessions = new ServerSessionFactory({
  publicExchangeFactory, // Its owner controls startup/refresh/cache/disposal.
  resolveNativeAccount: async ({ accountId, exchange }) => {
    return authorizedNativeTarget(accountId, exchange);
  },
});

const lifetime = new AbortController();
// Install before awaiting resolution/connection, including snapshot delivery.
client.onClose(() => lifetime.abort());
const session = await sessions.open({
  engine: 'rust',
  exchange: 'binance',
  accountId: authorizedAccountId,
  signal: lifetime.signal,
  sink: {
    onFrame: (buffer, { snapshot }) => client.sendFrame(buffer, snapshot),
    onNotice: (json) => client.sendText(json),
    onClose: () => client.close(),
  },
});
// Wire this only after open resolves: the snapshot has reached the sink.
client.onCommand((json) => session.sendCommand(json));
```

`sendFrame`/`sendText` must return a promise when the downstream transport is
asynchronous, resolving only after it accepts the message under its bounded
backpressure policy, and reject if the client disconnects. Frames and notices
are delivered in arrival order. Do not coalesce, drop or reorder frames, or
apply an independent partial-diff protocol around them. A delivery failure or
queue overflow closes the native connection and the downstream sink. The caller
owns client disconnect wiring and the session until it calls `close()`; the
factory owns neither the supplied public factory nor registered native accounts.

An AbortSignal closes a pending or ready native session. Account resolution and
initial connection/snapshot delivery each have a ten-second default deadline
(`resolveTimeoutMs` and `nativeOptions.connectTimeoutMs`). Resolver callbacks
receive a signal aborted by client cancellation or the resolution deadline;
they must honor it to cancel their own external work.
Late resolver results cannot create a connection after cancellation or timeout.

The default socket factory uses standard `WebSocket` (available in Node 22).
Other runtimes can inject `nativeOptions.socketFactory`. It must deliver future
events after factory return and honor `binaryType = 'arraybuffer'`; Blob
messages are rejected. Authentication uses the native server's
`safe-cex-frame.v1` and `bearer.<token>` subprotocols; the server echoes only the
version. The session also offers `safe-cex-frame.account-auth`, which opts it in
to `accountAuth` notices; servers that predate it ignore the offer. Error messages exposed by the adapter omit target URLs and tokens.

Clients use the shared `Frame` decoder and `FrameApplier` on every binary
message. The first binary message is a full snapshot; notices can arrive before
it. Subsequent BOOK_DELTA sections require that snapshot and later full BOOK
baselines. A reconnect is a new session with a **new** store/applier (with the
venue's native/inverse amount flags, as in `RustExchange`), never reuse a stale
book baseline. Applications using only raw frame consumers need no server-side
TS exchange/store. A sink may transfer the ArrayBuffer after accepting it.

Commands use the native/WASM JSON command protocol, not arbitrary worker method
calls. For example, `{"op":"subscribeOrderBook","symbol":"BTCUSDT"}` has no
result, while `{"op":"fetchLeverage","command":7,"symbol":"BTCUSDT"}` produces
`{"type":"done","command":7,"result":"...JSON..."}` (or `error`). IDs are u32,
unique among pending commands for this connection; the native server isolates
them across clients. `setTickerSymbols`, `setPollScale` and book/candle
subscribe/unsubscribe have no completion. `setPollScale` affects the whole
account. Native writes can first emit
`{"type":"commandProgress","command":7,"receipts":[{"symbol":"BTCUSDT","kind":"clientOrderId","identity":"native-id"}]}`.
These are selected native identities, not placement acknowledgements. Kinds
`clientOrderId`, `clientOrderIndex` and `orderCode` remain distinct; values stay
strings. Progress retains the initiating client's command ID and can arrive
after that application's waiting deadline until native completion. It is not
broadcast to other clients or replayed across disconnect. Command diagnostics use
`{"type":"commandError","command":7,"message":"placement rejected"}` with the
same client ownership. They do not settle the command: `done` remains its result.
Consumers should combine diagnostics with that result for one visible failure
per command, preserving a partial-success diagnostic and an unknown timeout.
Other notices are
`error`, `log`, `event` and `deactivate`, relayed as JSON.
`event` carries `level` (`debug`/`info`/`warn`/`error`), a stable dotted `code`,
`message` and optional flat `fields`; match on `code`, not message text.
Native typed decoding and venue capability checks remain authoritative.

The adapter never retries or replays a sent command. On disconnect a sent
trading request can have an unknown outcome; use venue reconciliation or the
client-order-ID lookup contract before deciding whether to place again. A local
`sendCommand` success means only the socket accepted the bytes, not venue
acceptance or completion. The application owns correlated-result timeouts.

Redis persistence, cached market/ticker freshness, publicdata integration and
PublicExchangeFactory startup/refresh stay in their existing exports. Cached
TS markets do not seed the native engine or overwrite its authoritative market
index. Worker chasers, TWAP, CLI and postprocessors remain TS shell behavior;
they are not native JSON commands. Application adoption must preserve those
behaviors before replacing its old worker route. No TS venues are deleted.

Run `yarn rust:test-server-adapter` for a deterministic real native WebSocket
test with snapshot, full BOOK, BOOK_DELTA, command correlation and reconnect.
