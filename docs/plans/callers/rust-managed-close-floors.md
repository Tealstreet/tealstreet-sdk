# Managed-feed close floors

The engine drops a venue's close code on managed feeds and retries on the
ordinary backoff. `FeedPolicy::close_floors: &'static [(u16, f64)]` declares a
minimum reconnect delay per close code. On a venue close frame the retry waits
for the longer of the floor and the jittered backoff, may exceed
`backoff_cap_ms`, and still counts a failure, like Lighter's legacy
`retry_after(30_000)` on 1013. Host route exhaustion, timeouts, auth recovery,
stops and expiry never take a floor. `LifecycleEvent::Retired` gains
`close_code: Option<u16>`, which is `Some` only for a `Closed` retirement by a
close frame.

## Obligations

- Every SDK `FeedPolicy` literal must state `close_floors`, since the field has
  no default. An open socket-adoption PR that adds a literal stops compiling
  after this lands. It re-merges master, declares its floors and needs a fresh
  run; no GO carries across this change.
- Each venue declares floors from its TS reference close handling, in its own
  adoption PR. Every adopter on master at landing declares `&[]`: Binance, BingX,
  Bybit, OKX, AsterDEX, Backpack, Hyperliquid, Kraken, Kraken Spot, Phemex,
  TxFlow, WEEX and WOO (plus BloFin once on master). Their TS sockets pass
  `onClose` to `base.ws.ts` with no per-code floor. The owners confirmed this
  for Binance (kuhwxi), BingX and BloFin (mxb4mq).
- Lighter (#2314) declares `(1013, 30_000.)` on both its public and private
  feeds, matching TS `ws-public.ts:61` and `ws-private.ts:103`, and restores its
  legacy 30 s pin. Its reason-text "rate limit" rule is not code-keyed and is
  recorded as a divergence in Lighter's own fragment.
- Binance and Bybit name the close code in their reconnect notice again, as
  their legacy sockets did ("Binance socket closed (1006); reconnecting").
- NX and server callers: no change. Notice text is not a contract.

## Known TS/Rust divergence, predating managed sockets

Rust has never had the generic `base.ws.ts` close handling: close 1000 resets to
a 1-3 s delay, a new close code resets the attempt count, and 8 failed closes in
a row pause reconnects for 120 s. These remain out of scope; do not assume
parity.
