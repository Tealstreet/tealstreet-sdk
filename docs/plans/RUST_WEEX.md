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

- Public phase committed in `e2b17244a9`; nesting merged in `3921f42d9f`.
  Public review fixed strict numeric parsing, subscription generations, stale
  failures, startup recovery and complete streamed-document validation. Final
  review also covered active ticker selection and shared ban deferral.
- Private, trading and query phases committed in `88acc4ddd8`. Independent
  reviews fixed staged settings, ban scheduling, stale events, per-symbol mode,
  rollback ownership, conditional amendment prices, income identity fallback
  and pathological paging budgets. No outstanding review blockers.
- Harness records real WEEX public traffic (20 seconds, BTCUSDT/ETHUSDT) and
  replays documented signed account/trading traffic. Generic body-aware request
  matching, response headers, paced snapshots and socket readiness preserve
  Binance parity. Independent harness and host reviews are clean after fixing
  Node socket error handling, disposal and Workers upgrade capabilities.
- Preliminary replay gates all PASS: WEEX public worker CPU 141.8ms TS versus
  126.1ms Rust, steady CPU 82.8ms versus 58.8ms, retained memory 2.6MB versus
  2.2MB; private replay 15 frames. Binance public CPU 226.5ms versus 99.9ms,
  retained memory 2.3MB versus 2.0MB; private replay 14 frames. Final rerun follows
  the last public ban/active-symbol changes and landing-branch merge.
- Final checks in progress: Rust native/wasm, SDK/host tests, root typecheck,
  lint and tests. Broad root tests passed all but unrelated timed-out suites;
  package suite rerun uses two workers and a 30-second test timeout.
- Crypto audit found no helper made unused by this port; signing/UUID remain
  live and the shared signing primitive API stays intact. No credentials used.

## Delivery

After every gate passes, merge `origin/feat/rust-rewrite` into the lane (resolve conflicts under the overseer
standing authorization, keeping the landing structure), rerun affected gates, push, open PR into
`feat/rust-rewrite`, post adversarial local sub-agent findings as PR comments,
fix/review until clean or diminishing returns, merge PR and branch from the
updated rewrite branch. Report real gate numbers and any unfinished work.
