# Phemex perpetuals Rust port

Branch: `feat/rust-phemex`, based on `origin/feat/rust-rewrite`.

## Plan

1. Public: typed streaming products/tickers, packed ticker updates, sequence-aware snapshot/delta books, candles, connection watchdog. Review and commit.
2. Private: exact HMAC signing, authenticated aop_p stream, independent USDT/USDC account slices, per-market mode/settings even when flat, stale snapshot guards, execution deduplication. `Triggered` maps Closed but is never terminal. Review and commit.
3. Trading: g-API PUT/DELETE sequenced jobs, preflight whole command and caller IDs, bracket/trailing formatting, cancel/amend/settings with acknowledgement guards and no write retries. Review and commit.
4. Queries: bounded typed paged reads and explicit unsupported contracts where venue/TS lacks a safe route; candle reads. Review and commit.
5. Harness/gates: wasm/SDK registration, strict signed-request and WS audit, documented private tape and recorded public fixture, known TS fixes, Binance regressions and all CI checks. Review and commit.
6. Landing: merge latest landing branch, rerun gates, push and PR into feat/rust-rewrite, post adversarial reviews, fix until clean, merge only green CI, cut fresh follow-up branch.

## Plan audit

USDT/USDC PerpetualV2 g-API uses Rp/Rv values. Legacy Ep/Er scaling is handled only where the TS wire contract needs it. Order snapshot is WS-owned; REST position polling must not erase orders. Position mode must survive zero-size account rows. Failed currency reads retain their own prior slice. Every socket requires server.ping and bounded reconnect/watchdog. No credentials or live private writes are needed for gates.

## Progress

- Read current landing brief/spec and Phemex runtime guidance. Full-auto user authorization overrides the plan skill confirmation pause.
