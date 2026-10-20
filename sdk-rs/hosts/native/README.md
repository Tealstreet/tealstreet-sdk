# Native transport routing

`NetworkTransport::new()` connects directly. Account-backed server callers must
pass the same resolved routing plan as the JavaScript host. The TypeScript SDK
owns the venue table, metadata defaults, proxy pools, region choice and feature
flags; the native host applies a plan and never resolves venue names itself.

Resolve once at the account boundary, after normalizing its `ExchangeOptions`:

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

Paradex delegated signing keeps the private subkey outside the engine. Set
`Options.payload_signer = true` and configure the host callback alongside routing:

```rust,ignore
use safe_cex_native::PayloadSigner;
let signer: PayloadSigner = Arc::new(move |request| {
    let account_context = account_context.clone();
    Box::pin(async move {
        // request carries exchangeName, the public key and the StarkNet message hash.
        account_context.sign_payload(request).await
    })
});
let transport = NetworkTransport::with_routing(plan, tokens).with_payload_signer(signer);
```

The host returns a signature event before the engine emits HTTP. Signer headers
travel with that exact request; fixed venue authentication headers take precedence.
Signing and HTTP share one 20-second deadline, and late signer answers cannot
resume a failed request. An unconfigured callback fails before network I/O.
Paradex JWT generation follows the TS raw-fetch path and always goes direct;
ordinary order requests retain the resolved account route.
