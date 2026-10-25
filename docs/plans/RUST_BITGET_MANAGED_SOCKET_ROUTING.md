# Bitget managed sockets and private-WS route parity (TS #8093)

Plan only, 2026-10-25, claude-4yjlw0. This supersedes #1890 (`RUST_BITGET_UTA_ROUTING.md`, which predates managed sockets).
Branch `feat/rust-bitget-managed-socket-routing` from landing `f3ed1f32bf0`.
Reviewer for the shared socket API: claude-dazzy3.

Status: dazzy3 CLEARed the legacy seam shape at `71fbebd9739`. The legacy-path seam is implemented on this branch: engine, Bitget, native host, WASM/JS host and TS transport. Managed adoption is deferred.

## Overseer rulings (2026-10-25, pending dazzy3's plan review)

1. **The legacy-path seam goes first.** The `reject_route` / `WsRouteExhausted` seam on Bitget's legacy socket path, with the 40018/40024 evidence, may go to source after dazzy3 CLEARs this plan. It is measured against the 1,409,024 allowance.
2. **Managed adoption is deferred.** Bitget's managed-socket adoption, item (1), is a separate budget question for Sam after slice 3's ruling. It gets no source until then and is not folded into the slice 3 ask.
3. **Exhaustion matches TS.** Route exhaustion stops reconnecting, with one visible notice and no silent retry loop, consistent with #2109's terminal shape. A test pins zero reconnects after exhaustion.

## Gap (unchanged on the tip)

- **The UTA private socket always connects direct.** Bitget opens classic `v2/ws/private` and UTA `v3/ws/private` on the `private` channel. The routing table lists the v3 URL in `directUrls` (`routingTable.ts:112-137`). The TS engine host (`engine.ts:839`) and the native host (`routing.rs:334`) therefore bypass every route for UTA. TS routes it through `/bitget/v3/private` or its relay.
- **No egress-refusal walk.**
  - Rust recognises 40018 only on REST (`account.rs:2679`), and 40024 nowhere.
  - On the socket, a 40018 or 40024 login refusal is a generic login failure: it reconnects with backoff on the same route.
  - Because the error frame counts as a received message, the silent-streak counter never advances the route.
  - TS (`bitget.ws-private.ts:19,233,402`) calls `WsDirectFallback.rejectCurrent()` and switches route. After the last route it reports once and stops.
- **No host signal for it.** No Rust-to-host signal says "this route's egress was refused", and no exhaustion feedback reaches the venue. Only the TS-only `rejectCurrent` exists (`wsDirectFallback.ts:79`).

## Work items

### (1) Bitget adopts managed sockets

- **Whole venue at once.** The ledger rules out mixed managed and legacy sockets per venue, so all four sockets convert: three public feeds and the private feed.
- **Feed policy** follows OKX `managed_connect` (`okx/src/socket_codec.rs:94-147`), with Bitget's native timings:
  - public: connect 20 s, idle 20 s, ping 5 s;
  - private: auth 10 s, idle 20 s, ping 5 s.
  - `AuthRecovery::Reconnect`.
  - `permanent_refusal_group` for the private feed. It is terminal only for the native permanent codes 30011 and 30012 (`socket_key_rejection`). Generic login failures keep reconnecting.
- **Removed from the venue:**
  - `Connection.supervisor`, the open/ping/auth bookkeeping and the socket arms of `on_timer`;
  - the `on_open`, `on_close` and current-ID filtering.
- **Mapped to lifecycle events:**
  - the login frame becomes `socket_control` (`Accepted` / `Rejected` / `Malformed`);
  - subscriptions go through `set_socket_interests_then`;
  - books are invalidated on `Retired`.
- **Preserved native behaviour:**
  - classic `recover` (`needs_recovery`) keeps its `SocketContext.recover` hint;
  - the UTA/classic URL selection and `promote_uta` reconnect;
  - the write-check reconnect;
  - startup ordering: markets, then `connect(0)`; tickers, then `connect(1)` and `connect(2)`;
  - topic chunking at 100.
- **REDs first**, from `socket_liveness.rs`, `startup_auth.rs`, `public_supervision.rs` and `public.rs:478`. The socket legacy inventory drops Bitget's 5 + 1 fingerprints and `socket-legacy-families.json:9`.

### (2) A route-rejection seam on the managed lifecycle (shared)

- **Field.** `SocketContext.reject_route: bool` sits beside `recover`, with the wire spelling `rejectRoute`. It is omitted when false, so it is wire-neutral for every other venue.
  - Set only by the managed driver, for the current epoch, from venue evidence.
  - Consumed by hosts: TS `WsDirectFallback.rejectCurrent()`, the native `SocketAttempts` advancing `candidate` once, and the WASM JS pass-through.
  - Wins over `recover`.
- **Evidence.** `AuthEvidence::RouteRefused(VenueError)` is a new pre-login control result. The driver retires the epoch with a new `RetireReason::RouteRefused`.
  - The next connect carries `reject_route`.
  - **Backoff bound (dazzy3 A, option i).** Only the first refusal of a streak skips backoff. Every later refusal in the same streak reconnects at ordinary backoff pace, so a host that ignores `reject_route` can never hot-loop on a deterministic 40018/40024. A conforming host still walks every candidate, paying the backoff per extra route. A successful login ends the streak. The producer owns this bound: the managed driver on the managed path, the venue supervisor on the legacy path. The TS counters (2 unauthenticated closes, 6 failed connects) and exhaustion also cap the total redials before the socket stops.
  - RED: a host that ignores `rejectRoute` sees one immediate redial, then backoff-paced attempts, then a stop. It never sees a tight loop.
- **Composition with `recover` (dazzy3 C).** `reject_route` and `recover` are orthogonal and are both carried. `reject_route` decides the candidate for that one connect: advance exactly one, and do not apply `recover`'s jump-to-last in the same connect. `recover` keeps its meaning for venue state: classic Bitget's `needs_recovery` resubscribe is still carried and honoured, so a route switch never drops recovery. In short, the host advances the route even when `recover` is set, and nothing else about `recover` changes.
  - RED: a classic route switch with `needs_recovery` set advances one candidate and still carries `recover`. The reconnect runs the classic recovery resubscribe.
- **Exhaustion.**
  - When a `reject_route` connect has no candidate left, the host does not dial. It emits `Event::WsRouteExhausted { socket }`.
  - That event is host-only, enqueued after `connect` returns, and dropped for a stale socket.
  - **It is that socket's only terminal.** A host that emits it emits no open and no close for the socket. The engine treats it as that epoch's sole terminal, so there is never a double retire.
  - The driver turns it into `LifecycleEvent::RouteExhausted`. It is never a peer close.
  - The default handling is today's close-and-retry path, so venues that don't opt in are unchanged.
- **The connect after exhaustion (dazzy3 B).** Any connect after `RouteExhausted` carries `reject_route = false`, and the host dials the last candidate: it stays where the walk ended, matching `WsDirectFallback`'s sticky choice. Native `SocketAttempts`, TS `WsDirectFallback` and the WASM pass-through agree on this. For Bitget, ruling 3 means no such connect happens: it stops. Venues on the default close-and-retry path use this rule.
  - REDs per host: refuse every route, then exhaustion with no dial, then the next connect dials the last candidate with no second exhaustion.
- **One host contract on both paths.** `rejectRoute` plus `WsRouteExhausted` is the same host contract on the legacy and managed paths; only the producer differs. Adoption later moves only the evidence source, not the hosts or the event.
- **Private UTA route.** The engine and native host plans gain a URL-scoped private route (`wsPrivateUta`), mirroring `public_ws_routes`. The UTA `directUrls` bypass and its comment are removed from `packages/safe-cex/src/transport/tealstreet/routing/routingTable.ts`, the TS single source. That TS change is part of the same cutover and is named in Callers, so native and TS can't diverge.
- **Owner and review.** This is the shared socket core: dazzy3 reviews it. The original REDs cover the engine, TS host, native host and WASM host.

### (3) Bitget 40018/40024 evidence

- **Pre-login refusal.** A 40018 (IP-bound key) or 40024 (restricted location) refusal before login maps to `RouteRefused`. It logs TS's "trying the next route" warning and reports no error.
- **Exhaustion (ruling 3, matches TS).** On route exhaustion, report the native egress refusal once as an `AUTH_REJECTED` notice, never a startup failure or deactivation, and stop reconnecting the private socket. There is no silent retry loop; a test pins zero reconnects after exhaustion. Recovery is a new start, as in TS.
  - **Startup settles (dazzy3 D).** Bitget's private startup readiness is the REST loads (`startup_ready` = balance && positions && orders, `account.rs:995`), not private-socket auth. Public readiness is markets && tickers. If exhaustion happens before the private socket ever authenticates, nothing waits on it.
    - `start()` resolves normally, with REST-loaded private state plus the one `AUTH_REJECTED` notice. It is never `StartupFailed` and never a deactivation.
    - The venue marks the account degraded, so the account poll refreshes balance and orders every tick in place of the stopped stream.
    - The zero-reconnects RED pins all of this: exhaustion before first auth gives no `StartupFailed`, loaded startup state, one notice, and zero further private connects.
  - dazzy3 proposed instead retrying the last route under a long capped backoff (60 s or more). That is recorded as the alternative; ruling 3 stands unless the overseer reopens it.
- **TS counters.** Carried over as data: `MAX_UNAUTHENTICATED_TUNNEL_CLOSES = 2` and `MAX_FAILED_TUNNEL_CONNECTS = 6`. Each sends `reject_route` once its threshold is met.
- **REST is unchanged.** REST 40018 keeps `AuthPermission`.

## Size (Bitget family at engine s; local raw, function names excluded; CI factor ~1.0)

The base is tip `f3ed1f32bf0`: Bitget is 1,340,440 in CI, and the allowance is 1,409,024 (68,584 of room).

| Item | Bitget family | Combined bundle |
| --- | ---: | ---: |
| `managed-sockets` linked, no use (measured, `ms-bare` vs `ms-feat`) | **+53,253** | about 0 (four adopters already unify the feature) |
| (1) adoption use: engine paths reached, net of Bitget's supervisor removal (estimate; WOO's whole adoption was +148,677, 77% engine, before fe7's -12.7K outline) | +25K to +50K | +5K to +15K (Bitget `Pending` instantiations) |
| (2) seam: field, evidence, retire reason, event arm | +1K to +3K | +3K to +8K (every managed family links the arm) |
| (3) Bitget evidence and counters | +1K to +2K | +1K to +2K |
| **Total, (1) to (3)** | **about +80K to +108K, so about 1,420K to 1,448K** | about +10K to +25K |

**Adoption alone does not fit the 1,409,024 allowance.** Even before slice 3, it is about 11K to 39K over.

### The first source item (ruling 1): the seam on the legacy socket path, without managed adoption

`SocketContext` already reaches the hosts from Bitget's legacy `cx.connect` (`lib.rs:313-356`). The same `reject_route` flag, the host-only `WsRouteExhausted` event and a default-close `Venue::on_route_exhausted` can therefore work without the managed lifecycle.

- Estimated size: about +3K to +6K on Bitget and +3K to +8K combined.
- Trade-off: it extends a path the engine marks as a temporary migration boundary. Because the host contract is the same on both paths, adoption later moves only the producer.
- **Ownership on this path.** The venue supervisor owns the skip bound (A) and treats exhaustion as the socket's only terminal. The connect-after-exhaustion rule (B) applies to venues that keep the default.

## Ordering against slice 3 under the Bitget budget

Both items compete for the same 68,584 bytes of room:

| Option | Bitget estimate | Over 1,409,024 by |
| --- | ---: | ---: |
| Slice 3 alone (exact CI) | about 1,467K | about 58K |
| Managed adoption alone | about 1,420K to 1,448K | about 11K to 39K |
| Both | about 1,550K to 1,575K | |
| Legacy-path seam alone | about 1,346K | fits |

Ruled order:

1. **The legacy-path route seam**, with the 40018/40024 evidence. It goes to source after dazzy3's CLEAR. It fits today and is measured against 1,409,024.
2. **Slice 3**, held for Sam's budget ruling.
3. **Managed adoption.** It is a separate later Sam budget question and gets no source until then.

## Callers

A custom host must honour `reject_route` and emit `WsRouteExhausted`, or Bitget UTA and egress parity is lost. No SDK option changes. Recorded in `RUST_CUTOVER_CALLERS.md` at production time.
