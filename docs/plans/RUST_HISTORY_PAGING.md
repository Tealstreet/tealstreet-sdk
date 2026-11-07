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

- Batch 1 (PR into `feat/rust-rewrite`): `CursorWalk` added; Lighter, GRVT,
  Coinbase and Paradex migrated with request-sequence pins written and
  passing first. Recorded behaviour differences: Lighter's 1000-page budget
  is per stage, and GRVT checks a stalled cursor only while the read still
  wants rows. Paradex fixes, each with a regression that failed first:
  - fills by order id stopped once raw rows reached the limit, before the
    order-id filter, so a lookup could end on a page of other orders' fills;
  - fills never sent `start_at`, so a sparse window paged through older fills
    it then discarded (Rust-only tape record; TS still sends only `end_at`);
  - order history and income kept paging past their limit; a bounded read
    (limit given, or a client-id lookup that found its order) now stops once
    it has enough rows, and only an unbounded read that exhausts its page cap
    with a cursor pending fails as INCOMPLETE. A bounded read at its cap keeps
    what it found, as before (a lookup reports a non-definitive miss).
    Because a bounded read no longer fetches pages after its limit, a
    malformed later page can no longer fail it.
- Batch 2: `paging::windows` (disjoint inclusive windows of a maximum width,
  newest or oldest first, with a window budget). Bybit, Bitget, MEXC and
  KuCoin futures use it, each with a request-sequence pin written and passing
  first. Fix (MEXC, regression failed first): windows ran oldest first and the
  read stops at its limit, so a limited read over more than 90 days returned
  the oldest window's rows; windows now run newest first. Not changed: Bitget
  applies its limit as the page size only; the UTA private tape shows the TS
  reference also follows the cursor past the limit and returns every row.
- Batch 3: `OffsetWalk` (row offsets or page numbers, advancing by rows
  returned or a fixed page size, with page and offset budgets and opt-in
  stalls). TxFlow and Backpack use it, each with a pin written and passing
  first. TxFlow keeps its fixed 2000-row step: the TS reference also steps by
  the page size, and no TxFlow contract shows short pages that report more.
  Orderly and WOO stay hand-rolled: completion follows the server's total and
  Orderly's budget spans its regular and algo stages, so the helper would only
  grow special cases without fixing anything.
- Batch 4: BloFin fills and income id-cursor paging use `CursorWalk` (pin
  written first and unchanged; any repeated cursor now stalls, not only the
  previous one). BloFin order history stays a single request, as in TS.
  OKX, Kraken futures, Crypto.com and Hyperliquid stay hand-rolled: OKX
  requires strictly decreasing 64-bit ids with a venue-specific failure
  contract (a warning globally, an error for EEA composite reads), and the
  others narrow by timestamp with tuned millisecond/nanosecond overlap and
  saturation rules the helpers would not simplify.
- Batch 5: Binance fills windows use the shared tiling, which also covers the
  final millisecond the old split dropped when a range ended one past a window
  boundary (regression failed first). A failed fills window keeps landing's
  bridge contract at that historical merge (a command error before an empty
  success, then pinned in `queries_variants.rs`). The later
  [owned-history migration](RUST_OWNED_HISTORY.md) supersedes that failure
  contract: before accepted rows reject typed; an actual later-page failure
  retains only accepted rows with command-owned incompleteness. The old
  empty-success assertions are replaced by regression-backed owner policy.
  WEEX's per-window truncation warnings and AsterDEX / Phemex single reads remain
  pending the owned-completion fan-out; this historical refactor is not the final
  cross-cutting enforcement boundary.
