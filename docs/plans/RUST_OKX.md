# OKX Rust port

Scope: `okx`, `okxeu`, `okxdemo`, `okxeudemo`, USDT/USDC linear SWAPs.
Implement the sans-I/O Venue contract; retain TS as the replay oracle until
all gates pass. EEA X-Perps and inverse swaps are outside this lane.

## Phases and verification

1. Public data: typed streaming instrument/ticker seeds, public and business
   sockets, watchdog/reconnect, seqId/prevSeqId books, candle stream/reads.
   Preserve OKX error-envelope metadata while streaming to reject 200 errors.
   Verify documented snapshot/diff/reset/gap, conversion, reconnect and errors.
2. Private data: passphrase plumbing, ISO REST and epoch WS signing, private
   and business login/subscriptions, account/position/order events, deduped
   execution fills, atomic REST reads with stale-snapshot guards, settings.
   Verify signing, demo/EEA hosts, finality, malformed and failed snapshots.
3. Trading: sequenced write jobs with no automatic write retry; preflight whole
   batches, verbatim caller IDs, contract lot sizing, split/batch constraints,
   bracket protection priority, regular/algo amend/cancel, account settings.
   Verify requests, partial failures, caller-ID rejection before any write.
4. Queries: typed fills/history/income/public-trades mappings and bounded
   cursor walks, candles with historical fallback. Verify JSON shapes and units.
5. Gates: harness registrations, documented private tape, real 20-second
   two-book recording and fixture; compare state, commands and requests; OKX
   public CPU/memory below TS; all Binance regression gates, bans, formatting,
   clippy, tests and package checks. Record numerical evidence here.

At each phase: audit plan, implement, verify, independent sub-agent review,
fix, commit progress. Full auto authorized. After all gates: merge current
origin/feat/rust-rewrite, rerun relevant gates, push/open PR to that branch,
post adversarial review findings, fix/repeat, merge PR and cut follow-up branch.
Never touch master, rebase, or force-push.

## Progress

- Planning: read port/rewrite spec, root/package/exchange guidance and TS paths.
- Public implementation in progress. Engine's streamed envelopes need opt-in
  metadata retention; existing Binance streaming behavior stays compatible.

### Public phase review

- Independent review found malformed seed/depth acceptance and streamed-array
  grammar/metadata-overflow gaps. Fixed before committing the phase; tests cover
  malformed envelopes (including errors after data), sequence resets, recovery,
  full-depth retention, both socket watchdogs, and ignored retired sockets.
- Engine + OKX native tests: 17 passed. OKX all-target clippy and bans passed.
- TS bug fixed: OKX filtered books against a stale last trade, deleting valid
  depth permanently. Removed from TS oracle too; explicit KNOWN_TS_FIXES note
  and independent TS/Rust regressions. Candle time bounds/history fallback
  remain in the queries phase.
- Tooling: corepack was absent from PATH; installed a temporary tooling copy
  outside the repository at /tmp/okx-port-tools, preserving repository deps.
- Public review follow-up: rejected incomplete ticker seeds and invalid UTF-8,
  suppressed repeated malformed-frame recovery, added variant routing tests.
  WASM registration builds natively; SDK passphrase forwarding is wired.
- Package typecheck passed. Package lint and focused regression are running.
- Checkpoint requested by overseer before workspace-layout merge (b32847fca6).
- Merged the overseer's nested workspace layout. The only merge overlap was
  Cargo workspace paths/members; applied the explicit venues/* layout and
  moved OKX to sdk-rs/venues/okx. No venue behavior conflict.
