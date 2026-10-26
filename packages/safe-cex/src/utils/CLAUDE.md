# CLAUDE.md - safe-cex/src/utils

Utility functions for exchange operations.

## Symbol Format Reference

Each exchange normalizes market symbols differently. The `symbol` field on market objects is what gets used throughout the app. Real samples are in `__testdata__/market-samples.json`.

### Symbol Formats by Exchange

| Exchange                              | Format             | Example          | Default Quote | Notes                                                    |
| ------------------------------------- | ------------------ | ---------------- | ------------- | -------------------------------------------------------- |
| binance, binancepm, binancespot       | `{B}{Q}`           | `BTCUSDT`        | USDT          | Concatenated, from API                                   |
| bybit, bybitdemo, bybitspot           | `{B}{Q}`           | `BTCUSDT`        | USDT          | USDC perps: `BCHPERP`                                    |
| bitget                                | `{B}{Q}`           | `BTCUSDT`        | USDT          | From API `m.symbol`                                      |
| bitgetusdc                            | `{B}PERP`          | `BTCPERP`        | USDC          | NOT `{B}{Q}` — uses PERP suffix                          |
| bitgetinverse                         | `{B}USD`           | `BTCUSD`         | USD           | Filters quoteCoin==='USD'                                |
| asterdex, bitunix, phemex, weex       | `{B}{Q}`           | `BTCUSDT`        | USDT          | Concatenated, WEEX Contract V3 `symbol`                  |
| bingx                                 | `{B}{Q}`           | `BTCUSDT`        | USDT          | API id is `BTC-USDT` but symbol is concat                |
| apexomni                              | `{B}{Q}`           | `BTCUSDT`        | USDT          | API id is `BTC-USDT`                                     |
| breakoutprop                          | `{B}USD`           | `BTCUSD`         | USD           | Quote hardcoded to 'USD'                                 |
| blofin                                | `{B}{Q}`           | `BTCUSDC`        | USDC          | API id is `BTC-USDC`                                     |
| okx                                   | `{B}{Q}`           | `ACTUSDT`        | USDT          | API id `ACT-USDT-SWAP`, normalized                       |
| woo                                   | `{B}{Q}`           | `BCHUSDT`        | USDT          |                                                          |
| bitrue                                | `E-{B}-{Q}`        | `E-BTC-USDT`     | USDT          | E- prefix, dash-separated                                |
| coinbase                              | `{B}-PERP-INTX`    | `ETH-PERP-INTX`  | USDC          | Perp suffix `-PERP-INTX`                                 |
| coinbasespot                          | `{B}-{Q}`          | `BTC-USD`        | USD           | Dash-separated, from product_id                          |
| paradex                               | `{B}-{Q}-PERP`     | `AAVE-USD-PERP`  | USD           | Dash-separated with PERP suffix                          |
| hyperliquid                           | `{B}-PERP`         | `BTC-PERP`       | USDC          | Quote omitted, also `PREFIX\|{B}-PERP`                   |
| hyperliquidspot                       | `{B}/USDC`         | `PURR/USDC`      | USDC          | Slash-separated                                          |
| kraken                                | `PF_{B}{Q}`        | `PF_XBTUSD`      | USD           | PF\_ prefix, XBT alias for BTC                           |
| krakenspot                            | `{B}/{Q}`          | `BTC/USD`        | USD           | Slash-separated wsname format                            |
| kucoin                                | `{B}{Q}M`          | `XBTUSDTM`       | USDT          | M suffix, XBT alias for BTC                              |
| grvt                                  | `{B}_{Q}_PERP`     | `AAVE_USDT_PERP` | USDT          | Underscore-sep with PERP                                 |
| backpack                              | `{B}_{Q}_PERP`     | `BTC_USDC_PERP`  | USDC          | Underscore-sep, spot: `{B}_{Q}`                          |
| cryptocom                             | `{B}{Q}-PERP`      | `BTCUSD-PERP`    | USD           | Concat base+quote then `-PERP`                           |
| lighter                               | `{B}`              | `APT`            | USDT          | Just base, some have `{B}/USDC`                          |
| orderly variants                      | `{B}{Q}`           | `BTCUSDC`        | USDC          | woofi, kodiakfi, mode, salsadex, what, tealstreet-dex    |
| polymarket                            | `{slug}-{outcome}` | `slug-Yes`       | USDC          | Non-standard, not convertible                            |

### Key Gotchas

- **bitgetusdc** uses `BTCPERP` format (NOT `BTCUSDC`) — confirmed via API
- **KuCoin** symbols have `M` suffix (`XBTUSDTM`) and use XBT for BTC — code does `split('_')[0]` which is a noop since no underscore exists
- **KuCoin** prefers `m.baseCurrency` for the base asset and only falls back to
  `m.rootSymbol`; do not reverse that order because `rootSymbol` can be the
  margin coin (`USDT`).
- **Coinbase** perps use `-PERP-INTX` suffix, NOT `-USD` — very different from coinbasespot
- **Lighter** has no separator or quote in many symbols — just the base asset
- **Hyperliquid** has vendor-prefixed symbols: `XYZ|NFLX-PERP`, `FLX|USA500-PERP`

### Base Asset Aliases

| Standard | Alias | Used by                             |
| -------- | ----- | ----------------------------------- |
| BTC      | XBT   | kraken, kucoin |

## `v()` / `vn()` helpers (`get-key.ts`)

These are the field lookup helpers used by every exchange adapter to
read values off raw API/WS payloads. They have a subtle failure mode
that has silently corrupted data in more than one adapter.

```ts
export const v = (obj, key) => {
  if (key in obj) return obj[key];
  if (snakeCase(key) in obj) return obj[snakeCase(key)];
  if (camelCase(key) in obj) return obj[camelCase(key)];
  return undefined;
};
```

**What it does:** tries literal / snake_case / camelCase forms of the
**key** against the object.

**What it does NOT do:** normalize the **haystack**. The helper can
only match field names that are exact case conversions of the key you
pass in.

### The trap

Calling `v(t, 'mark')` does **not** match a field named `markPrice`,
because `snakeCase('mark')` is `mark` and `camelCase('mark')` is
`mark` — neither is `markPrice`. The lookup silently returns
`undefined`, which turns into `parseFloat(undefined) || 0 = 0`, and
the bug is invisible at runtime.

**Real-world incident:** Kraken Futures REST (`/v3/tickers`)
publishes `markPrice`, `indexPrice`, `vol24h`, `volumeQuote`,
`change24h`, `openInterest`. The Kraken adapter was reading them via
`v(t, 'mark')`, `v(t, 'volume')`, etc., which **all returned 0** on
every REST fetch. Mark/index/volume/quoteVolume were only ever
populated after the first WS tick overwrote them. This had been live
in production since the adapter was written.

### When to use which

- **`v(obj, key)`** — use when the exchange publishes the field under
  a name that is a simple literal / snake / camel conversion of `key`
  (e.g. `last_price` ↔ `lastPrice` ↔ `last_price`).
- **`vn(obj, key1, key2, ...)`** — use when the exchange publishes
  the same field under multiple names across endpoints/feeds and you
  need to try them in order.
- **Read directly** (`parseFloat(t.markPrice)`) — use when the field
  name is a compound word that `camelCase()` of some shorter key
  can't produce. There is no magic here; the adapter author has to
  know.

When adding a new exchange, **spot-check every `v()` call against a
real payload** before trusting it. The fastest way is to log the raw
object and compare key-by-key.

## Orderbook utilities — two API shapes

`updateOrderBook.ts` exports two functions with the same semantics
but different argument shapes:

- **`updateOrderBook(orderBook, side, { price, amount, total: 0 })`** —
  the original object-arg form. Most exchanges still call this.
- **`updateOrderBookByPrice(orderBook, side, price, amount)`** — the
  primitive-arg form added for hot paths (Bybit V5 1000-level
  deltas at 30–60 Hz/symbol).

Both use the same internal `binarySearchPrice` and produce the same
mutations. The difference is **where the per-level wrapper object
is allocated**:

- **Object form:** caller allocates `{ price, amount, total: 0 }`
  per delta level, then the function uses `.price` for binary search
  and `.amount` for dispatch. On update / delete paths (~95% of
  delta levels) the wrapper is discarded immediately.
- **Primitive form:** caller passes raw numbers; the wrapper is
  allocated **inside** the function only on the insertion path,
  where it actually persists in the book.

Use the primitive form on per-level hot loops where the discard
ratio is high (orderbook deltas). Either form is fine for one-shot
updates (snapshot rebuilds typically use `levels.push(...)`
directly anyway).

`orderbook.ts` exports `calcOrderBookTotal` — runs once per
orderbook frame and walks both sides via indexed for-loops. It
accumulates with plain `+`, **not** `safe-math.add`. The cumulative
`total` field is consumed only by `Orderbook.tsx` /
`useSmoothOrderbook.ts` / depth chart for relative bar-fill
rendering, where IEEE-754 drift is invisible. See `../exchanges/CLAUDE.md`
→ "Public WebSocket Hot-Path Patterns" for the broader rule on
when `safe-math` is and isn't load-bearing.

## Sequenced delta orderbook foundation

`sequencedDeltaOrderBook.ts` is the transport-agnostic state machine for a
venue whose official book contract is an authoritative snapshot followed by
deltas carrying both `sequence` and `previousSequence`.

- A snapshot is the only operation that initializes or recovers the book.
- Sequences are compared as arbitrary-precision nonnegative integers. A
  JavaScript `number` is accepted only when it is a safe integer; prefer the
  upstream string when the wire uses 64-bit IDs.
- A delta is accepted only when it moves forward and its predecessor equals
  the last accepted sequence. Duplicate and stale frames are ignored without
  mutating the book.
- A missing snapshot, malformed frame, or gap clears the book and returns one
  `{ status: 'recover' }` signal. The caller owns the venue-specific
  unsubscribe/resubscribe operation; further deltas are suppressed until a
  valid snapshot arrives.
- Snapshot and delta levels are validated before mutation. Sorted upsert and
  zero-amount deletion delegate to `updateOrderBookByPrice`.
- The helper deliberately does not calculate cumulative totals or invoke UI
  callbacks. Do post-processing only after an `applied` result.

Do not migrate an exchange merely because its payload has fields named
`seqId`/`prevSeqId`. First verify the current official contract for snapshot
action, predecessor semantics, duplicate/replay behavior, reset behavior, and
numeric width. Also prove that a refcount-safe resubscribe produces a new
snapshot, reset the helper on close/final unsubscribe, and lock those behaviors
in a fresh audit regression. Existing bespoke OKX and BloFin recovery stays in
place until those adoption gates are reviewed venue by venue. OKX is explicitly
blocked today: its official maintenance-reset frame is an incremental update
with a predecessor equal to the last accepted sequence and a new sequence lower
than that predecessor, while this helper deliberately classifies lower
sequences as stale. A future OKX adapter must opt into reset semantics without
weakening the default monotonic contract.

## `virtualClock` time-source order — don't lead with direct `api.bybit.com`

`virtualClock.ts` syncs a client↔server clock offset (applied only when
drift > 1s). The fetch fallback chains **must not lead with a direct
`https://api.bybit.com/v5/market/time` call**: that endpoint sits behind
CloudFront and returns **403 "blocked access from your country"** from any
geo-blocked region (e.g. CI runners, SG/US IPs). The error is caught and
the chain falls through, so sync still works — but the browser logs a
**failed request + console error on every page load** for those users, and
it inflates the e2e load-gate's diagnostic counters.

**The Tealstreet endpoints are not in this file.** `virtualClock` takes a host
clock through `setServerTimeSource`; `transport/registerTealstreetTransport`
registers `transport/tealstreet/tealstreetTimeSource` (same-origin `/api/time`,
or `NEXT_PUBLIC_TIME_API_ORIGIN`, with half-RTT correction). Without a
registered source — a plain OSS consumer — generic sync uses venue time (Bybit
through the transport's public CORS proxy, which is `api.bybit.com` directly
under the direct transport and can 403 in geo-blocked regions or fail CORS in a
browser), then the local clock.

- `fetchServerTime` (generic): registered source, then venue time, then the
  local UTC clock. Do not add an unmaintained third-party time dependency to
  this signing path.
- `fetchBybitTime` (bybit-specific, wants Bybit's real clock) leads with venue
  time through the **public CORS proxy** (`exchange-proxy.jp.tealstreet.io`
  under Tealstreet — Bybit's real time via non-blocked egress), then the
  registered source.

If you "simplify" either chain back to a direct Bybit lead, you reintroduce
the 403 noise for every geo-blocked user. The offset only needs ~1s
accuracy, so the edge `/api/time` clock is plenty. Generic clock sync starts
immediately, then runs every 300 seconds; do not shorten it without a new
drift measurement.

## Files

- `httpFailure.ts` — `getHttpFailure` / `appendHttpFailure` / `describeHttpFailure`.
  The HOW adapter resolves non-2xx tunnel frames without axios' `settle()`, so a
  venue interceptor's SUCCESS handler receives the failure with its status
  intact but never inspected, and every venue then builds its message out of a
  body it cannot parse. These helpers only READ — nothing here throws. Making
  the adapter settle instead would change control flow at 24 call sites at once,
  and doing it on one venue alone cost four regressions. Note the tunnel spells
  the reason `status_text` (snake_case, no serde `rename_all`) and fills it from
  `canonical_reason().unwrap_or("")`, so `statusText` is `undefined` on every
  tunnelled response and `status_text` may be present-but-empty.
  `appendHttpFailure` is wrapped around venue messages in the five `.api.ts`
  response interceptors that have both the response and the message in scope
  (backpack, apexomni, asterdex, bitget, bitunix); it returns the message
  untouched on a 2xx, so wrapping is unconditional and control flow is
  unchanged. Both it and `describeHttpFailure` emit the status NUMBER only and
  never the canonical reason — see the health-classification note in
  `packages/safe-cex/CLAUDE.md` for the three separate collisions that rule
  replaced. `describeHttpFailure` is the standalone form, used where there is no
  venue error text to attach to; hyperliquid consumes it directly.
- `httpFailureContext.ts` — correlates a swallowed non-2xx to an error emitted
  just after it, for Sentry TAGS only. Needed because `axiosErrorEnhancer`'s
  causal `statusCode` only exists on the REJECTED path, and five venues
  (phemex, woo, coinbase, kucoin, bingx) have no response interceptor
  at all — so there is no point where the failing response and the eventual
  message are both in scope. Three rules are load-bearing: the tags are named
  `recentHttp*` and carry their own age because they are circumstantial, not
  causal; they go in `tags` and never `extra`, since `sanitizeWorkerSentryData`
  caps worker `extra` at 12 keys in insertion order; and the URL has its query
  stripped before being indexed, because `kraken.api.ts` rewrites `config.url`
  to `path?postData`. Expect a low hit rate — the tick loop runs parallel
  requests, so a concurrent success can clear the slot first.
- `venueRestDiagnostics.ts` — per-venue REST aggregates for the hosted server:
  request rate, the total-vs-signing latency split, the venue's OWN remaining
  quota read off response headers, and a drained buffer of failure samples.
  Three things are load-bearing. It reads **response** headers only — request
  headers carry signatures, so they are never touched. The URL is reduced to a
  path before storage, because several venues bake signed params into the query
  string. And like `socketHealth`, it imports no telemetry client: the OSS
  mirror forbids that, so the host process reads the snapshot and reports it.
  Built because a 403 from something in front of a venue, a reset mid-response,
  and a client timeout are indistinguishable from outside the container, and
  everything that separates them was being discarded at request time. Note the
  quota headers are observable on SUCCESS, so how close a poll runs to a venue's
  limit never needs a failure to sample. **`errorCounts` is the field that earns
  its keep**: the dominant failure class on the hosted path is not an axios
  error at all but a bare `throw` from inside the adapter chain, which arrives
  with no response, no code, no config and no url — every other field empty. The
  key is digit-masked so one cause is one key, scrubbed, capped at 60 chars, and
  the map is bounded; errors that DID get a response are excluded, since those
  already carry a status and a body. **The scrub requires a letter/digit MIX**
  rather than a bare length: at a flat 12 characters it redacted `disconnected`,
  `wallet-balance` and `Content-Type`, and `disconnected` is precisely the word
  that tells two signer failures apart. Keys are capped at 120, not 60 — the
  real messages carry an exchange prefix, so the discriminating clause starts
  past character 60 and a shorter cap collapsed three distinct causes into one.
- `calendarCandles.ts` — chart candle boundaries: `1w` is **Monday 00:00 UTC**
  and `1M` the **1st 00:00 UTC** (what Binance, Bybit `W`/`M` and OKX `1Mutc`
  stamp, and what TradingView draws); every other interval is the epoch floor.
  `getCandleStartTime` stays an epoch floor on purpose — it is public SDK
  surface, and its `1w` starts on THURSDAY — so anything that buckets, counts,
  steps or closes CHART candles uses the `chartCandle*` /
  `getNumChartCandlesInRange` / `createLiveCandleStart` helpers instead.
  `CandleCache` does: its `1M` slots are 28 days wide (no two months share
  one), its lookups step candle start to candle start, `1w`/`1M` keys carry a
  `-cal` suffix so chunks written on the old boundaries are never read, and a
  candle is cached only once `chartCandleCloseTime` (relative to the candle
  itself, so OKX's 16:00 UTC `1D`/`1W` candles count too) has passed.
  Bybit's trade-built live bar (linear, inverse, spot) uses
  `createLiveCandleStart`; the other venues that build bars from trades still
  epoch-floor `1w`.
- `get-key.ts` — `v()` / `vn()` field lookup helpers (see above)
- `updateOrderBook.ts` — Sorted upsert via binary search; object + primitive APIs (see above)
- `sequencedDeltaOrderBook.ts` — Snapshot/delta sequence state machine; emits transport-agnostic recovery signals
- `orderbook.ts` — `sortOrderBook` (full sort, snapshot-only) + `calcOrderBookTotal` (cumulative, hot-path safe)
- `safe-math.ts` — Decimal-aware arithmetic; for placement/display only — see `../exchanges/CLAUDE.md` for hot-path rules
  Scientific notation precision includes both mantissa decimals and the signed
  exponent (`2.5e-7` has eight decimal places); do not reduce it to the
  exponent's absolute value.
- `__testdata__/market-samples.json` — Real symbol samples from supported exchanges

## Live polling slowdown

`tickIntervalOverride.ts` publishes multiplier changes through
`subscribeTickIntervalMultipliers`. Engine pollers subscribe once and remove the
listener on disposal; Rust accounts send the resolved interval as a scale on
each venue's own cadence (plus that interval as a ceiling) rather than
reconstructing wildcard/exact precedence. A paused interval stays paused across configuration
updates. Other TS pollers continue reading the multiplier at their next tick.

## Native position sizing

`nativePositionSize.ts` is the shared sizing leaf for SDK bulk closes and the
existing script-api-core re-export. It converts base contracts for linear
markets and quote notional for inverse markets, retaining the existing maximum
of derived size and `extra.contractsNative`. Missing/zero contract size defaults
to one. Rounding and input validation belong to the caller; do not add a reverse
dependency on script-api-core.
