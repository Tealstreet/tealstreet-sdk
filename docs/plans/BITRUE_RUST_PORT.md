# Bitrue futures Rust port

Branch: `feat/rust-bitrue`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.

## Plan and audit

1. Public data: typed streamed contract metadata, ticker seed/stubs and channel
   lookup, full snapshot depth (simple-depth then timed fallback), candles and
   websocket history requests, server-ping responses and reconnect watchdog.
2. Private data: exact timestamp/method/path/body HMAC, real futures URLs,
   listen-key create/refresh, bounded safe read retry, account/order reconciliation,
   one-way settings and stale-snapshot guards. Running totals yield individual
   executions with duplicate/regressive suppression and exact large IDs.
3. Trading: whole-batch preflight, caller IDs verbatim on one regular order,
   integral quantity splitting, quote-sized opening market orders, atomic attached
   TP/SL, standalone conditional protection, cancel/replace and settings commands.
   Only an explicit signature rejection (-1022) authorizes one re-signed resend;
   ambiguous writes never retry. Margin-mode changes and hedged mode fail closed.
4. Queries: documented per-execution account trades with exact IDs and window
   filtering; enumerate official endpoints before declaring a query unsupported.
   Avoid synthetic fills, PnL or ledgers inferred from unrelated fields.
5. Harness gates: WASM/SDK registration, generic replay extension, documented
   private tape with signatures and mutation matching, live 20-second/two-book
   public fixture, Binance regression, native/wasm quality gates and CI.

Each phase ends with a sub-agent audit, fixes, verification and a commit.
Keep transport routing host-side; this venue emits actual Bitrue HTTPS/WSS URLs.
Use borrowed serde types and typed continuations; no JSON value trees or JS facades.
Check crypto leftovers for provably unused code. Preserve existing venue gates.

## Landing

After all gates: merge latest landing, push, PR into `feat/rust-rewrite`, post
local adversarial sub-agent findings, fix/repeat, wait for green CI, merge PR,
create a clean follow-up branch. Never stash, rebase, force-push or touch master.

## Progress

- Plan committed; source/API audit in progress. Official documentation:
  https://www.bitrue.com/api_docs_includes_file/futures/index.html and
  https://github.com/Bitrue-exchange/USDT-M-Future-open-api-docs/tree/main/v2 .
