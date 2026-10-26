# Crypto.com Rust port

Branch: `feat/rust-cryptocom`. Spec: `RUST_REWRITE.md`; process:
`VENUE_PORT_BRIEF.md` from origin/feat/rust-rewrite (2026-10-19).
Full auto; commit each phase after verification and a sub-agent review.

## Phases

1. Public data: typed venue crate; streamed nested REST lists; market/ticker
   seed and sockets; snapshot books, valuations and candles; socket watchdog.
   Verify documented payloads, bans, fmt, clippy and native tests. Review.
2. Private data: HMAC envelope signing, paced auth and subscriptions;
   streamed reconciliation with atomic order snapshots and stale-read guards;
   positions, settings, balance and execution deduplication. Review and test.
3. Trading: sequenced place/amend/cancel jobs; caller IDs preflighted across
   the whole batch; advanced stops; leverage; fail-closed unsupported margin
   switching. No retries of ambiguous writes. Review and test.
4. Queries: fills, order history, income and public trades using the existing
   JSON result contracts; candles and account validation. Review and test.
5. Integration and gates: register wasm/TS/harness, documented private tape,
   live two-book public recording and compressed fixture, TS defect register,
   Crypto.com and Binance correctness/performance gates, repository checks.
   Review and fix, then merge origin/feat/rust-rewrite into lane, push, PR to
   feat/rust-rewrite, adversarial review/comment/fix loop, merge PR and cut a
   follow-up branch. Never modify master or force/rebase.

## Venue-specific audit

- Public lists are under `result.data`; private reads are signed POSTs.
  Streaming must support both a nested array and a request body, and validate
  the success envelope even when its code follows the array.
- Default `book.<symbol>.50` is a complete snapshot: replace both sides.
- Both sockets answer `public/heartbeat` with the same id; silent sockets
  reconnect after 90 seconds; private auth waits one second after opening.
- Current `funding` differs from `estimatedfunding`; index topics use the
  instrument's underlying index; optional subscriptions respect the cap.
- TS has no parent bracket derivation; unsupported shapes must not silently
  lose caller IDs or risk controls.
- Failed reads must retain good state. REST must not overwrite newer stream
  events or terminal orders. Only executions emit fills, once per trade id.
- Fresh payload tests derive from official Exchange v1 documentation and the
  repository's documented runtime contracts, never deleted legacy fixtures.

## Progress

- Initial inspection and plan audit complete. No implementation yet.

### Public phase

Implemented typed market/ticker seed, valuations, default snapshot books,
candles, subscriptions and 90-second watchdog. Shared streaming supports nested
arrays and signed-body reads without changing existing callers. Review found
malformed list/UTF-8 acceptance; fixed with streamed syntax validation and
regressions. Checks: bans PASS; crate clippy PASS; engine 10 tests and venue 6
tests PASS. Public phase ready; private phase next.

### Private phase

Moved crate to `sdk-rs/venues/cryptocom` after merging origin's nested workspace
(`fe79b018cd`). Signed POST reads stream `result.data`; auth waits one second,
then subscribes to order/advanced-order/positions/balance plus `user.trade`.
Snapshots merge around newer events; failed reads retain state. Settings carry
leverage/margin without zero positions; terminal orders and position closures
have independent stale-event fences. Executions retain full-width ids and emit
once per trade id (bounded reconnect deduplication cache).

Sub-agent found incorrect position-id separator, lost closure timestamp fence,
and symbol-scoped snapshot fencing that dropped the other hedged side. Fixed
all with regressions. Bans/clippy PASS; 15 venue tests PASS (including signer).
TS defects intentionally corrected: failed reads previously committed empty;
order mappers lost `client_oid`; `user.trade` was never subscribed/emitted;
REST reset live leverage/liquidation enrichment; terminal/stale state could
reopen exposure. Register in harness KNOWN_TS_FIXES during gate phase.

### Trading phase

Preflighted typed jobs run sequentially with 20 ms pacing. Caller ids are sent
verbatim; duplicates and unsupported shapes reject the entire batch before any
write. Validation uses a conservative 36-byte ceiling (also rejects line
terminators) rather than introducing JS string-length emulation. Native amend
preserves caller ids. Regular/advanced cancel routing, best-effort batch cancel,
cancel-all, fixed hedged mode and fail-closed margin switching are implemented.

Official API audit found TS sends an invalid per-symbol/string account-leverage
request; Rust reads `account_id`, sends integer leverage, and updates global
settings. TS silently ignores attached brackets/opening triggers; Rust rejects
them. TS sends POST_ONLY as time_in_force; Rust sends the execution instruction
with GOOD_TILL_CANCEL and combines REDUCE_ONLY. Sub-agent found false HTTP
success, post-only wire shape, missing-id cancel abortion and Unicode length
mismatch; fixes and regressions added. Bans/clippy PASS; 25 venue tests PASS.

Next: history queries, exact nanosecond pagination, then wasm/harness gates.

### Queries phase

Added streamed fills/order history/transaction ledger/public trade reads in
Binance JSON shapes. Private paging uses exact u64 nanosecond boundaries,
overlaps and deduplicates boundary rows, walks short pages, and rolls back
partial pages before retrying. Ranged income walks the whole range without a
100-row total cap. Missing exact cursors fail closed when another page is
needed. Historical limit prices remain separate from trigger prices; absent
requested/average prices are omitted. SESSION_SETTLE uses the documented
realized_pnl field (official institutional get-transactions example).

Sub-agent found the income total cap, dead cursor-precision guard, trigger vs
limit history price and settlement-PnL mapping. Fixed all; regressions added.
Bans/clippy PASS; 33 venue tests PASS. Next: registration, tapes, cleanup, gates.

### First-port crypto cleanup

Reference audit proved wallet_utils, zklink::contract, lenient base64 decoding,
Ethereum Value/address/ICAP facades and fixed Aster ABI construction have no
venue callers. Removed them and their facade-only fixtures/tests. EIP-712 now
uses Alloy TypedData directly: hashing/signing primitives remain, while ethers
input-normalization/rejection emulation is deliberately removed. Retained
primitive vectors, nonce scrubbing and independent Lighter corpus all pass:
23 crypto unit tests, 1 corpus integration test and 3 doctests; clippy PASS.

### Integration/gates in progress

Wasm/TS/harness registrations and documented private tape implemented. Harness
now matches signed POST params, independently verifies REST/WS HMACs and methods,
checks private/book subscriptions and heartbeat ids, consumes mutations once and
compares open interest. Crypto.com private gate PASS (21 frames); Binance public
PASS (871 frames: TS CPU 234.1 ms / Rust 92.6 ms, retained 2.4 / 2.0 MB), Binance
private PASS (14 frames). Numbers are provisional until final combined checks.

Repository typecheck (44 tasks), lint (28 tasks) PASS. Full test run exposed an
existing wasm test inside production exchanges; moved it into audit-regressions.
Boundary regression and 4 real-wasm integration tests now PASS; full suite rerun
is running. Public recorder needed its streaming body handling fixed. First live
REST seed revealed null prices on irrelevant spot ticker rows; now identify and
skip non-derivative rows before decoding ticker fields. Rebuilding wasm before
the required fresh 20-second two-book recording and public performance gate.

Remaining: finish public tape/gate, final fmt/bans/clippy/checks, review gate phase,
commit fixture + results, merge latest origin/feat/rust-rewrite, push PR targeting
that branch, adversarial sub-agent PR reviews/comments/fixes, merge PR and create
fresh follow-up branch. No push/PR/merge landing action has occurred yet.

Live public capture found the venue's signed `id: -1` on unsolicited pushes;
response ids now decode as i64 and snapshot regressions cover it. Additional
stale-state audit found cached REST positions and same-timestamp frames could
reopen a closed position after the closure preceded the request. Closure times
now fence REST too, and REST timestamps fence later stale stream frames. Two
new regressions PASS (35 venue tests total); crate clippy PASS. Fresh successful
recording is pending the release relink. TS full-suite rerun encountered only
unrelated import/configuration timeouts under concurrent machine load; running
the complete package suite with 2 workers and 30-second test timeout.

### Gate audit and public performance

Fresh live capture succeeded: 20 seconds, two books; trimmed fixture contains
2,177 frames, 404 derivative markets/tickers and both full-depth snapshots.
Live payloads also contain null derivative quotes: Rust maps unavailable numeric
values to zero; the explicit KNOWN_TS_FIXES normalization corrects TS NaN.
The independent request oracle now preserves unsafe literal integer parameters
as BigInt, so nanosecond cursor signatures remain exact (Node 22/24 verified).

Public gate review found missing TS worker-to-main store transport and missing
ticker-subscription enforcement. The harness now invokes the actual production
ExchangeWorker payload packer and DefaultStore receiver, retains main-thread
books, compares received state, and requires ticker subscriptions. TS public-only
startup incorrectly skips all market subscriptions; an explicit reference hook
corrects it. Both paths omit app-derived postprocessors. Protocol audit parsing
now only examines heartbeat/auth candidates, leaving numeric data in the engine.

Rust decodes the common nested ticker envelope once, builds subscriptions lazily
up to the cap, and loads sorted snapshot sides directly. Venue 35 tests and
clippy PASS. First corrected public replay PASS: TS/Rust main CPU 169.5/91.6 ms,
steady 84.9/46.3 ms, retained 1.8/1.4 MB. Final repeated replay plus Binance
regressions are running; these numbers remain provisional until committed below.

Full package rerun: 450 files / 6,488 tests passed; one unrelated registry-wide
construction test hit its explicit 60-second timeout. An isolated rerun is being
checked. Root typecheck and lint had passed; no unrelated test behavior changed.

The isolated registry-construction rerun PASS: all 5 tests. Added Crypto.com
public/private replays and harness protocol regressions to existing Rust CI.

### Pre-integration gates PASS

All brief gates pass at 302d8c2399 (Node 24, Rust 1.99 release wasm):

| Tape | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| Crypto.com public (3 runs) | 2177 | 137.8 / 106.5 | 83.0 / 52.5 | 1.8 / 1.4 |
| Crypto.com private | 21 | 50.0 / 44.5 | 17.8 / 16.0 | 0.7 / 0.8 |
| Binance public (3 runs) | 871 | 213.5 / 86.5 | 121.6 / 36.7 | 4.2 / 2.0 |
| Binance private | 14 | 63.4 / 37.0 | 16.8 / 16.8 | 0.9 / 0.8 |

Private replays gate correctness only. Public outputs match, including both
Crypto.com books and 404 market/ticker rows. Wasm size 1,187,594 bytes.
Bans, fmt, venue all-target clippy, wasm clippy PASS. Engine 10, Binance 29 and
Crypto.com 35 native tests PASS; protocol harness 5 tests PASS. Gate-phase
sub-agent review findings (transport, book retention, ticker subscriptions)
are addressed. Next: merge current landing branch (now includes Bybit), rerun
combined gates, push/PR, adversarial review and green CI before merge.

### Landing integration

Merged origin/feat/rust-rewrite at 20dd66dddb, preserving nested crates and the
landing test location src/__tests__/rust.exchange.wasm.test.ts (including Bybit
coverage). Combined the two ArrayAt::Path implementations by keeping the
landing bounded-envelope/outside API plus this lane's item/separator validation;
signed POST response streaming remains additive. Preserved canonical Bybit
socket URLs, all registrations/gates and streaming recorder tee behavior.

Merged native suites (engine/Binance/Bybit/Crypto.com), venue clippy, wasm clippy,
bans and 5 harness regressions PASS. Combined three-venue wasm is 1,618,621 bytes;
size budget advances from 1.4 MiB (two venues) to 1.6 MiB (three venues), following
the landing branch's per-venue budget increment. Combined replays follow.

### Combined landing gates PASS

All six replays pass after the landing merge:

| Tape | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| Crypto.com public (3 runs) | 2177 | 188.9 / 141.1 | 123.5 / 74.0 | 1.9 / 1.5 |
| Crypto.com private | 21 | 41.1 / 49.1 | 15.2 / 20.6 | 0.7 / 0.8 |
| Binance public (3 runs) | 871 | 252.6 / 104.1 | 148.2 / 42.9 | 4.3 / 2.1 |
| Binance private | 14 | 60.1 / 35.9 | 22.4 / 17.6 | 0.9 / 0.8 |
| Bybit public (3 runs) | 8742 | 479.5 / 315.1 | 336.9 / 189.7 | 6.0 / 2.2 |
| Bybit private | 18 | 103.7 / 41.9 | 48.0 / 21.0 | 3.4 / 0.7 |

Private tapes gate correctness only. Package typecheck/lint and all six real
wasm integration tests PASS after merge. Native combined suites total 118 tests
(engine 11, Binance 29, Bybit 43, Crypto.com 35), all PASS. Full workspace
clippy/tests also running as CI preflight. Ready to push and open PR into
feat/rust-rewrite, followed by required adversarial reviews/comments and green CI.
Rust remains opt-in, with TS retained as the replay reference like the existing
Binance/Bybit lanes; global production TS removal is rewrite phase 9.

### PR #1303 adversarial review, round 1

Three independent reviewers found and reproduced stale cached order overwrites,
position resurrection after REST omission, splitter envelope truncation/scalar
whitespace repair, omitted JS buffer memory, absent Bybit write classification,
fractional history bounds, transfer variants and concurrent history rate-limit
violations. Findings are posted at
https://github.com/Tealstreet/tealstreet-sdk/pull/1303#issuecomment-6272520761.

Fixed all: timestamp-preserving order merge; per-position omission closures for
REST and stream; splitter overflow fails closed and keeps internal whitespace;
baseline-adjusted ArrayBuffer bytes counted once alongside wasm; generic tested
response queues and Bybit mutation classification; checked whole/fractional ms
conversion; documented deposit/withdrawal/rollback categories; per-method query
slots shared across commands/pages/retries. Non-2xx writes retain native rejection
details without reporting success. Added concrete regressions: 41 venue tests,
12 engine tests, 8 harness tests PASS; venue/engine/wasm clippy PASS. Full workspace
clippy/tests passed before these fixes. New release/replays are running; the
memory numbers above predate buffer accounting and are superseded by next results.

Initial PR Rust CI (all six jobs) PASS. First-branch push scan found an inherited
synthetic Binance signing vector in old commit 879fdc7461, not a lane credential;
PR-range secret scan PASS. Subsequent pushes scan only their new commits.

### Adversarial review, round 2 / follow-on fixes

Public/harness and trading/history reviewers passed round 2. Private reviewer
reproduced a valid reopen being erased by a pre-closure cached REST row. Resolve
newer same-ID live positions before closure filtering; the extended regression
now passes (13 account tests; 42 venue tests total). Private reviewer rechecked
and reports passing with no further concrete P1/P2 findings.

Buffer accounting exposed delayed release of the wasm loader's input buffer,
causing an invalid negative retained-memory delta. Baseline/final memory now
collects across event-loop turns and reports heap/buffer components; the gate
rejects negative retention. Fresh one-run public probe PASS with positive valid
retention: TS/Rust CPU171.4/113.2 ms, steady106.4/59.3 ms, memory1.8/1.5 MB.
Full repeated gates follow. Current combined wasm1,625,486 bytes (<1.6MiB).
Findings posted at
https://github.com/Tealstreet/tealstreet-sdk/pull/1303#issuecomment-6272649386.

Final measurement audit moves CPU/wall end sampling before forced memory cleanup,
requires three consecutive stable buffer observations (bounded eight passes),
and rejects negative buffer deltas even when wasm growth masks them. No clamping
or minimum-sample selection. All prior performance numbers remain superseded
until this final measurement model completes the six-gate rerun.

### Final corrected gates and review disposition

All six gates PASS at a0914cd9d6. These measurements supersede earlier tables:
CPU excludes forced end-of-run memory cleanup; retained memory includes V8 heap,
JS ArrayBuffer backing stores and wasm linear memory, with stable baseline and
nonnegative-component checks. Public values are medians of three runs.

| Tape | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| Crypto.com public | 2177 | 67.7 / 60.5 | 37.0 / 20.2 | 1.8 / 1.5 |
| Crypto.com private | 21 | 35.4 / 37.6 | 12.2 / 15.2 | 0.7 / 0.8 |
| Binance public | 871 | 168.0 / 64.7 | 93.3 / 20.4 | 4.3 / 2.5 |
| Binance private | 14 | 40.1 / 27.7 | 10.6 / 11.9 | 0.9 / 0.8 |
| Bybit public | 8742 | 186.4 / 116.1 | 120.7 / 60.3 | 6.0 / 2.5 |
| Bybit private | 18 | 69.7 / 29.4 | 27.4 / 13.8 | 3.4 / 0.7 |

Private gates validate correctness only. Crypto.com42native tests, engine12,
harness8(Node22/24), bans/fmt/clippy/wasm checks PASS; six wasm integration tests
and package/root TS checks described above pass. Combined wasm1,625,486bytes.
All three adversarial scopes now pass with no remaining concrete P1/P2 findings;
all confirmed findings were fixed and posted on PR1303. Final passing review:
https://github.com/Tealstreet/tealstreet-sdk/pull/1303#issuecomment-6272721603.

Port and local review phases complete. Landing remains subject to green CI on
the final PR head, then authorized merge into feat/rust-rewrite and a fresh
follow-up branch. PR: https://github.com/Tealstreet/tealstreet-sdk/pull/1303.

### Concurrent OKX landing integration

While final CI was starting, PR1302 landed at2b37754970. Merged that landing
commit, preserving OKX aliases/passphrase/configuration, opt-in streaming
envelopes, staged response headers, subscription/login checks, query dispatch,
all-run correctness checks, isolated outputs and stripped wasm metadata. Combined
these with exact Crypto.com body/HMAC validation, stable memory accounting,
production TS store transport and generic single-consumption response queues.
Added staged-snapshot/headers regression. Nine harness tests and all four venue
native suites plus engine PASS (engine16,Crypto.com42); wasm clippy PASS.
Combined four-venue wasm1,687,019bytes; aggregate budget1.8MiB. Revalidating public
and private gates, including all four OKX account variants, before updated CI.

### Four-venue integration final gates

All eleven local replays PASS after merging2b37754970. These public medians
(three runs, TS/Rust) supersede the earlier three-venue results:

| Venue | Frames | Main CPU ms | Steady CPU ms | Retained MB |
| --- | ---: | ---: | ---: | ---: |
| Crypto.com | 2177 | 80.1 / 60.7 | 37.4 / 20.8 | 1.8 / 1.4 |
| Binance | 871 | 169.8 / 66.2 | 94.1 / 20.2 | 4.3 / 2.5 |
| Bybit | 8742 | 265.9 / 149.2 | 182.5 / 76.3 | 6.0 / 2.6 |
| OKX | 8902 | 657.8 / 552.1 | 515.2 / 419.0 | 4.1 / 2.6 |

Private correctness gates PASS:Crypto.com21frames,Binance14,Bybit18,
OKX/OKXEU/OKXdemo/OKXEUdemo29each. Zero unmatched or unused writes; both protocol
audits pass. Four-venue native suites plus engine16PASS, nine harness testsPASS,
package typecheck/lint and all16wasm integration testsPASS; wasm clippy/bansPASS.
Integration sub-agent review at2a47b4e8bc passes with no concrete findings.
Wasm1,687,019bytes (<1.8MiB). All local implementation/review work is complete;
CI on the final pushed PR head is the remaining landing condition.
