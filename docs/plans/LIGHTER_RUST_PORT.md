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
