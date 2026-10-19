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
- Phase 3: pending.
- Phase 4: pending.
- Phase 5: pending.

Standing overseer authorization (2026-10-20): resolve future landing-branch
merge conflicts while keeping its layout; never stash, use WIP commits.

After all gates: merge origin/feat/rust-rewrite into lane, push, open PR into
feat/rust-rewrite, adversarial local sub-agent review with findings posted to
PR, fix and repeat; merge PR and create a fresh follow-up branch. No master
changes or release automation.
