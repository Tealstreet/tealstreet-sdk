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

### Phase 3 review and verification

- Sequenced jobs preflight every write, replan queued commands against confirmed
  state, and never retry uncertain writes. Normal batches, standalone/full-position
  TP/SL, opening triggers, cancel/amend and settings are implemented.
- Review caught and fixed nullable response messages, tiny partial sizes becoming
  full-position stops, edits dropping native entries or bracket provenance,
  sibling execution/reference changes and live-position margin overwrites.
- Both isolated-hedged leverage writes are issued together and joined before
  settings commit. Lot-unit splitting preserves quantity without rounding loss.
- Final review recheck: no remaining blockers. 34 venue tests pass (16 trading,
  9 account, 8 public, 1 signing); clippy and bans pass.
- Additional intentional TS corrections: validate every cancel row and its exact
  submitted identity even under code 0; reject unsupported TIF/tiny partial
  protection instead of silently changing semantics; conserve split lot units.

### Phase 4 implementation plan

- Borrowed history payloads stream into typed result rows; commands return the
  same JSON keys as Binance's history module. Preserve per-fill trade IDs,
  native-to-base execution quantities, fee sign conventions and time units.
- Orders preserve TS's native quantity history shape. Income uses cursor paging
  and client-side time windows because BloFin ignores begin/end on fills history;
  page failures fail the read rather than silently returning partial history.
- Public trades and candle reads validate symbols/options; rare reads never
  mutate the store. Test fill identity, tiny quantities, fee signs, filtering,
  paging failures and exact JSON shapes; run a phase-end sub-agent review.

### Phase 4 review and verification

- History/fills/income/public-trade queries return typed JSON without touching
  account state. Execution quantities retain tiny fills; history orders retain
  TS's native units and milliseconds. Fill events use seconds.
- Review fixes: arbitrary-length numeric cursors, invalid timestamp handling,
  execution aliases with zero/side precedence, symbol/non-swap filtering, and
  walking past newer pages when BloFin ignores requested fill time bounds.
- Intentional TS corrections: never fabricate executions from order-total
  history, walk older fill windows instead of false empty history, and fail a
  truncated/failed income walk rather than returning partial ledger rows.
- 43 venue tests pass, including 9 query regressions. History clippy passed after
  the style fix; final all-target checks will repeat on stable integrated code.

### Phase 5 implementation plan

- Register wasm and SDK venue; pass passphrase through engine options. Add
  documented private tape, write-body request comparison and fill-price parity
  as generic optional harness capabilities; preserve Binance defaults.
- Build wasm, run private correctness gate, record 20s BTC/ETH public tape and
  trim fixture, run public performance/correctness gate and Binance regressions.
- Fix workspace-path integration issues and move the existing Rust wasm test
  out of production runtime code to satisfy the package boundary invariant.
- Run root typecheck/lint/tests plus required Rust checks. Record exact results
  and phase review; merge landing branch, rerun affected checks, then PR reviews
  and merge only after every required gate is green.

### Phase 5 review checkpoint

- Registered BloFin in wasm and SDK, including passphrase forwarding. Added
  replay body matching, exact caller-ID preservation (including broker-prefix
  IDs), one-shot recorded mutations, missing-write detection, logical private
  subscriptions, and per-execution fill-price comparisons.
- Review found terminal-row resurrection in amend/protection merging after
  retaining terminal statuses. All planners now reject/skip terminal targets and
  siblings; late partial frames cannot reopen terminal rows. A regression covers
  terminal amendment, repeated cancellation, and new protection after cancellation.
- Paired TS fix: cancel-all filters open rows instead of resending retained
  terminal orders. Other defensive TS divergences are listed in gate.mjs.
- Fixed live recorder handling of streamed transport bodies. Recorded 20 seconds
  of BTC/ETH public traffic (2,922 records); public performance gate is running.
- 44 BloFin tests and clippy pass after review fixes. Root typecheck (44 tasks)
  and lint (28 tasks) pass; root tests and stable wasm/gates remain in progress.
