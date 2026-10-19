# Rust engine transport routing

Branch: `feat/rust-transport`, based on `origin/feat/rust-rewrite`.

Keep the engine sans-I/O and venue URLs unchanged. Resolve account routing in
TypeScript through the existing transport provider / `resolveExchangeRouting`;
apply the result at the JavaScript and native host boundary. No second routing
table and no credentials in committed fixtures.

## Phases

1. **Routing and HTTP.** Audit current TS option resolution and per-venue
   exceptions. Add a resolved host contract and route engine HTTP through the
   existing REST proxy authorization and HOW package, preserving signed bytes,
   status/error bodies and streaming direct/proxy responses. Wire the SDK realm
   transport factory without bypassing custom providers or replay transports.
2. **Sockets.** Apply resolved public/private/business host selection, retain
   sticky silent-connect fallback state across engine socket IDs, add relay via
   the existing HOW package, preserve upgrade credentials and lifecycle order.
3. **Native host.** Consume TS-resolved configuration without a Rust venue table;
   support proxy/auth and generic HOW/relay protocols for server deployments.
   Exercise routed effects through a recording transport/local protocol servers.
4. **Gates and landing.** Assert actual outgoing URLs/headers/socket hosts against
   the TS routing golden matrix and account metadata normalization. Run SDK,
   host, Rust, repository and replay regression gates; merge latest landing,
   push and open PR into `feat/rust-rewrite`. Post adversarial sub-agent findings,
   fix/repeat, wait for green CI, merge and cut a clean follow-up branch.

Each phase: plan audit, implementation, focused verification, independent
sub-agent review, fixes and committed progress. Full auto; never stash, rebase,
force push, or touch master. Landing conflicts may be resolved preserving the
nested workspace.

## Progress

- Initial inspection: engine SDK bypasses TransportProvider entirely; JS and
  native hosts go direct. Existing TS provider already owns proxy credentials,
  REST precedence, WS routes and upgrade headers. Native HOW requires a generic
  protocol implementation because it cannot invoke a JS package.
- Plan audit underway with independent TS routing and native-host reviewers.

### Implementation checkpoint

- HTTP and sockets now wrap the registered TS provider. REST policies reuse the
  cached resolver output, authorization stays per operation, and HOW uses the
  existing package with exact raw request/response strings. Preserved Binance
  public-client, BingX default tunnel and WEEX/GRVT/BloFin adapter exceptions.
- Added explicit sans-I/O socket roles through Cx/actions, wasm/native hosts and
  all six venues. Hosts reuse sticky fallback/relay behavior; JS relay messages
  share ordered binary decoding and close guards. Workers can install the
  account transport factory and fail closed on routing requests without it.
- Native consumes a serializable TS-resolved policy, with dynamic context JWTs,
  proxy rewriting, generic HOW/relay handshakes, raw/compressed response decoding,
  deadlines, cancellation and no sent-write replay. No Rust venue routing table.
- Phase 1/2 independent reviews fixed lowercase Binance key routing, missing
  Brazil site headers, relay send ordering/close handling and compressed relays.
  Phase 3 review fixed HOW v2 query parity, strict raw response handling, empty
  header selection and cancellation during token acquisition.
- Focused routing: 49 new TS tests (including 3,672 first socket destinations),
  existing routing goldens unchanged. Native: 19 tests; JS hosts: 13 tests.
  Repository typecheck 44/44, lint 28/28, test tasks 27/27 passed (6,540 safe-cex
  tests per suite before the final binary regressions). Full Rust checks and wasm
  rebuild/replay verification continue in phase 4.
- Existing secondary signing options cannot be honored by the engine's single
  signer; reject them before startup instead of silently using primary egress.

### Phase 4 verification checkpoint

- Rebuilt six-venue wasm and passed 21 real SDK/engine tests, including actual
  signed OKX HTTP effects reaching the proxy and private/business socket routes.
- Added a shared 25-case TS/native golden fixture. The TS suite regenerates its
  oracle from the existing provider/resolver; native deserializes that exact
  policy and checks 27 HTTP selections and 375 socket transitions.
- Adversarial cross-scope review found a Workers-specific gap: HOW and relay
  bypassed the Workers header-capable socket dialer, and proxy authorization
  omitted non-browser fetch headers. Implementing an injected raw WebSocket
  constructor in the existing HOW package, with per-adapter lifecycle/disposal;
  the browser/Node global clients retain their existing behavior.
- Landing advanced with MEXC while validation ran; merge it after this host
  correction and include its regression gates before opening the PR.
