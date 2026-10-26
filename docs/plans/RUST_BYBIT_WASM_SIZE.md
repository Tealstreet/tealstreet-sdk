# Bybit family WASM size recovery

Base: `673565c2e614` (`feat/rust-rewrite`, capability tail #1637 merged).
The fixed family budget is 1,343,488 bytes. The current CI Bybit artifact is
1,339,462 bytes (4,026 bytes headroom). Recover at least 30 KiB headroom
(target <=1,312,768 bytes) without changing behavior or raising the budget.

1. Profile the actual optimized CI artifacts, including the pre-row-isolation,
   #1620 row-isolation and #1624 entropy checkpoints. Use twiggy or equivalent
   section/function/call-graph analysis; record the tool, artifact hashes and
   measured attribution. Names are stripped from shipped artifacts, so state any
   limits rather than infer source attribution from anonymous functions alone.
2. Inspect row-isolation and entropy changes for duplicated formatting and
   generic/monomorphized bodies. Extract shared non-generic helpers where the
   measurements support it. Preserve every parser, row admission, repair,
   notification, client-ID byte stream and signing policy; no dual paths or
   behavioral changes. Do not change quantity calculations or increase budgets.
3. Verify focused touched-crate and Binance tests, strict Clippy/fmt, applicable
   JS checks, and independent adversarial reviews. CI owns release builds,
   family/combined size and all strict replay gates; no local release builds,
   full sweeps or timing matrices. Keep CARGO_BUILD_JOBS=4 locally.
4. Compare candidate CI sizes and full raw replay results with the original
   artifact. Require stable byte-identical observable outputs and unchanged
   strict comparators/declarations. Integrate current landing under lane rules,
   run fresh CI, then a separate revert guard and merge; report the actual pin.

Historical integrated-head CI checkpoints: #1615 1,313,430; #1620 1,331,273;
#1621 1,331,322; #1624 1,336,815; #1629/#1622 1,336,953; #1637 1,339,462 bytes.
Successive differences include intervening landings; isolate claims accordingly.
No downstream application/deployment changes are in scope.

## Baseline profile

Twiggy 0.8.0 `top`/`top --retained`/`paths` on the downloaded optimized Node
CI artifacts (the web artifact is identical). No local release build was used.

| Integrated checkpoint | Artifact bytes | Code bytes / functions | Data bytes |
| --- | ---: | ---: | ---: |
| #1615 | 1,313,430 | 1,220,276 / 1,120 | 88,763 |
| #1620 | 1,331,273 | 1,237,735 / 1,133 | 89,145 |
| #1624 | 1,336,815 | 1,244,259 / 1,155 | 88,776 |
| #1637 | 1,339,462 | 1,246,396 / 1,156 | 89,286 |

SHA-256 respectively: `9196bc699b733719e8eb9380ebb585f3fd04bcf22279dd475aba6190bb4fce66`,
`c2a93c74c84e79aa69f9102c9a75f045c2c868df6aed8c604e62db8f06f17271`,
`99a0d5f52ddeb6b49a97129a27ab2fcfc0c76c614d81e7ac264cbdab114d750e`,
`b486c5b22e8efa716b2806070a3b6344987e96998d69d84b47509505029a30b8`.
CI runs: 39614261328, 39620930357, 39629583459, 39640110159.

The current command export directly retains a 129,671-byte body, exactly the
same shallow size as #1615. Large unnamed engine/event bodies and their callees
retain most code. Names are stripped by `build.sh`; anonymous function numbers
are not stable identities across builds, and this profile does not prove which
Rust source function owns each increase. The code/data split supports measuring
parser call boundaries rather than compacting string data first.

First measured candidate: non-inlined, concrete order-row, position-row and
raw-topic parser helpers shared by existing call sites. Use the exact same
Serde parsers, lifetimes and error results; preserve Hyro's separate typed-frame
atomicity. Independent plan/source review agrees this is a low-risk size
experiment, not yet a measured saving. Existing row-admission/repair/Hyro tests
and strict CI replay outputs remain the behavior checks.

## First candidate measurement and named attribution

The concrete parser boundaries saved 179 bytes in normal stripped CI:
1,339,283 bytes, SHA-256
`807f33d74c05d51b453dd6b331b64231fadb2a5ff2e49cea552fce70a87626e3`,
run 39643522100 / Bybit job 119372994023. This does not meet the target.

With the overseer's explicit one-build exception, a scratch baseline build kept
the name section (`wasm-bindgen` 0.2.129; Binaryen 132 `-Oz -g`; normal Rust
release settings, four build jobs). It is for attribution, not size measurement.
The named 95,453-byte `on_public_response` incorporates private response dispatch;
several large concrete account/trade/history/closed/options handlers have no
separate symbols. `ArraySplitter::emit` is 17,949 bytes and incorporates streamed
row dispatch. Serde's row parsers are already mostly shared, explaining the small
first saving. Entropy-named functions occupy 4,410 shallow bytes in total.

The next measured candidate adds out-of-line boundaries to those existing
concrete response handlers and `Venue::on_item`. Their bodies, argument types,
parser grammar and error handling remain unchanged. Independent profile review
identified these boundaries; CI must establish the actual saving and public
performance before any final claim.

The second candidate measured 1,339,336 bytes (126 bytes below baseline),
SHA-256 `b9e848f6f7f5699df80808b0f611eaf0a72d8b1b5f6edd6dafa48cc80c507d20`,
run 39644779366 / Bybit job 119377661176. Handler inlining boundaries alone did
not recover enough space. The next candidate routes the five existing private
response payloads through a concrete function table and one shared out-of-line
call boundary, retaining the original response preamble and handler bodies.
The final raw-output proof must contain all 55 baseline outputs; fewer available
adaptive performance samples do not satisfy the overseer's proof requirement.

The third candidate measured 1,340,058 bytes (596 bytes above baseline),
SHA-256 `6eef9f5bd0c4580b499783e7bd69b2977650836aa33c965262ab68b1b15f4eb7`,
run 39646578013 / Bybit job 119384236814. The response table did not help.
All three source experiments are removed from the final change.

## Release profile experiments

The existing configuration already has fat LTO, one codegen unit and the WASM
target's abort panic behavior. Rust release and the Bybit package both use level
3; only the final Binaryen optimizer uses `-Oz`. Twiggy monos finds no Bybit
`SnapshotBuilder` instantiation: its rejected-row tracking is venue-local.

The overseer approved testing Rust `s` and `z` for the Bybit package and choosing
the smallest setting that passes strict public timing/memory gates and the full
55/55 raw replay comparison. Other packages retain their current profiles.
The source-level parser/handler experiments are removed; all six Rust source
files return byte-for-byte to the landed baseline. Normal CI artifact sizes
remain the measurement. Other level-3 families may offer similar code-size
headroom, but their savings and throughput effects are unmeasured and are not
changed here; families already using size profiles should not be assumed to
have the same opportunity.

The `s` profile measured 1,370,484 bytes (31,022 bytes above baseline and
26,996 bytes above budget), SHA-256
`2d1607a27924b6414d53788d4a01cc1540e5bd57a79d6b40bb60138f4e3719ef`,
run 39647878514 / Bybit job 119388893518. The hard size gate rejected it
before replay, so there is no timing or raw-output proof for this candidate.
The next candidate tests `z`, integrating landing `a8b1cb00a31` cleanly first.

The `z` profile measured 1,196,217 bytes (143,245 bytes below baseline),
leaving 147,271 bytes (143.8 KiB) of headroom under the unchanged budget.
SHA-256 `3c14c73b5af94fb1cd261aaebccb772fe9c3d3da1fb2d601c3d48080ca9b5cff`,
run 39648619392 / Bybit job 119391018923. Public timing/memory gates passed
for all five aliases; existing declared coverage failures remain unchanged.
The first downloaded raw artifact has 49 outputs because inverse, spot and
demo received seven adaptive performance samples. All available hashes match
the baseline, retaining every raw field except top-level metrics; this is
explicitly incomplete and does not satisfy the required 55/55 final proof.

Select `z` for Bybit alone: it exceeds the 30 KiB headroom target while `s`
fails the fixed budget. The package override applies to family/combined WASM
and native release builds; measured performance evidence here covers WASM.
No native debug profile, source algorithm or other package profile changes.
Final acceptance still requires current-landing integration, green CI including
all budgets and public gates, complete 55/55 byte-identical raw replay proof,
and a separate fresh revert guard before merging the exact green head.
