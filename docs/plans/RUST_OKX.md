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

### Private phase

- Implemented ISO UTC REST signing of exact query/body bytes and passphrase,
  plus epoch-second private/business login, auth deadlines, watchdog recovery,
  and login-triggered reconciliation.
- Typed snapshots join equity and positions, page all five order categories,
  retain per-market settings, reject failed/malformed reads, and preserve newer
  order/position/equity pushes. Terminal guards also reject late live events.
- Independent private review found malformed-position acceptance, hedged ID
  formatting, auth timeout, late-live finality, empty-push balance and metadata
  retention bugs. Fixed with regression tests; 19 OKX native tests and clippy
  pass. Engine tests passed at the public checkpoint (11 tests).
- Crypto audit: removed wallet_utils and its module export; repository-wide
  search found no callers. Retained uuid (Binance uses it).
- Trading and queries remain to implement. Harness integration is being
  prepared in a disjoint file scope by the public-phase reviewer.

### Trading and query phases

- Trading reviewed independently and fixed: native OCO amend preserves sibling
  protection and selected execution price; each bracket lot acquires SL/TP
  before the next entry; decimal-safe splitting honors a caller ID exactly once;
  all preflight finishes before writes; no write retry; failed cancellation
  prevents replacement; partial standalone cancellations continue other batches.
  Settings ACKs invalidate pre-write account joins. 17 trading tests passed.
- Queries implemented in a disjoint scope and independently reviewed: streamed
  fills/history/bills/public trades, contract conversion, dedupe, exact decimal
  cursors and bounded paced pagination. Review caught second-based fill bounds
  and missing optional execution fields; fixed. 15 history tests and all-target
  clippy passed.
- Candle bounds match TS limits (300 recent/100 historical), preserve explicit
  from semantics, fall back to history and subtract calendar months in UTC.
  TS oracle corrected for public trades, UTC month bounds and full depth.
- Harness matches OKX write bodies, preserves caller IDs, permits repeated reads
  but not duplicate writes, supports all variants, and includes documented
  private/business events, two deduped executions, regular/algo commands and
  public trades. TS-only OKX and EEA-demo replay: zero unmatched requests.
- Full release WASM build, public recording/performance gates and Binance
  regression gates are in progress. No gate numbers claimed yet.

### Harness phase checkpoint

- Recorded 20 seconds of live OKX traffic and committed a trimmed public
  fixture: 8,902 frames, 494 linear stablecoin markets/tickers, two full books.
  First complete public gate: identical output; TS/Rust median CPU 347.8/248.6
  ms, steady CPU 262.9/160.4 ms, retained memory 1.7/1.6 MB. Final gates will
  be repeated after the settings fix and release-size optimization.
- Strengthened documented private tape: 12 commands, 29 frames, exact write
  bodies, verified login HMAC/passphrase, staged leverage read and meaningful
  final private pushes. Public-trades/fills/history/income/candles are compared.
- Private replay caught settings refresh dropping terminal orders; refresh now
  reconciles equity/positions without replacing orders, matching TS settings.
- TS business socket bug fixed: keyless public subscriptions skip private
  login/topics. Documented in KNOWN_TS_FIXES and covered by regression.
- Fixed the recorder to tee streamed REST bodies rather than decode an async
  iterator as bytes. Moved the WASM test out of the runtime tree, matching the
  landing branch's independently committed boundary fix.
- Repository typecheck and lint passed before the final keyless correction.
  Full tests: 452 safe-cex suites passed; one unrelated HyroTrader suite timed
  out under parallel runs. Serial repository retry and independent harness
  review are in progress. No completion or landing claimed yet.

### Final audit corrections

- Independent reconnect review found pre-disconnect REST reads surviving login.
  Account and order generations now retire those reads on authenticated reopen;
  settings-only refresh preserves the active order walk. 10 account tests pass.
- Effective keyless/partial-credential public mode now reaches startup readiness;
  unsupported remote/payload-only private signing fails before transport starts.
  Targeted WASM exchange tests: 9 pass; package typecheck/lint pass.
- Harness audit tightened command deadlines, actual subscription coverage,
  login seconds freshness, every replay run and public command result comparison.
  Mutations and final fixture gates are being checked.
- Size experiment: size-oriented LLVM profiles increased output. Retained the
  original release optimization and removed wasm-bindgen debug name/producers
  sections instead; this saves over 500 KB without changing instructions.

### Final venue gates (landing merge f52132c2f2)

| Replay | Frames | Markets/tickers | Books | TS/Rust main CPU | TS/Rust steady CPU | TS/Rust retained MB | Result |
| --- | ---: | --- | ---: | --- | --- | --- | --- |
| OKX public, 3 runs | 8,902 | 494/494 | 2 | 1287.9/1152.6 ms | 1034.2/887.4 ms | 2.3/2.2 | PASS |
| Binance public, 3 runs | 871 | 917/783 | 3 | 238.0/92.1 ms | 142.8/44.0 ms | 2.3/2.0 | PASS |

- OKX private: all four aliases PASS, each 54 records, 29 frames, 12 command
  results, 2 markets/tickers, 2 executions and zero unmatched requests. Exact
  account/query output, fill prices and expected writes match. Private tapes
  gate correctness, not performance. Binance private: PASS, 14 frames, 3 command
  results, 2 executions and zero unmatched requests.
- Release build PASS; binary 1,074,448 bytes against unchanged 1,258,291 budget.
  Bans, workspace fmt, OKX all-target clippy, WASM clippy and 67 engine/OKX
  native tests PASS. Post-merge WASM exchange tests: 9 PASS; Workers host: 2 PASS.
  Repository typecheck (44 tasks) and lint (28 tasks) PASS; post-merge package
  typecheck/lint PASS. Root test retry with bounded workers and a 20-second
  per-test deadline remains running under concurrent machine load.
- Final independent harness review and negative probes PASS: missing private
  topic, wrong instrument, unsubscribe, command timeout, unused write, bad login
  signature and stale/millisecond timestamps all fail instead of passing parity.
- Next: finish repository test retry, push PR into feat/rust-rewrite, adversarial
  review/fix loop, merge and create a clean follow-up branch.

### PR adversarial review, round 1

- PR: https://github.com/Tealstreet/tealstreet-sdk/pull/1302, into feat/rust-rewrite.
  Three independent reviews ran against the PR diff; findings are posted there.
- Root repository tests finished: 27 tasks PASS, including 453 safe-cex suites /
  6,493 tests through each of test-ci and test-unit. Used two Vitest workers and
  20-second per-test timeout under machine contention; no code/test changes to
  HyroTrader were needed.
- Trading audit reproduced two inherited TS safety defects: positive protective
  dust became close-all after rounding, and standalone protective limit amends
  replaced the limit execution price with market. Dust now rejects the whole
  batch; ambiguous standalone protective limit amends fail before cancellation.
  Explicit zero close-all and native OCO amendment retain their semantics. Both
  corrections are listed in KNOWN_TS_FIXES; 20 trading tests PASS.
- Adapter audit found missing OKX caller-ID helper rule and dropped size in
  combined price/size updates. Account audit found lost out-of-order executions
  and missing outer risk rows incorrectly committed as empty equity. Fixes and
  targeted regressions are underway before another replay/build/review round.

### Review fixes verification checkpoint

- All seven reproduced round-1 findings are fixed. Native tests: 75 PASS
  (64 OKX + 11 engine); WASM adapter: 14 PASS; TS oracle/depth: 5 PASS.
  Bans, fmt, native/WASM clippy and package typecheck/lint PASS.
- Account/history cross-review PASS. Adapter cross-review PASS after replacing
  an overstrict random minted-ID length assertion with the actual allowed
  broker-prefixed alphanumeric range. Trading cross-review is finishing.
- Rebuilt release binary: 1,074,930 bytes; full replay gates are running again.
  No change to release optimization, binary budget or secret-scanning policy.

### Landing integration with Bybit

- Landing advanced to20dd66dddb while PR review ran, so merged it under the
  standing conflict-resolution authorization. Preserved the nested workspace,
  Bybit venue/fixtures/CI, both host registrations, and all SDK helper tests.
- Three disjoint resolutions: host/manifests/CI, engine streaming, and harness.
  Root combined both SDK client-ID rules and the two WASM test sets. Package
  typecheck/lint pass. Independent cross-reviews and rebuilt three-venue gates
  are in progress before the merge commit is considered ready.
- Engine keeps Bybit nested Path/error previews plus OKX strict grammar/UTF-8/
  overflow handling; only opt-in Path/EnvelopeField modes return metadata.
  Harness retains each venue's scoped corrections and exact write matching.
- Added an assertion that acknowledged leverage survives a malformed account
  refresh. The empty-risk mutation now retains all funds/positions; TS alone
  rolls acknowledged leverage12 back to10 after its failed refresh. Listed this
  intentional Rust correction without normalizing the mismatched setting.
- Landing budget is now1,468,006 bytes for the aggregate browser engine. This
  lane preserves that budget and still removes debug name/producers metadata.

### Integrated final gates (fe226c8211)

All local gates PASS after merging the current landing branch. Three independent
integration cross-reviews PASS; findings and resolutions are posted on PR1302.

| Public replay (3 runs) | Frames | Markets/tickers | Books | TS/Rust main CPU ms | TS/Rust steady CPU ms | Memory improvement |
| --- | ---: | --- | ---: | --- | --- | --- |
| OKX | 8,902 | 494/494 | 2 | 626.6/588.3 (1.07x) | 489.9/445.0 (1.10x) | 1.02x |
| Binance | 871 | 917/783 | 3 | 169.4/60.2 (2.81x) | 96.5/20.5 (4.70x) | 1.12x |
| Bybit | 8,742 | 912/912 | 2 | 174.0/118.5 (1.47x) | 123.6/62.0 (1.99x) | 1.36x |

- All public outputs identical. All six private tapes PASS: OKX aliases each29
  frames/12 commands, Binance14 frames, Bybit18 frames; no unmatched requests.
- Release WASM1,444,636 bytes (landing budget1,468,006). Workspace bans/fmt,
  locked all-target clippy, locked native tests, and locked WASM clippy PASS.
  OKX64 native tests + engine14 tests; combined SDK WASM16 and Workers host2
  tests PASS. Post-integration package typecheck/lint PASS.
- Initial full repository verification PASS(27 tasks); targeted fixes and merge
  have their own passing regressions. PR must now pass the updated CI before
  merge; no requested implementation work remains beyond landing/CI handling.

## WebSocket default-port migration (2026-10-21)

OKX's [September 30 changelog](https://www.okx.com/docs-v5/log_en/)
retires public-internet WebSocket port 8443 on October 31, 2026. Remove the
explicit port in Rust and TS global/EEA live/demo URLs and routing metadata,
preserving every host/path. Update the URL spec and transport expectations;
verify focused OKX/Binance and TS transport checks, review, then merge on green
CI after the overseer's hold lifts.

Implemented in Rust, TS and relay metadata for all four variants. Historical
replay URLs alias only the four exact OKX socket hosts, the retired port and
the three documented channels; recorded public fixture bytes stay intact.
New private tape URLs and smoke allowlists use default TLS. The four private
tape SHA bindings change with those URLs; diagnostic keys and reasons remain
exactly the same. Offline generation verifies each new tape is byte-identical
to the old tape after removing only `:8443` from socket URLs.

Validation: 443 OKX/Binance native tests, 15 TS transport tests, 162 focused
harness tests, package typecheck, touched ESLint, Rust fmt/strict Clippy/bans
pass. The transport regression failed against the old URLs before the fix.


CI exposed the shared native routing golden fixture retaining six old relay
URLs; corrected only those ports. Its TS generator comparison and native
selector-consumer regression both pass.
