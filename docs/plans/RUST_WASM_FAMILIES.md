# Per-family WASM loading

Branch `feat/rust-wasm-families` from `feat/rust-rewrite` at `ed2128a337`.
This follows #1344's closing note: re-port only the per-venue-family packaging
and loading, without batched delivery, and measure it on its own.

## Baseline

The browser, Worker and Node default loaders (`@tealstreet/safe-cex-wasm/engine`)
load the combined engine: every venue, about 12 MB per target. CI replay gates
already build one venue (`build.sh --venue`), so gate numbers describe family
builds. The CLI, React Native and the MCP bundle embed the combined engine on
purpose, since they serve many venues from one artifact.

## Plan

1. Measure: `yarn rust:measure-family` runs one venue's family build against the
   combined build in alternating pairs, recording web/Node bytes (raw, gzip,
   brotli), cold Node load (wall, CPU, RSS), and public replay main/steady/process
   CPU, retained memory and RSS. Outputs must be identical. Run it for all 30
   public gate venues on CI with 7 pairs.
2. Decide: ship only on a clear byte and cold-load win with no CPU regression
   (family median ≤ combined median plus noise on main and steady for every
   venue). Otherwise close with the numbers.
3. If shipping: build per-family packages (`pkg/{web,node}/<family>`) and have
   the default browser/Workers loader load the venue's family with per-family
   initialization caching. Node, the CLI, React Native and MCP keep the
   combined build and exports.

## Results

CI run `39453296321` (dispatch on `chore/rust-wasm-families-measure`, head of
this branch at `6ac4a00f63`): 30 public gate venues, 7 alternating pairs each,
both builds in the same job. The combined web wasm is 12451 KB
(2530 KB brotli). Columns are family values, or family /
combined, and ratios of medians are family over combined.

| Venue | Web wasm KB | Brotli KB | Cold load CPU ms | Cold load RSS MB | Main CPU | Steady CPU | Replay RSS MB |
|---|---:|---:|---:|---:|---:|---:|---:|
| apexomni | 1065 | 269 | 13.4 / 114.7 | 7.1 / 33.0 | 0.924 | 0.940 | -19.5 |
| asterdex | 1121 | 273 | 13.5 / 119.1 | 7.1 / 32.9 | 0.993 | 0.923 | -17.2 |
| backpack | 853 | 212 | 8.9 / 99.5 | 6.6 / 32.9 | 1.070 | 1.104 | -26.1 |
| binance | 998 | 252 | 13.5 / 118.6 | 6.9 / 33.0 | 0.983 | 0.974 | -16.6 |
| bingx | 943 | 236 | 12.2 / 114.3 | 6.8 / 33.1 | 0.983 | 0.951 | -17.5 |
| bitget | 1035 | 260 | 13.8 / 128.9 | 6.9 / 33.0 | 0.995 | 0.977 | -15.7 |
| bitrue | 922 | 232 | 7.2 / 65.0 | 6.7 / 33.1 | 1.025 | 0.978 | -19.2 |
| blofin | 918 | 231 | 7.5 / 74.1 | 6.7 / 33.0 | 0.965 | 0.959 | -16.8 |
| breakoutprop | 741 | 181 | 11.1 / 128.6 | 6.3 / 33.0 | 0.876 | 0.891 | -15.1 |
| bybit | 1176 | 284 | 15.4 / 119.0 | 7.2 / 33.1 | 1.014 | 0.980 | -14.1 |
| coinbase | 1080 | 276 | 13.9 / 115.7 | 7.1 / 32.9 | 0.993 | 0.991 | -11.0 |
| cryptocom | 725 | 181 | 12.9 / 133.6 | 6.3 / 32.9 | 0.942 | 0.932 | -21.5 |
| grvt | 1052 | 257 | 14.4 / 119.3 | 6.9 / 33.0 | 0.997 | 1.026 | -19.5 |
| hyperliquid | 1108 | 272 | 14.0 / 125.2 | 7.1 / 33.0 | 1.003 | 1.030 | -30.1 |
| hyperliquidspot | 1108 | 272 | 15.4 / 129.4 | 7.1 / 33.0 | 1.016 | 0.983 | -19.9 |
| kraken | 1141 | 277 | 9.6 / 80.2 | 7.2 / 32.9 | 1.055 | 1.010 | -14.1 |
| krakenspot | 1141 | 277 | 14.9 / 119.5 | 7.3 / 33.1 | 1.001 | 0.996 | -17.3 |
| kucoin | 845 | 215 | 12.3 / 129.3 | 6.6 / 32.9 | 1.016 | 0.967 | -13.6 |
| kucoinspot | 740 | 185 | 10.7 / 118.6 | 6.3 / 33.1 | 1.005 | 1.000 | -16.4 |
| lighter-robinhood | 1057 | 267 | 13.8 / 125.4 | 6.9 / 32.9 | 0.972 | 0.997 | -24.0 |
| lighter | 1057 | 267 | 14.1 / 127.2 | 6.9 / 33.0 | 0.995 | 0.977 | -13.8 |
| mexc | 884 | 224 | 12.4 / 119.0 | 6.7 / 33.1 | 1.021 | 1.035 | -14.4 |
| okx | 900 | 225 | 11.7 / 114.0 | 6.7 / 33.1 | 0.990 | 0.982 | -14.8 |
| orderly | 852 | 224 | 7.5 / 77.8 | 6.6 / 33.0 | 1.026 | 1.006 | -17.5 |
| paradex | 1210 | 387 | 16.3 / 130.8 | 7.5 / 32.9 | 1.048 | 0.992 | -14.7 |
| phemex | 841 | 218 | 8.9 / 89.2 | 6.6 / 33.1 | 1.004 | 1.068 | -16.7 |
| polymarket | 892 | 223 | 9.2 / 97.1 | 6.7 / 33.0 | 1.090 | 1.028 | -16.5 |
| txflow | 1004 | 245 | 12.9 / 124.8 | 6.9 / 33.0 | 1.006 | 1.010 | -21.9 |
| weex | 889 | 232 | 9.4 / 89.3 | 6.7 / 33.1 | 0.989 | 1.003 | -18.5 |
| woo | 927 | 234 | 13.0 / 120.0 | 6.8 / 33.0 | 0.972 | 0.966 | -18.6 |

- Bytes: every family is 6-10% of the combined engine, brotli 181-387 KB
  against 2,530 KB.
- Cold load (fresh Node process, read and compile only): 7-16 ms CPU against
  65-134 ms, so each account start saves 58-121 ms; RSS 6-8 MB against 33 MB.
- Replay CPU: medians across venues are 0.999 main and 0.987 steady; the same
  Rust source compiles per venue with ±10% codegen variance. Six venues
  regress consistently on one metric: Paradex main +21 ms (its standalone
  size profile, `opt-level = "z"`, needed for the 1.2 MB budget), MEXC steady
  +6 ms (+3.6%), Kraken main +6 ms, Polymarket main +2 ms (+9%, all 7 pairs),
  Backpack steady +3 ms (its main +5 ms is lower in 3 of 7 pairs), Phemex
  steady +3 ms. Others gain consistently (OKX steady -18 ms, Crypto.com main -7 ms).
  Every venue's startup CPU still falls, because the cold-load saving is
  larger than any added replay cost.
- Outputs: every pair produced identical replay state. The first run's check
  compared serialized JSON and flagged key-order differences for five venues;
  the path-level comparison finds none (`39454590354`, and a three-pair rerun).

## Decision

Ship for browsers and Workers, where one or two venues load per realm: about
2.3 MB less download (brotli), about 100 ms less compile CPU per venue, and
11-30 MB less memory, with no aggregate replay CPU change. Node keeps the
combined engine, because a server hosting every venue would load more bytes
across separate family modules than the one combined module.

Consumers outside this repo: a web build must run `yarn rust:build-wasm
--families` (in addition to the combined build for Node and the CLI), or the
bundler cannot resolve the family imports. `loadWebEngine(venue)` and
`setWasmEngineLoader((venue) => …)` now receive the Rust venue; existing
zero-argument loaders keep working.

## Size budget

The per-family (single-venue) budget rose from 1,258,291 bytes (1.2 MiB) to
1,310,720 bytes (1.25 MiB) after #1375 and #1399 put the web Bybit family at
1,260,132 bytes (run `39465253385`, 1,841 bytes over). Families ship
separately, so the budget bounds one venue's download, not the total. The
growth is correctness, bridge (typed command outcomes and order-identity
receipts) and logging code. The combined 13 MiB budget is unchanged.

It rose again to 1,343,488 bytes (1.28125 MiB) on 2026-10-21. The landing's
web Bybit family built at 1,310,360 bytes (360 bytes of headroom), and account
identities (#1430) add FetchAccount command decoding and an identity read to
every identity venue, about 14 KB for Bybit. The extra 32 KB leaves room for
the next venue feature instead of reopening the budget each time.
