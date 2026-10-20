# Shared history pagination

Branch `feat/rust-history-pagination` from `feat/rust-rewrite`. Scope:
RUST_QUALITY.md "Lower": `RangeWalk` serves six venues while the rest
hand-roll windows, cursors and pages in their `history.rs`. History reads only.

## Rules

- Before migrating a venue, pin its request sequence (URLs in order) for each
  paged read in a test, then migrate and keep the pin unchanged. A deliberate
  request change is a bug fix, listed in the PR with its own regression.
- Venues keep their wire parsing, row limits and error messages; the shared
  helper owns paging state: cursor, page budget, stall detection, dedupe,
  window splitting and offsets.

## Helpers (`engine/src/paging.rs`)

- `RangeWalk` (existing): span-complete time windows narrowed by row time.
- `CursorWalk`: continuation by an opaque token or an id the venue returns.
  It records each page and answers More(cursor), Done or Stalled(reason),
  where the reasons are a repeated cursor, an empty page with a cursor, a
  page of no new rows and the page budget. Each venue opts into the stall
  rules it has today.
- Fixed-span windows (newest or oldest first, inclusive and disjoint) and
  page/offset walks, added with the batch that first needs them.

## Batches (one PR each)

1. Cursor venues: Lighter, GRVT, Coinbase, Paradex. Paradex bugs: the fills
   limit counts rows before filtering, and its page caps only warn.
2. Window venues: Bybit, Bitget (output limit ignored), MEXC (oldest-first
   windows return the wrong subset at a limit), KuCoin futures.
3. Offset venues: Orderly, TxFlow (offset advances by 2000 instead of the
   row count), Backpack, WOO's hand-rolled pages.
4. Id cursors and timestamp narrowing: OKX, BloFin (order history never
   pages), Kraken futures, Crypto.com, Hyperliquid.
5. Leftovers on RangeWalk venues: Binance fills (a failed window returns an
   empty success and drops earlier windows), WEEX (per-window caps and
   truncation only warn), AsterDEX, Phemex.

## Progress
