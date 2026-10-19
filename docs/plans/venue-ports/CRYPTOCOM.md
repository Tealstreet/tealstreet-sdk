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
