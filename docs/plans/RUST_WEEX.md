# WEEX Contract V3 Rust lane

Branch: `feat/rust-weex`. Scope: WEEX USDT perpetual futures on the sans-I/O
engine. Follow `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`. Full auto; review
by a sub-agent at each phase boundary. Never touch master or rebase.

## Phases

1. Public data: new typed venue crate, streamed markets/ticker seed, public
   socket watchdog/ping and subscription budgets, depth snapshot/diff sequence
   machine with gap healing, streamed and requested candles. Register workspace
   and wasm; tests from current official Contract V3 payloads.
2. Private data: exact REST HMAC over timestamp/method/path/query/body; optional
   signed socket upgrade headers across native/Workers hosts and browser REST
   fallback; streamed reconciliation, settings, position mode, stale guards,
   terminal order tombstones, per-execution fills, read-only backoff.
3. Trading: sequenced jobs; all-batch client-ID preflight; batches of five
   correlated by echoed ID; conditional/brackets/protective orders; cancel,
   cancel-all, cancel/replace and protective amend; leverage/margin/mode.
4. Queries: Binance result shapes for fills/history/income/public trades.
   Income uses engine `paging::RangeWalk`, 100-day chunks, 100 rows, paced pages,
   identity deduplication and explicit incomplete notice.
5. Harness and gates: generic venue hooks, signed documented private tape,
   live 20-second two-book fixture, correctness/CPU/memory gates and Binance
   regression gates; bans/fmt/clippy/tests/wasm and repository checks.

## Venue-specific audit points

- Upgrade authentication cannot be silently dropped or put in a URL. Browser
  continues its 2-second REST safety loop. Reconnect regenerates auth.
- An HTTP error or malformed/truncated streamed list cannot commit an empty or
  partial snapshot. In-flight snapshots cannot reopen a terminal order or
  overwrite newer private events/settings.
- Depth diffs are sequential (`U`..`u`), ignore stale frames; bound buffering
  and backoff resync; no depth truncation beyond the venue's 200-level feed.
- Subscription limits: 100 channels, 90 ticker slots, 240 operations/hour.
- Never retry writes, including ambiguous failures. Preserve caller IDs verbatim
  on exactly one regular native order, or fail before any write.
- Synthetic protective IDs cannot select the protective amend endpoint.
- Source: https://www.weex.com/api-doc/llms-full.txt (current futures sections;
  old `/contract/` documentation URLs redirect to the landing page).

## Progress

- Planning: complete. Repository/engine/reference/TS contracts inspected;
  current official futures docs retrieved. No implementation changes yet.
- Observed TS defect to cover: `fetchOrders` turns a failed normal/algo read
  into an empty half-snapshot. Rust must retain last-known-good state and
  document the deliberate difference in gate and commit message.

## Delivery

After every gate passes, merge `origin/feat/rust-rewrite` into the lane (stop
on conflicts per AGENTS.md), rerun affected gates, push, open PR into
`feat/rust-rewrite`, post adversarial local sub-agent findings as PR comments,
fix/review until clean or diminishing returns, merge PR and branch from the
updated rewrite branch. Report real gate numbers and any unfinished work.
