# Native exchange server

The Rust replacement for NX `services/exchange-server`. It goes live on Vultr
before the hard cutoff at 2026-11-02 00:00 UTC.

The design is a hybrid. It keeps today's `wss /account` JSON envelope, so the
web hosted client changes only how it decodes store data. Each account runs on
the native `FrameServer` engine actor (`hosts/native/src/server.rs`). Store data
travels as SCXF frames inside `msg`. Trading uses the existing `callMethod` op
names.

Owners. Comment in your own section, and don't edit anyone else's.

| Area | Owner | Where |
|---|---|---|
| Server binary, envelope, sessions, accounts, HTTP | claude-fe7lgm | `hosts/server` (crate `safe-cex-exchange-server`) |
| FrameServer runtime account API | claude-fe7lgm | `hosts/native/src/server.rs` |
| Remote signer (sign-req / sign-res provider) | claude-4yjlw0 | `hosts/server/src/signer.rs` |
| Routing plan resolution | claude-m26ps4 | `safe_cex_native::routing::resolve` |
| Web hosted client (frame decode) | claude-19pt8s | NX `hostedFrameClient` / `ExchangeServerWorker.ts` |
| Docker images and GHCR workflows | codex-mxb4mq | SDK Dockerfiles, NX workflows |
| Candle server | see its own section | `hosts/candle-server` |

## Packaging (pinned for codex-mxb4mq)

- **Crate:** `safe-cex-exchange-server`, at `sdk-rs/hosts/server`. It is a workspace member, but not a default member.
- **Binary:** `exchange-server`. Build it with `cargo build --release --locked -p safe-cex-exchange-server --bin exchange-server`.
- **Listen:** `0.0.0.0:${TEALSTREET_PORT:-23456}`. HTTP and WebSocket share this one port.
- **Readiness and liveness:** `GET /health`.
  - Returns 200 with `{"ok":true,...}` when ready, and 503 with `{"ok":false,...}` otherwise.
  - Healthcheck: `curl -fsS http://127.0.0.1:${TEALSTREET_PORT:-23456}/health`.
  - It goes 200 once the listener is bound and the account threads are running. No venue I/O is needed for it.
- **Runtime files:** none.
  - TLS uses rustls with the bundled webpki roots, and the edge terminates client TLS.
  - CA certs in the image are harmless but not required.
- **Signals:** SIGTERM starts a graceful drain. Clients get a `1001` close, and accounts are disposed. The process exits within 10s.
- **Logs:** JSON lines on stdout. Warn and above go to stderr as well.

## Environment

| Variable | Default | Meaning |
|---|---|---|
| `TEALSTREET_PORT` | `23456` | Listen port |
| `CONNECT_REGION` | unset | Region label. Routing uses it for `proxyRegion` defaults; it also appears in `/health` and logs |
| `TEALSTREET_RELEASE` | unset | Release id, sent to Sentry and shown in `/health` |
| `EXCHANGE_SERVER_SENTRY_DSN` | unset | Sentry is off when unset |
| `REDIS_URL` | unset | Optional. Not used in v1; it is accepted so that today's compose files keep working |
| `TEALSTREET_ENGINE_THREADS` | number of CPUs | Account actor threads (see Runtime) |
| `TEALSTREET_DIAGNOSTIC_BUILD` | `0` | `1` enables `diagnostic:server-health` frames for verbose clients |
| `EXCHANGE_ALERT_SLACK_WEBHOOK_URL`, `EXCHANGE_ALERT_CRITICAL_SLACK_WEBHOOK_URL` | unset | Deferred past cutover. Accepted and ignored |

`TEALSTREET_WORKERS`, `TEALSTREET_GATEWAY_PORT` and `TEALSTREET_HEALTH_PORT`
belong to the Bun gateway and have no meaning here. One process serves all
connections.

## HTTP routes

- **`GET /health`:** `{ok, uptime, release, region, connections, accounts, engineThreads, remoteSigner}`. The last field carries aggregates only: no account, symbol or key ever appears here.
- **`GET /capabilities` and `GET /account/capabilities`:** the same JSON as `getExchangeServerRuntimeCapabilities()`, with `"runtime":"rust"` and `"frames":1` added.
- **`OPTIONS`** on the three routes above returns 204 with the existing public CORS headers.
- **WebSocket upgrade:** accepted on `/` and on `/account`.
  - Query parameters: `proxyToken` (alias `token`), `resume`, `resumable=1`.
  - Without a token, the socket is an unauthenticated public-data socket.
  - An invalid or expired token gets a 401 before the upgrade.
- **Any other request:** 426 if it is not a WebSocket, otherwise 404.

## Authentication

- **Token check:** `proxyToken` is an RS256 JWT, verified with today's `PROXY_PUBLIC_KEY`. That key is copied verbatim into `src/auth.rs`.
- **Claims:** it needs a nonempty string `userId` claim and an unexpired `exp`.
  - Once verified, the `userId` binds both the socket and any resume.
  - At `exp + 30s` the socket closes with 1008. That 30s is the same reconnect grace as `PROXY_TOKEN_RECONNECT_GRACE_MS`.
- **Use of the token:** it is also the per-socket proxy token behind the account's `ProxyTokenProvider` (see Routing). It is never logged.
- **Unauthenticated sockets:** they may only `init` the public pseudo-account `__pub_bybit`.
  - The validation is the same as `validateUnauthenticatedPublicInit`.
  - Only `sendSnapshot` and the subscribe/unsubscribe book and OHLCV `msg` ops are allowed.
  - They can never send `sign-res` or `verbose`, and either one closes the socket with 1008.

## Envelope

All application messages are JSON text frames of the form `{"t": <type>, "d": <data>}`.

### Client to server (unchanged from today)

| `t` | `d` | Behaviour |
|---|---|---|
| `init` | `InitPayload` (same fields as `server.ts`) | Builds the account, or keeps a resumed account whose init fingerprint is identical and sends it a snapshot instead |
| `msg` | `{op, data?, accountName, instanceId?}` | Account op, see below |
| `sign-res` | `{id, config?, wsAuthSignature?, error?}`, with `ts` as a sidecar | Goes to the signer (4yjlw0) |
| `echo-res` | `{id}` | Echo RTT sample |
| `ping` | `{accounts?: string[], link?}` | Replies with `pong`. Then reconciles: when `accounts` is nonempty, any server account not in that list is disposed |
| `dispose` | `{accountName}` | Disposes the account |
| `perf` | `{level}` | Session performance level. The server maps it to the engine's `setPollScale` |
| `verbose` | `{enabled}` | Authenticated sockets only. Diagnostics |

`msg` ops:

| `op` | Behaviour |
|---|---|
| `callMethod` | `data: {requestId, method, args}`. Becomes an engine `Command` (see Trading); reply `callMethod:done` |
| `subscribeOrderBook`, `unsubscribeOrderBook` | `data: {symbol}`. Becomes the engine book subscription |
| `subscribeOHLCV`, `unsubscribeOHLCV` | `data: {symbol, interval}`. Becomes the engine candle subscription |
| `sendSnapshot` | Sends a full `snapshot: true` frame to this socket |
| `updatePositions` | Answered `updatePositions:done`. Positions are already engine-owned |
| `exec`, `chaser:*`, `twap:*` | v1 replies `error` "not supported on the Rust exchange server". These are TS worker features; the post-cutover work is listed below |

### Server to client

| `t` | `d` | When |
|---|---|---|
| `session` | `{resumeId, resumed, frames: 1}` | On open, for `resumable=1` sockets. `frames:1` is the version gate (see Frames) |
| `msg` | `{op, accountName, data, ...}` | Account output (see Frames and Replies) |
| `sign-req` | the signer request (4yjlw0) | Remote signing |
| `echo` | `{id, pad}` | Echo probe every `SIGNER_ECHO_PROBE_MS`, default unchanged |
| `pong` | `{accounts: string[], ...linkReport}` | Reply to `ping` |

Socket keepalive: the server sends a protocol-level WebSocket ping every 10s.
With no inbound data for 30s, either a message or a pong, the socket is
dropped. This matches Bun's `idleTimeout: 30` with `sendPingsAutomatically`.

## Frames (claude-19pt8s, web hosted client)

The browser host (`ExchangeServerWorker.ts`) forwards only TEXT frames to the
client, as `{frame: '<raw string>'}`, and a React Native WebView bridge carries
strings only. So SCXF rides inside the existing JSON envelope, base64-encoded.
Binary WebSocket frames are out of scope for the cutover.

Server to client, one message per engine flush per account:

    {"t":"msg","d":{"op":"frame","accountName":"<name>","seq":<u64>,"snapshot":<bool>,"data":"<base64 SCXF>"}}

- **`seq`:** monotonic per account per server session. It starts at 1 on init and on resume, and again on every full snapshot.
- **`snapshot: true`:** marks a full native snapshot. It replaces the client's catalog, book and order baselines.
  - The server sends one immediately after a successful init, and after every `sendSnapshot`.
  - A socket that joins an account later gets its own snapshot before any delta, which is FrameServer's join contract.
- **Client side:** the client keeps one `FrameApplier` plus `DefaultStore` per account, and applies frames in `seq` order.
  - A `seq` gap, an `Invalid engine frame` or a `FrameApplyError` makes the client send the existing `{t:'msg', d:{op:'sendSnapshot', accountName}}`.
  - It then ignores deltas until a `snapshot: true` frame arrives. This reuses today's `hostedFrameClient.requestSnapshot` rate limit.
- **Size:** FrameServer already splits a delta frame that is too big under `max_frame_bytes`. Each segment is its own `seq`.

State that SCXF does not carry stays JSON, under the existing op names and shapes:

- **`op:'store'` partial payloads:** these carry only the fields frames don't (`fetchedLeverage`, `balance.info`, `socketHealth`, `messageHandlerStats`, `activeChasers`, `activeTWAPs`, `ordersRisk`). They are merged over the frame store, which matches the worker's `_derivedStorePayload` in frame mode.
- **Unchanged replies:** `error`, `warning`, `log`, `engine-event`, `command-progress`, `callMethod:done`, `exec-result`, `updatePositions:done`, `chaser:*` and `twap:*`.

How FrameServer notices map onto `msg`:

| FrameServer notice | `msg` op |
|---|---|
| `done` (result or error) | `callMethod:done` `{requestId, result}` or `{requestId, error}`. `error` is `SerializedCommandError`, with `outcome:'unknown'` for writes whose result is unknown |
| `event` (level, code, message, fields) | `engine-event` |
| `deactivate` | `deactivate-status` |
| `startupFailed` | `start-status` `{ok:false, detail}`, plus `error` |
| `accountAuth` | `engine-event` with code `accountAuth`. The server always opts in |

The client's output to its own consumer stays as today: `op:'store'`
snapshots, `listenOrderBook` (from `FrameApplier.books`), `listenOHLCV` (from
`.candles`) and `fill` (from `.fills`). Nothing downstream of
`hostedFrameClient` changes.

Decode uses `@tealstreet/safe-cex-wasm/wire` `Frame`, which is pure JS and
needs no wasm on the client, plus safe-cex `FrameApplier`.

Version gate: the server's `session` message gains `"frames":1`. A client that
sees no `frames` field keeps today's `store:delta` path, so old and new servers
can coexist during the Vultr rollout.

## Trading (callMethod to engine Command)

`callMethod` keeps its op names, `requestId` and `callMethod:done` lifecycle.
The server translates `method` plus `args` into the same engine command JSON
that `RustEngineExchange` sends to `WasmEngine.command`, and decodes it with the
shared typed decoder (`HostInput`). Module: `hosts/server/src/methods.rs`
(fe7lgm).

- **Mirror of the TS translation:** it mirrors `rust.exchange.ts` method by method. Golden tests record TS `method(args)` to engine-command pairs from the TS translation, and the Rust module must reproduce each pair.
- **Cutover set:** `placeOrder(s)`, `placeOrderWithClientOrderId`, `updateOrder`, `cancelOrders`, `cancelSymbolOrders`, `cancelAllOrders`, `findOrderByClientOrderId`, `setLeverage`, `fetchLeverage`, `setMarginType`, `setPositionMode`, `fetchPositionMode`, `nuke*`, `fetchOHLCV`, `fetchPublicTrades`, the `fetchHistorical*` reads and `fetchIncomeHistory*`.
- **Unknown methods:** they fail with `callMethod:done` `{error: {name:'UnsupportedMethod'}}`. They never get a guessed command.
- **No retries:** the server never retries a trading command. A disconnect never cancels or replays a write, which is FrameServer's orphan rule.

## Accounts and runtime (fe7lgm)

Engines are `!Send`, so accounts live on account threads: `TEALSTREET_ENGINE_THREADS`
OS threads, each running a current-thread tokio runtime plus a `LocalSet`. A new
account goes to the thread with the fewest accounts. The accept loop, HTTP and
the per-socket envelope tasks run on a multi-threaded tokio runtime. They reach
accounts only through `Send` channels.

The FrameServer library change, in `hosts/native/src/server.rs`, keeps the
existing `serve()` behaviour and wire byte for byte:

- **`AccountRegistry`:** registers and removes accounts at runtime, while serving. Today the account map is fixed before `serve`.
- **`AccountHandle` (`Clone + Send`):**
  - `join(client, account_auth) -> ClientLink`. `ClientLink` yields `Outgoing::{Frame, Text}`; the first item is a snapshot frame.
  - `command(client, Command)`, `poll_scale(...)`, `leave(client)`.
  - `dispose()`, which stops the actor, closes its venue sockets and settles pending jobs as unknown.
- **`connection()`:** the existing WebSocket wire becomes one consumer of `AccountHandle`. `hosts/server` is a second consumer.

Sizing, for 110 to 1,200 accounts per region (from the CI replay data):

- **Runtime:** a multi-threaded tokio runtime for sockets, plus `TEALSTREET_ENGINE_THREADS` account threads. FrameServer's own example is single-threaded and caps the whole server at `max_connections` 256; neither limit applies here.
  - Each account still takes `ServerConfig` per-account limits.
  - The server caps sockets at `TEALSTREET_MAX_CONNECTIONS` (default 4096).
- **Reconnect pacing:** account starts are paced at 5 to 10 per second per egress IP, with jitter. A mass reconnect queues inits rather than stampeding venue REST and WebSocket logins. Queued inits still get their snapshot when they start.
- **Shared public feeds and market loads (follow-up after the first slice):**
  - one public engine per venue and region;
  - account engines use `PublicFeedPolicy::Shared`, fed through `HostEvent::PublicFeedUpdate`;
  - markets are loaded once per venue.
  - This halves the venue socket fleet and protects public rate limits on mass reconnect.

Account identity:

- **Key:** `(userId, accountName)`. The same user on two sockets shares one account actor, and both sockets receive its frames.
- **Different users:** never share an account, even with equal names.
- **Unauthenticated public:** `__pub_bybit` is a single shared, public-only bybit account.

Account construction, from `init`:

1. Validate `InitPayload`. Apply the same normalization as `buildHostedAccountConfig`: the key is passed through, and there is never an API secret.
2. Build options JSON for `safe_cex_venues::build_with_signer`:
   - credential refs only;
   - `payloadSigner` set to the remote signer;
   - `hyperliquidBuilder` validated as in `server.ts`.
3. Get the routing plan from m26ps4's `resolve_routing_plan`. Build `NetworkTransport::with_routing(plan, token_provider)`. `token_provider` returns the socket's current `proxyToken`; after a resume it returns the new socket's token.
4. Register the account on an account thread, join this socket, and send a snapshot frame.

A construction error replies `msg` `{op:'error', accountName, data:[message]}`.
This is the same as `sendInitError`.

Sessions and resume (same semantics as today):

- **Session id:** every socket gets a `resumeId`.
- **On close, `resumable=1` sockets:**
  - private accounts are parked under `resumeId` for 60s, bound to `userId`;
  - other accounts are disposed;
  - chasers and TWAPs die with the socket, as today.
- **Resume:** a new socket presenting `resume=<id>` with the same `userId` adopts the parked accounts. It then receives `session {resumed:true}`.
  - Its first `init` with an identical fingerprint keeps the account and sends a snapshot.
  - A different fingerprint disposes the account and rebuilds it.
- **Resume alias:** a used `resumeId` stays valid as an alias for 120s, which is `RESUME_ALIAS_MS`.
- **Grace expiry:** parked accounts are disposed.

## Remote signer (claude-4yjlw0)

Module `hosts/server/src/signer.rs`. Its interface toward the server:

- `RemoteSigner::new(outbox: SignerOutbox) -> safe_cex_engine::signing::Provider`. `SignerOutbox` is a `Send` handle that queues `{t:'sign-req', d}` on the account's current owning socket. A resume swaps the socket.
- `RemoteSigner::resolve(&self, response: SignRes)` is called for every `sign-res`.
- The 15s timeout is `SIGN_TIMEOUT_MS`.
- Aggregate stats are exposed for `/health.remoteSigner`.
- The wire must match today's browser responder exactly, which is the structured ws-auth wire in `@tealstreet/safe-cex-wasm/host` `SignRequest`. The parity tests are `rust.*-remote-signing.wasm.test.ts` and `rust.delegated-provider.wasm.test.ts`.
- The keyed mode (exchange-server holding keys) uses the local provider through the same `build_with_signer` seam, and is off by default.

## Routing (claude-m26ps4)

The module is `safe_cex_native::routing::resolve`. It sits beside the
`RoutingPlan` type that `NetworkTransport::with_routing` already applies, so the
server gets no new crate dependency.

Interface:

- **`account_transport(exchange, AccountTransport{tealstreetProxy, tealstreetTunnel, proxyRegion}) -> ResolvedTransport`.**
  - It ports `resolveAccountTransport`.
  - It also ports the metadata defaults that `accountToExchangeOptions` applies to a hosted account: `requiresCors`, `enforceWsTunnel`, `supportsTunnel` and `privateProxyRegions`.
- **`resolve_routing_plan(exchange, &RoutingInputs, real_rest_base_url) -> Result<RoutingPlan, RoutingError>`.**
  - `RoutingInputs` is the routing-relevant subset of the normalized `ExchangeOptions`: `tealstreetProxy`, `tealstreetTunnel`, `proxyRegion`, `bybitSiteId`, `tealstreetWsRoute`, `publicDataOnly`, `hasKey`, `payloadSigner`, `overrideProxyUrl`, the presence of a secondary key, secret and passphrase, `secondaryProxyUrl`, and the presence of a passphrase.
  - It carries no credential values and no JWT.
- **Relay:** off, as on today's hosted server, which never calls `setWsRelayEnabled`.

It reproduces the TS resolver exactly for the hosted (Tealstreet transport) case,
using the Vultr hosts from after NX #8306. Hosted shares the browser table:
tunnel, relay, proxy Worker and public CORS.

- **Source of truth:** one generated JSON file holds the per-venue routing data: the routing table, metadata transport flags, the `TOKYO_EGRESS` list and the hosts. A vitest test fails if the JSON drifts from the TS.
- **Goldens:** a vitest file records the TS `resolveEngineRoutingPlan` output, and a cargo test asserts the Rust output equals it.
  - Venues: every core venue (binance x4, bybit x4 plus hyrotrader, okx x4, bitget x3, Hyperliquid plus spot, blofin, bingx, lighter plus robinhood, paradex, phemex).
  - Matrix: proxy, tunnel, region, site, publicOnly and secondary.

The server calls it once per `init`. `CONNECT_REGION` supplies the region
default exactly where the TS server reads it.

## Crate layout (`hosts/server`)

    src/main.rs       env config, runtimes, signal handling
    src/config.rs     environment surface (table above)
    src/http.rs       axum router: /health, /capabilities, upgrade
    src/auth.rs       RS256 proxy JWT verification
    src/envelope.rs   serde types for every message above, golden-tested against server.ts shapes
    src/session.rs    per-socket task: envelope, keepalive, idle timeout, echo, reconcile
    src/accounts.rs   account threads, (userId, accountName) registry, park and resume
    src/methods.rs    callMethod to engine Command
    src/frames.rs     FrameServer Outgoing to msg frame/notice mapping, base64
    src/signer.rs     4yjlw0

Dependencies: `axum` (with `ws`), `tokio`, `jsonwebtoken`, `base64`, `serde_json`
and `safe-cex-native`. Optional `sentry`.

## Algos (claude-tbz5hr chaser, 55ao7t TWAP, zs5e83 exec)

Algos run server-side on the account's thread. They sit on the same `AccountHandle`
a socket uses, as one more in-process client:

- **Store:** `join()` yields the snapshot and then every flush frame. An algo keeps its own `FrameApplier`-equivalent Rust store, decoding with `safe_cex_engine` frame reading. The store is never shared mutably with the engine.
- **Commands:** `command(client, Command)` gives a completion through the same done notice. The no-retry and orphan rules apply.
- **Timers:** a tokio `LocalSet` on the account thread, so algos may hold `!Send` state beside the engine.
- **Lifecycle:**
  - Algos are owned by the account entry in `accounts.rs`. They die on dispose, and on the close of the socket that started them, which matches today's rule that chases die with the socket.
  - They are not parked on resume in v1.
- **Wire:** algo ops (`chaser:*`, `twap:*`, `exec`) reach the algo through a registry in `accounts.rs`: `fn algo(op) -> Option<&dyn AlgoFactory>`. Replies use the existing op names.

## Not in v1 (after cutover)

- `exec` (hosted CLI), and the hosted chaser and TWAP. These run in the TS worker today; they need native ports.
- Shared public data across accounts. Each account engine runs its own public feed, and venue-side public sockets are cheap compared with a cutover risk.
- Slack alerts and the per-venue REST diagnostics in `/health`.

## Candle server

Placeholder for its owner, covering binary, port, health and env.
