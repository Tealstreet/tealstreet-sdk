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

## Proxy & Tunnel Architecture

Exchanges connect to APIs in three modes, controlled by metadata flags in `exchangeMetadata.ts`. The decision logic lives in `createExchangeFromAccount.ts` and request wiring in `safeCexAxiosCreate.ts`.

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
- **Package-boundary discipline**: do not import proxy helpers from `apps/web` into `packages/safe-cex`. Shared routing logic must live in `packages/safe-cex/src/utils/getPrivateProxy.ts`.
- **Multi-tenant token contexts**: `accessToken.ts` supports a per-async-context
  proxy token provider used by exchange-server. Contextual tokens are checked
  before the browser shared-worker or CLI/global refresh path, but invalid,
  expired, empty, or throwing contextual providers must fall back to the normal
  module token path. Do not replace this with module-global `setProxyToken` in
  multi-user hosts.
- **Static validation**: `exchangeMetadata.test.ts` has invariant tests that catch impossible configurations (e.g., `requiresCors + forbidCors`). Run `yarn test-ci` after changing metadata.
- **Tradetunnel access requires an allowlisted egress IP, not a
  User-Agent.** CloudFront's WAF in front of
  `fsjnpy143pbz.{jp,asia}.tealstreet.io` returns 403 to WS upgrade
  requests with no `User-Agent`, and Bun's built-in `WebSocket`
  constructor sends no UA with no public API to add one — `new
WebSocket(url, protocols, { headers })` silently drops the
  options. So the bun exchange-server can only reach tradetunnel
  hostnames from an IP allowlisted at the CDN (production hosted
  server on the DO droplet works; local `bun --hot run` does not).
  Local exchange-server development against
  tradetunnel-gated exchanges has to either deploy to the allowlisted
  droplet (`docker --context tealstreet-exchange-services ...`) or
  swap the WS client for the `ws` npm package, which honours `{
headers }` under Bun.

## Key Files

- `src/exchangeMetadata.ts` — All exchange configuration metadata
- `src/createExchangeFromAccount.ts` — Translates account config → exchange options (proxy/tunnel logic here)
- `src/safeCexAxiosCreate.ts` — Axios instance creation, HOW adapter setup
- `src/utils/getPrivateProxy.ts` — CORS proxy URL resolver used by safe-cex REST clients
- `src/types.ts` — Shared types (`ExchangeOptions`, `ExchangeAccountConfig`, etc.)
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

`src/webworker/commands/router.ts` emits `exec-result` only when the
incoming `exec` payload includes a `reqId`. Shape:
`{ reqId, handled, success?, error?, data? }`, where `data.orderIds`
is used by the web CLI capture syntax. Preserve `success` / `error`:
`cli-core` commands commonly signal failures by returning
`CommandResult.success === false` instead of throwing, and the browser
runner depends on that bit for `retry`, blocking steps, and capture
writeback. For fire-and-forget worker commands with no `reqId`,
handled failures should still emit `exec-log` so the terminal displays
the error.
