# Kraken futures Rust port

Branch `feat/rust-kraken`, based on latest `origin/feat/rust-rewrite`.
Scope: `kraken` futures only; leave the distinct `krakenspot` adapter alone.
Process: full auto, no stash; commit stable phases, sub-agent code-correctness
review after each phase, fix findings, all brief gates, PR into
`feat/rust-rewrite`, local code-correctness reviews posted on PR, green CI,
merge and cut a fresh branch. User authorization supersedes skill approval pauses.

## Phases

1. Public data: streamed instruments/tickers, ticker stream, sequenced full-depth
   books, charts polling and candle queries, public socket watchdog and retries.
2. Private data/signing: challenge state machine, subscriptions, REST reconciliation,
   source revision fences, deduplicated execution fills, store settings.
3. Trading: serialized typed jobs, brackets, trailing, cancel and amend, settings,
   caller-client-ID rejection and fresh internal IDs, strict ACK acceptance.
4. Queries: typed streaming fills/ledger/order history/public trades with pagination,
   time/identity/unit handling and explicit incomplete-read errors.
5. Integration/gates: wasm/wrapper, harness real TS signing validation, documented
   private tape, live public 20-second recording and trimmed fixture, venue/Binance
   gates, root checks, PR code-correctness review/fixes/CI/merge/fresh branch.

## Authentication design and audit

REST: one per-engine signer owns decoded base64 secret (zeroizing) and nonce state
shared by reconciliation, trading, and queries. Allocate nonce as
`max(floor(now_ms)*1000, previous+1)`; never modulo-wrap or regress on a clock change.
Build alphabetical RFC1738 forms once; sign precisely those bytes and send the same
string as the GET query or POST/PUT body. Batch uses `json=` plus percent-encoded
serialized typed JSON. Signature is base64(HMAC-SHA512(decoded secret,
SHA256(encoded_form + nonce + signing_path))). Derivatives URL
`/derivatives/api/v3/*` signs `/api/v3/*`; ledger URL `/api/history/v3/*`
signs `/api/history/v3/*`. Public derivatives and charts never carry credentials.
Accept no API-level error/refusal as an empty successful read. Do not retry writes.
Verify REST forms, both path families, Unicode/spaces, batches, and challenge
signatures against independent JS/official vectors, including tamper tests.

Private WS: separately tagged Private socket at the same upstream URL as Public.
On each open reset prior challenge/subscriptions, request `{event:challenge,api_key}`.
Only the current socket in WaitingChallenge may accept a nonempty challenge.
Sign SHA256(challenge) with decoded-secret HMAC-SHA512/base64, then include api_key,
original_challenge and signed_challenge on EVERY authenticated subscription.
No synthetic authenticate event. Track ACKs per feed; reject errors, enforce auth
and subscription deadlines. On close/auth failure clear connection-bound state;
reconnect with fresh socket/challenge, never reuse an old signed challenge.
Use engine timers for ping at <60 seconds, receive timeout and missing-payload
recovery; heartbeat is transport liveness, not account data completeness.
Subscribe fills/open_orders/open_positions; retain REST balance polling instead of
TS's deliberately invalid account_balances feed. Snapshot fills are history, not
new executions; dedupe native execution IDs across replay/reconnect. Order totals
never emit fills. Reject stale REST completion after newer WS/accepted writes;
failed reads retain previous state, retry reads with bounded backoff.

## Source references

- `docs/plans/VENUE_PORT_BRIEF.md`, `RUST_REWRITE.md` and package CLAUDE files read.
- Kraken REST: https://docs.kraken.com/exchange/guides/futures/rest
- Kraken WS auth: https://docs.kraken.com/exchange/guides/futures/websockets
- Book: https://docs.kraken.com/exchange/api-reference/futures-websocket/book
- Existing TS `kraken.*` behavior and engine/Binance patterns.

## Progress

- Planning: repository and official signing/book docs inspected; authentication
  risks and phase boundaries recorded. Implementation pending.

### Implementation checkpoint

- Public data implemented with streamed seeds, role-tagged socket, engine ping,
  sequenced full depth, polling charts and subscription generation fences.
  Phase code-correctness review found stale book snapshots and candle error
  envelopes; fixes covered by nine public tests. Live instruments additionally
  prove negative quantity precision (PEPE -3 means 1,000-contract steps).
- Private/signing implemented; independent official challenge and REST vectors,
  monotonic nonce stress, streamed atomic reads and execution-ID dedup. Review
  tightening confirmed settings versus defaults and authoritative omissions.
- Trading implemented; review fixes include remaining-quantity protective replace,
  queued margin settings resolved after prior ACK, and late ACK/WS ordering.
  Thirty regression tests pass on author verification.
- Queries implemented; fifteen tests cover exact native IDs/ISO time, first-page
  PnL, cursor boundaries/refusals, raw-ledger paging and cash/position leg split,
  documented v3 order events and public trade history. Independent review ongoing.
- Harness integration underway; independent Node crypto validates sent form bytes,
  paths and current per-socket challenges. Four oracle tests pass (179 harness
  total). First per-venue Wasm build 837,439 bytes, below 1,258,291 budget.
- Full gates/PR not yet complete. No credentials, stash, push or merge performed.

### Phases 1–4 reviewed

- Public review: stale snapshots and malformed/error/incomplete candle replies
  fixed; 10 public regressions now include the live negative-precision case.
- Private/signing review: default versus confirmed settings separated, complete
  preference omissions reset cross mode, malformed snapshots retain last good
  state, accepted settings survive initial/delayed socket snapshots, disconnects
  fence REST, source clocks preserve richer live positions. Native remaining
  execution quantity updates orders monotonically without duplicate fill events.
- Trading review: partial protective replacement, execution race suppression,
  late placed/edit ACK fences, and queued margin changes all accepted after fixes.
- Queries review: saturated descending ledger can succeed only on a proven lower
  boundary crossing; zero-quantity rejected order events supported; page budget
  cannot be hidden by the requested output limit. 18 history regressions pass.
- Independent reviewers accepted all four implementation phases. Native venue
  tests and all-target Clippy pass; harness gate integration continues separately.
- Deliberate TS corrections include monotonic nonces, full depth/sequence safety,
  negative precision powers, execution-ID deduplication, remaining protective
  size, accepted-write/source fences, documented index triggers and explicit
  failed/incomplete reads. The harness lists these in KNOWN_TS_FIXES.

### Harness phase reviewed / landing integration next

- Recorded live 20 seconds: 28,048 WS frames, two books. Trimmed fixture: 9,641
  frames, 207 markets/tickers. Public gate PASS: TS 1,714.5 ms versus Rust
  457.6 ms (3.75x); steady 1,275.7/294.7 ms (4.33x); retained 3.6/1.7 MB.
  Single-venue browser Wasm 842,175 bytes, budget 1,258,291 bytes.
- Private PASS: 35 WS events + 13 commands + one close; 11 signed writes;
  challenge A, reconnect with challenge B, fills replayed across reconnect,
  bracket/amend/batch-cancel/settings/history, retained order and nonzero position.
- Independent integration code-correctness review found oracle gaps for minted-ID
  reuse, stale challenge proof, duplicate form/JSON economic keys and broad page
  normalization. All fixed with tamper and mixed-query regressions. 186 harness
  tests pass, independently rerun; final review accepted.
- Native workspace 1,165 tests (Kraken 80), workspace/all-target Clippy, Wasm
  Clippy, bans and formatting pass. Root typecheck 44/44, lint 28/28 and test
  tasks 27/27 pass. Combined build/Binance regression gates still running.
- Crypto audit: reuse the existing UUID primitive; HMAC-SHA512 is venue-local
  over zeroizing decoded key bytes. No newly unused crypto facade found. Removed
  the redundant direct getrandom dependency (entropy belongs to crypto::uuid).
- Latest fetched landing is f66bdc26f9 (Orderly). Integrate its structure after
  this stable checkpoint, repeat affected gates, then open the PR as authorized.

### Landing merge reviewed

Merged landing f66bdc26f9 (Orderly) by preserving incoming workspace/host structure
and unioning Kraken registration with Orderly's broker aliases and protocol
hooks. Five conflict files resolved: harness build, venues, gate, Wasm Cargo and
constructor. Shared Tape retains both correlation/generation audits and sticky
Kraken authentication failures. Independent code-correctness review accepted the
union; 215 harness tests pass. Full landing-source gates are being rerun.

### Startup validation hardening

Code-correctness review accepted fail-fast malformed base64 credentials at the
Wasm constructor, before engine/host/network creation. Native startup also emits
a clear signing error without exposing credentials. Added native and real Wasm
boundary regressions so account startup cannot hang silently on malformed keys.
Quiet-account watchdog semantics and bounded execution/tombstone retention are
being audited before final acceptance.

### Quiet-account and retention review fixes

Official fills/open_orders feeds are event driven after their initial snapshots.
A code-correctness audit confirmed that payload-age reconnects were incorrectly
closing healthy quiet accounts. The watchdog now requires initial snapshots and
continues transport ping/receive deadlines, while initialized quiet feeds remain
open. A 30-minute virtual-clock regression failed before and passes after.

Execution and terminal-order ID sets now use bounded FIFO recent windows (8,192
and 16,384 identities) and survive reconnects; duplicate IDs do not grow their
queues. Before/after cap regressions prove bounded retention, and a fresh-challenge
reconnect regression proves recent executions stay deduplicated. No load tests.

Landing-source proofs before these focused fixes: 1,247 workspace tests; root
44 typecheck, 28 lint, 27 test tasks; 215 harness tests. Binance public and private
PASS (public CPU 221.1/100.5 ms, retained 4.2/2.4 MB). Kraken combined-build public
and private PASS (public CPU 1,781.2/377.9 ms; retained 3.7/1.9 MB). Final source
reverification follows this checkpoint.

### Wrapper journal parity reviewed

Retained Kraken's existing account-wide fills journal declaration in the Rust
wrapper. The generic base otherwise throws an unsupported-path error. A real
shared-fold regression failed before, passes after, and was independently rerun
and accepted in code-correctness review; it asserts a closed round trip and its
account, symbol, PnL, fills provenance and single account-wide warmup fetch.

### Bybit variants landing integration

Pre-integration final proofs: 84 Kraken tests; 1,251 workspace tests; native and
Wasm Clippy, bans, formatting, 44 typecheck and 28 lint tasks pass. Kraken public
PASS: 9,641 frames, 207 markets/tickers, two books; CPU 1,866.6/415.2 ms (4.50x),
steady 1,399.8/257.7 ms (5.43x), retained 3.7/1.7 MB (2.13x). Single-venue Wasm
844,826 bytes. Private 49-frame correctness PASS. Binance public/private PASS;
public CPU 245.9/104.8 ms and retained 4.2/2.4 MB. Real Wasm boundary 53 tests pass.
One memory-baseline settling failure passed on an unchanged rerun. Root tests
hit unrelated cold-import five-second timeouts under concurrent local load;
repeat with bounded workers before acceptance.

Fetched landing advanced to 922b5fdc89 (Bybit variants). Preserve its shared
engine, wrapper, request/gate and host changes; union Kraken normalization once
with the incoming generic fix dispatcher, and retain both constructor tests.
Independent merge review and affected final gate reruns follow this checkpoint.

Integration code-correctness review accepted after two fixes: native Kraken now
rejects the entire batch containing Bybit-only execution controls before writes,
and protective amendment initializes the new shared optional fields. Regression
failed before on an observed HTTP write and passes after; Kraken 85 tests pass.
Kraken collateral normalization now requires successful recorded account evidence
instead of inventing coins for an unrelated fake account. Incoming universal gate
CLI smoke failed before, passes after; independent harness 235/235 pass. Shared
protocol hooks, book readiness, constructor validation and single fix dispatch
were independently reviewed with no remaining findings.

### All local gates pass on integrated source

Source e0f50e7a27 integrates landing 922b5fdc89. Final proofs:

- Bans, workspace formatting, workspace/all-target Clippy and Wasm Clippy PASS.
- 85 Kraken native tests; 1,354 workspace tests PASS.
- 235 harness tests; 75 real-Wasm boundary tests; 45 host tests PASS.
- Root typecheck 44/44, lint 28/28, test tasks 27/27 PASS. Bounded workers and
  serial task scheduling resolve the unrelated cold-import timeouts without
  changing test timeouts or source. Safe-cex suite: 6,971 tests per task.
- Live-recorded Kraken public PASS: 9,641 frames, 207 markets/tickers, two books;
  main-thread CPU TS/Rust 1,697.8/417.9 ms (4.06x), steady 1,274.4/276.9 ms
  (4.60x), process 1,881.9/562.1 ms (3.35x), retained 18.3/2.1 MB (8.76x).
- Single-venue browser Wasm 851,373 bytes, below 1,258,291-byte budget.
- Kraken private correctness PASS on both single and combined builds: 49 frames,
  13 commands, 11 signed writes; fresh challenge after reconnect and dedup replay.
- Combined build and Binance regression gates PASS: public CPU 258.2/138.5 ms
  (1.86x), retained 19.0/3.0 MB (6.25x); private correctness 14 frames.

Private proof is scripted documented traffic, not a funded live account. No
credentials committed. All five phase reviews and the landing integration review
accepted. Next: PR into feat/rust-rewrite, fresh local code-correctness reviews
posted on the PR, address findings, green CI, merge, fresh follow-up branch.

### PR #1323 code-correctness review

Opened https://github.com/Tealstreet/tealstreet-sdk/pull/1323 into feat/rust-rewrite
at f8037544e8 after all local gates passed. Three fresh independent adversarial
code-correctness reviews posted on the PR. History/oracle review has no findings
(18 history / 235 harness tests independently pass). Private review reproduced
local ACK timestamps suppressing native fill progress. Public review reproduced
omitted-zero funding and frozen 24-hour statistics; both TS defects also occur
in Rust. Before/after regressions and follow-up reviews are in progress.

CI secret scan flagged only the deterministic base64 replay fixture whose decoded
value is the public literal replay-secret, paired with replay-key. Add an exact
historical commit/path/rule/line fingerprint exception; no broad rule/path bypass.
Both PR-range and initial-push-range local scans now pass (9 / 348 commits).

Public review fixes accepted: complete native tickers reset omitted zero funding
and update change/open, volume, quote volume and open interest. Both native
regressions failed before and pass after (13 public tests). The oracle derives
only those expected fields from recorded WS payloads and excludes subscription
ACKs. Independent follow-up: original isolated proofs plus ACK proof 3/3 and
harness 236/236 pass; units, change precedence and unrelated field preservation
accepted with no remaining scoped findings. Listed as deliberate TS corrections.

Private review fixes accepted: merge monotonic native cumulative fills across
local ACK arrival clocks, preserving accepted economic fields and terminal fences.
Active-order provenance prevents ambiguous remaining quantities from inventing
progress after quantity edits, including subsequent price ACKs and shrinking
sizes. Native timestamp floors survive local ACKs and advance only from native
REST/WS/execution observations. Terminal and snapshot removal clean up provenance.
Five regression tests cover deltas, snapshots, REST, executions, edits and source
floors; the original reproduction and new native-floor case failed before and
pass after. Final venue tests 92/92 and all-target Clippy pass. Independent final
review: 22 account tests pass; no remaining private/trading/signing findings.
All fresh code-correctness findings and accepted follow-ups are posted on #1323.
Final affected workspace/Wasm/replay checks are being repeated before push.

### Review fixes: all affected gates green

Final source 7b8fc04d9a: 92 Kraken / 1,361 workspace tests, 236 harness tests,
75 real-Wasm tests and 45 host tests PASS. Bans, workspace format, workspace
all-target Clippy and Wasm Clippy PASS. Root checks remain 44 typecheck, 28 lint
and 27 test tasks green; no TS source changed in the review fixes.

Rebuilt single-venue public PASS: CPU TS/Rust 890.8/139.0 ms (6.41x), steady
670.5/74.3 ms (9.03x), process 1,005.1/257.4 ms (3.91x), retained 17.8/2.1 MB
(8.67x). Same 9,641-frame live fixture, 207 markets/tickers and two full-depth
books. Browser Wasm 858,567 bytes, budget 1,258,291. Kraken 49-frame private
correctness PASS on both single and combined builds. Binance public/private
PASS: public CPU 183.7/83.4 ms (2.20x), retained 18.9/3.0 MB (6.29x).
Final local secret scan passes over the PR range (12 implementation commits).

All code-correctness review findings and accepted follow-ups are posted on #1323;
final native-source floor independently reviewed with 22 account tests passing.
First CI also exposed a WOO steady-state timing miss (17.9/18.9 ms); verify its
unchanged strict gate in the updated CI run. Push reviewed fixes, wait for all CI
to pass, then merge #1323 into feat/rust-rewrite and cut a fresh follow-up branch.

### KuCoin landing integration

Base advanced to 373cc09864 while final reviewed head CI was running, creating
registration/build conflicts. Adopt incoming layout and union Kraken with KuCoin:
Cargo.lock retains incoming packages plus the exact Kraken package/dependency;
Wasm feature defaults retain incoming multiline form; build aliases and imports
include both. Preserve incoming request/lot/generation contracts alongside the
Kraken sticky protocol oracle, ticker corrections and ACK progress fixes.
Independent integration code-correctness review and all affected gates follow.

KuCoin integration review accepted with no findings; independent harness 258/258
passes. Final merged-source local gates all PASS: 92 Kraken / 1,460 workspace
tests, 258 harness, 76 real-Wasm and 45 host tests; bans, formatting, workspace
all-target Clippy, Wasm Clippy; root 44 typecheck / 28 lint / 27 test tasks
(safe-cex 6,972 tests per task). Kraken public CPU 1,741.7/354.6 ms (4.91x),
steady 1,284.3/211.5 ms (6.07x), retained 18.3/2.1 MB (8.77x); same live fixture
and 858,567-byte browser Wasm. Kraken private PASS on single and combined builds.
Binance public CPU 288.1/142.0 ms (2.03x), retained 19.0/3.0 MB (6.25x), private
correctness PASS. No Kraken implementation changes in this landing merge.
All phase, PR and integration code-correctness reviews accepted and posted.
Only hosted CI acceptance/merge/fresh-branch actions remain.

### AsterDEX landing integration

Final KuCoin-integrated head a244a10542 passed all 27 hosted CI checks, including
Kraken public/private and the unchanged WOO strict timing gate. Hosted Kraken
public medians: CPU 760.8/110.0 ms (6.92x), steady 560.6/51.1 ms (10.98x),
retained 17.6/2.1 MB (8.58x); node Wasm 858,577 bytes. Before merge, base advanced
to d9aa597fd3 (AsterDEX), creating registration conflicts. Integrate incoming
layout, host HTTP deadlines/routes, typed currency/STP/cancel references and lazy
Aster crypto initialization. Union both venue entries, constructors, build and
feature sets, TS corrections, exact fixture fingerprints and roster changes.

Independent integration code-correctness review found Kraken collateral coins
would be stripped by the new balance projection. Dedicated compareBalanceCoins
opt-in restores both TS/Rust comparisons without enabling inverse sizing extras.
Kraken rejects the new unsupported per-order STP control across the whole batch
before writes and initializes the shared field in protective replacements.
93 Kraken tests and 268 harness tests pass; independent review reran harness
268/268 and accepted all integration fixes with no remaining findings.

Push this reviewed merge promptly while full affected local checks run in
parallel with hosted CI. Post final gate results on #1323; avoid another docs-only
CI restart. Merge only when the final head's hosted checks and local gates pass.
