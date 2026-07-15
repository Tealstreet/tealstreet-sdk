# CLAUDE.md - safe-cex Package

Core exchange integration library. See also exchange-specific `CLAUDE.md` files in `src/exchanges/*/`.

> **CLAUDE.md is a living spec.** When you change non-obvious
> behavior here or in any per-exchange CLAUDE.md, update the relevant
> doc in the same commit as the code change. See the root
> `CLAUDE.md` → "RULE — CLAUDE.md files MUST be kept current" for
> the directive.

For exchange migrations that add first-class opening trigger/conditional orders,
read `CONDITIONAL_ORDERS.md` before changing mapper or order lifecycle code.

## Mirror CI invariants (don't "clean these up")

safe-cex ships in the `tealstreet-sdk` OSS mirror, whose CI does a clean
sliced install with **no dependency hoisting and no leaked DOM lib** (see
`mirrors/README.md`). Two things look removable but are load-bearing:

- **`tsconfig.json` keeps `dom` in `lib`.** The package uses DOM/WebWorker
  globals (`MessagePort`, `Worker`, `PromiseRejectionEvent` in
  `src/webworker/store.webworker.ts`, plus `CompressionFormat` /
  `BufferSource` from imported `utils`/`reconnecting-websocket`). In the
  monorepo these leak in transitively even with `lib: [esnext]`; the mirror
  has no leak, so dropping `dom` breaks mirror typecheck.
- **Every imported package is declared in `package.json`** — including
  `jsonwebtoken` (used by `accessToken.ts` / `proxyTokenClient.ts`). It
  resolves via hoisting in the monorepo but must be explicit for the mirror.

## Commands (run from `packages/safe-cex`)

```bash
yarn lint          # ESLint
yarn typecheck     # TypeScript check
yarn test-ci       # Run all tests (deterministic, no network)
yarn test          # Watch mode
```

## Test placement and trust boundary

`src/exchanges/**` is production runtime code only. The legacy tests, manual
probes, and debug scripts in that tree were removed in July 2026 because they
were untrusted integration-generation scaffolding. Do not restore, copy, or
migrate assertions and fixtures from that deleted corpus.

Fresh exchange audit regressions live in
`src/__tests__/audit-regressions/`. Derive each oracle from current official
upstream specifications, verified audit evidence, or an explicit runtime
contract documented in this CLAUDE.md tree. Tests committed to the package must
be deterministic and must not require network access, API keys, or local secret
files. Live probes belong outside the test runner and outside `src/exchanges/`.

Vitest discovers deterministic `*.test.*` files throughout `src/` and excludes
only explicitly named `*.integration.test.*` and `*.network.test.*` suites.
It does not blanket-exclude `src/exchanges/**`, and coverage includes that tree
so reports expose the real production-code coverage floor. Keep the runtime tree
test-free; the absence of an exchange-tree exclusion is intentional and prevents
future test files from being silently ignored.

## Reconciliation fetch and store safety

`fetchOrders()`, `fetchPositions()`, and `fetchBalance()` return reconciliation
data. Callers must assign, return, or explicitly commit that data; a bare,
awaited-only, voided, or unused `Promise.all` fetch is not a refresh. The local
`safe-cex/no-discarded-fetch-result` ESLint rule enforces this in
`src/exchanges/**`. Keep fetch implementations pure: store mutation belongs in
the caller after disposal and stale-snapshot guards, so the same fetch can be
used safely by startup, tick, and WebSocket healing paths.

Hyperliquid is the documented narrow legacy exception: its DEX-aware order
reconciler performs surgical store mutation inside `fetchOrdersInternal()`.
Fire-and-forget callers use the explicitly named `refreshOrders()` wrapper; do
not generalize that exception or discard `fetchOrders()` directly.

Error paths must preserve last-known-good state. A failed reconciliation should
rethrow, or return the relevant current store value when best-effort behavior is
part of the venue contract. Never turn a transport/auth/rate-limit failure into
`[]` and then commit it as an authoritative empty snapshot. This convention is
documented rather than statically enforced because distinguishing a legitimate
successful empty response from an error fallback requires venue-specific
control-flow and response knowledge.

## Metadata capability non-fallback baseline

`BaseExchange` deliberately supplies inert defaults for `start`, `placeOrder`,
`fetchExecutions`, and `fetchHistoricalTrades`. These functions carry a private
runtime brand exposed through `isBaseCapabilityFallback`; derived class fields
replace the branded function during construction. The deterministic capability
suite in `src/__tests__/audit-regressions/exchange-metadata-capabilities.test.ts`
loads the real registry and enforces the following minimum contracts:

- `supportsTradesHistory` requires a non-fallback executions or historical-
  trades path, while `supportsExecutionsHistory` specifically requires a real
  `fetchExecutions` override;
- trailing-stop, partial/full TP/SL, or trigger-type advertising requires a
  non-fallback `placeOrder` and `features.orders: true`;
- `supportsPublicOnly` requires construction with `publicDataOnly: true` and a
  non-fallback `start` implementation;
- TP/SL trigger defaults must belong to the unique advertised trigger set.

This baseline proves only that metadata does not point at a known inert base
method. It cannot prove that an override maps every advertised order type or
trigger correctly, that a history endpoint returns valid rows, or that public
startup succeeds end-to-end. Those claims need venue-specific deterministic
request/response regressions; live startup and authentication additionally need
network or credentialed probes. Do not replace that evidence with source-text
grep, and do not add a second capability manifest that duplicates metadata.

## Proxy & Tunnel Architecture

Exchanges connect to APIs in three modes, controlled by metadata flags in `exchangeMetadata.ts`. The decision logic lives in `createExchangeFromAccount.ts` and request wiring in `safeCexAxiosCreate.ts`.

**Routing source of truth (`src/routing/`).** Which actual _host_ a request/socket uses — the REST CORS pool (jp/asia/tiny), the HOW egress region, and each exchange's public/private/business WS host — is declared per-exchange in `routing/routingTable.ts` and resolved by `resolveExchangeRouting(exchangeName, options)`. REST (`getProxyForAxios`) and every per-exchange `ws-*.ts` class read their host from the resolver instead of hardcoding `wss://fsjnpy143pbz.*` literals. To change where an exchange's traffic egresses, edit the table — not the ws class. Behavior is locked by `src/routing/__tests__/routing.equivalence.test.ts` + `src/transport-resolution.characterization.test.ts` (the snapshots must not move except for intentional changes). Variant exchanges that reuse a parent's ws class (okxeu→okx, bitgetinverse/usdc→bitget, coinbasespot→coinbase, bybit family) get their parent's routing because the table declares them with the same shared `WsGroup`.

**`howEgress` is the one derived field — it is NOT a literal in the table.** The HOW (REST-over-WS) tunnel egress region (jp/sg) is owned by the `@tealstreet/http-over-websocket` package, which is what actually picks the egress at request time. The routing table computes `howEgress` via `isTokyoEgress(name)` imported from `@tealstreet/http-over-websocket/src/egressRegion` (the single Tokyo-egress list), so the table never duplicates that list. The exception is the region-pinned families (bitmex always, bybit when tunnelling) whose api factories force the HOW adapter region from `proxyRegion`; those are declared `regionConditional`. The single-source lock — table's name-based egress must equal the adapter's runtime baseURL-based egress for every exchange — is enforced in `routing.equivalence.test.ts`. To move an exchange's HOW egress, edit the list in `egressRegion.ts`, not here.

Canonical saved account shaping lives in
`src/accounts/normalizeExchangeAccountConfig.ts`. Web, CLI, key-audit, and
other "add API key" flows should produce an `ExchangeAccountConfig`, normalize
that account, then persist/test/pass it to `createExchangeFromAccount()`.
Do not reimplement `tealstreetProxy` / `proxyRegion` account defaults in app
code.

Exchange API factories must install HOW and other transport adapters through
`setInnerAdapterForRemoteSigning(instance, adapter)`, never by assigning
`instance.defaults.adapter` directly. Hosted exchange-server instances keep the
remote-signing adapter as the outer layer and execute the exchange-selected
transport only after the browser returns a signed request. A trusted audit
regression scans every `*.api.ts` factory for direct replacement.

Private-proxy REST header injection is Node/Bun-only. Browser workers also lack
`window`, so use `isNodeOrBunRuntime()` rather than `typeof window` when deciding
whether code may set browser-forbidden `Origin`, `Referer`, `User-Agent`, or
`sec-fetch-*` headers.

Before changing proxy/tunnel routing, the typed source of truth is the
`routing/` table + resolver, locked by `routing/__tests__/routing.equivalence.test.ts`
and its golden-master snapshot (the always-current, machine-checked per-exchange
routing map). The repo-root `docs/proxy-routing-flow.html` is a manually rendered human-facing
**mirror** of the same web → safe-cex → HOW flow (it re-implements resolution in
vanilla JS for an interactive table) — useful for browsing, but trust the snapshot
over it. If routing behavior changes, update or delete that page in the same PR.

**Extension direct-connect (web-side override).** The web app can force an
account fully-direct (`tealstreetProxy=false`, `tealstreetTunnel=false`) when
the user opts in (`ExchangeAccountConfig.useExtensionDirect`) and the Tealstreet
browser extension is present (`window.__tealstreetExtension`). The decision is
web-only (`apps/web/src/utils/exchange/extensionTransport.ts`), passed as a
last-wins `ExchangeOptions` override — safe-cex stays app-agnostic and does not
read `window`. The extension's declarativeNetRequest rules supply CORS/headers
for the direct calls. Bybit only for now; no-op (proxy stays the fallback) when
the extension is absent.

**Secondary (read-only) key — split read/write transport.** When an account has
a `secondary_apiKey`, `createApiWithSecondaryKey.ts` routes all REST **GET (read)**
requests to a second axios client built from the `secondary_*` credentials, while
writes stay on the primary. That secondary client's transport is set by
`ExchangeOptions.secondaryProxyUrl` (resolved by `src/utils/getReadFleetProxy.ts`
from exchange/region), so reads can later egress through a separate cheap proxy
fleet while writes keep the primary/whitelisted transport. The feature is
**default-off**: with **no** `secondary_apiKey`, `createApiWithSecondaryKey` returns
three references to the _same_ primary client (the read Proxy is never built), so
routing is **byte-identical** to before. Precedence for the secondary read transport:
`options.secondaryProxyUrl || otherOptions.proxyUrl || <jp default>`, applied as the
secondary client's `overrideProxyUrl`. `getReadFleetProxyUrl` returns the live
**read fleet** endpoint, region-matched to the **same region the primary REST pool uses
for that exchange** via `restEgressRegion` (`routing/resolveExchangeRouting.ts`) — the
per-exchange declared pool + the `JA` override: the jp/okx/bitget pool **and** grvt's
`tiny` pool → `read-fleet-jp.tealstreet.io`, every `asia`-pool exchange →
`read-fleet-sg.tealstreet.io`, both `/api/exchange_proxy`. This is **true parity** with
`getPrivateProxy`/`resolveExchangeRouting`, not a coarse `SG`-or-Tokyo split — e.g.
`binance`+`SG` stays Tokyo (its declared jp pool; only `JA` overrides), and `bybit` with
no region goes Singapore (its `asia` pool). These are cheap commodity
Vultr VPS running the same HAProxy CORS forwarding proxy as the AWS exchange-proxy
boxes, fronted by Cloudflare (orange-cloud, zone SSL "Full" so CF accepts the boxes'
self-signed certs) — so an **opted-in** account's reads egress from the fleet's many
IPs (rate-limit spreading) while writes stay on the primary/whitelisted transport.
The fleet is public/tokenless, so no proxy Bearer token is required. The shared
public CORS proxy for unwhitelisted/public requests (`PUBLIC_CORS_PROXY` in
`utils/transportResolution.ts`) was migrated off AWS `exchange-proxy.jp` to the same
`read-fleet-jp` endpoint, so that hostname is no longer used in code. Terraform for the
fleet lives in `terraform/vultr-read-fleet/` (isolated local state, not the prod AWS
workspace). Still **default-off**: with no `secondary_apiKey`, none of this is reached.

**Wiring is per-exchange, gated by `exchangeSupportsSecondaryKey()`.** Every HMAC
exchange builds its authed `this.xhr` via the base helper
`BaseExchange.createXhrWithSecondaryKey(opts, createApi, otherOptions)`
(`src/exchanges/base.ts`), which calls `createApiWithSecondaryKey` and stashes the
raw primary/secondary clients on `this._primaryXhr` / `this._secondaryXhr`. The set
that wires it MUST equal the set the UI offers it for — i.e. everything **not** in
`exchangesWithoutSecondaryKey` (`src/utils/exchangeSupportsSecondaryKey.ts`). That
exclusion list is the wallet / single-keypair venues (hyperliquid, dydx, paradex,
grvt, apexomni, lighter, polymarket, and the orderly ed25519 family
incl. `tealstreet-dex`) **plus** the session-login venues `breakoutprop` (DXtrade
session token) and `btcc` (MD5 login) — there a shared per-session token is injected
on every request so a second client can't authenticate as a distinct key (and btcc
issues REST via `this.xhr.request()`, which the `.get`-only read Proxy doesn't even
intercept). A read-only "second API key" is not a coherent concept for any of these.
When adding an exchange, wire the helper **and** reconcile that list.
`binancepm` is the one bespoke case: it has no single `this.xhr` (uses
`papiXhr`/`fapiXhr`), so it calls `createApiWithSecondaryKey` directly for each and
keeps papi's primary/secondary for validation.

**Only the helper-wired client splits reads.** A few exchanges also build an
`unlimitedXHR` (rate-limiter-bypass client for urgent order placement) that is **not**
routed through the helper, so any GET issued on it stays on the primary transport.
Today the only authed read on that path is bitrue's `fetchOrdersForSymbol`
(`unlimitedXHR.get`); everywhere else `unlimitedXHR` is writes-only and the bulk of
read volume (tick-loop `fetchPositions`/`fetchBalance`/`fetchOrders`) flows through
`this.xhr`, which does split. Wiring `unlimitedXHR` for 100% read coverage is a
deliberate follow-up, not in this pass.

**`validateAccount` caveat (opt-in path only).** Because `this.xhr.get()` routes to
the secondary key once one is set, an exchange's `validateAccount` (which is a GET)
validates the **read** key, not the primary write key, when a secondary key is
present. binance and binancepm validate **both** explicitly via
`_primaryXhr`/`_secondaryXhr`; the other HMAC exchanges validate only the read key
on the opt-in path (write key is exercised on first order). The default-off path
(no secondary key) is unchanged. Validating both keys everywhere is a deliberate
follow-up, not an oversight.

### Metadata Flags

| Flag              | Default | Purpose                                             |
| ----------------- | ------- | --------------------------------------------------- |
| `requiresCors`    | `false` | REST calls go through Tealstreet IP proxy servers   |
| `supportsTunnel`  | `false` | Exchange supports HTTP-over-WebSocket (HOW) adapter |
| `enforceWsTunnel` | `false` | Force HOW adapter on (overrides `supportsTunnel`)   |

### Decision Flow

```
Is requiresCors true?
├── NO → Direct API calls (browser → exchange)
│   │
│   └── Is enforceWsTunnel true?
│       ├── YES → WS tunnel only, no CORS proxy
│       │   Examples: Hyperliquid, Paradex, Arkham, KuCoin, Kraken,
│       │            AsterDEX, BreakoutProp, BitUnix, BybitDemo
│       └── NO → Fully direct, no proxy or tunnel
│           Examples: OKX, dYdX, Binance, BinancePM, BinanceSpot,
│                     Bybit, BybitSpot, BybitInverse, BTCC, Woo, WEEX
│
└── YES → tealstreetProxy = true (REST routed through IP proxy)
    │
    └── Is enforceWsTunnel true OR supportsTunnel not false?
        ├── YES → Proxy + Tunnel (REST via HOW adapter through proxy)
        │   Examples: BingX, BitMEX, Bitrue, Bitget, GRVT,
        │             ApexOmni, Phemex, Coinbase
        └── NO → Proxy only (REST via CORS proxy, no WS adapter)
            Examples: Crypto.com, Backpack, BloFin, Polymarket
```

### How It Works

1. **Direct** (`requiresCors: false`, no `enforceWsTunnel`): Browser makes REST calls directly to the exchange API. Works when the exchange sends CORS headers or the app runs in a true server context (Node/CLI).

2. **CORS Proxy only** (`requiresCors: true`, `supportsTunnel: false`): REST calls are routed through Tealstreet proxy servers that add CORS headers. The proxy IPs must be whitelisted on the exchange. No WebSocket adapter involved.

3. **WS Tunnel** (`enforceWsTunnel: true` or `supportsTunnel: true`): REST calls are wrapped into WebSocket frames using the HTTP-over-WebSocket (HOW) adapter (`axiosHttpOverWebsocketAdapter`). This routes REST through an existing WebSocket connection. Can be combined with CORS proxy or used standalone.

### Common Pitfalls

- **Do NOT set `enforceWsTunnel: true` without ensuring the exchange's tunnel infrastructure is deployed.** The WS tunnel requires server-side support (tradetunnel).
- **`supportsTunnel: false` in defaults**: Many exchanges inherit this. If adding `enforceWsTunnel: true` to a new exchange, you don't need to also set `supportsTunnel: true` — `enforceWsTunnel` takes precedence in `createExchangeFromAccount.ts`.
- **`requiresCors: true` needs `privateProxyIps`**: The proxy IPs are shown to users for IP whitelisting. Without them, users can't configure their API keys.
- **Runtime checks for proxy resolution**: treat browser main thread and WebWorker as browser runtimes. Do not use `typeof window === 'undefined'` alone to infer "server"; that breaks worker-local proxy routing.
- **Package-boundary discipline**: do not import proxy helpers from `apps/web` into `packages/safe-cex`. Shared routing logic lives in `packages/safe-cex/src/routing/resolveExchangeRouting.ts` and `packages/safe-cex/src/utils/transportResolution.ts`.
- **Multi-tenant token contexts**: `accessToken.ts` supports a per-async-context
  proxy token provider used by exchange-server. Contextual tokens are checked
  before the browser shared-worker or CLI/global refresh path, but invalid,
  expired, empty, or throwing contextual providers must fall back to the normal
  module token path. Do not replace this with module-global `setProxyToken` in
  multi-user hosts.
- **Static validation**: `exchangeMetadata.test.ts` has invariant tests that catch impossible configurations (e.g., `requiresCors + forbidCors`). Run `yarn test-ci` after changing metadata.
- **Unknown metadata fails closed**: `getExchangeMetadata` throws for runtime
  names outside the exhaustive registry. Never restore a venue-shaped fallback;
  it can apply the wrong transport and capability flags to corrupt saved data.
- **Non-browser tradetunnel callers must use the shared header-capable
  transport with a browser-like `User-Agent`.** CloudFront/WAF in front of
  `fsjnpy143pbz.{jp,asia}.tealstreet.io` can reject WebSocket upgrades before
  the Rust tradetunnel sees them. Browser clients send their own UA. Node/Bun
  clients (CLI, `yarn keys`, exchange-server) must use either
  `@tealstreet/http-over-websocket` for HoW REST or `base.ws.ts`'s Tealstreet
  URL branch for exchange WebSockets. Both attach `Origin:
https://preview.tealstreet.io` plus a browser-like `User-Agent`, and rely on
  vendored `ReconnectingWebSocket` to force HTTP/1.1 via `ALPNProtocols`. Do not
  use Bun native `WebSocket`, undici/tcp-websocket, or a hand-rolled socket for
  tradetunnel/HOW.
- **Only required WebSocket upgrade headers fail closed without `ws`.**
  Set `options.headersRequired: true` for header-authenticated sockets; then
  `BaseWebSocket` rejects a native/browser constructor that would silently drop
  auth. Static browser-identification headers and implicit Tealstreet routing
  headers are best effort, so browser/native fallback remains supported.
  Header-authenticated exchanges must handle the typed unavailable-transport
  error and retain their independent REST safety path where present.
- **The reconnecting-WebSocket path alias targets `thirdparty/`.** Keep
  `tsconfig.json` pointed at the vendored source so safe-cex tests and
  typechecks do not resolve a hoisted/stale workspace copy instead.

## Key Files

- `src/exchangeMetadata.ts` — All exchange configuration metadata
- `src/accounts/normalizeExchangeAccountConfig.ts` — Canonical saved-account proxy/region shaping
- `src/createExchangeFromAccount.ts` — Translates account config → exchange options (proxy/tunnel logic here)
- `src/index.ts` — `buildPublicExchangeOptions()` is the shared public-only
  routing helper used by `createPublicExchange()` and exchange-server shared
  public exchanges. Keep those paths aligned through this helper.
- `src/safeCexAxiosCreate.ts` — Axios instance creation, HOW adapter setup
- `src/httpOverWebsocket/` — Thin safe-cex compatibility re-exports and
  `configureHOW` wiring; the adapter implementation lives in
  `@tealstreet/http-over-websocket`
- `src/routing/resolveExchangeRouting.ts` + `src/utils/transportResolution.ts` — CORS proxy URL / transport resolver used by safe-cex REST clients (the old `src/utils/getPrivateProxy.ts` no longer exists)
- `src/types.ts` — Shared types (`ExchangeOptions`, `ExchangeAccountConfig`, etc.)
- `src/health/errorPatterns.ts` — Fuzzy health classification. Critical auth
  expiry requires key/token/session/signature context; never add bare lifecycle
  words such as `expired`, `timestamp`, or `clock` as global patterns.
- `src/exchanges/*/` — Per-exchange implementations

## Debugging exchange REST traffic

REST routed through the HOW adapter appears as WS frames, not XHRs.
The exchange emits `grafana-http-request` / `grafana-http-response` /
`error` / `warning` events on `BaseExchange.emitter` — check your
consuming app's CLAUDE.md for the capture playbook.

## Worker Position Refresh Bridge

`BaseExchange.updatePositionsInWebworker()` emits the `updatePositions`
worker op handled by `src/webworker/handlers/UpdatePositionsHandler.ts`.
Account-wide refreshes (`symbols` omitted) must use
`store.addOrUpdatePositionsFull(positions)`, including empty arrays, so
positions omitted by the exchange are zeroed/cleared. Symbol-scoped refreshes
remain partial and use `addOrUpdatePositions(positions)` when rows are
returned. Do not replace this with a main-thread `fetchPositions()` + direct
store write from the web app; the worker owns live exchange state and its
`updateIds.positions` counter.

## Worker CLI Exec Result Bridge

`src/webworker/handlers/CommandHandler.ts` emits `exec-result` only when the
incoming `exec` payload includes a `reqId`. Shape:
`{ reqId, handled, success?, error?, data? }`, where `data.orderIds`
is used by the web CLI capture syntax. Preserve `success` / `error`:
`cli-core` commands commonly signal failures by returning
`CommandResult.success === false` instead of throwing, and the browser
runner depends on that bit for `retry`, blocking steps, and capture
writeback. For fire-and-forget worker commands with no `reqId`,
handled failures should still emit `exec-log` so the terminal displays
the error.
