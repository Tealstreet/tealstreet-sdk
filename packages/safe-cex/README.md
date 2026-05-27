# @tealstreet/safe-cex

Unified exchange integration SDK for 28 crypto exchanges (CEX, DEX, and
prediction/prop markets) — one API for market data, orders, positions,
balances, and real-time WebSocket streams.

## Consuming it

`@tealstreet/safe-cex` is distributed as **source** (`private` — not published
to npm). Consume it like the rest of the SDK: vendor this package — via the
`tealstreet-sdk` mirror as a git submodule, or as a yarn workspace — and let
your bundler transpile the TypeScript. There is no build step to run;
resolution is source-based.

## Entry points

Import from the package root (`@tealstreet/safe-cex`):

```typescript
import { createExchange, createPublicExchange, getAvailableExchanges } from '@tealstreet/safe-cex';

// Public market data only (no API keys):
const md = await createPublicExchange('binance');
await md.start();

// Authenticated (orders, positions, balances):
const account = await createExchange('bybit', {
  key: process.env.BYBIT_KEY!,
  secret: process.env.BYBIT_SECRET!,
});
await account.start();

// Discover what's available:
getAvailableExchanges(); // => ['binance', 'bybit', 'hyperliquid', ...]
```

`createExchangeFromAccount` (in `src/createExchangeFromAccount.ts`) translates a
stored account config — keys plus the per-exchange proxy/tunnel metadata — into
a ready-to-`start()` exchange. Use it when you already have an account record;
use `createExchange` for the low-level path.

## Runtime model — read this

- **In a browser, run the exchange inside a Web Worker — never call
  `exchange.start()` on the main thread.** It opens long-lived WebSockets and
  does heavy normalization; the main thread is for UI only.
- In **Node / a server / the CLI**, `start()` runs directly.
- Some exchanges are IP-whitelisted and route REST through a CORS proxy or an
  HTTP-over-WebSocket tunnel. The decision flow and the metadata flags that
  drive it live in [`CLAUDE.md`](./CLAUDE.md) → "Proxy & Tunnel Architecture"
  and `src/exchangeMetadata.ts`.

## Layout

- `src/index.ts` — public factory surface (`createExchange`, `createPublicExchange`, `getAvailableExchanges`, `preloadExchange`)
- `src/exchanges/<name>/` — per-exchange implementation (each has its own `CLAUDE.md` with API links + quirks)
- `src/exchangeMetadata.ts` — exchange capability + proxy/tunnel metadata
- `src/types.ts` — shared types (`ExchangeOptions`, `ExchangeAccountConfig`, …)

See [`CLAUDE.md`](./CLAUDE.md) for the full architecture, the proxy/tunnel
decision flow, and per-exchange notes.
