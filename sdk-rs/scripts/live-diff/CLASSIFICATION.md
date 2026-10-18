# Public blockers, 2026-10-18

These follow-up observations classify every previously blocked venue. No test
credentials or order writes were used. Public response and message fixtures
remain under ignored `.cache/live-diff/fixtures/`; checked-in follow-up evidence
contains hashes and representative differences.

| Venue        | Cause and result                                                                                                                                                                                                                                                                                                                                                     | Next owner/action                                                                                                                                              |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AsterDEX     | Probe treated intentionally caught keyless leverage 401 and unsupported bulk OI 400 as fatal. Exact source fallbacks now accepted; **605 markets / 628 tickers pass**.                                                                                                                                                                                               | Probe repaired.                                                                                                                                                |
| Coinbase     | Live HTTP 200 contains 310 INTX instruments but zero `TRADING` perpetuals: 131 paused, 133 delisted, 18 cancel-only, 28 external. Both maintained filters return empty.                                                                                                                                                                                              | `NO_TRADING_MARKETS`; wait for real venue data, never fabricate active instruments.                                                                            |
| Lighter      | `fetchTickers` is inherited/unimplemented; real startup hydrates through WS/REST fallback. A bounded WS capture observes the connection greeting; actual REST `exchangeStats` supplies prices. After native processing completes, **all 213 market/ticker records match** (197 live prices). Earlier 60s stalls and partial Store callbacks remain latency evidence. | Probe repaired; current public startup passes. Slow native processing routed to `codex-wdtab1` and `codex-0w5ohi`. This does not claim a live WS price stream. |
| Polymarket   | Probe rejected POSTed midpoint reads. Allowing the exact `/midpoints` route yields **200 markets / 200 tickers passing**.                                                                                                                                                                                                                                            | Probe repaired.                                                                                                                                                |
| WOO          | TS accepts a literal null index for `PERP_SOLV_USDT`; native typed output rejects null as `f64`. A fresh native ticker run instead stalled at 60s before output. Exact HTTP 200 futures body is captured.                                                                                                                                                            | Native null projection/stall routed to `codex-wdtab1`.                                                                                                         |
| ApexOmni     | `fetchTickers` is inherited/unimplemented. Real startup + recorded public frames hydrates 89 TS tickers. Native partially hydrates, then its native event-loop barrier stalls for 60s.                                                                                                                                                                               | Native stall routed to `codex-wdtab1`.                                                                                                                         |
| Bitrue       | `fetchTickers` is inherited/unimplemented. Real startup receives gzip binary ticker messages. The same recorded messages reach native; TS updates live prices, native logs `Unknown data [object]` and retains zeros after a successful event-loop barrier.                                                                                                          | Native binary message projection routed to `codex-wdtab1`.                                                                                                     |
| TXFlow       | Both implementations intentionally register a Phase 0 scaffold with no REST, WebSocket or trading support.                                                                                                                                                                                                                                                           | `UNSUPPORTED_BY_TS`; no live adapter can provide an absent source implementation.                                                                              |
| BreakoutProp | Every instrument/quote path requires DXtrade login and an account code. No anonymous public API exists in the maintained connector.                                                                                                                                                                                                                                  | `AUTH_REQUIRED`; test-account row and authorized keys CLI login required. Authenticated recorder permits session login and POSTed quote reads.                 |
| OKX          | Native ticker processing times out after identical recorded HTTP 200 input.                                                                                                                                                                                                                                                                                          | `codex-0w5ohi`, [#1017](https://github.com/Tealstreet/tealstreet-sdk/pull/1017).                                                                               |
| Bitget       | Native ticker processing times out after identical recorded HTTP 200 input.                                                                                                                                                                                                                                                                                          | `codex-0w5ohi`, [#1017](https://github.com/Tealstreet/tealstreet-sdk/pull/1017).                                                                               |
| Phemex       | Native ticker processing times out after identical recorded HTTP 200 input.                                                                                                                                                                                                                                                                                          | `codex-0w5ohi`, [#1017](https://github.com/Tealstreet/tealstreet-sdk/pull/1017).                                                                               |

Merged [#1044](https://github.com/Tealstreet/tealstreet-sdk/pull/1044) was rebuilt
in this worktree at `33c2ae3eed` and checked against fresh identical HTTP bodies.
Backpack's 188 markets and Crypto.com's 404 markets match TS completely. Both
compiled recorded-market regressions also pass. Their remaining ticker ID
differences depend on uncontrolled wall-clock input and remain reported.
Paradex's 66 markets still lose their nested `fees` objects. Bitunix's 697
markets still convert `limits.amount.max` from a string to a number; its 697
tickers pass. Both remaining market defects were routed back to `codex-wdtab1`.

The requested [#1017](https://github.com/Tealstreet/tealstreet-sdk/pull/1017)
candidate was independently exercised using the owner-provided addon and revision
`0d9632aca90b99c708178554af614152cacb8151`. The observed binary SHA-256 is
`1129c766c0d0d31394960064e968245ee25f0480aaf72c85528415f07d0bc796`.

- ApexOmni: all 89 markets pass; both receivers get the same three WS frames.
  The native barrier still exceeds 60 seconds after partial ticker delivery.
- Lighter: all 213 markets pass; both receive the same connection greeting and
  `exchangeStats` response. This run exceeds the native barrier deadline too.
  Earlier baseline runs did finish with exact complete ticker parity, so this
  remains a latency/stall observation rather than a claimed permanent deadlock.
- Bitrue: all 779 markets pass; both receive the same 126 gzip binary frames.
  After a successful barrier, TS has 126 live tickers and native has zero, with
  repeated `Unknown data [object]` diagnostics. `codex-wdtab1` acknowledged this
  projection defect independently of the old Store facade.

Reports and hashes are retained in
[`evidence/confirmations-2026-10-18.json`](evidence/confirmations-2026-10-18.json).
Results were sent to `codex-0w5ohi`, `codex-dwfl8p`, and `codex-wdtab1`; no new
root cause is asserted for the ApexOmni/Lighter stalls.

## Bybit concrete-helper dispatch

The `Unknown method` findings for `fetchOptionMarkets`, `fetchTickersRaw` and
`fetchTickersWithoutParse` are harness-only, not missing public native methods.
Reviewed SDK master `b10cec94d31da76b8d5e86b1a590cb69300a2f29` declares them
on the concrete Bybit class but not in `exchanges/base.ts`'s public `Exchange`
interface or `CallableMethodsInWorker` union. The actual
`webworker/handlers/CallMethodHandler.ts` rejects each without invoking its
receiver property. A valid `fetchOrders` call is the positive dispatch control.

The planned exchange-server (#8030, reviewed checkout `7695533a25f4`) imports
`store.webworker`, forwards account messages through `ctx.dispatchMessage`, and
uses the same `ExchangeWorker` / `CallMethodHandler`; it adds no helper dispatch.
The maintainer also confirmed no application callers on monorepo master.
The earlier direct TS helper result versus native rejection remains historical
probe evidence, rather than a public API parity defect.

The shared REST plan/allowlist now excludes these names for both implementations,
including raw ticker helpers on other venues. Public market and ticker reads
remain, and Bybit's option-chain/expiry profile still uses public methods after
option-enabled market loading. Actual TS and Rust probe children reject direct
helper messages with the same visible error before either runtime is accessed.
This does not qualify any differing result through those public paths as parity.
