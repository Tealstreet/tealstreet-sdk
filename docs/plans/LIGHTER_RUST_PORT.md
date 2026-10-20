# Lighter Rust port

Branch: `feat/rust-lighter`; base and PR target: `origin/feat/rust-rewrite`.
Scope: `lighter` perpetuals. Follow VENUE_PORT_BRIEF and RUST_REWRITE. Full auto,
with a sub-agent implementation review at the end of every phase. Never stash,
rebase, force-push, or touch master. Keep incoming landing layout on integration.

## Phases

1. Public data: typed streamed market metadata with orderBooks fallback, ticker
   REST seed and WS updates, full books and resync, candles/backfill. Both sockets
   use engine timers for watchdog/reconnect rather than TS-owned close loops.
2. Private data: account/index resolution, signer-backed auth, account_all and
   account_all_orders, atomic REST reconciliation, settings, source/revision
   guards and terminal fences. Emit one fill per execution, never order totals.
3. Trading: validate all caller client IDs before any write (Lighter rejects
   every caller ID); serialize nonce acquisition/sign/submit jobs; scaled order
   integers, conditional guards, brackets, batches, cancel/amend, leverage and
   margin settings, fixed one-way mode. No retry of an uncertain write.
4. Queries: history/fills/income/public trades and candle reads in engine JSON
   contracts, verified paging/ranges/units and failure preservation.
5. Harness and gates: actual TS signer vectors for zk/Poseidon/Goldilocks;
   remove only provably unused crypto helpers; register wasm/wrapper/harness/CI;
   deterministic private tape, recorded public fixture, correctness/CPU/memory
   and single-venue size gates. Re-run Binance gates. Push PR into landing,
   post adversarial sub-agent reviews, fix findings, merge only green CI and
   create a fresh follow-up branch.

## Venue-specific audit points

- Keep API key index distinct from account index; domain 304 mainnet/300 testnet.
- Auth token is signed padded ASCII expiry:account:key-index. Signed writes use
  PascalCase transaction JSON and fp5 integrator aggregation; compare both
  preimages and serialized transactions with existing independent TS/WASM corpus.
- Every internal timestamp ID stays exact u64; client-order-ID refusal applies
  to every order in a batch, including brackets and conditional shapes.
- Builder attribution defaults to account 158385, maker 0/taker 250; preserve
  tier and approval checks and validate expiry and both fee caps.
- Full snapshot success is required before removing any absent row; late HTTP
  must not overwrite fresher WS orders, positions, balance or settings.
- Only execution rows emit fills; deduplicate execution IDs across both private
  topics/reconnects. FILLED order rows never emit cumulative quantity.
- Spot metadata is filtered. WS optional ticker fields preserve prior values;
  funding converts hourly percentage points to 8h decimal, including TS floor.
- Public metadata/list reads stream one row at a time and keep typed data only.

## Progress

- Read latest remote brief, rewrite spec and package/exchange/Lighter guidance.
- Created fresh branch from landing with a clean worktree.
- Public phase in progress. Parallel bounded signer/account/trading work is
  isolated to separate source modules and tests; root owns public integration.
- Existing crypto independent corpus contains 92 transactions across four
  profiles, with captured Go-WASM hashes/signatures and TS transaction preimages.

## Known TS defects to declare in gate and PR

Pending verification: duplicate order-total/execution fill notifications;
partial successful order snapshots committed after a failed per-market read;
runtime integrator approval check does not validate fee caps required by docs.

### Public checkpoint

- Implemented typed streamed metadata/fallback, tickers, books, candles and
  engine timer watchdog/reconnect. Official WS contract confirms nonce chains
  use begin_nonce == prior nonce, never consecutive API-server offsets:
  https://apidocs.lighter.xyz/docs/websocket-reference#order-book.
- Public sub-agent review found nested stats, per-market ticker snapshot races,
  invalid book responses and pre-gap REST races; fixed each in root integration.
- Seven public tests and four signer tests pass; signer test covers 88 tx and
  four auth cases against independent TS/Go-WASM preimages/signatures/wire.
  Native library Clippy, format and bans pass. Independent review regressions
  continue alongside private phase.
- No Lighter/Poseidon/Goldilocks helper is unused: create/grouped/builder signing
  consumes every hash primitive. No speculative deletion of other lanes' crypto.

### Resume checkpoint

- Quota interruption left private/account and trading source, native regressions,
  wrapper/wasm registration and initial independent-signature harness work on disk.
  Commit these as work in progress before merging the landing branch; no stash.
- Private/trading cross-review identified settings write/read fences, late fills
  between cancel ACK and replacement, terminal ACK resurrection, and strict
  snapshot business-status/account validation. Fixes are in progress; not gated.
- Remaining: finish native verification and phase review; implement queries;
  finish strict harness/private tape; remove unused grouped/approval signer
  surface and any crypto helper that only that unused surface kept alive;
  public recording, all gates, PR reviews and green-CI merge.

### Private, trading and query phase review checkpoint

- Account: 23 native regressions pass, including atomic scoped order snapshots,
  source clocks for positions/orders, stale-close protection, accepted/uncertain
  settings barriers, sparse position payloads, available-balance progress during
  busy order streams, dual execution IDs and never-open socket handshakes.
- Trading: 19 regressions pass. Private/trading cross-review fixed source fill
  races before replacement, terminal ACK resurrection, queued settings and
  unknown-write barriers. Caller IDs reject before any write, including batches.
- Queries: 16 regressions pass. Independent account reviewer accepted native
  execution/ledger sources, exact IDs, cursor/range failure behavior, signed fee
  ticks/rebates, source-side PnL and shared official hyphenated order mapping.
- Signer adversarial review checked preimages/FP5/auth, entropy and key cleanup;
  corrected protocol bounds and current market IDs. Current active perps include
  IDs 4095..4099. Independent high-market vectors cover 4098 and 32767.
- Removed unused grouped/approval signer surface and the now-unused 32-byte
  Poseidon hash/compression facade functions and standalone dead fixture.
  Retained fp5 hashing, key derivation, signing and security verification vectors.
- Public review follow-ups fixed pre-market book unsubscribe, REST snapshots
  incorrectly authorizing WS nonce continuity, reconnect generations, and stale
  background candle replies. Live stats showed empty BBO strings; row-wise
  decoding now retains all other markets and handles absent quotes.
- Wrapper: 316 targeted tests pass (caller-ID suite plus Lighter wrapper).
  Six real-Wasm Lighter boundary tests pass. Root lint: 28 tasks passed.
- Private gate first PASS: 14 events, 8 commands, 11 signed transactions in 10
  HTTP writes. Exact auth/economic bodies and fresh signatures independently
  verified with Go-WASM; TS signer is real. Performance is informational for
  private tapes (155.1/189.6 ms TS/Rust; retained 1.2/1.0 MB).
- Public live recording: 20 seconds, 27,080 WS frames, both ETH/BTC books.
  Six-second fixture: 8,880 records. First public gate found stats decode bug;
  no normalizer added. Rebuild and public gate rerun pending.

Additional deliberate TS fixes to list in final gates/PR: native fee rate units
and signed rebates; execution history rather than cumulative inactive orders;
first-five-second live fills; by-market execution side/market identity;
approval expiry and both fee caps; current large-ID leverage; net-mode no-op;
hyphenated terminal statuses and late-read/late-WS source protection.

### Committable implementation checkpoint

Private, trading, query and signing reviews are accepted after fixes. Native
Lighter coverage is 78 tests (23 account, 16 history, 19 trading, 10 public,
5 public review regressions and 5 signing tests). Targeted all-target Clippy and
bans pass. Harness coverage is 158 tests, including 11 Lighter verifier/oracle
regressions. All four original public-review regressions are retained alongside
the new candle race test in its own file. Remaining work is final replay and
workspace gates, PR adversarial reviews, CI and merge.

### Harness phase accepted

Independent harness review found and fixed two P1 issues: command results could
reuse another command's acknowledged generated ID, and the fill oracle could
lose inherited market identity or ignore trades beside an empty fills array.
Results now bind exact ID order/cardinality to each command's acknowledged
creations. The oracle derives canonical identity only from recorded metadata and
execution rows, combines both sources and preserves lossless trade IDs. Query
and header auth are verified independently before matching. All 163 harness
regressions pass, including prior-command IDs, reversed/missing/extra batch IDs,
actual Tape command ordinals and forged signatures. Review accepted.

Public gate PASS: 8,875 replayed WS frames, 213 markets/tickers, two books;
TS/Rust CPU 590.3/434.2 ms (1.36x), steady 407.4/280.5 ms (1.45x), retained
3.0/1.8 MB (1.66x). Single-venue Wasm: 961,231 bytes against 1,258,291.
Private latest PASS: 14 events, eight commands, 11 tx in ten writes;
TS/Rust CPU 164.2/171.8 ms (private correctness only), retained 1.2/0.9 MB.
Landing advanced to Bitrue/transport integration; merge it before final proof.
