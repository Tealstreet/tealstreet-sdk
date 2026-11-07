# Resilience lane: backoff, private rows, startup remainder

Scope: `RUST_QUALITY.md` shared helpers 3 and 5; `RUST_AUDITS.md` cross-venue
"rows and subscriptions" and "per-coin balances and snapshot starvation"; the
private-row, app stop payload, BreakoutProp authenticated startup and MEXC
bootstrap remainder handed over from the public-startup work
(`RUST_PRIVATE_ROWS_TICKERS.md`, `RUST_APP_STOP_PAYLOADS.md`).

## PRs

1. `engine::backoff::Backoff` (#1391): one formula for the inline exponential
   retry delays (REST reads, snapshot retries, startup discovery) and the socket
   supervisor, constants unchanged for every reachable attempt. Socket
   reconnects all moved onto `engine::socket::Supervisor`, which owns their
   jitter (75-100% of the delay) through `backoff::jitter`.
2. App stop payloads (#1396, merged): protective orders accept the app's
   `triggerPrice == price`; Kraken Spot refuses a mismatched trigger; Lighter
   refuses trailing stops instead of placing a market order.
3. Per-currency balances on Crypto.com, GRVT, MEXC and Phemex (#1399, merged).
4. Snapshot starvation (#1419, merged): `engine::fence::StaleGuard` on Bitrue,
   Backpack, BingX, Kraken Spot, KuCoin Spot, Orderly, Polymarket and WOO;
   WEEX balance reads no longer fence.
5. MEXC refused key fails startup instead of polling forever (#1431, merged;
   test helper fix #1440).
6. Private-row isolation (#1459): `engine::coverage::SnapshotCoverage` on
   Bitget (replacing #1356's whole-snapshot freeze), Binance USD-M/COIN-M,
   Hyperliquid, Lighter, Orderly and Paradex. MEXC, Crypto.com, BingX, BloFin,
   Bitrue, KuCoin, OKX, Kraken futures, Backpack and GRVT catalogue-filtered
   rows belong to claude-cbzmw6; WOO/AsterDEX/WEEX/ApeX/Polymarket rows to the
   venues-b lane.
7. Startup discovery retries honour 418/429 and `Retry-After` (#1481);
   Bybit HTTP-200 retCode 10006 is covered by the follow-up below.

Not done, by decision: BreakoutProp authenticated discovery startup. The venue
is disabled in production and preview (`enabled: false`,
`enableInPreview: false`; the legacy DXtrade pool is frozen), so by the
production-impact rule the work has no user impact (overseer, 2026-10-21).

Each fix has a reproducing regression; each PR gets adversarial sub-agent
reviews posted on it.

## Remaining resilience fixes (Codex, 2026-10-21)

One PR each from current `feat/rust-rewrite`, in this order:

1. Rejected-row severity: Binance/Coinbase/WEEX/Kraken futures order rows,
   plus Coinbase spot holdings, report coded `row.skipped` at Error when no
   stored copy is retained and Warn when one is kept. Preserve reconciliation,
   terminal fences and identity matching. Keep bounded once-per-identity
   reporting, permitting one escalation if a warned row loses its stored copy;
   a mapped row or clean snapshot re-arms it. Rebuild on `Cx::notice`, treating
   the pre-log-removal sketch as reference only. Regression tests exercise mixed
   retained/missing rows, repeat suppression, recovery and terminal identity.
2. Orderly margin-mode lists: isolate rejected rows and retain their symbol's
   old value, with a coded skip notice; valid peer rows still commit.
3. Bybit discovery: honour HTTP-200 retCode 10006 and its reset-timestamp
   cooldown without shortening existing Retry-After/backoff protections.

Each phase gets a failing regression before implementation, focused touched
venue plus Binance tests, scoped formatting/Clippy, independent review, fresh
CI and a separate fail-closed revert guard before its merge. No local release
WASM builds, deployments or downstream application edits.

Rejected-row severity implementation: the four venues use coded `row.skipped`
notices and bounded severity-aware identity caches. A retained copy warns; no
retained copy errors; a prior warning may escalate once. Coinbase includes
spot holdings and REST, user-snapshot and deferred-detail orders. Existing
reconciliation and parse-site notice timing stay intact. Retention tests cover
terminal/symbol fences, repeat suppression, recovery and missing-copy upgrades.

### Orderly margin-mode row isolation

Plan: change `Request::Modes.rows` to the existing `SnapshotBuilder` so rejected
symbols retain their old default while accepted peers commit. Recover the
symbol independently if mode decoding fails; an unidentified row protects the
old map's scope. Keep the structural `invalid` flag and settings-revision fence,
valid-empty clearing, broker suffixes, empty-mode-as-Cross, and existing accepted
duplicate last-wins behavior. Use fixed `row.skipped` diagnostics without raw
payloads. Full-engine startup regressions preseed the private defaults cache
and check identified/unidentified rejection, replacements, empty/duplicate
rows, failed envelopes and a newer settings revision. Verify Orderly plus
Binance, strict Clippy/fmt, independent review and full CI. Merge remains held
behind the overseer's #1602/#1487 queue until it is lifted.

Implemented with `SnapshotBuilder` and fixed Warn-level `row.skipped` notices.
Six full-engine regressions cover retained identities, unidentified scope,
valid peers, broker suffixes, empty/duplicate rows, malformed envelopes and
settings-revision fencing. Four fail on the old implementation; all pass with
the fix. Orderly plus Binance crate tests and scoped strict Clippy/fmt verify
the change; independent review requested the additional malformed-envelope
cases and found no production blockers. No downstream callers change.

### Bybit discovery envelope rate limits

Plan: recognize HTTP-200 `retCode:10006` on Markets, Tickers and OptionSeed
responses, including missing-list error envelopes. Preserve the Response through
the stream-error path only for this typed discovery rejection. A private helper
combines a finite future `X-Bapi-Limit-Reset-Timestamp` (epoch milliseconds) with
existing Retry-After parsing, a ten-second fallback and the shared one-hour cap;
keep the HTTP403 ten-minute floor and maximum existing deadline. Extend already
waiting category backoffs using their existing helper without increasing attempts
or creating retries for idle categories. No transport-wide gate or write change.
Regress actual retries, both envelope shapes, malformed/stale/huge hints, mixed
hint precedence, success-header suppression, options seeds and cross-category
timer extension. Verify failing baseline first, Bybit plus Binance tests, scoped
Clippy/fmt, independent review, CI and a separate current-landing trial/guard.

After this phase, reconcile skipped-row auth evidence per RUST_AUTH_RESULTS as
a separate bridge follow-up PR; explicit network resync stays parked.

Implemented with a private Bybit pause helper and typed stream-error forwarding.
Ten new regressions cover the actual missing-list and list-bearing envelopes,
market/ticker/option-seed retries, hint precedence and bounds, ordinary response
suppression, monotonic windows and cross-category extension without extra
backoff attempts. Eight fail before the fix, then all pass. Other streaming
errors, optional-product degradation, private reads and writes keep their
existing handling. No downstream callers change.
All 532 Bybit/Binance tests, strict scoped Clippy and formatting pass; independent
review is clean. The existing headerless-10006 public regression now checks the
ten-second fallback while preserving its error and recovery assertions.

### Auth evidence follows envelopes, not mapped rows

The follow-up reconciles BingX, MEXC and WEEX through the shared pure
`AuthOutcome::accepted_list` helper. A typed success envelope with the required
list accepts credentials even when row mapping fails; missing/malformed required
data yields no outcome before grouped auth state mutates. Existing typed
credential rejection, signer selection, fences, financial reducers and non-list
settings validation stay intact. See [the single rule](RUST_AUTH_RESULTS.md).
The two old BingX/MEXC false-row None cases are now explicit Accepted cases; all
other original controls remain. Four baseline regressions reproduce the old
row-quality gates and MEXC grouped-state clearing; twelve new tests also cover
valid peers, diagnostics, list shapes and grouped orders/settings recovery.
All 705 touched-venue/Binance tests and 11 scoped engine tests pass, along with
strict scoped Clippy and formatting. Independent implementation/final reviews
are clean. Full CI and the separate merge guard remain the final gates.
