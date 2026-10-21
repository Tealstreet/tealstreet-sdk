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

Review fixes are independently re-reviewed clean: bounded reconciliation with
late-generation guards and paced-page deadline renewal, imported trailing-order
routing, current REST snapshots revoking stale rollback ownership, preserved
remaining quantity and per-leg bracket triggers, fill-only uncertainty refusing
replacement until fresh reconciliation, serialized settings mutations and explicit
income INCOMPLETE notices for advertised cursors. Focused tests pass: 26 trading,
17 history, 18 account; WEEX clippy passes. Final shared integration now merges
the newly landed Bybit nested-array metadata support, followed by all gates.


## Final gate evidence

Final code after adversarial fixes and landing merge: `e10b87b95d`.
All six replay gates PASS in [GitHub run39290020868](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39290020868).

| Public tape | Frames / books | TS worker CPU | Rust worker CPU | TS retained | Rust retained |
| --- | --- | --- | --- | --- | --- |
| WEEX | 2028 / 2 | 146.2ms | 132.6ms | 2.5MB | 2.2MB |
| Binance | 871 / 3 | 240.0ms | 111.1ms | 2.2MB | 1.8MB |
| Bybit | 8742 / 2 | 268.9ms | 197.8ms | 3.2MB | 2.1MB |

WEEX steady worker CPU: 76.2ms TS / 52.0ms Rust. Whole-process CPU including
background compilation: 200.0ms TS / 304.7ms Rust; worker-thread CPU is the
established gate. Private correctness: WEEX19, Binance14, Bybit18 frames.

Final local workspace tests PASS: WEEX76, Bybit43, Binance29, engine15,
crypto27 unit +1 integration +3 doctests, native host1. SDK wasm7, host7 and
oracle5 tests pass. Bans, workspace fmt/clippy, wasm clippy, root typecheck44
and lint28 tasks pass. Broad TS suite previously passed6489/6489; general CI
is the final landing check. All local adversarial findings and merge reviews
are posted on PR#1301 and re-reviewed clean.

Combined artifact: 1,887,079 bytes locally / 1,886,574 bytes in CI. Set a2MiB
combined-venue cap (11% local headroom); the first1.8MiB estimate left357bytes
on macOS. This adjusts the static binary budget for three venues and platform
variance. CPU/memory thresholds stay Rust<TS for every public tape.

PR merge waits for green final-head CI, then the lane cuts a fresh follow-up
branch from the updated `feat/rust-rewrite`. TS retirement/client adoption and
live private-account validation remain outside this opt-in phase8 lane.

## Landing update: OKX

The landing branch advanced to OKX merge `2b37754970` while final CI ran; the
lane merged that base too. Its EnvelopeField metadata API, public-only credential
rules, aliases, signed body/subscription validation, staged snapshots and write
consumption checks are preserved alongside WEEX/Bybit. Independent merge audit
is clean. Shared engine tests19, SDK wasm17, workspace tests/clippy, wasm clippy,
root typecheck44 and lint28 pass. Combined artifact is1,913,238 bytes locally,
under the existing2MiB cap. Final replay matrix now includes four public venues
and seven private variants, including all four OKX aliases.

WEEX private tape now has20 frames: a stream event exposes its opening algorithmic
order before cancel, avoiding dependence on safety-read timing. It passes strict
request-body/signature matching and requires every expected write to be consumed.
No optional-write exception remains. Final matrix results are posted on PR#1301;
landing still waits for green CI on this merged head.

## Combined-venue memory regression

The four-venue build exposed a local OKX public retained-memory miss: Rust
2,526,248 bytes versus TS2,508,376. Public startup previously retained the
full 5×market topic vector alongside serialized messages. It now serializes
100 topics at a time, preserving channel order, instrument IDs and existing
chunk boundaries. The 101-market regression covers all505 topics across
channel boundaries; OKX public13/13 and clippy pass. Independent sub-agent
review found no correctness issues. No gate thresholds or accounting changed.

Rebuilt wasm is1,912,416 bytes. The isolated OKX replay passed with one fewer
64KiB linear-memory page and111,880 bytes less retained memory than TS. The
complete eleven-gate replay matrix and final-head CI are the remaining landing
checks; final numbers will be recorded on PR#1301.

## Landing update: Crypto.com

The landing branch advanced to `c63c3e5670` and conflicted before PR merge.
Merged its Crypto.com crate, registration, exact protocol audits and stricter
memory measurement. Preserve WEEX headers, independent conditional oracle,
caller IDs, time-aware staged reads and mandatory write consumption; a separate
late TS hook handles Crypto.com subscriptions. Preserve strict streamed JSON
validation and both metadata APIs; engine21/21 and harness14/14 pass. Incoming
provably unused Ethereum/zkLink convenience-code removal is retained.

Measurement now counts JS backing buffers and samples CPU before forced GC,
using stable collection across event-loop turns. Independent review found that
a negative heap delta could be masked by WASM memory; reject nonfinite/negative
heap, buffer, total and WASM components instead of clamping. Seven adversarial
invalid-component probes reject correctly. Private TS WEEX/Crypto.com smokes
have zero unmatched requests, unused writes or protocol errors. Harness/merge
reviews are clean. Five-venue combined build, thirteen replays and final-head CI
remain the landing checks; all earlier performance numbers are historical and
will be superseded by the final measurement model on PR#1301.

The final five-venue artifact is2,150,200 bytes:53,048 bytes above the former
four-venue2MiB cap. Raise only the static combined binary budget to2.25MiB
(2,359,296 bytes,9.7% headroom). CPU/memory gates remain Rust<TS; measurements
retain all heap/backing-buffer/WASM components. Workspace checks, SDK wasm17,
host7 and harness14 tests pass on this combined build. The artifact addition
is measured rather than inferred from isolated venue builds.

### Port-review lows plan (2026-10-21)

1. Reverify isolated leverage and fetched rails against current native code and
   existing regressions; strike stale findings with concrete evidence.
2. Record why index-source placement, unidentified fills and asset-less income
   remain risky without independently verified wire/accounting contracts.
3. Keep this family documentation-only if revalidation finds no safe remaining
   behavior fix. Independently review the audit disposition, check the diff and
   use CI before merging; documentation-only PRs are exempt from the code hold.

- Revalidation is documentation-only. `trading.rs::SetLeverage` retains the
  opposite isolated rail from native config/settings, validates both rails and
  sends the required pair. `isolated_side_leverage_keeps_the_other_payload_and_setting_leg`
  covers Long/Short/omitted side; `weex_side_is_coupled_in_cross_and_rejections_keep_settings`
  covers coupled cross and rejection preservation. The existing package CLAUDE
  already records these contracts. `RustEngineExchange::fetchLeverageForSymbol`
  returns fetched native rails under the Settings capability, not market max.
- `history.rs::FillRow` accepts id/tradeId/fillId. An absent identity field
  rejects deserialization and therefore the query; an empty identity string
  still parses and is skipped. The cited malformed-row regression covers invalid
  price, not absent/empty identity. Inventing identity is
  unsafe, and the empty-string skip remains a limitation. Income with neither
  asset nor its coin alias rejects that page; range walks continue with an
  explicit incomplete marker. It cannot safely default to USDT without accounting
  evidence. Rust refuses unverified IndexPrice placement; TS silently maps it to
  MARK_PRICE, which is not evidence that native index semantics are supported.
- No code or caller contract changes. Audit entries now distinguish stale
  findings from deliberately retained risky changes; no new tests are needed
  for these documentation corrections. Existing focused regression evidence and
  CI support the revalidation.
- Re-ran the three cited isolated-side/cross/rejected-write/malformed-fill
  regressions successfully. Diff whitespace check passes; full CI follows.
