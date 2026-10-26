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

### Workers and cancellation correction

- Workers now expose a header-capable raw WebSocket constructor used by the
  existing HOW and relay clients; injected HOW clients belong to the account
  adapter and close on disposal. Server fetch authorization adds the required
  proxy headers without relying on Node globals. Added typed Workers exports.
- Independent reviews fixed pending-JWT writes after host disposal in JS and
  native, included authorization/streaming in the JS request deadline, and made
  the HOW adapter honor per-request cancellation. Shared refresh waiters recover
  when a different request/adapter is canceled; accepted writes are never retried.
- Resolve provider base URLs without signed query data, then append the original
  request suffix verbatim, so a query containing a venue name cannot select a
  different proxy pool. Secondary-key rejection covers all host entry points.
- Verification: 46 HOW tests, 22 native tests, 17 JS host tests and 21 real SDK /
  wasm tests pass. Routing tests cover proxy-token disposal/deadlines, abandoned
  streams, Workers upgrade headers and raw/compressed protocol messages. All six
  public replay gates pass; private replay verification is finishing.

### Landing integration

- All 15 six-venue replay gates passed before the landing merge. Merged
  `origin/feat/rust-rewrite` through `bc2b61cc4a` as `fc7b493896`, preserving both
  README sections and MEXC's model, streaming, host error and replay contracts.
- Tagged MEXC's public/private sockets explicitly, including reconnects on their
  shared URL. Its factory does not install HOW; retained that gate and its
  separate proxy-authorization header. Updated the port brief so future venues
  supply socket roles from their first implementation.
- Re-running workspace, SDK, host and all seven-venue gates on the merged source.

### Final local gates

- Seven public + ten private replay gates: **17/17 PASS** on the merged build.
  Public gates retain strict correctness, CPU and retained-memory comparisons;
  private gates validate account/trading/query/request/subscription correctness.
- Workspace: **486 Rust tests**; bans, formatting, workspace/all-target Clippy and
  workspace wasm32 Clippy pass. SDK: **6,621 tests per suite**, repository
  typecheck **44/44**, lint **28/28**, test tasks **27/27**.
- Routing: **59 tests**, **3,672 golden socket destinations**, and the shared
  **25-case** TS/native fixture (**27 HTTP selections / 375 socket transitions**).
  **46 HOW**, **21 real SDK/wasm**, **21 JS host**, and **40 harness** tests pass.
- Seven-venue wasm: **2,416,972 bytes**, below the existing **2,621,440-byte**
  budget. Native Node hosts inject their actual header-capable socket constructor
  into HOW/relay, avoiding bundled dynamic-require shims; a real ESM loopback
  upgrade verifies the exact path and headers. Browser global clients remain
  compatible; Node/Workers engine adapters own and dispose their tunnel clients.
- Next: push, PR into `feat/rust-rewrite`, post independent adversarial reviews,
  address findings, wait for green CI, merge and cut a follow-up branch.

### Eight-venue integration

- Merged landing through `d9db55921c` as `2b27b5e608`, including BloFin, and
  tagged its public/private sockets. Independent integration review confirmed
  its forced HTTP proxy behavior and socket routing still match the TS factory.
- Re-ran all workspace checks: **559 Rust tests**, workspace/all-target and
  wasm32 Clippy, bans and formatting pass. Repository typecheck **44/44**, lint
  **28/28**, test tasks **27/27** (**6,623 SDK tests per suite**) pass.
- Fresh eight-venue wasm: **2,579,548 bytes**, below the unchanged
  **2,621,440-byte** budget. **22 real SDK/wasm**, **23 host**, and **45 harness**
  tests pass. The 59 routing and 46 HOW regressions remain green.
- Full **eight public / eleven private** replay gates: **19/19 PASS**. Private
  coverage includes all eight venues and four OKX region/demo variants.

Eight-venue public replay measurements (CPU in ms, retained memory in MB):

| Venue | TS CPU | Rust CPU | TS retained | Rust retained | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Binance | 270.6 | 146.4 | 4.2 | 2.2 | PASS |
| BingX | 147.2 | 104.5 | 3.6 | 2.0 | PASS |
| Bybit | 517.5 | 404.0 | 6.3 | 2.4 | PASS |
| OKX | 632.3 | 528.6 | 5.1 | 3.4 | PASS |
| WEEX | 138.8 | 94.1 | 5.0 | 2.1 | PASS |
| MEXC | 501.7 | 248.9 | 10.0 | 3.4 | PASS |
| BloFin | 249.3 | 213.2 | 3.3 | 1.9 | PASS |
| Crypto.com | 142.0 | 103.6 | 2.0 | 1.4 | PASS |

BloFin's first run rejected a negative TS retained-memory baseline; repeated
that gate unchanged and obtained valid passing measurements above. The final
OKX EU demo private run also required a retry after its TS backing-buffer
baseline failed to settle before replay. No gate
thresholds, normalizations or size budgets changed for this transport work.

### PR and adversarial review

- Opened [PR #1313](https://github.com/Tealstreet/tealstreet-sdk/pull/1313) into
  `feat/rust-rewrite` after all 19 eight-venue gates passed.
- Landing then added Backpack. Merged through `b265a0bab0` as `ecc6332396`;
  preserved PATCH, recvWindow, virtual signing time and the host's late-callback
  disposal guards. The Backpack clock override retains the wrapped transport's
  dispose hook; all four new socket connects have explicit public/private roles.
  The aggregate size budget is the landing branch's inherited 3 MiB.
- Posted round-one adversarial findings on the PR. TS/provider/SDK integration
  review is clear. Native review found array response headers rendered as JSON
  instead of JS strings, and malformed binary messages resetting silence before
  decoding. Fixed both, with **16/16 native wire tests** and native all-target
  Clippy passing. A separate reviewer cleared the fixes and regressions; the
  independent JS/runtime review also found no actionable issues.
- CI passed all 21 nine-venue replays. The companion repository job caught a
  wall-clock-sensitive HOW assertion expecting exactly 20,000 ms after setup;
  freeze that routing test's clock while keeping the real deadline regressions
  and production request budget unchanged.

### Bitget integration phase

- Bitget landed as `3a7b46407d` while CI was starting. Merged as `688ef2ac6d`.
  Its two old action patterns need `..` for the optional socket metadata.
- Independent review found three public sockets sharing one real URL, a
  hardcoded Tealstreet fallback inside the venue, and Classic/UTA recovery
  differences. Resolve these at the host boundary before landing this PR.
- Add generic `SocketContext { feed, recover }` to connect effects. Stable feed
  identity isolates simultaneous connections and persists across reconnects;
  recovery is a one-connect protocol-health hint. Bitget emits only real v2/v3
  URLs. Hosts keep the selected fallback sticky when a candidate is declared.
- Extend the existing TS routing descriptor with generic initial silent count,
  exact ignored messages and exact direct-only URLs. Bitget starts at one silent
  attempt, private `pong` is ignored, and UTA stays on its real v3 endpoint.
  Native consumes these resolved fields without a venue table. Independent
  enumeration confirmed counter equivalence with TS across 512 traffic patterns.
- Verify independent public feeds, Classic recovery, private pong/auth behavior,
  UTA direct routing and absent-provider fallback, then re-review the integrated
  policy and run the ten-venue CI gates. Inherit the landing size budget.
- Implemented and independently reviewed: **87 focused TS**, **28 native**,
  **78 Bitget**, **28 real SDK/wasm**, and **34 JS host** tests pass. Root
  typecheck **44/44**, lint **28/28**, workspace/all-target Clippy, bans and
  formatting pass. The shared TS/native fixture now has **31 cases**; URL
  goldens retain their hosts and gain only Bitget's declared policy fields.
- Ten-venue wasm: **3,499,657 bytes**, below the inherited **3,670,016-byte**
  budget. Public/private Bitget and Binance replays and final CI are running.

### Hyperliquid integration phase

- Merged Hyperliquid through `5c47683969` as `ee81ce021a`, preserving its
  address-only accounts, builder/vault/testnet options and perps-only guard.
  Its shared socket carries an explicit public/private role; venue URLs remain
  real endpoints and the host owns routing.
- Independent review found its factory installs HOW whenever the original
  `publicDataOnly` option is false or absent, even with `tealstreetTunnel: false`.
  Preserve that exact factory behavior for perps and spot. Tests invoke the
  production TS factories across all flag permutations and address-only/keyless
  accounts instead of deriving the oracle from the engine gate.
- Verification: **108 focused TS tests**, package typecheck/lint and the native
  **48-case** shared routing matrix pass. Hyperliquid's **57 venue tests** and
  scoped native/wasm Clippy pass. Rebuild the combined engine, run SDK/host tests,
  then replay Bitget, Hyperliquid and Binance on one settled source revision.
- Landing now builds per-venue bundles with an inherited **1,258,291-byte**
  budget. Final CI covers **25 replay comparisons** (11 public, 14 private),
  including all four OKX account variants. No transport gate thresholds changed.

### Final CI test isolation

- All **25/25** eleven-venue replay comparisons pass on `837fc996fa`; fresh
  local Bitget, Hyperliquid and Binance public/private replays also pass.
  Workspace verification passes **762 Rust tests**, all native/wasm lint and
  bans/format checks; repository typecheck **44/44**, lint **28/28**, harness
  **147/147**, SDK/wasm **28/28**, JS host **34/34**.
- The PR repository job passes **6,713 SDK tests**, but the companion push job
  times out one new socket matrix test: it executes all 3,672 destinations in
  one five-second case. Split the matrix into cases per venue, retaining every
  option/channel assertion and the existing timeout. Audit the matching HTTP
  matrix for the same avoidable coupling, run focused verification, request an
  independent review, commit and rerun CI. No production or gate changes.
- Implemented the per-venue matrix split for both HTTP and sockets. **204/204**
  focused tests, package typecheck/lint and diff/format checks pass; all 3,672
  socket destinations and HTTP option permutations remain asserted, with no
  timeout increase. Independent re-review is the final commit gate.
- Independent review is clear: every matrix permutation is retained, per-venue
  mock state is reset, and the socket token/relay absence checks now run for
  every venue. Commit the test-only correction and require the full CI rerun.

### Coinbase native authorization integration

- Coinbase venue code emits standard `Authorization: Bearer` JWTs bound to the
  real venue method/host/path. Ordinary proxied HTTP needs a host-only move to
  `Coinbase-Authorization`, reserving `Authorization` for the proxy context JWT.
  Direct requests and HOW preserve the standard venue header.
- The TS resolver serializes optional per-HTTP-route `headerAliases`; the native
  host applies that generic policy before route authorization, without a venue
  routing table. Missing aliases preserve compatibility with existing plans;
  serialization omits empty maps. Conflicting destination headers fail closed
  without echoing credential values. Signed request methods/URLs/bodies and
  aliased credential values stay intact through the account transport.
- Added both Coinbase aliases to the routing boundary tests and four resolved
  policy golden cases (direct, REST proxy, spot proxy, HOW). Native loopback
  assertions cover actual proxy packets, direct requests, and unchanged HOW
  authorization. Existing routing cases remain part of the same golden matrix.
