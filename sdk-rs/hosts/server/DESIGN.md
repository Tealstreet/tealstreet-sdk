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

### Image contract (codex-mxb4mq)

The SDK Dockerfiles build from the repository root and install native binaries
only into a Debian slim runtime running as UID/GID 65532. Exchange publishes as
`ghcr.io/tealstreet/exchange-server-native`, port 23456; candle publishes as
`ghcr.io/tealstreet/candle-server-native`, port 23457. Both use `GET /health`.
Candle's `candle-cache-owner` is installed beside `candle-server` after a separate
Cargo invocation, preserving engine/cache JSON feature isolation.

NX owns `docker-exchange-server-rust.yml` and `docker-candle-server-rust.yml`.
Each publishes `latest` and the full source SHA to GHCR, then keylessly signs
the exact pushed digest. Vultr requires GitHub OIDC issuer
`https://token.actions.githubusercontent.com` and an exact certificate identity
(`cosign verify --certificate-identity`, not a workflow-directory prefix):

- Exchange: `https://github.com/Tealstreet/tealstreet-next/.github/workflows/docker-exchange-server-rust.yml@refs/heads/master`
- Candle: `https://github.com/Tealstreet/tealstreet-next/.github/workflows/docker-candle-server-rust.yml@refs/heads/master`

Images do not contain credentials and the workflows do not deploy hosts.

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
- **`GET /capabilities` and `GET /account/capabilities`:** the same JSON as `getExchangeServerRuntimeCapabilities()`, with `"runtime":"rust"` added.
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
| `session` | `{resumeId, resumed}` | On open, for `resumable=1` sockets |
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

No version gate (hard cut): the client handles `op:'frame'` unconditionally and still accepts `store` and `store:delta`.

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
   - the remote signer's `options_json` (`remoteSigning: true`, `payloadSigner: false`);
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
  - chases and TWAPs live with the account and keep running through the park (Sam, the COSIGNED blip window); a socket close no longer ends chases. A joining socket gets the algo store rows again.
- **Resume:** a new socket presenting `resume=<id>` with the same `userId` adopts the parked accounts. It then receives `session {resumed:true}`.
  - Its first `init` with an identical fingerprint keeps the account and sends a snapshot.
  - A different fingerprint disposes the account and rebuilds it.
- **Resume alias:** a used `resumeId` stays valid as an alias for 120s, which is `RESUME_ALIAS_MS`.
- **Grace expiry:** parked accounts are disposed with the reason `no browser for 60s`, recorded in the order audit log. No browser is attached to tell, so when chases or TWAPs were running the registry keeps one notice per (user, account) and sends it as an `error` on that account's next init; it is dropped once delivered or after 24 hours.

## Remote signer (claude-4yjlw0)

This has landed as the crate `safe-cex-remote-signer` (`sdk-rs/hosts/remote-signer`, #2381). `src/signer.rs` only wires it in.

- **`SignerLink::new(socket_id, outbox)`:** create one per account. `socket_id` is a `u64` the server assigns per client socket. It is the twin of TS `WebSocketRemoteSigner`, which is also per account.
  - `Outbox` is `Arc<dyn Fn(String) -> bool + Send + Sync>`. It queues one text frame on the account's owning socket and returns false when the frame was not sent.
  - The frames are `{"t":"sign-req","d":RemoteSignerRequest}`, with `t` first, and the timeout is the 15s `SIGN_TIMEOUT`.
- **`remote_account(link, venue, Some(accountName), engine_options_json)`:** returns a `RemoteAccount` with:
  - `provider`, which you pass to `build_with_signer` / `FrameServer::add_venue_with_signer`;
  - `options_json`, which you construct the engine with;
  - `on_fatal(cb)`, which fires once after 10 consecutive failures spanning at least 30s. Dispose the account, as today.
- **Input:** `engine_options_json` holds the options the server would otherwise build. That includes the public `key` and an optional `passphrase`, with no secret.
- **Output:** `options_json` has those fields stripped and carries `signing` (opaque refs), `remoteSigning: true`, `payloadSigner: false` and `publicMode`.
- **`sign-res`:** call `link.resolve(socket_id, d)` only on links of accounts this socket joined. A request resolves only from the socket it was sent on, and only once. An oversized field settles as an error.
- **Resume:** on park, call `link.suspend()`. A sign already sent keeps waiting, and a sign requested while suspended (an algo order during the gap) waits for `attach`. Both wait within their one 15s `SIGN_TIMEOUT`, then fail as unavailable without counting toward the fatal streak. Neither is queued past the timeout, and the chase or TWAP reports the failed placement. On resume, call `link.attach(new_socket_id, new_outbox)`, which re-sends every pending sign over the new socket; only that socket's answer resolves it, and a sign it cannot take aborts. On dispose, call `link.mark_unavailable()`, which aborts everything.
- **Echo:** `link.send_echo()` alternates bare and padded-to-last-sign-req probes, one outstanding at a time. Feed `echo-res` `d` to `link.echo_res(d)`, which returns `(rtt, EchoSize)`.
- **Wire coverage:** the wire is the existing browser responder's ws-auth wire. That covers:
  - the HMAC families, with hex, base64 and hex-base64 encodings, a passphrase Present, and the extraData codecs `secondsTimestamp`, `bybitExpiry` and `binanceSpot`;
  - `bitunix-double-sha256`;
  - `hyperliquid-address` and `hyperliquid-l1`;
  - `lighter-tx`;
  - `paradex-public` and `paradex-stark`.
- **Address and key binding:** Hyperliquid's address and Paradex's Stark key bind through the engine's owned address setup (`create_public_address_setup`). This needs the native host's `address-nonce` feature for those venues.

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
    src/signer.rs     4yjlw0 (wires safe-cex-remote-signer)

Dependencies: `axum` (with `ws`), `tokio`, `jsonwebtoken`, `base64`, `serde_json`
and `safe-cex-native`. Optional `sentry`.

## Algos (claude-tbz5hr chaser, 55ao7t TWAP, zs5e83 exec)

Algos run server-side on the account's thread. They sit on the same `AccountHandle`
a socket uses, as one more in-process client:

- **Store:** `join()` yields the snapshot and then every flush frame. An algo keeps its own `FrameApplier`-equivalent Rust store, decoding with `safe_cex_engine` frame reading. The store is never shared mutably with the engine.
- **Commands:** `command(client, Command)` gives a completion through the same done notice. The no-retry and orphan rules apply.
- **Timers:** a tokio `LocalSet` on the account thread, so algos may hold `!Send` state beside the engine.
- **Lifecycle:**
  - Algos are owned by the account entry in `accounts.rs` and die on dispose. Chases and TWAPs keep running while the account is parked.
- **Wire:** algo ops (`chaser:*`, `twap:*`, `exec`) reach the algo through a registry in `accounts.rs`: `fn algo(op) -> Option<&dyn AlgoFactory>`. Replies use the existing op names.

## Multi-node (fe7lgm; post-cutover design, not implemented)

Today `connect-tyo` is one Cloudflare tunnel to exchange-01. exchange-02 is a
cold standby with cloudflared off. Losing exchange-01 drops every hosted user
until someone starts the standby by hand. This section plans two boxes
without letting one account run on both.

### Invariants

1. **One owner.** A private `(userId, accountName)` has a running engine and algo host on at most one node at a time. Two owners could mean two TWAP schedules or two chase loops placing orders for the same key.
2. **Resume where issued.** A `resumeId` only means something on the node that issued it, because parked accounts live in that process's memory.
3. **Fail closed.** If a node cannot tell whether it owns an account, it does not run algos for it. A user can always recover with a fresh `init`.

### Topology: active/standby behind a load balancer

- **Both boxes run the server.** Each has its own Cloudflare tunnel and cloudflared. Neither box is cold any more.
- **Per-node hostnames.** Each tunnel serves `connect-tyo-<node>.tealstreet.io`, for example `connect-tyo-01`. It is used only for resumes.
- **`connect-tyo.tealstreet.io` sits on a Cloudflare Load Balancer:**
  - the primary pool is exchange-01's tunnel and the fallback pool is exchange-02's;
  - pools are health-checked on `GET /health`;
  - it is failover, not round-robin, so all fresh inits land on one node and invariant 1 holds without coordination in the steady state.
- **No automatic fail-back.** After a failover, the operator marks the old primary's pool disabled until a quiet window. Otherwise new inits split across both nodes while old sessions still run on the other one.
- **New environment variable:** `TEALSTREET_NODE` (for example `01`) is the node's identity. It appears in `/health`, in every log line (including `order audit`) and in the session frame. It is identity, not a feature switch: every node runs the same code.

### Session frame and client resume

- **The session frame gains the node:** `session {resumeId, resumed, node}`. The web worker (`ExchangeServerWorker.ts`) stores `node` beside `resumeId`.
- **Reconnect with a `resumeId`:**
  1. The client first dials `wss://connect-tyo-<node>.tealstreet.io/account?resume=<id>&resumable=1`, with a 5s open timeout.
  2. `session {resumed:true}` continues as today.
- **Fallback:** the client uses a fresh `init` on `wss://connect-tyo.tealstreet.io/account` (no `resume`) and drops the old id when:
  - the per-node dial fails or times out;
  - it returns `resumed:false`;
  - it closes with 1013 (overloaded).
- **A resume that reaches the wrong node** (an old client, or the load balancer) finds no parked session. It gets `resumed:false` with a fresh `resumeId`, as today, and the client re-inits on that socket. The server never forwards sockets between nodes.

### Parked sessions on failover

- **When a node dies:** its live and parked accounts die with it.
  - Resting venue orders stay on the venue.
  - The client's fresh `init` on the survivor rebuilds the account from the venue, so open orders and positions reappear in the snapshot.
- **When a node is alive but unreachable** (a tunnel or network partition): its parked accounts would otherwise live out the 60s grace while the client re-inits elsewhere. Two measures cover this:
  - **Parked algos keep running (Sam's decision, superseding an earlier "parked algos hold").** Chases and TWAPs keep placing for up to the 60s grace; their orders need the browser's signature, so during the gap each waits up to the 15s signing timeout and then fails visibly. Grace expiry stops them and disposes the account.
  - **Evict on init.** Before building a private account, a node sends `POST /internal/evict {userId, accountName}` to each peer (`TEALSTREET_PEERS`, a node to private-address list).
    - The request is authenticated with an HMAC under `EXCHANGE_SERVER_PEER_SECRET` and is served only on the private interface, never through the tunnel.
    - The peer disposes any parked holder of that account, running algo shutdown first.
    - A peer that holds the account for a live socket replies `409 {node}`. The initiating node then refuses the init with `msg {op:'error', data:['account is active on node <node>']}` plus a `redirect {node}` hint. A client that gets the hint retries on that node's hostname.
    - An unreachable peer does not block the init; the bounded risk of not waiting is covered by the next point.
- **A bounded window remains.** If a peer is alive, holds a parked account, and cannot be reached for eviction, it can still hold that account for up to 60s. Its algos are held, so it places nothing. This is the accepted residual.

### Server-side algos on failover

- **Chases end** when their socket closes (unchanged), so a failover never moves a chase.
  - A chase's resting limit order stays on the venue as an ordinary open order.
  - The client shows it, and the user can cancel or re-chase.
- **TWAPs move only through the client.** The browser keeps its TWAP state and sends `twap:state` after the init on the new node, which restores schedules there; restore already skips ids it holds. There is no server-to-server algo transfer: the client's copy is the one record both nodes can see.
- **Duplicate risk.** `twap:state` restoring on a node while another node still runs the same TWAP is the only path to duplicate algo orders. Invariant 1 (load balancer failover, held parked algos, evict on init) closes it. Note that `twap:state` places orders, so it is a write: the `order audit` log must record it like `twap:start`.
- **Exec** commands are one-shot. A failover mid-command reports through the order's normal outcome, `unknown` when the completion was lost, and the client reconciles from the snapshot.

### Later: a shared lease

If active/active is ever wanted, the server should hold an owner lease per `(userId, accountName)` in Redis instead of relying on the evict call:

- The lease is `SET acct:<user>:<name> <node> NX PX 15000`, renewed every 5s.
- A node that loses its lease stops the account's algos at once and disposes it.
- `REDIS_URL` is already in the environment contract.
- This adds a dependency, so it waits for the Cargo.lock freeze to lift.

## Hosted method scope (claude-dazzy3)

A COSIGNED account's main-thread mirror forwards each engine read to this
server by its `callMethod` name, using the same list as LOCAL
(`rust.mirror-forwarding.test.ts`, SDK #2506).

- **Translated:**
  - `fetchOrderBook` (when the venue has `orderBookQuery`);
  - `fetchStpMode` (`stpControls`; private accounts);
  - `fetchLeverageForSymbol` as the engine's `fetchLeverage`. The mirror answers from its frame-fed rails first and asks only when they can't; a public account gets `null`.
- **Unsupported on hosted.** These are visible `UnsupportedMethod` refusals, never a guessed command, and are pinned by `options_stp_writes_and_rails_max_leverage_stay_unsupported_hosted`:
  - `fetchOptionChain`, `fetchOptionExpiries`, `fetchAccountGreeks`: options are out of scope;
  - `changeStpMode`: AsterDEX only, not a core venue;
  - `fetchSymbolMaxLeverage` with the `rails` source (Polymarket). The `market` source answers on the mirror.
- HyroTrader accounts are refused at init (`HYROTRADER_UNHOSTED`): their endpoints are browser build configuration.

## Not in v1 (after cutover)

- `exec` (hosted CLI), and the hosted chaser and TWAP. These run in the TS worker today; they need native ports.
- Shared public data across accounts. Each account engine runs its own public feed, and venue-side public sockets are cheap compared with a cutover risk.
- Slack alerts and the per-venue REST diagnostics in `/health`.

## Candle server

Placeholder for its owner, covering binary, port, health and env.
