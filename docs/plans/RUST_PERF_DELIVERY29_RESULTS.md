# Fixed collection29 results

Acceptance fails. All 64 private gates pass. All 40 public profiles pass the official output comparisons, and their six same-implementation outputs match across runs and arms. There are no unresolved cases, missing cases, retries or input changes. Memory margins widen in 40/40 profiles; CPU margins widen in 6/40. Only six profiles meet the complete improvement requirement. Candidate Breakout, Paradex and Hyro fail absolute CPU gates.

The fixed roster runs 20 AB and 20 BA public pairs, three runs per implementation per arm, with pinned Node 22.23.3. Ratios are TS divided by Rust; above1 means Rust uses less CPU or retained memory. These are three-run medians. Raw ranges, TS drift, audits, all 608 replay outputs and 144 gate logs remain in `RUST_PERF_DELIVERY29_RESULTS.json.gz`; no significance claim is made. TS median CPU changes between arms range from −38.923% (OKX EU) to +61.191% (Blofin); OKX changes −21.850%, Bitrue −38.082%, Bybit spot +41.127%, and Bybit demo +56.966%. Ratios describe this cohort and do not isolate causal engine CPU effects.

Original control `9c34020741`; shared helpers and unchanged Rust build `7bc9e30d32`; final JavaScript host `8150295a7c`. The collection freezes at documentation/cap commit `95c0336b39`. The first control Breakout gate also fails CPU, and that failure is retained. Source and artifact guards verify before and after collection.

| Profile | CPU before → after | Steady CPU before → after | Memory before → after | Full requirement |
| --- | ---: | ---: | ---: | --- |
| breakoutprop-public | 0.986 → 0.975 | 1.219 → 1.057 | 6.807 → 13.115 | FAIL |
| apexomni-public | 2.334 → 2.011 | 5.011 → 3.731 | 6.663 → 8.709 | FAIL |
| asterdex-public | 1.814 → 1.446 | 1.551 → 1.165 | 4.039 → 4.631 | FAIL |
| woo-public | 1.369 → 1.409 | 1.317 → 1.540 | 6.155 → 9.201 | PASS |
| binance-public | 2.006 → 1.679 | 2.907 → 2.073 | 3.766 → 4.494 | FAIL |
| orderly-public | 1.752 → 1.577 | 1.873 → 1.619 | 5.438 → 8.143 | FAIL |
| coinbase-public | 2.432 → 2.435 | 4.643 → 4.701 | 1.800 → 1.823 | PASS |
| bitget-public | 1.516 → 1.218 | 1.454 → 1.245 | 3.933 → 4.662 | FAIL |
| backpack-public | 2.190 → 2.150 | 2.761 → 2.543 | 4.786 → 6.138 | FAIL |
| blofin-public | 1.216 → 1.127 | 1.472 → 1.337 | 4.352 → 5.356 | FAIL |
| mexc-public | 2.565 → 2.329 | 3.485 → 2.694 | 4.773 → 5.955 | FAIL |
| polymarket-public | 2.115 → 1.146 | 4.796 → 1.562 | 4.104 → 4.707 | FAIL |
| kucoin-public | 2.223 → 1.848 | 2.640 → 1.993 | 4.401 → 5.225 | FAIL |
| bingx-public | 1.676 → 1.589 | 1.477 → 1.363 | 4.039 → 4.895 | FAIL |
| bybit-public | 1.400 → 1.324 | 1.907 → 1.903 | 3.841 → 4.551 | FAIL |
| okx-public | 1.472 → 1.089 | 1.658 → 1.194 | 3.082 → 3.751 | FAIL |
| cryptocom-public | 1.571 → 1.101 | 1.873 → 1.170 | 4.749 → 6.793 | FAIL |
| weex-public | 1.452 → 1.013 | 1.514 → 1.044 | 4.420 → 5.500 | FAIL |
| grvt-public | 1.349 → 1.198 | 1.415 → 1.380 | 10.373 → 14.671 | FAIL |
| phemex-public | 3.436 → 3.168 | 5.673 → 4.596 | 3.777 → 4.473 | FAIL |
| krakenspot-public | 2.365 → 1.956 | 3.206 → 2.399 | 3.267 → 3.805 | FAIL |
| hyperliquidspot-public | 1.142 → 1.017 | 1.138 → 1.308 | 2.755 → 3.089 | FAIL |
| kraken-public | 7.229 → 8.415 | 9.209 → 12.559 | 4.842 → 6.143 | PASS |
| lighter-public | 1.986 → 1.713 | 2.214 → 1.979 | 4.807 → 6.221 | FAIL |
| lighter-robinhood-public | 2.500 → 2.640 | 3.254 → 3.935 | 3.507 → 3.953 | PASS |
| bitrue-public | 1.272 → 1.048 | 1.286 → 1.034 | 4.173 → 5.072 | FAIL |
| hyperliquid-public | 6.746 → 5.047 | 1.846 → 1.317 | 7.137 → 10.046 | FAIL |
| paradex-public | 1.083 → 0.872 | 1.643 → 1.229 | 6.714 → 8.082 | FAIL |
| txflow-public | 1.770 → 1.187 | 2.234 → 1.245 | 7.998 → 12.031 | FAIL |
| binancespot-public | 1.313 → 1.242 | 1.517 → 1.150 | 2.383 → 2.568 | FAIL |
| binanceinverse-public | 2.439 → 2.322 | 2.924 → 2.723 | 5.254 → 7.279 | FAIL |
| binancepm-public | 2.112 → 1.969 | 2.819 → 2.328 | 3.713 → 4.441 | FAIL |
| bybitinverse-public | 3.537 → 3.083 | 5.353 → 5.071 | 5.857 → 8.379 | FAIL |
| bybitspot-public | 1.384 → 1.333 | 1.801 → 1.589 | 4.179 → 5.118 | FAIL |
| bybitdemo-public | 1.435 → 1.236 | 2.190 → 1.511 | 3.732 → 4.423 | FAIL |
| hyrotrader-public | 1.069 → 0.871 | 1.063 → 0.964 | 7.957 → 14.307 | FAIL |
| okxeu-public | 1.461 → 1.529 | 1.546 → 1.811 | 2.827 → 3.249 | PASS |
| okx-inverse-public | 1.283 → 1.374 | 1.318 → 1.449 | 3.100 → 3.661 | PASS |
| okxeu-xperp-public | 1.420 → 1.347 | 1.724 → 1.610 | 2.650 → 2.920 | FAIL |
| kucoinspot-public | 1.256 → 1.175 | 1.230 → 1.128 | 3.664 → 4.873 | FAIL |

Raw report SHA `ec48b251b07a1ca2b09d2e9b3098784c976239d0012193e94f8c9938436d9688`; compressed complete evidence SHA `349d770b7dc97966a83714a4577682bc73d9c700e14a1f5d3bc46ffda76b5f6f`.

CI at `95c0336b39` finishes 38 checks green and two replay checks red: Paradex main CPU 345.3 ms TS / 348.0 ms Rust (0.99×); Bybit Hyro steady CPU 6.0 ms TS / 6.7 ms Rust (0.89×). These failures block merge. All 54 family size checks pass after the documented rebaseline; size acceptance is separate from performance acceptance.

Next: use fixed unscored method profiles and separate CPU samples on existing artifacts to isolate crossings/packing, combined Rust work, frame writing and shared replay overhead. The original control already has sorted ticker lookup; a broad new ticker-search regression is not established. Apply a measured engine fix, reverify affected behavior, and declare a new source cohort. Do not retry this cohort for favorable numbers.

## Main-thread CPU medians

Milliseconds; changes compare after with before within each implementation. Per-run ranges are retained in the raw evidence.

| Profile | TS before → after | Rust before → after | TS change | Rust change |
| --- | ---: | ---: | ---: | ---: |
| breakoutprop-public | 24.490 → 20.314 | 24.845 → 20.827 | -17.052% | -16.172% |
| apexomni-public | 103.789 → 99.497 | 44.469 → 49.488 | -4.135% | +11.287% |
| asterdex-public | 225.040 → 180.269 | 124.027 → 124.703 | -19.895% | +0.545% |
| woo-public | 56.238 → 64.450 | 41.065 → 45.738 | +14.602% | +11.380% |
| binance-public | 197.261 → 226.630 | 98.323 → 134.990 | +14.888% | +37.292% |
| orderly-public | 113.640 → 127.516 | 64.853 → 80.882 | +12.210% | +24.716% |
| coinbase-public | 969.448 → 866.964 | 398.623 → 356.064 | -10.571% | -10.677% |
| bitget-public | 488.218 → 500.975 | 322.078 → 411.294 | +2.613% | +27.700% |
| backpack-public | 141.007 → 146.225 | 64.399 → 68.003 | +3.701% | +5.596% |
| blofin-public | 126.294 → 203.575 | 103.820 → 180.565 | +61.191% | +73.921% |
| mexc-public | 613.688 → 570.571 | 239.239 → 245.021 | -7.026% | +2.417% |
| polymarket-public | 43.380 → 37.608 | 20.514 → 32.828 | -13.306% | +60.027% |
| kucoin-public | 366.257 → 338.635 | 164.770 → 183.214 | -7.542% | +11.194% |
| bingx-public | 105.411 → 103.474 | 62.896 → 65.124 | -1.838% | +3.542% |
| bybit-public | 169.661 → 159.148 | 121.189 → 120.211 | -6.196% | -0.807% |
| okx-public | 485.551 → 379.459 | 329.970 → 348.402 | -21.850% | +5.586% |
| cryptocom-public | 163.381 → 147.327 | 103.988 → 133.773 | -9.826% | +28.643% |
| weex-public | 264.365 → 222.194 | 182.080 → 219.295 | -15.952% | +20.439% |
| grvt-public | 173.654 → 160.833 | 128.712 → 134.267 | -7.383% | +4.316% |
| phemex-public | 483.981 → 489.871 | 140.853 → 154.617 | +1.217% | +9.772% |
| krakenspot-public | 323.705 → 323.304 | 136.889 → 165.300 | -0.124% | +20.755% |
| hyperliquidspot-public | 51.475 → 49.347 | 45.082 → 48.543 | -4.134% | +7.677% |
| kraken-public | 1268.338 → 1029.288 | 175.458 → 122.316 | -18.847% | -30.288% |
| lighter-public | 283.789 → 266.427 | 142.914 → 155.539 | -6.118% | +8.834% |
| lighter-robinhood-public | 241.062 → 240.825 | 96.407 → 91.236 | -0.098% | -5.364% |
| bitrue-public | 255.697 → 158.322 | 200.963 → 151.123 | -38.082% | -24.801% |
| hyperliquid-public | 421.476 → 386.147 | 62.481 → 76.512 | -8.382% | +22.456% |
| paradex-public | 356.699 → 417.814 | 329.486 → 478.873 | +17.133% | +45.339% |
| txflow-public | 128.577 → 115.848 | 72.660 → 97.603 | -9.900% | +34.328% |
| binancespot-public | 851.125 → 922.073 | 648.056 → 742.196 | +8.336% | +14.527% |
| binanceinverse-public | 228.885 → 226.439 | 93.840 → 97.511 | -1.069% | +3.912% |
| binancepm-public | 209.904 → 248.602 | 99.363 → 126.265 | +18.436% | +27.074% |
| bybitinverse-public | 125.486 → 114.860 | 35.482 → 37.251 | -8.468% | +4.986% |
| bybitspot-public | 122.618 → 173.047 | 88.591 → 129.854 | +41.127% | +46.577% |
| bybitdemo-public | 223.708 → 351.145 | 155.915 → 284.122 | +56.966% | +82.229% |
| hyrotrader-public | 32.060 → 25.655 | 29.997 → 29.463 | -19.978% | -1.780% |
| okxeu-public | 831.695 → 507.972 | 569.199 → 332.231 | -38.923% | -41.632% |
| okx-inverse-public | 494.369 → 478.974 | 385.258 → 348.540 | -3.114% | -9.531% |
| okxeu-xperp-public | 445.969 → 490.240 | 314.090 → 363.927 | +9.927% | +15.867% |
| kucoinspot-public | 544.346 → 559.915 | 433.487 → 476.472 | +2.860% | +9.916% |
