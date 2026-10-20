# Historical delivery29 diagnostics

All 20 declared calls ran exactly once and passed same-arm logical output comparison. No retries, builds, or source changes occurred. This records candidate JS8150295 / Rust7bc against original9c with shared7bc helpers; subsequent landing merges do not change this evidence.

These instrumented one-shot observations do not replace the official negative gate cohort. Method CPU includes observer overhead. Sample weights measure sampler elapsed time, not CPU milliseconds.

| Profile | Arm/delivery | Thread CPU ms | Steady ms | Gross WS ms / calls | Gross HTTP ms | Frame ms | Host drain residual ms |
|---|---|---:|---:|---:|---:|---:|---:|
| okx-public | before/single | 382.261 | 231.237 | 89.791 / 22663 | 13.587 | 2.969 | 0.000 |
| okx-public | after/single | 411.905 | 221.283 | 90.463 / 22663 | 15.597 | 3.223 | 0.000 |
| okx-public | after/batch | 264.213 | 147.191 | 42.203 / 2260 | 13.579 | 2.967 | 17.567 |
| bybit-public | before/single | 137.389 | 45.711 | 20.255 / 4294 | 24.390 | 4.168 | 0.000 |
| bybit-public | after/single | 157.645 | 58.649 | 23.917 / 4294 | 20.963 | 4.584 | 0.000 |
| bybit-public | after/batch | 143.919 | 48.639 | 14.651 / 1080 | 22.565 | 4.639 | 8.280 |
| hyrotrader-public | before/single | 17.989 | 3.426 | 1.584 / 160 | 4.142 | 0.910 | 0.000 |
| hyrotrader-public | after/single | 20.064 | 3.597 | 1.632 / 160 | 4.124 | 1.009 | 0.000 |
| hyrotrader-public | after/batch | 20.600 | 3.138 | 1.486 / 54 | 4.427 | 1.086 | 0.822 |
| paradex-public | before/single | 390.811 | 136.038 | 37.816 / 15219 | 162.932 | 1.280 | 0.000 |
| paradex-public | after/single | 411.152 | 143.364 | 40.429 / 15219 | 169.029 | 1.389 | 0.000 |
| paradex-public | after/batch | 390.000 | 124.044 | 14.755 / 1990 | 171.699 | 1.448 | 12.336 |

A zero host residual on the original or single mode means no queued drain was observed; it does not mean zero host work. Residual includes packing, action dispatch and observers.

| Profile | Arm | Thread CPU ms | Samples | Idle % | Inspector % | WASM HTTP % | WASM WS % | Tape % | Node builtins % | GC % |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| okx-public | before | 275.749 | 243 | 22.3 | 3.3 | 3.6 | 8.3 | 27.8 | 7.5 | 5.9 |
| okx-public | after | 265.924 | 252 | 21.3 | 3.3 | 4.3 | 11.3 | 19.6 | 8.5 | 4.2 |
| bybit-public | before | 155.840 | 737 | 66.6 | 1.1 | 2.5 | 1.3 | 2.8 | 10.5 | 0.4 |
| bybit-public | after | 202.320 | 760 | 68.7 | 1.0 | 2.0 | 1.3 | 2.1 | 9.0 | 1.2 |
| hyrotrader-public | before | 24.978 | 20 | 15.7 | 24.8 | 11.9 | 0.0 | 11.6 | 7.8 | 3.9 |
| hyrotrader-public | after | 19.964 | 15 | 5.2 | 30.9 | 10.4 | 0.0 | 0.0 | 5.1 | 0.0 |
| paradex-public | before | 372.057 | 296 | 18.0 | 5.9 | 36.4 | 1.0 | 11.9 | 5.3 | 1.2 |
| paradex-public | after | 377.873 | 314 | 22.2 | 3.6 | 39.1 | 2.4 | 10.3 | 6.3 | 1.9 |

All sample percentages use total observed weight, retaining idle and inspector overhead. Other categories remain in analysis.json. Inclusive stacks overlap and cannot be summed.

Evidence-backed directions after the new landing is reviewed:

- Paradex: HTTP chunk processing dominates engine method CPU: 1,189 chunks, 161.129ms before and 169.926ms after/batch. After/batch total engine methods190.784ms; frame writing1.448ms. CPU samples independently put39.1% of all weight in WASM under HTTP and show174.83ms inclusive sample weight under httpChunk. Investigate streamed HTTP parsing/item processing and its allocation/compiler profile before targeting frames or Store ticker search. This identifies a subsystem, not its internal Rust function.
- OKX: batching reduces gross WS89.791→42.203ms and calls22,663→2,260; frame cost remains about3ms. The sample still carries19.6% Tape,7.2% request/response oracle,2.8% subscription/protocol audit weight. The current diagnostic does not support a frame-writer or universal lookup regression. Shared harness work is material and must remain identical between arms if investigated.
- Bybit: gross WS20.255→14.651ms, but HTTP22.565ms and frame4.639ms remain. The sampled run has68.7% idle,9.0% Node builtins,1.3% WASM WS; it does not identify Store search as dominant. Review incoming direct numeric ticker-update/startup-reserve fixes before proposing overlapping work.
- Hyro: only160 arrivals,54 after/batch crossings; gross WS1.584→1.486ms and drain residual0.822ms. Total method-observed CPU17.989→20.600ms. Book commands account for1.343→2.734ms gross, but only20/15 CPU samples with24.8%/30.9% inspector overhead are too sparse to identify a reliable inner-engine hotspot. Startup/JIT/command path is a candidate for a separately declared investigation, not a measured lookup cause.

Source correction: control9c already uses the same sorted-u32 ticker index/binary search. OKX ticker/mark/index updates use price_tickers HashMap→update_ticker_at, bypassing Store ticker/market search. The new borrowed market-ID index affects book/candle paths; no diagnostic proves it caused the broad CPU regressions.

No no-op crossing/syntax-floor build was made; gross WS timings cannot separate crossing from typed parsing/state/action serialization. WASM function indices are stripped and not a Rust symbol attribution.

Raw report: report.json SHA62815c53f6cb3e16805dfbf20a09ac4aac1ff62fdb59d25b004561b00088eb97. Analysis: analysis.json SHAd673144d247f8c1b573832e9c4aa60c496e2ebf2c0d384f7fc73554f7f1dd576. All raw outputs, logs and eight CPU profiles remain beside the report; report records their hashes and confirmed process cleanup.

The complete raw diagnostics and scripts are preserved in `RUST_PERF_DELIVERY29_DIAGNOSTICS.json.gz` (SHA-256 `5be68783177a5feb14028db9b1cfee2e5a5fa9d26fd51c04fdc21631f5b9f212`, 54 files). The official scored results remain in `RUST_PERF_DELIVERY29_RESULTS.md`; these observations do not replace their failed acceptance outcome.
