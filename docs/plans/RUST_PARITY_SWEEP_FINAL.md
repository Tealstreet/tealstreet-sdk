# Final parity sweep verification

Historical `96a39` runtime only. Latest merged-source checkpoint: [RUST_PARITY_SWEEP_PRODUCT_UNION.md](RUST_PARITY_SWEEP_PRODUCT_UNION.md); final union gate receipts are in PR #1340.

Frozen runtime: `96a39f04219d1777b3a898808de9823d1b8fbd4e`.
Replay source fingerprint (1689 files): `a3f0cda5464f5d01605b1d907c66d8eeacc02df83af6cebf688bd7ce128f7d3e`.
Workflow fingerprint: `fffd4da88eafac670ebf3f8af35cd4d998cc9f130f4c84f895fd9531beeaab08`.

All **71 planned local venue gates PASS: 33 public and 38 private**. The additional combined-engine GRVT private replay also passes, for **72 accepted replay checks** across **74 attempts**, including the preserved first WOO and Breakout failures. Accepted evidence covers **136 unique manifest steps** with no source or binding ownership drift. The initial sequence stopped at WOO public steady CPU (TS 22.272 / Rust 24.042 ms); an independent source/artifact/per-run review justified exactly one unchanged three-pair diagnostic control, which passed before the remaining steps continued. The tail later stopped on Breakout main CPU (TS 15.463 / Rust 15.906 ms), with steady CPU/output/memory passing. An independent review justified exactly one unchanged three-pair Breakout control; it passed at main 18.8 / 14.6 ms and steady 7.1 / 4.1 ms before private/contract checks passed. Both failed binaries are identical to earlier passing local binaries, and same-head CI independently passed both public gates. No further unchanged retries were allowed or performed. Public scores use three runs; private scores are informational and gates require correctness.

- Locked workspace: **2,633 native tests +3 doctests**; strict native/wasm32 Clippy, formatting and bans PASS. The Rust source at the frozen runtime is byte-identical to the passing 4e3dd1622e source (428 files; fingerprint `edfec9d8b6ee80724913cd7136273524549716118268a1f8e8b5acf1ad38f787`).
- Root typecheck/lint/test: **44/28/27 successful tasks**. Both SDK targets pass **7,652 tests across 519 files** on the frozen runtime.
- Actual combined engine: **421 compiled SDK tests across 12 files**, **68 host tests**. Shared harness **574/574**. Venue contract controls **19/19**; frame-wire, scripted smoke and server adapter integration **11/11**.
- All **26 unique feature budgets PASS** at unchanged **1,258,291 bytes**, across 29 profile builds including parent OKX. The three duplicate alias builds share existing Kraken, Hyperliquid and Lighter features. Largest: paradex **1,255,331 bytes**, headroom **2,960 bytes**. Node/web sizes and hashes agree.
- Combined module (not a selected-venue budget artifact): **11,536,529 bytes** per target; SHA256 `d2a39ffd46f51f88f22f6ca6368e8872be93cbee4d1bf24d5287986581b2dbbc`.
- CI inventory: **101 replays (38 public/63 private)** in 29 venue-profile jobs plus 5 quality jobs. Exact final PR-head CI must be green before merge; this document records local receipts, not a prediction of CI.

| Feature | Node bytes | Web bytes |
| --- | ---: | ---: |
| apexomni | 1,002,463 | 1,002,463 |
| asterdex | 1,044,472 | 1,044,472 |
| backpack | 791,729 | 791,729 |
| binance | 928,181 | 928,181 |
| bingx | 864,873 | 864,873 |
| bitget | 961,174 | 961,174 |
| bitrue | 741,668 | 741,668 |
| blofin | 810,728 | 810,728 |
| breakoutprop | 649,489 | 649,489 |
| bybit | 957,296 | 957,296 |
| coinbase | 1,001,865 | 1,001,865 |
| cryptocom | 678,277 | 678,277 |
| grvt | 997,328 | 997,328 |
| hyperliquid | 1,054,492 | 1,054,492 |
| kraken | 1,060,611 | 1,060,611 |
| kucoin | 776,174 | 776,174 |
| lighter | 1,000,003 | 1,000,003 |
| mexc | 798,326 | 798,326 |
| okx | 819,500 | 819,500 |
| orderly | 812,325 | 812,325 |
| paradex | 1,255,331 | 1,255,331 |
| phemex | 772,553 | 772,553 |
| polymarket | 848,894 | 848,894 |
| txflow | 947,838 | 947,838 |
| weex | 811,352 | 811,352 |
| woo | 864,684 | 864,684 |

| Public gate | TS / Rust main CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: |
| backpack | 143.0 / 64.8 | 77.7 / 29.7 | 16.0 / 2.5 |
| binance | 183.5 / 84.0 | 98.3 / 32.6 | 18.0 / 4.0 |
| binancespot | 581.4 / 400.9 | 252.1 / 122.4 | 27.4 / 10.8 |
| binanceinverse | 169.7 / 62.6 | 110.8 / 34.4 | 13.7 / 1.9 |
| binancepm | 189.0 / 88.4 | 108.3 / 35.8 | 17.8 / 4.0 |
| bingx | 107.3 / 64.4 | 40.6 / 26.3 | 17.2 / 3.5 |
| bitget | 679.4 / 575.2 | 456.3 / 381.7 | 20.9 / 4.6 |
| bitrue | 126.7 / 106.0 | 72.7 / 55.7 | 17.0 / 3.3 |
| blofin | 126.8 / 104.5 | 68.4 / 48.8 | 16.4 / 3.0 |
| bybit | 263.2 / 168.4 | 143.3 / 69.8 | 22.2 / 4.5 |
| bybitinverse | 116.7 / 32.5 | 74.7 / 12.6 | 14.2 / 1.7 |
| bybitspot | 146.6 / 105.8 | 79.2 / 46.2 | 16.2 / 2.7 |
| bybitdemo | 212.3 / 152.7 | 110.9 / 63.0 | 21.8 / 4.5 |
| hyrotrader | 22.0 / 18.1 | 3.9 / 3.2 | 13.3 / 0.8 |
| cryptocom | 99.4 / 87.9 | 52.7 / 37.3 | 13.5 / 2.1 |
| kraken | 1088.6 / 209.3 | 813.9 / 123.9 | 16.8 / 2.7 |
| krakenspot | 309.3 / 187.8 | 230.4 / 102.5 | 16.4 / 4.3 |
| hyperliquid | 357.2 / 52.5 | 29.4 / 17.3 | 24.2 / 2.6 |
| hyperliquidspot | 42.0 / 38.4 | 13.2 / 11.5 | 18.6 / 6.0 |
| kucoin | 510.1 / 294.0 | 328.7 / 179.9 | 21.5 / 4.6 |
| mexc | 486.0 / 235.4 | 287.8 / 126.4 | 27.4 / 5.0 |
| phemex | 372.8 / 96.4 | 254.9 / 32.5 | 20.0 / 4.5 |
| weex | 142.3 / 96.3 | 73.6 / 42.4 | 19.0 / 3.5 |
| woo | 55.4 / 41.0 | 21.1 / 16.7 | 14.0 / 1.5 |
| asterdex | 155.5 / 83.0 | 54.4 / 36.4 | 27.8 / 6.2 |
| txflow | 130.3 / 84.5 | 33.5 / 24.6 | 17.2 / 1.4 |
| lighter | 282.3 / 166.8 | 179.9 / 90.7 | 17.2 / 2.9 |
| lighter-robinhood | 236.1 / 78.6 | 165.0 / 34.8 | 19.8 / 5.0 |
| grvt | 131.3 / 101.9 | 77.3 / 57.7 | 26.4 / 1.8 |
| breakoutprop | 18.8 / 14.6 | 7.1 / 4.1 | 11.8 / 0.8 |
| okx-linear-public | 485.7 / 410.9 | 335.9 / 272.4 | 18.7 / 4.7 |
| okx-inverse-public | 2129.8 / 1795.2 | 1562.6 / 1319.1 | 19.4 / 5.4 |
| okx-eea-public | 1726.9 / 1554.3 | 1321.6 / 1179.5 | 22.2 / 7.3 |

Private selected profiles passed: backpack, binance, binancespot, binanceinverse, binancepm, bingx, bitget, bitgetinverse, bitrue, blofin, bybit, bybitinverse, bybitspot, bybitdemo, hyrotrader, cryptocom, kraken, krakenspot, hyperliquid, hyperliquidspot, kucoin, mexc, phemex, weex, woo, asterdex, asterdex (asterdex-v3-private), txflow, lighter, lighter (lighter-testnet-private), lighter-robinhood, lighter-robinhood (lighter-robinhood-testnet-private), grvt, breakoutprop, OKX, OKX EU, OKX demo and OKX EU demo. The independent combined GRVT replay is counted separately.

Evidence boundaries: Breakout public uses its authorized documented scripted fixture because legacy DXtrade requires login. Hyro origins/live credentials are unavailable; its documented synthetic capture is not live connectivity proof. Other public recordings preserve their source fixtures; new OKX inverse/EEA captures and landed spot captures are actual public recordings. Private fixtures/signing vectors do not claim funded-account validation. GRVT remains PERPETUAL only under the explicit landed product decision; superseded synthetic non-perpetual work is not current capability evidence.

Original failed builds/gates and review findings remain in `RUST_PARITY_SWEEP.md` and PR #1340 comments, including the inverse CPU miss before the independently reviewed real price-index optimization. No scoring threshold, financial normalization, output assertion or WASM ceiling was weakened. TS-unsupported, live-key and separate phase 9 work remains itemized in the PR and `RUST_PARITY_SWEEP_AUDIT.md`.

Independent correctness reviews, repair re-reviews and final evidence review are posted on PR #1340. This final record supersedes the pre-landing numbers in `RUST_PARITY_SWEEP_GATES.md` without erasing them.
