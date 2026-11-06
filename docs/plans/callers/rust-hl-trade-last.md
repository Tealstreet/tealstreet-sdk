# Hyperliquid last from trades

Hyperliquid's `ticker.last` was always the mid: `allMids` (every ~5.05 s) and
`fastAssetCtxs` `midPx` (a coin appears every ~1.3-2.1 s at the median, up to
6-10 s), exactly as TS did. A coin with a live order book or candle subscriber
now also streams `trades{coin}`, and its `last` is the newest trade's price.

- One `trades` interest per watched coin, shared by its books and charts and
  dropped with the last of them; perps and spot alike, on the Direct feed only.
- Until a watched coin's first trade, and after a reconnect until the next
  one, `last` stays the mid. Unwatched coins keep `last` as the mid.
- `mid_price` and `mark` are unchanged: mids keep moving `mid_price` and never
  overwrite a trade-sourced `last`.
- Live-balance uPnL (`current_upnl`) values at `mid_price`, falling back to
  `last`, so uPnL and free balance are what they were when `last` was the mid.
- Spot market and stop-market limits (`base_price`, standalone spot and spot
  pairs in the perp account) base at `mid_price`, then `last`, so a print off
  the mid on a thin pair cannot move the ±5% IOC limit. Perps keep mark first.

Callers:

- NX and SDK: `ticker.last` on a watched Hyperliquid coin is now a trade price.
  Within one store, a watched coin's `last` is a trade price while every other
  coin's is the mid. Nothing else changes shape. Read `midPrice` when the mid is
  meant.
- Shared public feed (NX provider, exchange-server in-process hub): the source
  engine is a Direct, public-data-only engine whose books and candles are the
  union its clients asked for, so it streams `trades` and forwards `last`
  itself. A shared account sends no `trades` subscription.

Per-IP cost: +1 interest per watched coin on each Direct engine.

Deletion prerequisites: none added or cleared.
