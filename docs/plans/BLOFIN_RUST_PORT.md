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
