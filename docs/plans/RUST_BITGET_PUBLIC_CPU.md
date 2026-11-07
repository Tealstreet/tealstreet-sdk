# Bitget public CPU investigation

Owner-directed scope, 2026-10-22. Repeated public steady-state CPU results sit
near the unchanged gate boundary; #1739's initial run measured TS464.556ms and
Rust418.161ms against418.1004ms. The later integrated run passed. A single
sample is not a regression verdict: identify the public hot path and obtain
before/after CI evidence without changing timing thresholds or replay work.

The initial diagnostic plan started from actual #1735
`b1dfdb43291d5a6de7dd8808b486adddda4cb118`. This fresh CI experiment starts
from actual shared setup #1738 `ecc84a798163c9f61ea9439e90001a98bb07b907`.
Kraken Spot now uses clean replacement #1742; #1728 remains blocked on the
hosted-signing bridge. Neither source change is bundled into this experiment.
No private account/signing/provider edits belong in this performance slice.

1. Commit and independently review this plan before changing source. Read the
   Bitget public envelope decoder, book update/checksum path and TS public
   handlers. Audit whether a proposed optimization repeats a cross-cutting
   concern; extend an existing engine boundary if required, rather than adding
   a parallel venue policy. No optimization is selected before evidence.
2. Authenticate a CI Bitget package from the exact current build-input tree
   using the no-fallback fingerprint tool, live artifact ID/digest and package
   hashes. No local release build. Use the owner's one allowed local paired
   public workload: one direct TS performance replay and one direct Rust
   performance replay of the same public tape/books, with CPU profiles. The
   public gate itself requires at least seven pairs and remains unchanged.
   This local pair is diagnostic, not gate certification; account for profiling
   overhead and startup versus steady-state samples. Preserve full raw outputs.
3. Report sampled public book/trade/JSON work and the source call graph; identify
   uncertainty when stripped wasm names prevent exact attribution. Propose the
   smallest evidenced public-path optimization, or an explicit profile change
   with measured wasm size cost. Review the concrete change before applying it.
4. Preserve malformed-message, field-order, number validation, checksum bytes,
   sequence/duplicate/gap handling, reconnect/resubscribe and book/trade outputs.
   Add only meaningful protocol/boundary equivalence controls. Run native
   engine/Bitget/Binance checks, strict Clippy/fmt/bans and root typecheck/lint;
   obtain independent code/adversarial review and commit the bounded change.
5. CI owns before/after multi-pair CPU numbers and size measurements. Compare
   authenticated full public/private Bitget-family and Binance raw outputs
   except top-level metrics and all ordered gate diagnostics; no allowances,
   normalizer/comparator changes or threshold relaxation. Refresh required
   shared dependencies, full exact-head green CI and independent evidence
   review before a fresh guard and a separate merge call. Coordinate queue
   placement with the overseer; this work does not bypass the signing bridge.

Architecture inventory correction before source edits: the same IEEE CRC32
byte-update loop occurs in Bitget `book.rs:144-148` and Kraken Spot
`spot/book.rs:51`; the initial narrow search missed the latter spelling of the
polynomial. The per-venue serialization differs, but the accumulator primitive
is cross-cutting. This is reported to the overseer before proceeding. If the
profile identifies CRC as material work, extend the engine with one streaming
IEEE CRC32 primitive and migrate both byte accumulators; do not optimize a
second venue-owned copy. Review that concrete API and evidence first. If the
profile points elsewhere, retain this finding as a separate architecture
follow-up rather than expanding an unrelated optimization.

## Concrete first experiment

Concrete profile experiment for the existing Bitget CPU plan (before source edit):

The one authorized paired diagnostic replay is complete. In 39,454 WS rows,
39,341 carry tickers and 109 books; no book contains a checksum. CRC is not a
material candidate on this tape. The independent stripped-WASM attribution
confirms public envelope/ticker visitors plus shared JSON number/string scanning
as hot work. Inclusive profile samples overlap and are not additive; local
profiled TS/Rust steady CPU 412.884/385.182 ms are diagnostic, not gate evidence.
The current Bitget venue crate is already opt-level 3; repeating that override is
ineffective. Baseline matching CI WASM is 1,044,199 bytes.

First bounded experiment: in build.sh family_args, append exactly
--config 'profile.release.package.serde_json.opt-level=3'
when normalized family == bitget. It applies to Bitget and BitgetUSDC standalone
family builds, including --families, after current shared z selection. It does
not change other family argument lists, combined or native profiles, venue
source, engine numerical semantics, wasm-opt flags, or any workload/threshold.
This is shared JSON throughput policy selected at the existing build boundary,
not another venue-specific parser implementation. No serde_core or engine
profile override is selected without evidence. If CPU/size do not justify this
small candidate, do not merge it; retain the experiment and propose the next
bounded option with owner coordination.

CI before/after must use exact actual-landing build fingerprints, full green
heads, unchanged multi-pair public gates, measured optimized WASM size and the
same full raw Bitget + BitgetUSDC + Binance outputs/ordered diagnostics. Report
all changes and keep historical runs/failed experiments labeled. No additional
local replay or release build is allowed. Existing native/strict checks plus
shell syntax and captured Cargo argument comparisons can verify bounded family
selection without building WASM. No tests merely mirroring the new if clause.

Shared Bitget/Kraken Spot CRC primitive remains a reported architectural
follow-up: extraction is not selected here because it is inactive in the
measured workload and would expand this bounded profile experiment.

Independent plan/attribution review: CLEAN. Architecture: YES, shared JSON
parsing is cross-cutting; the candidate changes only the existing central
build-profile selection, without adding a parser. The owner approved a bounded
CI-only experiment and prefers a demonstrated shared parsing fix over a family
profile change. Source audit found the ordinary path already borrows identities,
streams rows once, skips unknown fields, and uses the engine numeric policy and
short-decimal fast path. All eleven decoded ticker numeric fields are consumed.
No semantics-preserving unused-field shortcut has been demonstrated; report
experiment results before selecting any change.

## Scope superseded before adoption

The initial Bitget-only candidate `d1b2ccae9845110e4588cd52462ca32ce54c06c1`
is historical, not the selected policy. Its first nine-pair CI result measured
TS/Rust steady CPU 463.199/411.473 ms on baseline versus 461.766/389.646 ms
on candidate; optimized wasm grew from 1,045,083 to 1,063,838 bytes. USDC
measured 4.522/3.060 ms versus 4.474/2.737 ms. Both public gates passed.
This is one baseline/candidate CI pair, without a broad statistical claim.

The owner applied the architecture gate: shared JSON parsing must use one
all-family policy rather than another one-off override. Follow the committed
[shared JSON profile plan](RUST_JSON_WASM_PROFILE.md). The Bitget-only branch
condition will be removed. Native profiles, thresholds and budgets stay fixed;
all-family CPU/size/raw evidence determines whether the new policy is adopted.
