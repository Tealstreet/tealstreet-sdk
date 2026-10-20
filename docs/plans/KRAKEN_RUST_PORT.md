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
