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
- Final replay gates after landing merge `22ccd27bd4` all PASS. WEEX public:
  2028 frames, 1040 markets/tickers, two books; worker CPU 150.8ms TS versus
  109.5ms Rust (1.38x), steady CPU 83.9ms versus 49.2ms (1.70x), retained
  memory 2.6MB versus 2.2MB (1.19x), wasm linear memory 1.7MB. Whole-process
  CPU including background compilation was 204.8ms versus 230.0ms; the existing
  gate measures worker-thread CPU. WEEX private: 15 frames, exact account,
  command and signed-request parity. Binance public: 871 frames, CPU 228.2ms
  versus 100.5ms (2.27x), steady 126.6ms versus 42.1ms (3.01x), memory 2.3MB
  versus 2.0MB (1.15x). Binance private: 14 frames, exact parity.
- Native tests: 60 WEEX, 10 engine and 29 Binance passed. Root typecheck (44
  tasks) and lint (28 tasks) passed. SDK wasm tests5 and host tests7 passed.
  Bans/fmt and native/wasm clippy passed. Post-cleanup workspace/wasm clippy and release build also pass; wasm is
  byte-identical to the replayed artifact (1,443,799 bytes). The broad TS suite
  passes all 6,489 tests with two workers and a 30-second test timeout; earlier
  root runs passed the other packages but hit unrelated suite timeouts.
- Crypto cleanup removes unused wallet wrappers, address predicate, fixed
  venue-specific ABI tuple builder and lenient base64 decoder. Shared signing
  primitives remain; 27 unit, one integration and three compile-fail doctests
  pass. No credentials used.
- This is phase8 opt-in integration (`engine: 'rust'`), matching Binance.
  Existing TS remains actively reachable through its registry and used by
  the replay oracle. Phase9 shipping retirement and client adoption are not
  claimed complete by this lane.

## Delivery

After every gate passes, merge `origin/feat/rust-rewrite` into the lane (resolve conflicts under the overseer
standing authorization, keeping the landing structure), rerun affected gates, push, open PR into
`feat/rust-rewrite`, post adversarial local sub-agent findings as PR comments,
fix/review until clean or diminishing returns, merge PR and branch from the
updated rewrite branch. Report real gate numbers and any unfinished work.

## PR review checkpoint

PR #1301 targets `feat/rust-rewrite`. Round1 local adversarial findings are
posted as PR comments. The shared review fixed a circular conditional-price
oracle: expected trigger/type now comes independently from taped venue payloads,
including exact large IDs and protective markers. Five oracle regressions pass;
the private fixture now retains a conditional and final changed position/order
stream state (19 frames), and its replay passes. Real HTTP transports have
20-second body-inclusive deadlines; the splitter exposes validated metadata so
income continuation flags are available without retaining streamed rows.

Account/trading/history fixes and independent re-reviews are in progress for
hung reconciliation, imported trailing-order cancellation, rollback ownership,
partial-fill and bracket-preserving replacement, settings mutation ownership,
and positive income continuation. Final verification and landing follow those
fixes. Initial GitHub Rust gates and PR CI passed. The first-push scan identified
one old Binance unit-test vector from published official documentation; an exact
historical fingerprint exception is verified locally, without broad rule changes.
