# Orderly family Rust port

Branch: `feat/rust-orderly`, created from fetched `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md` from landing.
Full auto; no master, stash, rebase or force-push. PR targets `feat/rust-rewrite`.

## Scope and design

One `safe-cex-venue-orderly` crate under `sdk-rs/venues/orderly` with a typed
broker option. Registered aliases: kodiakfi, mode, salsadex, tealstreet-dex,
what, woofi. All share the existing EVM REST/WS hosts and Tealstreet broker
UUID; regular order tags respectively TEALKODIAK, TEALMODE, TEALSALSA,
TEALSTREET, TEALWHAT, TEALSTREET. Testnet selects its own hosts and broker UUID.
No TS method transcription or generic JSON values. Borrowed typed payloads,
streamed large REST arrays, sans-I/O state machines, ED25519 from crypto.

## Phases

1. Public data: crate, borrowed wire types, market/ticker seeds, stream routing,
   socket watchdog/reconnect, refcounted candles/books and snapshot buffering,
   FetchCandles, native documented-payload tests. Audit official timestamp and
   book continuity contract rather than invent sequence numbers.
2. Private data: exact base58-seed ED25519 signing, startup validation,
   margin defaults/account leverage, positions + balance, normal/algo orders,
   authenticated WS, read reconciliation, stale guards and per-execution fills.
   Preserve simultaneous Cross/Isolated identity through optional compatible
   frame data; ensure failed reads never clear last-good state.
3. Trading: full-batch preflight, caller IDs verbatim on one native order,
   broker attribution, normal and positional TP/SL/brackets with sibling repair,
   strict stable-ID native amend, normal/algo bulk cancellation without repair,
   per-symbol margin/leverage, fixed OneWay. No automatic write retries.
4. Queries: typed documented history/trade/candle endpoints in engine JSON
   result shapes, bounded paging/ranges and honest unsupported surfaces.
5. Integration and gates: wasm and SDK aliases/options, recording/replay config,
   strict signature/body matching private tapes with all broker/testnet variants,
   live read-only 20-second public recording for two books, fixture, correctness
   and CPU/memory gates; Binance and all existing venue regressions; bans, fmt,
   clippy, native/wasm tests, SDK typecheck/lint/tests. Remove provably unused
   replaced helpers and resolve TS removal vs replay oracle without losing gates.

Every phase: plan audit, implement, meaningful checks, independent sub-agent
review, fix and recheck, commit this progress. Deep planning only for actual
Orderly risks: dual-margin identity, REST-proven amend, positional sibling jobs,
and timestamp-only books. Genuine TS bugs are declared in gate KNOWN_TS_FIXES,
commit messages and PR, with independent expected results.

After all gates: fetch/merge landing, revalidate, push, PR, local adversarial
sub-agent reviews posted to PR, fix/review until clear or diminishing returns,
wait every required CI green, merge to landing, cut clean follow-up branch.
Report PR, exact correctness/performance numbers, TS fixes, unfinished work.

## Progress

- Planning: brief/spec and nearest docs read; six aliases and tags confirmed.
- Independent account, trading and harness plan audits in progress.
- Early audit: store keys positions by ID, but frame currently reconstructs IDs;
  optional identity extension required for simultaneous margin modes.

### Public phase

Implemented public crate and typed broker options, streamed market/ticker
seeds, four aggregate streams, refcounted early subscriptions, candle reads
and stream, heartbeat and reconnect, full-depth snapshot buffering with exact
integer predecessor timestamps and generation-correlated snapshot requests.

Official current docs omit the incremental topic; a read-only live probe
confirmed `@orderbookupdate` contains `data.prevTs`. Snapshot `data.ts` is the
same book clock. First buffered update bridges `prevTs <= snapshot.ts < ts`,
then predecessor must equal the last applied timestamp. Gaps/malformed frames
hide the stale book and resnapshot. No floating-point sequence conversion.

Public live sockets reject invented identifiers with normal close1000. The
official js-sdk `packages/net/src/ws/ws.ts` uses COMMON_ID
`OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY`; this identifier works keylessly.
Documented WS `event:request` orderbook snapshots returned full depth (372
bids/356 asks in probe), avoiding TS's private signed REST snapshot in public
mode. Source: https://github.com/OrderlyNetwork/js-sdk/blob/main/packages/net/src/ws/ws.ts
and https://github.com/OrderlyNetwork/documentation-public/blob/main/build-on-omnichain/websocket-api/public/request-orderbook.mdx .

Independent phase review found and fixed startup subscription loss, quiet-book
snapshot starvation, malformed update retaining a stale book, missing candle
timestamps/OHLC bounds, unsupported intervals, and snapshot symbol mismatch.
Re-review found no phase blockers; also tightened null timestamps and missing
depth data. Native tests: 11 passing; fmt/clippy/bans passed. SDK wiring and
record/replay performance checks remain in the integration phase. Genuine TS
book continuity/public snapshot fixes will be declared in gate/PR.
