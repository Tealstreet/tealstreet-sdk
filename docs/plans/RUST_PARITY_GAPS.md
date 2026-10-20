# Rust venue parity gaps

Branch: `feat/rust-parity-gaps`, based on `origin/feat/rust-rewrite`
(`1f9e9a5206`). Follow `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`;
full automatic execution, independent review at each phase, stable commits,
PR into `feat/rust-rewrite`, review findings posted, merge only on green CI.

## Phases and verification

1. **Inventory and plan.** Compare current TS runtime capabilities with the
   merged Rust implementations. Record skips where TS itself lacks the feature.
   The roster-wide limitations audit belongs to `feat/rust-parity-sweep` per
   the user's subsequent scope note.
2. **Paradex delegated signing.** Trace both TS `remoteSigner` and
   `payloadSigner`, including JWT acquisition, exact signing material, headers,
   private-socket policy, failures, disposal and request correlation. Preserve
   the sans-I/O engine: host delegates signatures and returns typed results;
   venues name real URLs and retain existing account transport routing. Verify
   local/delegated equivalence, missing/failed/malformed/late signer replies,
   concurrent auth and writes, and absence of automatic write retries.
3. **Bitrue lookup and capability audit.** Implement the TS cache/open-order
   client-ID lookup with its non-definitive miss contract. Verify existing
   supported fills filters and incomplete-page handling. Pin absent TS
   order/PnL/funding histories and hedge/margin setters with runtime contract
   tests and document the skips. Check native request shapes, aliases and
   cache fallback; failed reads must not clear last-known-good state.
4. **KuCoin spot.** TS has a separate HF spot adapter. Add a typed spot venue
   in the existing crate, reuse signing and host routing, register the exchange
   in WASM/SDK/harness. Cover public markets/tickers/full books/candles, private
   balances/orders/executions and recovery, HF trading/amend/cancel and caller
   IDs, and precisely the queries/settings TS supports. Test spot units and
   market buys, token recovery, snapshot races and page traversal.
5. **Integration and gates.** Update docs for these three venues and add
   deterministic regressions. Run bans, fmt, Clippy,
   native tests, wasm32 Clippy, SDK typecheck/lint/tests, host/harness tests;
   build and replay public/private gates for Paradex, Bitrue, KuCoin futures
   and spot, plus Binance public/private. Record fresh public traffic for the
   touched venues. Standard egress comparison remains enabled. Keep the
   1,258,291-byte budget unchanged; use incoming per-venue build support and
   record exact artifact sizes and provenance.
6. **Landing.** Push, create PR into `feat/rust-rewrite`, post independent
   adversarial correctness reviews, fix/re-review, incorporate landing changes
   without rebase/stash, wait for exact-head green CI, merge, fresh branch.

Each implementation phase follows plan audit, implementation, verification,
independent implementation review and fixes before committing. Venue work may
proceed in disjoint scopes; shared host/engine/SDK/harness changes are coordinated.

## Inventory

- Paradex: merged local signing explicitly rejects delegated modes; compare
  current TS modes before designing the signature boundary.
- Bitrue: TS `BitrueExchange` inherits the branded empty order/PnL/funding
  histories from `BaseExchange`, inherits its throwing hedge-mode setter, and
  explicitly throws in `changeMarginType`. Those capabilities are skipped.
  Its supported fills query is one bounded `myTrades` page; Rust already has
  coverage including explicit incomplete filtered pages. The real gap is
  `BaseExchange.findOrderByClientOrderId`: cache-first, symbol-scoped open-order
  lookup, including the shared native/parent-ID aliases and sentinel checks.
- KuCoin spot: TS supports spot. Its HF adapter does not override order/PnL/
  funding history or public trades; do not invent capabilities TS lacks.
- Other venue limitations and Phemex gaps are assigned to the separate
  `feat/rust-parity-sweep` lane and excluded from this PR.

## Progress

- Created branch from the fetched landing head; working tree was clean.
- Read the current brief and rewrite spec. Parallel bounded discovery and
  implementation owners established for each venue, with shared files owned
  by the integration lane.
- Phase 1 audit: inspected current TS implementations rather than interpreting
  the venue API's potential capabilities as the adapter's actual support.
  The user's subsequent scope note assigns the broader roster audit to another
  lane; reverted this lane's unrelated status-row changes accordingly.
- Bitrue phase peer review: KuCoin owner independently compared the complete
  lookup diff and tests against the TS base helper, PASS with no blocker.
  Lookup never commits account snapshots; failures retain cache semantics and
  a miss remains non-definitive because closed orders were not queried.

## Paradex signing design and review

TS `signParadexTypedData` passes the StarkNet typed-data message hash, an
opaque public subkey, `scheme: stark`, and `encoding: felt-pair` to
`payloadSigner`. Auth uses seconds and a 24-hour expiry; Order/ModifyOrder use
millisecond signature timestamps. Signature strings may be stand-ins and are
inserted verbatim with signer headers, matching TS rather than re-parsing keys.
More than one delegated batch entry is rejected before any write.

The engine adds a typed `SignPayload` action and signed/failed result events.
It holds the original typed HTTP action by ID with a bounded pending map and
whole-operation deadline. A valid result resumes exactly that request; failed,
timed-out, disposed and duplicate/late replies cannot send a write. A signing
failure occurs before I/O and can release an unsent amendment guard, whereas
an ambiguous HTTP failure retains that guard. JS and native hosts perform the
callback and return the result event; no venue performs asynchronous I/O.

TS `remoteSigner` is the generic API HTTP adapter, distinct from StarkNet
payload signing. Its request receives account name, real base URL, method,
path, decoded params/body and headers. Returned URL/params/data replace the
old fields, including deliberate deletion, and usable bearer headers retain
the existing fallback contract. It still needs a locally usable key or a
payload signer for JWT acquisition; remoteSigner alone is not a new keyless
StarkNet protocol. JWT auth uses raw direct fetch in TS and bypasses that
remote adapter. Payload signing disables the API factory's HOW adapter while
ordinary provider proxy selection remains intact.

Independent Bitrue-owner review found two P1 cases and one P2 case: auth URL
contains `/auth/{subkey}`, JWT must bypass account HOW/proxy routing, and
returned query arrays require TS's comma serializer. Fixes are being verified
with real suffixed paths, host route metadata and array regression cases.
Independent re-review passes for the corrected wrapper, route policy and
inventory scope. Root focused TS checks passed 241 tests before the final
auth-path/array cases were added; full rerun is required for the final head.

## Implementation checkpoint

- Bitrue implementation `c114e7af84`, fresh fixture `62b5c23ca6`: 67 native
  tests, 22 TS contracts/history tests and the real WASM SDK boundary pass.
  Standard public/private gates pass; exact proof is `/tmp/parity-bitrue-proof.json`.
  Public medians TS/Rust 272.549/221.129 ms CPU, 178.799/143.902 ms steady,
  18,556,825/2,658,340 retained bytes. WASM 748,567 bytes.
- Paradex phase peer review passes after auth-route and query serializer fixes.
  Engine 61, native host 40, Paradex 113 tests pass; host tests 58 and a real
  SDK-to-WASM delegated signing vector pass. Native disposal now cancels the
  pending signer callback when the host receiver closes. Standard behavioral
  replays pass, but the signing addition currently exceeds the fixed WASM
  budget by 1,453 bytes; size work is still required before landing.
- KuCoin spot peer reviews found and fixed dropped late executions, unsafe
  partial/malformed snapshots, and missing caller context on uncached native
  amendment/cancellation. Legacy amendment parsing remains unchanged; a
  spot-only `amendOrderWithSymbol` operation carries the explicit context.
  Entire KuCoin crate with spot at that checkpoint: 122 tests and all-target
  Clippy pass (corrected the earlier summed total; individual groups unchanged).
- Shared integration peer review passes. Currency and lookup result comparison
  symmetrically project transfer-model financial fields and identity for both
  implementations; tests prove financial/identity corruption still differs.
  Harness 314 tests pass. Routing/SDK focused suite 239 tests passes. Full root
  typecheck (44 tasks) and lint (28 tasks) pass before the final small edits.
- KuCoin private HOW now follows actual TS API factories even when the tunnel
  preference is false/unset; eighteen real-factory cases and six serialized
  native golden cases cover that policy. Paradex JWT fetch always goes direct.
- Intentional KuCoin spot TS fixes: deduplicate executions, preserve newer
  private state and last-good orders across stale/failed/malformed reads, and
  encode caller IDs as URL components. Listed in `KNOWN_TS_FIXES`; no financial
  output rewriting is used.
- Remaining: size budget, all final-head public/private replays (including
  KuCoin futures/spot and Binance), complete checks, adversarial PR review and CI.

### Landing integration and review follow-up

- Merged landing Phemex `45d7a3fb98` in `db10d6bfd3`, retaining both sets of
  host/SDK registrations, harness controls, optional byte/text APIs and signing
  callbacks. This imports the landing structure only; Phemex capability gaps
  remain outside this lane's scope.
- Fresh independent review found two inherited TS financial bugs in KuCoin
  spot: malformed WS executions/order rows could reach the store, and missing/
  null required REST balance fields became zeros. Fixes preserve good state,
  validate execution fields before emission, and retain late executions before
  stale-order/terminal fences. Follow-up review also guards a newer WS row
  whose total size is smaller than a previously observed cumulative fill.
- Bitrue private tape now includes a positive client-ID lookup in addition to
  non-definitive misses; the comparison promotes documented raw client-ID
  aliases while retaining all financial/native identities.
- Repository tests pass (27 tasks; safe-cex 7,254 tests in each invocation),
  root typecheck and lint pass. Subsequent small fixes need focused/final checks.
- Size experiment: removing the body re-parser and using a bounded vector
  made the pre-landing artifact only 125 bytes over budget. The landing's
  optional text byte/batch ABI raises the excess to 3,591 bytes. A formatter
  experiment added 36 bytes and was reverted. The next reviewable trim is to
  omit optional text byte/batch exports from the standalone Paradex build:
  its hot path is SBE binary, and the host already supports ordinary text
  fallback for control messages. Combined builds retain those optional APIs.
- Optional text ingress saved 985 bytes but left Paradex 2,606 bytes over the
  unchanged budget. The next trim replaces only the WASM signer-header JSON
  ingress with validated header lines; the callback and native APIs stay typed.
  Peer review passes for byte preservation and rejection of CR/LF injection.
- Full workspace Clippy exposed Bybit's exhaustive test action match after the
  shared signing action was added. Its harness now explicitly rejects delegated
  signing, preserving the local signer assumption; independent review passes.
- Landing advanced to Coinbase `5c0e437faf`; merge it before final artifact
  builds. Coinbase capabilities and the roster-wide sweep remain outside scope.
- Coinbase landing merged in `916fc8c498`, Polymarket landing `a0c11afd87`
  merged in `5be8a2c727`. Independent merge reviews pass. Both retain this
  lane's scoped registrations and the incoming venue APIs/transfer layouts.
- Header-line ingress `77ebeed5e3` passes six standalone WASM tests, 37 JS host
  tests after Coinbase integration, and two actual SDK signing tests. The latter
  proves exact bridge bytes separately from normal HTTP header whitespace
  normalization. The Coinbase-only `accountId` export is now feature gated in
  `3d6144d555`; combined/Coinbase builds retain it.
- The latest artifact is 1,262,278 bytes, 3,987 over budget after incoming
  prediction metadata added common Store/frame work. Add an engine feature for
  this optional metadata, enabled by Polymarket, to keep other standalone builds
  lean while preserving its complete behavior and frame layout when enabled.
- Independent Spot re-review found execution geometry needed validation before
  emitting or consuming a trade ID. Bound by explicit native total/cumulative
  quantities; preflight the known-fill floor on current/newer open rows while
  retaining legitimate older/terminal executions. Regression/fix in progress.
- Current complete root lint passes 28 tasks. The merged-source native and
  harness pipelines are running; prior incomplete runs spanning landing merges
  are not final gate evidence.
