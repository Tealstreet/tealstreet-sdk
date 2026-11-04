import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Single-venue family bundles: 1.28125 MiB (RUST_WASM_FAMILIES.md), raised from
// 1.25 MiB for FetchAccount decoding in every identity venue. The combined ceiling caps
// total package size; Sam raised the combined ceiling to 16 MiB on 2026-10-27.
export const WASM_BUDGETS = Object.freeze({ venue: 1_343_488, combined: 16_777_216 });
// #2219 Base64Url/EvmHex signing on the #2208 tip: CI Bitget 1,482,166 rounded up to KiB (overseer ceiling 1448 KiB);
// #2224 serial call pacing: CI 1,482,920, +1 KiB under the 2 KiB shared-code rule.
// AsterDEX: managed sockets (#2212) measured 1,384,485 in CI, within the standing 5% venue
// allowance over the opt-s 1,330,120 cap; #2219 signing encodings: CI 1,385,845, then #2224 serial call
// pacing: CI 1,387,161, rounded up to the KiB.
// Bybit managed-socket adoption: overseer ruling (ddi2il) 2026-10-26, 1412 KiB, replacing the
// Plan-adoption allowance; it removes Bybit's legacy socket supervisor and inventory sites.
// #2219 shared signing encodings: CI 1,445,978, 1413 KiB by overseer ruling.
// Kraken composed CI: 1,399,905, then #2219 signing encodings 1,400,974, rounded up to KiB under the
// standing venue allowance.
// #2223 shared credential core: CI Bitget/USDC 1,481,804 and AsterDEX 1,385,869 rounded up to
// KiB, each a <=2 KiB shared-code raise pre-approved by the overseer while the combined holds.
// #2223 composed with #2219 on the #2245 tip: CI Bitget 1,482,944, AsterDEX 1,386,952 and Kraken
// 1,402,082, each rounded up to the KiB as a <=2 KiB shared-code raise (standing ruling);
// the overseer confirmed Bitget 1449 KiB with a 1452 KiB ceiling.
// Kraken beta error custody CI: 1,404,713 bytes, rounded up under the standing
// 5% venue allowance; unrelated venue overrides remain unchanged.
// #2223 on #2247: CI Kraken 1,405,315 rounded up to 1373 KiB, a <=2 KiB shared-code raise (pre-approved).
// #2223 on #2224/#2248: CI Bitget 1,483,869 (1450 KiB, within the 1452 KiB ceiling) and AsterDEX
// 1,387,670 (1356 KiB), each a <=2 KiB shared-code raise.
// #2223 on the #2259 tip: CI Bybit 1,446,915 rounded up to 1414 KiB, a <=2 KiB shared-code raise.
// AsterDEX host address setup (#2233): CI 1,417,307, rounded up to 1385 KiB under the
// standing venue allowance; the larger of it and the incoming cap.
// Tip after #2223 plus #2233: CI AsterDEX 1,418,786-1,419,038 rounded up to 1386 KiB, a <=2 KiB shared-code raise.
// #2262 shared hosted-codec layout (Bitunix hosted signing): CI AsterDEX 1,419,317, rounded up
// to 1387 KiB, a <=2 KiB shared-code raise over 1385 KiB; the larger of it and the incoming cap.
// #2279 (engine uncertain-placement fence and re-ask): CI Bitget 1,485,945 (1452 KiB, at the
// ceiling), AsterDEX 1,420,478 (1388 KiB), Bybit 1,448,312 (1415 KiB) and Kraken 1,407,904
// (1375 KiB), each a <=2 KiB shared-code raise. CI on b666a1a9a54 (with the re-ask flag):
// Kraken 1,408,110, rounded up to 1376 KiB (overseer: push the CI number now).
// AsterDEX status-table classification (#2270) composed on the tip with the #2279 fence engine:
// 1,423,639 measured locally, rounded up to 1391 KiB under the standing venue allowance.
// #2288 (engine fence without a lock scope, Phase B): CI Bitget 1,487,624 (1453 KiB; the
// overseer raised the P1 fence ceiling to 1456 KiB), Bybit 1,452,004 (1418 KiB), AsterDEX
// 1,422,195 (1389 KiB) and Kraken 1,409,432 (1377 KiB), within the overseer's P1 fence ruling
// (at most +4 KiB over the pre-fence tip). Bybit fence opt-in (#2291): CI 1,452,035,
// rounded up to 1419 KiB. Engine settle-only condition wait (#2274): CI 40359589419 Bitget
// 1,488,071, rounded up to 1454 KiB (shared engine growth, <=2 KiB pre-approval).
// #2309 run 40467597908, job 122069099302: CI Kraken/Spot 1,410,076 (+337 B ungated verdict surface).
// Rounded to 1,411,072 (+1 KiB) under the <=2 KiB shared-code rule.
// #2317 final HL/HLSpot: CI run 40506842978, job 122199595636 measures 1,418,513 B.
// ddi2il rule: ceil KiB + 2 KiB = 1388 KiB; required correction and breakdown in PR.
// One linked secp256k1/keccak implementation; dependency features and combined cap unchanged.
// #2334 hosted-signing feature split, run 40610920529 artifacts: Paradex 1,403,676 and HL/HLSpot
// 1,363,789 B; each lowered to ceil KiB + 2 KiB (1373 and 1334 KiB).
// Cutover 10-29: HL/HLSpot raised to 1344 KiB (release run 40859840322 measured 1,366,837 B),
// leaving room for HL structured remote signing (#2362).
// #2356 European Options cutover: local optimized Binance family 1,448,609 B;
// #2368 links the advertised Options decoders: 1,460,760 B locally; ddi ruled
// ceil KiB + 2 KiB (1429 KiB). Split Options from the family after cutover.
// #2367 Paradex keyless remote signing (device descriptor parser, PublicKey setup,
// hosted-wallet): local optimized Paradex 1,482,511 B; ddi ruled ceil KiB + 4 KiB
// (1452 KiB) for local/CI drift, without waiting on CI. Split the device build later.
// Cutover 10-29: asterdex raised to 1403 KiB (release run 40863850081 measured 1,434,289 B).
// Frame-fed engine core (frameFed mode, ingest skeleton, VENUE_STATE): local optimized
// Bybit 1,455,883 B; ddi ruled ceil KiB + 2 KiB for the frame-fed work (1424 KiB).
// Bybit frame-fed (VENUE_STATE) with the merged ingest decoders: 1,468,570 B (1437 KiB).
// Hyperliquid frame-fed with the merged ingest decoders: 1,377,242 B (1347 KiB).
// Frame-fed ingest on master efe84b645c5 (every beta family measured locally): Binance
// 1,484,450, Bitget 1,522,283, Bybit 1,479,891, AsterDEX 1,453,174, Kraken 1,448,429 and
// Paradex 1,515,321 B; ddi ruled ceil KiB + 2 KiB for frame-fed core growth.
// Binance frame-fed USD-M/COIN-M (limit-stop VENUE_STATE, source-leverage ingest) on the
// cap-sweep master: local optimized Binance 1,501,037 B; ceil KiB + 2 KiB (1468 KiB), frame-fed ruling.
// Binance spot and PM frame-fed (VENUE_STATE v2 routing): local 1,505,790 B, +4,753 B;
// ceil KiB + 2 KiB (1475 KiB), frame-fed ruling.
// LOCAL Rust algos (chaser, TWAP, exec) link into every family (Sam, option a):
// each cap is its CI wasm-family node size on master 3617d7fad63 (run 41052374903)
// plus 10 KiB, rounded up to a KiB.
// Tightest families at CI (abc18ecd30a) + 10 KiB, rounded up to KiB, before the exec store fix.
// #2650 Bitget flat leverage read: CI 2,140,041 B; ddi's rule, CI + 10 KiB, rounded up to a KiB.
// #2700 Phemex replacement condition wait: CI 1,931,732 B; ddi's rule, CI + 10 KiB, rounded up to a KiB.
// #2812 readPositions: CI run 41375427208; Sam's order, CI + 10 KiB, rounded up to a KiB, for every family over its cap.
// #2819 hosted Nuke fresh read: CI run 41378308674; ddi's fix-forward, CI + 10 KiB, rounded up to a KiB, for every family over its cap.
// 11-02 nuke account cancel (#2831, #2852): CI run 41415587553 measured backpack 1,832,476, bitrue
// 1,772,155, breakoutprop 1,471,040, coinbase 1,773,663, grvt 1,739,939, kucoin 1,550,412, kucoinspot
// 2,063,720, mexc 1,687,491, orderly 1,682,273, polymarket 1,560,737, txflow 1,790,189, woo 2,327,820 B.
// ddi2il ruling: each raised to CI size plus 10 KiB, rounded up to the KiB.
// CI run 41423920329: bitgetusdc 2,242,622, hyperliquidspot 2,037,311, krakenspot 2,065,964, lighter-robinhood
// 1,938,379 B; each CI size plus 10 KiB, rounded up to the KiB, and its shared-family sibling to the same.
// CI run 41564406653 (master 473d18325b0): backpack 1,849,438 (run 41560014089), bitrue 1,787,754,
// breakoutprop 1,483,576, coinbase 1,789,891, grvt 1,756,323, kucoin 1,566,571, kucoinspot 2,087,973,
// orderly 1,697,103, polymarket 1,576,543, txflow 1,806,532, weex 1,898,411, woo 2,354,045 B. ddi2il
// ruling: each raised to CI size plus 10 KiB, rounded up to the KiB.
// #2945 managed-socket data silence: CI run 41599669844 measured bingx 1,970,123 B; ddi2il ruling,
// CI size plus 10 KiB, rounded up to the KiB.
// #2958 shared broker-role setup: CI 41617609699 measured BloFin 1,923,215, Bybit 2,148,560,
// OKX 1,910,027 and Kraken 2,097,992 B; ddi2il approved CI + 10 KiB rounded up, including the Kraken sibling.
// #2961 KuCoin provider custody: CI 41611993463 measured 1,630,161 / 2,211,621 B.
// ddi2il approved each CI size plus 10 KiB, rounded up to the KiB, after shared-provider attribution.
// #2981 WOO account-read attribution: CI measured 2,365,883 B; ddi2il approved
// CI size plus 10 KiB, rounded up to the KiB.
// Batch #2989/#2994/#2995 on master 05275f74aec: CI run 41774217377 measured bingx 1,987,316,
// hyperliquid 2,066,112 and phemex 2,023,431 B; ddi2il ruling, CI size plus 10 KiB, rounded up to the KiB.
export const WASM_VENUE_OVERRIDES = Object.freeze({
  apexomni: 1_957_888,
  asterdex: 2_097_152,
  backpack: 1_860_608,
  binance: 2_152_448,
  bingx: 1_997_824,
  bitget: 2_275_328,
  bitgetusdc: 2_276_352,
  bitrue: 1_798_144,
  bitunix: 1_824_768,
  blofin: 1_934_336,
  breakoutprop: 1_494_016,
  bybit: 2_159_616,
  coinbase: 1_800_192,
  cryptocom: 1_701_888,
  grvt: 1_767_424,
  hyperliquid: 2_076_672,
  hyperliquidspot: 2_065_408,
  kraken: 2_108_416,
  krakenspot: 2_108_416,
  kucoin: 1_640_448,
  kucoinspot: 2_222_080,
  lighter: 1_965_056,
  'lighter-robinhood': 1_967_104,
  mexc: 1_714_176,
  okx: 1_921_024,
  orderly: 1_708_032,
  paradex: 2_196_480,
  phemex: 2_034_688,
  polymarket: 1_587_200,
  txflow: 1_817_600,
  weex: 1_908_736,
  woo: 2_376_704,
});
export const wasmBudgetBytes = (profile, venue) =>
  profile === 'venue' && venue !== undefined && Object.hasOwn(WASM_VENUE_OVERRIDES, venue) ? WASM_VENUE_OVERRIDES[venue] : WASM_BUDGETS[profile];
export const wasmBudgetError = (bytes, profile, venue) => {
  if (!Object.hasOwn(WASM_BUDGETS, profile) || !Number.isSafeInteger(bytes) || bytes <= 0) return 'invalid wasm budget input';
  const budget = wasmBudgetBytes(profile, venue);
  return bytes > budget ? `${profile} wasm ${bytes} exceeds ${budget} bytes` : undefined;
};

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const { values } = parseArgs({ options: { combined: { type: 'boolean', default: false }, venue: { type: 'string' } } });
  const profile = values.combined ? 'combined' : 'venue';
  const { venue } = values;
  const artifacts = ['web', 'node'].map((target) => {
    const path = `sdk-rs/hosts/wasm/pkg/${target}/safe_cex_wasm_bg.wasm`;
    const bytes = readFileSync(path);
    return { path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex'), error: wasmBudgetError(bytes.length, profile, venue) };
  });
  const report = { profile, venue, budgetBytes: wasmBudgetBytes(profile, venue), artifacts };
  mkdirSync('sdk-rs/harness/out', { recursive: true });
  writeFileSync(`sdk-rs/harness/out/wasm-budget-${profile}.json`, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
  if (artifacts.some((artifact) => artifact.error)) process.exit(1);
}
