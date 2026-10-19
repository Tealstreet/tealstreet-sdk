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
