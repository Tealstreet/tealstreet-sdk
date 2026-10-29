# Native transport routing

`NetworkTransport::new()` connects directly. Account-backed server callers must
pass the same resolved routing plan as the JavaScript host.

A native server resolves it with `resolve::account_transport` (the hosted
account's proxy/tunnel/region defaults) and `resolve::resolve_routing_plan`, a
port of the TS resolver for the Tealstreet and direct providers with the browser
relay gate off. Its data is `src/routing/table.json`, generated from the TS tables
by `packages/safe-cex/src/transport/engine.native-routing-port.test.ts`
(`UPDATE_NATIVE_ROUTING_PORT=1`), which also writes the TS plans that
`tests/routing_port.rs` holds it to. Secondary read keys are not routed natively yet.
The deployment can replace the proxy hosts: `PROXY_PRIVATE_URL` sets every
whitelisted private pool and `PROXY_PUBLIC_URL` the public CORS proxy (https, no
query; a trailing `/` is dropped). Unset keeps the table's hosts.

A JavaScript launcher instead resolves once at the account boundary, after normalizing its `ExchangeOptions`:

```ts
import { resolveEngineRoutingPlan } from '@tealstreet/safe-cex/src/transport/engine';

const plan = resolveEngineRoutingPlan(exchange, normalizedExchangeOptions, realRestBaseURL);
const routingJson = JSON.stringify(plan);
// Give routingJson to the native process alongside its engine configuration.
```

Do not serialize API credentials or proxy JWTs in the routing policy. Native
callers provide a function that obtains the current token from their account or
request context, including the exchange-server token provider's errors:

```rust,ignore
use safe_cex_native::{NetworkTransport, ProxyTokenProvider, RoutingPlan};
use std::sync::Arc;

let plan: RoutingPlan = serde_json::from_str(&routing_json)?;
let tokens: ProxyTokenProvider = Arc::new(move || {
    let account_context = account_context.clone();
    Box::pin(async move { account_context.current_proxy_token().await })
});
let transport = NetworkTransport::with_routing(plan, tokens);
let host = safe_cex_native::Host::new(engine, transport);
```

A plan describes private/public HTTP routes, required request headers, regional
HOW endpoints, socket hosts, and the feature-gated relay/fallback chain. Explicit
engine socket channels select public/private/business routes; substring rules in
the plan support older untagged effects. The engine continues to sign and emit
the venue's actual URLs and headers.

Optional `readHttp`, `readHeader` and `readPublic` fields preserve the TS
secondary client's GET-only egress. Optional `readPublicHttp` preserves a distinct
public route after the selected client removes authentication headers (Binance
Spot); absent fields retain the previous read route. The engine selects its read credentials;
this routing policy contains no read key or secret. Explicit `primary` request
hints retain the account route for definitive order lookups and independent
primary-only reads. Mutations and posted reads never select `readHttp`.

The native host applies proxy prefixes before sending, refreshes proxy tokens
per operation, preserves original request bodies, and streams HTTP results in
16 KiB chunks. A HOW request uses a dedicated tunnel connection with raw response
negotiation, supports TSB1 gzip and older text encodings, and honors automatic
IP-whitelist authentication. Token acquisition, authentication and response
handling share one 20-second deadline. Transport recovery never replays a sent
HTTP write.

Private socket relays hide tunnel authentication and report open only after the
upstream is ready, under a 15-second deadline. Per-feed silent connections
advance the sticky candidate chain, matching the SDK's `WsDirectFallback`.
Resolved policy supplies initial silence, exact ignored messages and real URLs
that stay direct. Protocol recovery hints prefer a declared fallback; stable
feed identities isolate sockets that share a role and URL. Other decoded venue
messages reset the silent streak. The native TLS WebSocket upgrade uses
HTTP/1.1; Tealstreet upgrade and REST headers come from the resolved policy.

Paradex signs through its trusted local Stark provider: `build` consumes the
account key in trusted setup and seals the provider's public key. There is no
delegated payload-signing callback; a private Paradex account with an external
signer refuses. Paradex `/auth` always goes direct; ordinary requests retain the
resolved account route.

# Network frame server

`FrameServer` runs one preconfigured `Host` per account, on a tokio `LocalSet`.
Accounts start independently of client arrival, and keep running after the last
client disconnects. The embedding application supplies each venue engine,
credentials, transport/routing plan and account token. The library can register
heterogeneous venues with `add_account`, or by venue name with `add_venue`,
which builds the engine through `safe-cex-venues` from the same options JSON the
wasm host accepts (the default `venues` feature). It does not accept account
creation or credentials over the client protocol.

For migrated dispatch-signing families, prefer opaque credential refs plus a
provider injected with `Host::with_signer` or `add_venue_with_signer`. The local
provider owns the keys; engine/venue options retain refs only. `add_venue` still
normalizes legacy credential inputs through trusted setup until cutover callers
migrate. Neither registration method adds credential setup to the client wire.
See [the signer contract](../../../docs/plans/RUST_SIGNER.md).

Register engines before sending them any commands, including subscriptions. All
commands must enter through the server so its internal IDs cannot collide with
pre-existing venue jobs. Registration rejects an engine that has received any
command. Store seeding and command-free startup are allowed.

Run the Binance example with an opaque base64url-style token in
`FRAME_SERVER_TOKEN`:

```bash
cargo run -p safe-cex-native --example frame-server
```

It binds `127.0.0.1:9001` by default (`FRAME_SERVER_BIND` changes this). Optional
`BINANCE_API_KEY` and `BINANCE_API_SECRET` together enable private account access;
without them it is public-only. Ctrl-C shuts down the listener, clients, account
actors and timers. TLS termination, application sessions, credential provisioning,
process supervision and production routing remain the embedding deployment's
responsibility. Use `NetworkTransport::with_routing` for accounts whose resolved
policy requires a proxy; the simple example uses the direct transport.

Connect to `/accounts/<id>` with `Authorization: Bearer <account-token>`. Browser
clients, whose WebSocket API cannot set that header, send subprotocols:

```ts
const ws = new WebSocket(url + '/accounts/binance', [
  'safe-cex-frame.v1', 'bearer.' + accountToken,
]);
ws.binaryType = 'arraybuffer';
```

The upgrade response echoes only `safe-cex-frame.v1`. Tokens are ASCII HTTP
protocol tokens (base64url without padding works), nonempty, unique per account,
and never accepted in query strings. A token grants full state and command
access to its account; book subscriptions govern upstream interest, not a
security filter between clients sharing an account. Terminate TLS before using
this protocol across an untrusted network.

The first binary message is a full SCXF snapshot. Subsequent binary messages are
ordered flush frames; `BOOK_DELTA` is enabled on these engines. Apply each frame
once, in order through the shared `FrameApplier`. A connection establishes the
current shared flush boundary before taking its snapshot, so late arrival cannot
lose a level that subsequently reverts. Reconnect always starts with a new full
snapshot; never retain a previous connection's delta baseline.

Send the exact same JSON command objects as `WasmEngine.command`, for example:

```json
{"op":"subscribeOrderBook","symbol":"BTCUSDT"}
{"op":"fetchCandles","command":1,"symbol":"BTCUSDT","interval":"1m","limit":100}
```

Polling configuration uses the same host-input decoder as WASM:
`{"op":"setPollScale","scale":2,"ceiling":10000}` scales each venue poll's own
cadence (2x slower here) and bounds slowed polls at the optional `ceiling`
milliseconds unless a native cadence is already slower; `scale: null` pauses
optional polling. Zero, negative, missing or wrongly typed values are rejected. Configuration has no command ID or result and
applies to the account, including other connected clients. Very long future
timers remain cancelable even when their deadline exceeds the system clock.

All trading, account and query variants use the shared typed decoder; individual
venues still reject operations they do not implement. Text
responses have `type: "done"`, the caller's `command`, and either `result` (a
JSON string, as in the WASM host) or `error`; a structured failure adds
`detail` (`engine/src/error.rs`) beside the unchanged `error` text. IDs need only be unique among that
client's pending commands; the server allocates separate internal IDs. Duplicate
pending IDs and resource-limit failures produce `type: "error"` (with `command`
where known). Malformed JSON and binary commands produce an error without venue
side effects. Diagnostics (`type: "event"`, `level: "debug" | "info" | "warn" | "error"`,
`code`, `message`, optional `fields`), `type: "deactivate"`
and `type: "startupFailed"` (with `detail`) notices go to all clients of the
account. `type: "accountAuth"` credential evidence goes only to clients that also
offered the `safe-cex-frame.account-auth` subprotocol, because earlier v1 clients
close on an unknown notice; the response still echoes only `safe-cex-frame.v1`.
Results go only to their requester.

Repeated book/candle subscriptions are idempotent per client, and the venue sees
only first-subscribe/last-unsubscribe transitions. Another subscription to a live
book requests a full BOOK baseline on the next flush. Ticker interest is the
union of clients' explicit symbols; an empty set means all symbols, matching the
existing command contract. Disconnect releases interest. No-client ticker
interest restores the engine default. All clients still receive the account's
shared frames.

`ServerConfig` bounds connection count, clients per account, outbound queue depth,
message/frame size, subscriptions and pending commands. Default flush cadence is
60 ms, matching the existing worker book cadence; default inbound JSON limit is
64 KiB and outbound frame limit is 16 MiB.
Symbols are at most 256 bytes and candle intervals at most 64. A slow peer is
canceled when its outbound queue fills; messages are never silently discarded.
Socket upgrade/write/close operations have a deadline. An oversized outgoing
frame disconnects affected clients; deployments must choose a frame limit large
enough for their full account snapshot. Limits cover current interest and all
pending engine jobs, including orphaned jobs whose client has disconnected.
Orphan IDs remain reserved until the venue finishes: disconnect never cancels or
replays a potentially submitted trading write. A permanently stuck venue job
therefore continues to occupy a bounded slot. Exhausted internal IDs fail closed
rather than wrapping. The server never retries client trading commands.

Diagnostic codes are stable; messages are display data. Engine and host-origin
warnings use the same event envelope. Warn+ is written to stderr without a
client. Deploy safe-cex-server's event-aware NativeFrameSession before native
clients rely on Info+ venue events; older sessions reject the envelope. This
library change does not deploy the server.
