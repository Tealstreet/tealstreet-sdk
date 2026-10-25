# Proven coverage bound on incomplete history

Base: `feat/rust-rewrite` `c6a5c4bb0a4`. Owner: `claude-kuhwxi` (B11 has no
other holder). Reviewer: `claude-4yjlw0`. Follows #1666
([RUST_HISTORY_COMPLETENESS_TRANSPORT.md](RUST_HISTORY_COMPLETENESS_TRANSPORT.md)).
The NX caller is tealstreet-next `docs/plans/RUST_CUTOVER_NX_CALLERS.md` W6b,
which waits for this to land and sync.

## Problem

An incomplete history result says only `{ incomplete: true }`. A caller that
must not record missing rows as absence (NX `historical_coverage`) can therefore
never close any part of the window: a deterministic truncation, such as
Kraken's 500-row ledger page or a page cap, truncates the same way on every
retry. Keep-up then leaves a permanent hole once the window exceeds its span
cap, a cold start never anchors backfill, and backfill stalls on that step.
The caller cannot infer the covered part from row timestamps, because paging
direction, sort order and boundary rules differ per venue and are not part of
the contract.

The engine has no range either. `Completion.incomplete` is a first-cause
string (`sdk-rs/engine/src/history.rs:217-244`). Pagers hold cursors and the
oldest row only while walking, and drop them at `finish`.

## Decision

1. **An optional proven bound.** The completeness record becomes
   `{ incomplete: true, proven?: { fromMs, toMs } }`. It is a promise that every
   venue row the query would return, with time `t` (the query's own row-time
   semantics) and `fromMs <= t < toMs`, is in the result. **No `proven` means
   no claim**, which is today's meaning. The bound never appears on a complete
   result, and nothing manufactures one from rows after the fact.
2. **The bound is one range that touches a query edge**, either `[from, x)` or
   `[x, to)`, and it lies inside the query's `[from, to)`. A caller resumes from
   the other side of `x`. A result whose provable rows sit in the middle (chunk
   gaps, skipped rows) carries no bound. A bound never has an open edge: a query
   with no `from` can only carry `[x, to)`, and a query with no `to` uses the
   read-time cap of rule 3 as its `toMs`.
3. **Only a pager that proves it sets it.** A pager may set a bound only when
   all of these hold:
   - its direction is fixed by the request it sends (a time-ordered cursor or
     `sort`), not assumed;
   - it verified at runtime that every accepted page was monotone in that
     direction, and drops the bound on any violation;
   - it rounds inward at exclusive edges. A cursor that refetches or may split
     rows sharing a millisecond moves the bound past that millisecond;
   - it computes the bound from native pre-filter, pre-truncation times, then
     narrows it to the rows kept. A row dropped by the query's own filter
     (symbol, order id, kind) is fine. A row dropped as malformed, unmappable or
     skipped voids the bound, because it leaves a hole;
   - a newest-first bound is capped at read time: `toMs = min(to, the venue's
response clock on the first page)`, or the engine's clock at that page's
     receipt when the venue gives none. Otherwise a caller closes `[x, to)` and
     rows later written below `to` are recorded as absent. An oldest-first
     bound already ends at a row it observed.

   Everything else stays boolean.

4. **Composition is intersection.** A result built from several reads (multi-axis
   journal folds, serial stages such as categories, families, currencies or
   funding-then-fills) carries a bound only when every input has one, and the
   bound is their intersection. Any input without a bound means none for the
   whole result, and so does a stage that was never reached. Phase 1 sets no
   bound on composed results.
5. **Transport rides #1666 unchanged in shape.** The wasm host defines `proven`
   beside the non-enumerable `incomplete` marker. `historyCompletenessOf`,
   `withHistoryCompleteness` and `copyHistoryCompleteness` carry it, and so do
   the worker reply and MCP rows-plus-completeness serializations. The engine
   echoes the query's own range beside it,
   `{ incomplete: true, proven?: { fromMs, toMs }, query?: { fromMs?, toMs? } }`,
   because a worker reply or MCP row set does not otherwise carry the request.
   A receiver validates `proven` against that echo: finite integers,
   `fromMs < toMs`, inside the query range, touching an edge, no open edge. If
   validation fails, or there is no echo, it drops `proven` and keeps
   `incomplete`. A 1:1 SDK mapping of the same rows keeps `proven`;
   `copyHistoryCompleteness` never copies it onto rows that were re-filtered or
   merged after the fact (Decision 4), and copies only `incomplete` there.
   `history.incomplete` events carry no bound and still never mark a result.

## Phase 1 venues (bound set)

These were chosen from the inventory because their own request fixes the
direction and the boundary is handled:

| Venue / axis          | Site                                          | Bound                                                                                                                                                                                                                                                                      |
| --------------------- | --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Crypto.com history    | `cryptocom/src/history.rs:527-560`            | `fromMs = floor(oldestNs / 1e6) + 1`, newest-first. The `end_time = oldestNs + 1` cursor is exclusive at `oldestNs`, so rows in the oldest millisecond with smaller nanoseconds may sit on the next page; that millisecond is never inside the bound, on a boundary or not |
| Bitunix orders        | `bitunix/src/history.rs:309-345, 605-632`     | `(oldestCtime, to)`, newest-first; `endTime = oldest` is inclusive, so `fromMs = oldest + 1`                                                                                                                                                                               |
| Kraken Futures fills  | `kraken/src/history.rs:776-782, 858-869`      | `(oldestNativeMs, to)`; native ms, not the seconds-floored output                                                                                                                                                                                                          |
| Kraken Futures orders | `kraken/src/history.rs:451-466, 745-775`      | `(oldestEventMs, to)`, i.e. `fromMs = oldestEventMs + 1`, from native event times, `sort=desc`: a saturated page may split rows sharing the oldest millisecond                                                                                                             |
| Hyperliquid fills     | `hyperliquid/src/history.rs:343-355, 652-692` | `[from, newestMs)`, oldest-first and exclusive at the saturated ms                                                                                                                                                                                                         |

Kraken Futures ledger (#1542) gets a bound only on a runtime-verified
descending page, as `(oldestNativeMs, to)`. An unsorted page, or one
indistinguishable from unsorted, carries none. Every other site in the
inventory stays boolean for now, including:

- Binance and WEEX range walks, which have mid-window holes;
- id-space cursors (OKX general, BloFin), which have no time proof;
- offset paging (Bitunix income, Backpack, WOO);
- non-pageable stubs.

A later venue joins only by meeting rule 3 with its own red test. Every Phase 1
bound is also capped at read time (rule 3) where it is newest-first.

Phase 2 candidates (reviewed by `claude-4yjlw0`, not in Phase 1):

- Binance income (7-day chunks, oldest-first): `[from, start of the first
incomplete chunk)`, plus `[chunkStart, lastRowMs)` inside it, only with a
  runtime-verified ascending check on every page, since Binance documents no
  income sort order.
- Binance fills, once a saturated window is flagged (the Binance owner's
  separate fix): `[from, start of the saturated window)` plus
  `[windowStart, lastRowMs)` inside it, under the same rules.
- WEEX income (100-day range walk): up to the first gapped chunk's start, with
  the same runtime monotone check. A venue `has_next` gap marks the whole
  chunk.

## Tests (red before implementation)

- Per Phase 1 site:
  - a capped or failed later page yields `proven` equal to the expected inward
    bound;
  - a non-monotone page yields no bound;
  - same-millisecond rows split across pages are never inside the bound.
- A complete result never has `proven`. A boolean-only site never has `proven`.
- A newest-first bound with an open or future `to` is capped at read time.
- A malformed or skipped row voids the bound; a query-filtered row does not.
- Transport: the bound and its query echo survive structured clone (worker) and
  a JSON round trip (MCP) through the copy helpers, not by enumeration. A
  malformed, out-of-range, open-edged, mid-window or echo-less bound is dropped
  while `incomplete` stays. Concurrent bounded and unbounded requests never
  cross. `copyHistoryCompleteness` onto re-filtered or merged rows keeps
  `incomplete` and drops `proven`.
- A composed result with one unbounded input has no bound.

## Separate findings (not fixed here; owners assigned by the overseer)

- OKX orders/income: a failed later page emits a command error and resolves
  `[]`, discarding accepted rows (`okx/src/history.rs:1206-1224`).
- OKX closed positions: `after = oldest - 1` can drop rows that share the oldest
  millisecond even on a complete walk (`okx/src/closed.rs:198-221`). Both OKX
  fixes: `claude-kuhwxi`, separate PRs.
- Binance fills/orders: a saturated single-page window is truncated without a
  flag. Owner: `claude-4yjlw0`.
- `RUST_CUTOVER_CALLERS.md`: the non-pageable set is Kraken Spot, BreakoutProp
  and Hyperliquid Spot. The Bitunix #1633 entry says HTTP failures reject, but
  the code returns accepted rows plus the marker. Docs fix: `claude-kuhwxi`.

## Out of scope

- NX coverage changes (W6b(b)).
- Paging limits, retry policy, financial folding, signing.
- Any new history failure policy.
- Releases and deploys.
