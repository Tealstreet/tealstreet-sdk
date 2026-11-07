# Bitunix closed positions and journal on Rust

Owner: claude-4yjlw0. Beta gap (overseer, Bitunix readiness): on the Rust engine, the Bitunix journal threw "no journal records path declared" and closed PnL was always empty. TypeScript serves both from the venue's reported position history, and it notes that Bitunix "folds to nothing from fills", so the fills fold is not an option. Review: claude-dazzy3. GO: claude-8x5tm1 (engine capability enums, wasm host wire).

## Change

- **Engine.** `PositionHistory::Bitunix` and `ClosedPositionFormat::Bitunix` are appended to the enums, so existing discriminants keep their wire values (7 in bits 20-23, 4 in bits 24-27). Pinned by `bitunix_journal_and_closed_format_append_new_discriminants`.
- **Venue.** Bitunix declares `closed_positions`, `PositionHistory::Bitunix` and `ClosedPositionFormat::Bitunix`, and serves `VenueAction::FetchClosedPositions` from `GET /api/v1/futures/position/get_history_positions`.
  - It runs on the existing managed history path: legs of the command's Read root, signed with the read key.
  - Pagination, the window and dedupe match TS `fetchBitunixHistoryPages` (`bitunix.exchange.ts:1226-1274`): `limit=100` and `skip=page*100`, time `mtime ?? ctime` and key `positionId`, the window checked before dedupe, a short page or a page that adds nothing ends the walk, and the cap is 100 pages, after which the result is marked incomplete.
  - Rows go to the host raw, in page order.
  - A failed read is a typed failure classified through the shared `BITUNIX` table. Rows already accepted are kept and marked incomplete.
- **Host and TS.** The wasm wire decoders and the capability conformance list gain `bitunix`.
  - `rust.exchange.ts` maps rows with the existing TS mappers: `mapBitunixPositionHistory` for the journal, with the settle currency falling back to `USDT` as TS does, and `mapBitunixClosedPosition` for closed PnL, newest first.
  - The journal keeps the adapter's documented rule (`rust.exchange.ts` journal paths): a failed read throws rather than present an under-counted history as complete. Closed PnL reports a failure and yields `[]`, as for the other reported-closed venues.

## Tests

- `venues/bitunix/tests/closed_positions.rs`: the signed path and query with the read key, two pages with dedupe, the window and stop rules, typed failures (HTTP 503; 10007 → `authInvalid`), a failed second page keeping the accepted rows, and the capabilities.
- TS audit-regression tests check the journal paths and closed PnL for equality against a real `BitunixExchange` on the same rows.
