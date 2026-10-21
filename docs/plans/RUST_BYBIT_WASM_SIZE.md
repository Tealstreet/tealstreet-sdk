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
