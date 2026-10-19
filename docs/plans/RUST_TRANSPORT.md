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
