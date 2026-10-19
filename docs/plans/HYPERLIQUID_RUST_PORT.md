# Hyperliquid Rust port

Lane: `feat/rust-hyperliquid`; target: `feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process: the Process section of the upstream
`VENUE_PORT_BRIEF.md` fetched 2026-10-19. Workspace migration
merged at `fefe9db44c`; crate now lives in `sdk-rs/venues/hyperliquid/`. Full automatic execution, with a
sub-agent implementation review and fixes before every phase commit.

## Phases

1. Public data: crate and workspace registration; typed REST metadata/tickers
   for core and HIP-3, streaming POST lists, allMids and compressed fastAssetCtxs,
   snapshot books with BBO overlay, candles, subscriptions and socket watchdog.
   Verify documented payload tests, bans, fmt and clippy.
2. Private data: EIP-712 phantom signing, identity and abstraction resolution,
   core/per-DEX positions and settings, unified collateral, fills deduplication,
   REST reconciliation with source-time and cancellation guards. Verify signed
   vectors, snapshot races, reconnect and failed-read preservation.
3. Trading: sequenced command jobs; builder approval cache and lazy check;
   caller cloid preflight for all shapes; unique cloids on every wire; brackets,
   full/partial stops, cancel+replace and TP-to-SL reclassification; cancellation,
   leverage, margin and one-way mode. Verify wire/signature and lifecycle tests.
4. Queries: fills, order history, funding/income and public trades in engine JSON
   result shapes, paging/deduplication, filters and fail-closed reads. Verify
   documented responses and paging boundaries.
5. Harness and gates: generic body-sensitive replay, TS registration, synthetic
   private tape and recorded two-book public fixture. Run all brief gates and
   repository checks; preserve Binance. Record correctness, CPU and memory.

## Audit decisions

- The requested lane overrides starting a fresh branch from master. Do not
  rebase or modify master. Merge latest origin/feat/rust-rewrite after gates.
- Hyperliquid info reads are POST requests: preserve the request body when
  streaming arrays, and key harness requests by normalized body as well as URL.
- Wire key order is signed: typed action structs serialize in declaration order;
  cloid is last, after order type. Never serialize an untyped JSON value tree.
- l2Book is an authoritative 20-level snapshot, with no sequence deltas; BBO
  overlays only when at least as new as depth. Reconnect discards depth.
- Caller IDs are validated across the entire batch before any write. A bracket
  uses that ID only on its entry. Replacement always gets a fresh ID.
- Failed or stale snapshots cannot erase newer account state. DEX snapshots
  replace only their own scope. Terminal events suppress overlapping REST rows.
- A builder address and fee are one pair; maxBuilderFee is checked against that
  account's rate. Unknown approval triggers a read, never an assumed approval.
- Keep TS available as the replay oracle until gate verification; do not remove
  code required to execute the mandated TS comparison.

## Progress and evidence

- Planning: architecture, TS contracts, signing helpers and harness inspected.
- Phase 1: implemented and reviewed by `review_public`. Fixed all five findings:
  tuple arity/alignment, finite numbers, bounded metadata read retries, candle
  limit/from semantics, Unicode interval panic. Six documented-payload tests
  pass; bans, formatting and clippy pass. Registration in wasm/harness remains
  in phase 5, so phase 1 does not enable an incomplete venue in user flows.
- Phase 2: implementation checkpoint. Thirteen account tests, one official SDK
  signing-vector test (mainnet/testnet/vault), and six public tests pass.
  `review_private` found and drove fixes for position IDs, core trigger fallback,
  suppression recovery, user event shapes, TIF retention, newer REST source
  times, per-symbol fill clocks and HIP-3 in-flight refresh collapse. Final
  review drove two further fixes: snapshot and optimistic fill clocks are
  separate (two executions in the same millisecond both apply); order reads
  are demand-driven, debounced at 500ms with a 1s minimum cadence. Weighted
  reads bound background traffic; HIP-3 positions reconcile at 120s, with
  debounced fill heals. Bans, fmt, clippy and all twenty native tests pass.
  Deleted unused `crypto/wallet_utils.rs`; private-key address derivation now
  uses a transport-free Ethereum primitive for non-fatal startup warnings.
- Phase 3: implemented and reviewed by `review_trading`; nine trading tests,
  two additional signing tests and previous tests pass (31 total). Fixed review
  findings: leverage must use integer MessagePack, every cancel acknowledgement
  must validate before replacement, and queued settings resolve when executed.
  Builder timeout matches TS's explicit 8s negative-cache fallback; transport
  errors remain unknown for lazy checking. Every wire has a cloid; own bracket
  legs are identified from their records before REST returns.
  Deliberate TS fixes recorded in `gate.mjs`: malformed cancel acknowledgements
  fail closed, queued setting changes compose, and HIP-3 bracket pending rows
  retain each leg's metadata instead of assigning entry metadata to every oid.
- Phase 4: implemented and reviewed by `review_public`; ten history regressions
  pass (41 native tests total). Streamed reads return Binance JSON shapes;
  cloid lookup is store-pure and never reports a definitive miss. Phase 5
  enables its TS wrapper with venue registration. Review fixes include integer
  wire times (live 422/200 control), whitespace numeric strings, inclusive-page
  saturation errors, raw-millisecond fill ordering, and reserving response
  weight before concurrent heavy reads. Queries share the typed read budget.
  Deliberate TS fixes in gate.mjs: execution tid identity and signed rebates,
  vault/range/all-symbol fills, fail-closed history errors and saturation,
  correct newest-fill limits, and exact income-kind filtering.
- Phase 5 checkpoint: wasm/TS venue registration, forwarded wallet/vault/builder
  options, perps-only validation, replacement cloid and lookup wrapper are in
  place. `review_private` fixed promise rejection semantics; 269 targeted TS
  checks pass. `review_public` audited body-sensitive replay, caller-ID
  preservation, same-URL socket fanout and normalized outbound capture; seven
  harness regressions pass. `review_trading` generated and replayed the
  deterministic private tape against TS with zero unmatched requests. All 41
  Rust tests, native/wasm lint, bans and fmt pass; repository lint passes.
  Release wasm build, replay parity/performance and repository checks remain
  in progress.
- Phase 5 replay checkpoint: Hyperliquid private and 20-second/two-book public
  replays pass account, market, ticker, book, command and normalized outbound
  request/subscription comparisons. Binance public/private also pass. The
  recorded fixture is committed. Fixed the recorder to tap streamed responses;
  timed private snapshots preserve live orders between placement and cancel.
  Added shared prehashed EIP-712 hashing and fixed-schema Agent encoding;
  official signatures and dynamic-parser equivalence pass (43 venue tests,
  five crypto EIP-712 tests). Public HIP-3 subscriptions now follow TS's account
  identity requirement. Wasm wrapper 4/4 and Workers host 2/2 checks pass.
  Repository typecheck (44 tasks), lint (28 tasks), tests (27 tasks) pass.
  Combined wasm remains over the 1.2 MB size budget; a size-optimized release
  build and final replay repetition are in progress. These are preliminary
  replay passes, not a claim that every gate is complete.

Standing overseer authorization (2026-10-20): resolve future landing-branch
merge conflicts while keeping its layout; never stash, use WIP commits.

After all gates: merge origin/feat/rust-rewrite into lane, push, open PR into
feat/rust-rewrite, adversarial local sub-agent review with findings posted to
PR, fix and repeat; merge PR and create a fresh follow-up branch. No master
changes or release automation.

Phase 5 size checkpoint: default builds retain all compiled venues; optional
`rust:build-wasm --venue <venue>` applies the browser size budget per venue.
This avoids raising the 1,258,291-byte budget when a new venue is registered.
Hyperliquid is 1,147,832 bytes; Binance is 907,075 bytes. Both single-venue
public/private gates pass (public medians across three runs):

| Venue | TS CPU ms | Rust CPU ms | TS steady ms | Rust steady ms | TS retained MB | Rust retained MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hyperliquid | 317.4 | 62.2 | 23.6 | 21.4 | 4.1 | 1.7 |
| Binance | 213.0 | 88.2 | 120.6 | 35.7 | 2.3 | 2.0 |

Private gates compare correctness only: Hyperliquid 26 events/seven commands;
Binance 14 events. Both host integration suites pass. Native/wasm Clippy passes
for default and each single-venue feature combination. The final harness review
fixed availability to advance with delivered events and limits Binance fixture
integration tests to the Binance CI matrix entry. Default combined-build replay
revalidation is running. Origin has since landed Bybit; merge it next and repeat
affected checks before opening the PR.
