# BloFin Rust port

Branch: `feat/rust-blofin`. Target: `feat/rust-rewrite`; never master.

## Phases

1. Public data: typed borrowed wire records, streamed instrument/ticker seeds,
   bounded subscription frames, sequenced books and books5, candles, socket
   watchdog/reconnect. Verify documented snapshots, gaps, reconnect and candles.
2. Private data: hex-then-base64 HMAC signing, authenticated private socket,
   independent account/order reconciliation, atomic cursor pagination,
   stale-snapshot guards, settings and bounded incremental execution tracking.
3. Trading: preflight entire commands, native contract sizing, caller IDs,
   normal batches, conditional/TP-SL jobs, cancel/amend and account settings.
   Writes are never retried after uncertain transport failure.
4. Queries: candle/history/fill/income/trade reads, typed JSON result shapes,
   bounded pagination and request validation.
5. Harness and gates: register wasm/worker/harness, documented private tape,
   live 20-second two-book public fixture, parity/performance gates and Binance
   regressions; repository typecheck/lint/tests. Record exact measurements.

Each phase ends in a sub-agent review, fixes, verification and a focused commit.
After all gates pass, merge origin/feat/rust-rewrite, rerun affected gates, push,
open a PR into feat/rust-rewrite, post adversarial review findings, fix/review,
merge that PR, and create a fresh follow-up branch. No rebase or force push.

## Risks reviewed before implementation

- Sizes on orders/positions/fills are native contracts; books remain native.
- Fills use differences of cumulative quantity AND traded value, retaining
  terminal rows (500-order bound) to suppress replays and regressive frames.
- Normal/TPSL/trigger snapshots must all finish through an empty page before
  replacing orders. Failed reads preserve last-known-good state.
- Every socket has ping plus inbound-traffic liveness; reconnect invalidates
  full-depth book sequences and restores all active subscriptions.
- TP/SL cancel-replace preserves the sibling leg and uses new generated IDs.
- Account-global modes and per-market settings survive position closure.

## Progress

- Read port brief, Rust spec, root and BloFin package guidance, engine and
  Binance reference. Fetched current process instructions from origin.
- Official payload reference: https://docs.blofin.com/index.html .
- No implementation or gates yet.

### Phase 1 verification

- Added public crate, bounded subscriptions, books/gap recovery, candles and
  liveness watchdog. `ArrayAt::EnvelopeField` retains API status around streamed
  data without retaining the list; validates separators, item syntax and UTF-8.
- Phase review found and corrected books5 ordering corruption (also fixed in
  TS, with whole-book gate comparison retained), duplicate candle emission,
  malformed-list acceptance and candle-window request differences.
- Rust: 8 public tests + 11 engine tests pass; venue clippy and bans pass.
- safe-cex typecheck and lint pass; focused BloFin TS regressions: 10 pass.
  Full TS suite still running; required final root checks remain in phase 5.
- Review recheck added malformed whitespace rejection, exact integer sequences,
  and authoritative replacement for thin/empty books5 sides.

### Phase 2 implementation plan

- `sign.rs`: request path+query/method/ms/unique nonce/body, HMAC hex then base64;
  private WS signs `/users/self/verifyGET...` and subscribes after login success.
- `account.rs` + borrowed `wire.rs`: independent account and order snapshot jobs;
  normal/TPSL/trigger cursor families complete only at empty pages, with
  non-advancing cursor and page caps rejecting the entire snapshot. Signed read
  retries only, bounded backoff; failed reads preserve state.
- Snapshot revisions protect orders, positions, balances and settings against
  newer socket/command updates. Settings survive closed positions. Streams map
  native contracts to base once; bounded cumulative fill tracker (500 rows)
  differences quantity and traded value to recover execution price.
- Verify signatures, login, fills/replays/regressions, setting preservation,
  stale snapshots, later-page failures, and private socket watchdog/reconnect.


### Workspace update

- Merged landing structure in `e7e0be289f`; crate is now
  `sdk-rs/venues/blofin`, shared core `sdk-rs/engine`, hosts `sdk-rs/hosts/*`.
- Removed unused `crypto::wallet_utils` (repository search found no caller);
  kept signing primitives and the shared nonce helper used by Binance.
- Full safe-cex run reached 6,488 passes; failures were a concurrent edit/run
  race in the now-passing BloFin regression and the pre-existing production-tree
  `rust.exchange.wasm.test.ts` violating the runtime-boundary test. Rerun final
  checks on stable sources, and relocate that integration test in phase 5.

### Phase 2 review and verification

- Typed private snapshots, hex/base64 signing, authenticated socket and liveness,
  cursor pagination and snapshot revisions implemented. Cumulative fills emit
  execution quantity/price once across duplicate, regressive and terminal frames.
- Review fixes: remainder-sized inline protection, valid positive leverage
  fallback, login startup dependency, separate collateral rounding, and rejection
  of malformed global modes. Global margin is projected to flat-market settings.
- Intentional TS corrections: malformed modes cannot initialize trading; empty
  position pushes clear used/upnl. Listed in gate comments/normalization.
- Final private checks: 1 signing + 9 account + 8 public tests, clippy and bans
  pass. Review recheck reports no remaining phase blockers.

### Phase 3 implementation plan

- A preflighted `Job` holds sequenced typed write steps; successful responses
  apply effects, errors stop the job, uncertain writes are never retried.
- Native contract sizing/rounding, caller IDs honored once or rejected across
  the entire batch before writes; normal batches at most 20, TP/SL single objects,
  conditional single objects. Generated IDs carry broker attribution.
- Cancel steps preserve exact native identity and correlate every row, including
  already-gone responses; pending TP/SL sibling protection is reconstructed
  before cancellation and re-placed afterwards. Pace TPSL/trigger writes.
- Amend is bounded cancel-replace with a fully preflighted replacement; keep
  raw conditional margin/position side, trigger reference, execution price and
  attached protection. Replacement client IDs are always new.
- Account setting commands commit only after confirmed success. Isolated hedge
  leverage updates both legs; changes update positions and persistent settings.
- Verify all-or-nothing IDs, placement sizing/brackets, response-loss no-retry,
  conditional provenance, cancel correlation and sibling preservation, mode and
  leverage rejections, optimistic states and cleanup.
