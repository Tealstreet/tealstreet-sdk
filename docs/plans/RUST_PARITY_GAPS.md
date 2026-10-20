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
returned query arrays require TS's comma serializer. Fixes were verified
with real suffixed paths, host route metadata and array regression cases.
Independent re-review passes for the corrected wrapper, route policy and
inventory scope. The final repository suite also covers
the auth-path, array and callback receiver/arity regressions.

## Implementation and reviews

- Bitrue: `c114e7af84`, positive-query follow-up `3d7f54506a`. Native 68
  tests pass. Cache/open-order lookup matches TS aliases, sentinels, symbol scope,
  non-definitive misses and failure fallback without account-store mutation.
  Duplicate placement-unit bookkeeping is removed only from the returned lookup
  copy; financial extras and the original Store remain intact. Independent review
  passes. Unsupported TS histories and mode setters remain skipped, as above.
- Paradex: delegated host/engine boundary, direct JWT route, generic remote HTTP
  adapter and callback receiver/arity fixes are implemented and reviewed. Native
  venue 115 tests pass. The exact opaque signature/header bridge is verified by
  two actual SDK-to-WASM tests, including CR/LF rejection before HTTP and native
  cancellation/deadline tests. `remoteSigner` alone does not replace TS's local
  key/payloadSigner requirement for JWT acquisition.
- KuCoin Spot: typed public/private/trading/query state machine, feature-gated
  registration, fresh bullet-token recovery and SDK context commands are in place.
  Native 135 tests pass, including 36 Spot. Current/newer malformed financial rows
  are rejected before execution emission/dedup; rejected IDs remain reusable.
  Late distinct executions survive stale/terminal state fences. Missing/null
  required balances preserve good state. Missing/null/empty amend ACK IDs retain
  the original ID, matching TS. Root, Bitrue and independent correctness reviews
  pass after fixes (`84dd4f063e`, `d784821eed`).
- Intentional Spot TS fixes are listed in `KNOWN_TS_FIXES`: deduplicate executions;
  preserve newer private state and good snapshots across stale/failed/malformed
  reads; reject malformed financial rows; encode native caller IDs as URL parts.
  The entry performs no financial output rewriting. Lookup/currency comparison
  symmetrically projects transfer-model fields for both implementations, with
  mutation tests retaining financial and identity differences.
- Landing integration: Phemex `db10d6bfd3`, Coinbase `916fc8c498`, Polymarket
  `5be8a2c727`, GRVT `168a639673`. Independent merge reviews preserve incoming
  capabilities and this lane's SDK/host/harness changes. Exhaustive test action
  handlers reject unexpected delegated signing; new Spot-only commands receive
  explicit unsupported replies in the strict Polymarket/GRVT dispatchers.
  Other venue capability gaps and roster rows remain outside this lane's scope.

## Size and packaging

The fixed per-venue budget remains **1,258,291 bytes**. Paradex signing was
trimmed through a typed precomputed hash, bounded pending vector, in-place
signature field replacement and validated header-line WASM ingress. Callback
headers remain a Record; native event headers remain typed pairs. Exact bridge
bytes are distinguished from ordinary HTTP header whitespace normalization.

Standalone Paradex omits optional text byte/batch ingress (SBE binary and stable
text control entry remain) and Coinbase's account-ID ABI; combined builds retain
those APIs. Optional prediction storage/frames use the engine `prediction`
feature, enabled by Polymarket. Feature-off/on engine tests (67/69), exact frame
layout/lifecycle tests and Polymarket's 87 native tests pass. Feature-enabled
behavior is unchanged.

Only the standalone Paradex host uses `opt-level=2`; ordinary/default builds
retain their existing profiles. Standard Yarn builds reproduce the measured
artifact exactly: web and Node each **1,257,099 bytes**, SHA-256
`75a8611dbcbbb0ebe6b217bbbe97baf64f85a2113c4dc451c7a5c3700be4765b`.
Its CPU, memory, private replay and SDK tests pass; this is a measured compiler
tradeoff with no budget or gate relaxation.

Bitrue's initial final replay missed main-thread CPU by 0.3 ms (259.623 versus
259.853 ms) despite matching behavior and passing steady CPU/memory. Raw runs
showed narrow margins. A standalone venue-crate `opt-level=3` candidate is being
measured for a robust margin; the combined build retains its size profile. The
failed checkpoint is retained, and no failed gate is reported as passed.

## Final gates

All gates use normal egress comparison. The GRVT landing's symmetric
before-verifier-setup baseline and async major-GC measurement are retained;
performance checkpoints from before that merge have been refreshed.

| Venue | Public frames | TS/Rust CPU ms | TS/Rust retained bytes | Private frames/commands | Status |
| --- | ---: | ---: | ---: | ---: | --- |
| Paradex | 15,085 | 1,362.011 / 1,143.063 | 18,758,073 / 2,352,276 | 33 / 15 | PASS, 3 public runs; SDK 2 |
| Binance | 873 | 298.521 / 157.508 | 16,384,705 / 2,511,524 | 14 / 3 | PASS, 3 public runs |
| Bitrue | 3,208 | pending optimization | pending final proof | 23 / 8 | Private + SDK 1 PASS; public pending |
| KuCoin Spot | pending fresh capture | pending | pending | pending | Pending package handoff |
| KuCoin futures | pending fresh capture | pending | pending | pending | Pending package handoff |
| Polymarket packaging | existing fixture | pending | pending | pending | Pending feature-on replay |

Paradex public capture has 66 markets/tickers and two books. Binance has 917
markets, 783 tickers and three books. Both post-GRVT standard artifact hashes
match their earlier builds byte for byte. Exact commands, tape hashes, raw runs
and artifact provenance are in `/tmp/parity-paradex-proof.json` and
`/tmp/parity-binance-proof.json`; final numbers will be retained here and in PR.

Repository verification passes: **2,228 native workspace tests**, fmt,
workspace/all-target Clippy, wasm32 Clippy and bans; root typecheck **44 tasks**,
lint **28 tasks**, harness **391 tests**, repository tests **27 tasks** with
safe-cex **7,333 tests in each test-ci/test-unit invocation**. The final repository
run uses serial Turbo tasks and four Vitest workers after import/constructor
wall-clock timeouts under parallel load; assertions and timeouts are unchanged.
Spot's final ACK adjustment also passes its full 135-test crate and Clippy.

Remaining: Bitrue public performance; feature-on Polymarket packaging replay;
KuCoin Spot actual SDK, fresh capture/public/private gates and futures gates;
combined build/host/SDK tests; PR correctness reviews and exact-head green CI.
