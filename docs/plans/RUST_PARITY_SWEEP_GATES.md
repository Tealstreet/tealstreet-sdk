# Parity sweep final local evidence

Current final receipts: [RUST_PARITY_SWEEP_FINAL.md](RUST_PARITY_SWEEP_FINAL.md).

This records the completed pre-`86ed5e3d2e` landing-integration sequence.
The later landing union and PR-review repairs require fresh affected checks;
the progress plan records that follow-up without erasing these receipts.

Frozen runtime: `680bdfcbb016b0515030232c53a75e5f6b574a5a` (the final documentation commit changes no runtime).
Source fingerprint over1,595 relevant files: `f4a3b16b3b21ac879b404c75caa08893fb2c42d8a534778e85a380ec0466db84`.
Workflow fingerprint: `7247a7ab4e5ccc1159d8bb9d9c4617b6208182c70365602e89fa2e3ca9aed2bd`.

All57 planned local venue gates PASS:27public and30private, including Binance USD-M/PM/Spot/COIN-M and each touched venue/profile. The additional mixed GRVT private replay on the combined engine also passes (58recorded gate checks total). The serial runner completed106/106 steps without a failure or retry; independent aggregation review confirms counts and source stability.

- Locked workspace:2,464native unit/integration tests +3doctests; strict native and wasm32 workspace Clippy, formatting and bans all PASS.
- Root typecheck44/44, lint28/28, test27/27 tasks PASS. Both SDK test targets pass7,491tests; final bounded SDK typecheck covers the last fixture edit.
- Compiled SDK379/379 across11files and host68/68 PASS on the final rebuilt combined engine. Harness537/537 PASS; venue controls Bitget4+Aster3+Breakout4=11; integration controls frame-wire1+scripted smoke9=10.
- The combined engine is10,889,392bytes (not a single-venue budget artifact), node/web SHA256 `4197a0ed735365ca5b2f7eaa080b9b2aaa28562c1746096b2090de03001a3e67`.
- All25 single-venue node/web budgets PASS below the unchanged1,258,291-byte ceiling. Maximum Paradex1,255,331bytes; headroom2,960bytes.

| Single-venue feature | Node bytes | Web bytes |
| --- | ---: | ---: |
| apexomni | 1,002,463 | 1,002,463 |
| asterdex | 1,044,472 | 1,044,472 |
| backpack | 791,729 | 791,729 |
| binance | 928,133 | 928,133 |
| bingx | 864,873 | 864,873 |
| bitget | 961,174 | 961,174 |
| bitrue | 741,668 | 741,668 |
| blofin | 810,728 | 810,728 |
| breakoutprop | 649,489 | 649,489 |
| bybit | 957,296 | 957,296 |
| coinbase | 1,001,865 | 1,001,865 |
| cryptocom | 678,277 | 678,277 |
| grvt | 997,292 | 997,292 |
| hyperliquid | 1,011,666 | 1,011,666 |
| kraken | 882,526 | 882,526 |
| kucoin | 776,174 | 776,174 |
| lighter | 993,184 | 993,184 |
| mexc | 798,326 | 798,326 |
| okx | 814,314 | 814,314 |
| orderly | 812,325 | 812,325 |
| paradex | 1,255,331 | 1,255,331 |
| phemex | 772,553 | 772,553 |
| polymarket | 848,894 | 848,894 |
| weex | 811,352 | 811,352 |
| woo | 864,684 | 864,684 |

Public gates use three runs. CPU values are main-thread and steady CPU, retained memory includes WASM linear memory; every public row passes strict output, CPU and memory comparisons. Private gates require correctness; their CPU/memory are informational.

| Public venue | TS / Rust CPU ms | TS / Rust steady CPU ms | TS / Rust retained MB |
| --- | --- | --- | --- |
| backpack | 143.1 / 65.0 | 71.8 / 28.8 | 14.8 / 1.8 |
| binance | 281.3 / 155.7 | 146.9 / 80.2 | 16.4 / 2.5 |
| binancespot | 682.4 / 417.9 | 296.4 / 120.5 | 24.2 / 7.7 |
| binanceinverse | 198.7 / 87.7 | 134.0 / 50.8 | 13.3 / 1.5 |
| binancepm | 287.5 / 162.5 | 159.8 / 73.5 | 16.2 / 2.5 |
| bingx | 136.3 / 83.6 | 57.6 / 38.0 | 16.0 / 2.2 |
| bitget | 1724.3 / 682.1 | 1056.3 / 457.0 | 19.0 / 3.7 |
| bitrue | 150.1 / 114.2 | 77.3 / 57.4 | 16.0 / 2.7 |
| blofin | 143.4 / 112.5 | 75.3 / 51.7 | 15.4 / 2.3 |
| bybit | 271.0 / 183.6 | 143.6 / 78.6 | 19.6 / 3.1 |
| bybitinverse | 121.7 / 35.9 | 76.5 / 14.2 | 13.7 / 1.3 |
| bybitspot | 145.7 / 121.0 | 79.3 / 55.7 | 15.1 / 2.1 |
| bybitdemo | 210.1 / 165.1 | 105.9 / 64.9 | 19.2 / 3.1 |
| hyrotrader | 20.9 / 17.4 | 5.0 / 3.0 | 13.3 / 0.8 |
| cryptocom | 98.8 / 89.9 | 50.2 / 36.9 | 13.0 / 1.8 |
| kraken | 925.0 / 193.4 | 687.0 / 92.8 | 15.3 / 2.1 |
| kucoin | 608.3 / 407.7 | 398.9 / 256.5 | 19.1 / 3.9 |
| mexc | 554.3 / 264.8 | 331.3 / 143.4 | 22.4 / 3.5 |
| phemex | 528.2 / 156.5 | 350.4 / 68.5 | 17.3 / 2.9 |
| weex | 144.4 / 104.5 | 68.3 / 46.8 | 16.8 / 2.5 |
| woo | 88.2 / 67.9 | 44.7 / 36.9 | 13.6 / 1.2 |
| asterdex | 197.7 / 103.4 | 72.4 / 49.1 | 25.4 / 5.6 |
| grvt | 178.6 / 171.1 | 112.1 / 106.6 | 26.1 / 1.5 |
| breakoutprop | 50.2 / 14.7 | 6.3 / 3.9 | 12.5 / 0.8 |
| okx linear | 488.2 / 419.4 | 339.4 / 277.0 | 17.5 / 4.2 |
| okx inverse | 2046.0 / 1906.9 | 1484.1 / 1415.9 | 18.3 / 4.9 |
| okxeu X-Perp | 1707.3 / 1596.3 | 1305.2 / 1217.1 | 19.7 / 5.8 |

Selected OKX node/web SHA256: `4eaad3f31104d8e87601016e51236a32c1415b67771dbfd0a5678e2a4a0d6550`.
Mixed GRVT private:130records,17commands,8products,72frames; strict native identity, signatures, account quantities and command results pass on both combined and selected GRVT artifacts.

No funded live-account proof is claimed. Breakout's public fixture is its authorized documented DXtrade script; Hyro's fixture is documented/synthetic. See the audit for TS-unsupported capabilities, live-key needs and separate phase9 adoption differences. Public recording scope does not prove every market or full account lifetime memory retention.

Failures discovered before the final frozen run remain recorded in the progress document and local logs, including the original EEA performance attempt/control, semantic constructor collision, missing EEA fixture input, rustfmt line, and GRVT keepalive/subscription identity issues. Economic/output comparisons, scoring thresholds and budgets were not relaxed. Exact raw GRVT heartbeats are independently audited before subscription identity canonicalization; malformed/missing/extra replies and pre-credential sends remain failures.

CI configured on this source:91replays (34public+57private),25venue jobs and5quality jobs. CI results and local adversarial PR reviews must still pass before merge; these local receipts do not assert future CI success.
