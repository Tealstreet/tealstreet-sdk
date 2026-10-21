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
