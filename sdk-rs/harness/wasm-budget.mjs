import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Single-venue family bundles: 1.28125 MiB (RUST_WASM_FAMILIES.md), raised from
// 1.25 MiB for FetchAccount decoding in every identity venue. The combined ceiling caps
// total package size; Sam raised the combined ceiling to 17 MiB on 2026-11-06 after #3140 measured 16,778,652 B.
export const WASM_BUDGETS = Object.freeze({ venue: 1_343_488, combined: 17_825_792 });
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
// 1,772,155, coinbase 1,773,663, grvt 1,739,939, kucoin 1,550,412, kucoinspot
// 2,063,720, mexc 1,687,491, orderly 1,682,273, polymarket 1,560,737, txflow 1,790,189, woo 2,327,820 B.
// ddi2il ruling: each raised to CI size plus 10 KiB, rounded up to the KiB.
// CI run 41423920329: bitgetusdc 2,242,622, hyperliquidspot 2,037,311, krakenspot 2,065,964, lighter-robinhood
// 1,938,379 B; each CI size plus 10 KiB, rounded up to the KiB, and its shared-family sibling to the same.
// CI run 41564406653 (master 473d18325b0): backpack 1,849,438 (run 41560014089), bitrue 1,787,754,
// coinbase 1,789,891, grvt 1,756,323, kucoin 1,566,571, kucoinspot 2,087,973,
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
// Master CI run 41783084277: apexomni 1,959,768, asterdex 2,099,176, backpack 1,860,786,
// binance 2,157,319, bybit 2,163,030, coinbase 1,800,411, hyperliquid/spot 2,069,390 and weex 1,909,946 B.
// ddi2il ruling: CI size plus 10 KiB, rounded up to the KiB; shared-family siblings carry the same cap.
// Read-cover batch (#3010/#3016/#3005/#3007 engine plus venue covers) at SDK 3b9f6b0fe2b: cut run 41796230942
// measured binance 2,172,329, bingx 2,002,885, bitget 2,293,016, bitunix 1,826,666, blofin 1,939,942, bybit 2,177,290, cryptocom 1,704,291, hyperliquid 2,083,308, lighter 1,968,734, mexc 1,717,351, orderly 1,710,152, paradex 2,198,375 B;
// ddi2il ruling: CI size plus 10 KiB rounded up to the KiB, shared-family siblings set equal.
// #3020 OKX read covers: cut run 41797896337 at e7e0251a87d measured okx 1,924,239 B;
// ddi2il ruling: CI size plus 10 KiB rounded up to the KiB.
// Paradex at b716d928440: cut run 41798863064 measured paradex 2,211,804 B;
// ddi2il ruling: CI size plus 10 KiB rounded up to the KiB.
// #3023 page rule at 2495abee4f1: cut run 41799876396 measured kraken 2,111,267, kucoinspot 2,224,069,
// lighter 1,980,286 and woo 2,377,495 B; ddi2il ruling: CI size plus 10 KiB rounded up, siblings equal.
// #3027 single-pass aggregate decoding: CI run 41800378776 measured AsterDEX 2,110,514 and BingX 2,009,588 B.
// ddi2il ruling: CI size plus 10 KiB rounded up to the KiB; typed envelope/row staging removes repeated JSON scans.
// #3035 CI run 41810769240: bitrue 1,803,160 B; txflow 1,819,613 B.
// CI + 10 KiB rounded up; TxFlow retains #3031's cap from pre-refactor CI 1,819,696 B.
// #3019 KuCoin 400002 resync: CI run 41794279458 measured kucoin 1,657,796 and kucoinspot 2,237,656 B.
// ddi2il ruling: CI size plus 10 KiB, rounded up to the KiB. Shared engine queue, issuance and
// admission code is newly linked by KuCoin; Binance already carries this machinery.
// #3028 refresh ask (step 3) at 466b16cd11e: PR run 41804469904 measured
// apexomni 1,974,257, asterdex 2,113,727, backpack 1,876,304, binance 2,184,476, bingx 2,015,091,
// bitget 2,306,497, bitrue 1,810,954, blofin 1,952,470, bybit 2,189,590,
// coinbase 1,816,299, grvt 1,768,339, hyperliquid 2,095,474, kucoin 1,647,848, kucoinspot 2,235,996,
// okx 1,936,660, paradex 2,223,483, phemex 2,040,251, polymarket 1,587,809, txflow 1,827,125,
// weex 1,925,507, woo 2,388,474 B;
// ddi2il standing rule: CI size plus 10 KiB rounded up to the KiB, shared-family siblings equal.
// #3028 after merging #3039 at 9bf12da7671: PR run 41845929983 measured bitunix 1,838,517,
// cryptocom 1,716,657, kucoin 1,670,273, kucoinspot 2,253,260, lighter 1,991,579, mexc 1,729,817 B.
// #3018 protective amend at 824ff543b6d: CI run 41854021043 measured apexomni 1,972,043, backpack 1,874,324,
// binance 2,183,974, bybit 2,189,768, coinbase 1,813,226, grvt 1,770,412, phemex 2,038,226, polymarket
// 1,590,031 and weex 1,923,438 B; ddi2il ruling: CI size plus 10 KiB rounded up to the KiB.
// #3018 after merging master 78bc1ae5a39: CI run 41870743418 at c9d4de158d5 measured bybit 2,201,340 and
// kraken 2,125,098 B; ddi2il ruling: CI size plus 10 KiB rounded up to the KiB, krakenspot equal to kraken.
// #3047 Phemex read covers at d8899b7a3e0: PR run 41868980693 measured phemex 2,054,401 and
// kraken 2,121,746 B; CI size plus 10 KiB rounded up to the KiB, krakenspot equal to kraken.
// #3018 after merging master d171fbb0e31: CI run 41893192741 at b94d76fa368 measured asterdex 2,125,918,
// bingx 2,026,253, mexc 1,743,880 and orderly 1,721,849 B; ddi2il ruling: CI size plus 10 KiB rounded up to the KiB.
// master b0d2122742d (#3018 + #3056) release cut: run 41901638294 measured binance 2,195,793 B;
// CI size plus 10 KiB rounded up to the KiB.
// #3073 run 41931095253 measured bitget and bitgetusdc 2,334,811 B.
// ddi2il ruling: CI size plus 10 KiB rounded up to the KiB; siblings equal.
// #3094 run 42062090938 measured bybit 2,214,254 B; CI size plus 10 KiB rounded up to the KiB.
// #3106 run 42097859473, job 127493304972: hyperliquid 2,108,435 B at 8d94727188a.
// CI size plus 10 KiB rounded up to the KiB; hyperliquidspot shares the family cap.
// #2314 Lighter managed sockets: job 127434185656 measured lighter 2,034,142 B;
// CI size plus 10 KiB rounded up to the KiB, lighter-robinhood equal to lighter.
// #3118 run 42112075305 measured asterdex 2,139,837 B; CI size plus 10 KiB, rounded up to the KiB.
// #3114 run 42104530272 measured hyperliquid 2,106,596 B: a single-venue family pays the outlined
// shared-ticker/orders-held calls without the combined dedup. CI size plus 10 KiB rounded up; siblings equal.
// #3122 run 42126229513 measured paradex 2,239,351 and blofin 1,979,025 B (guarded stop edits,
// put-back, BloFin native amends and read-backs). CI size plus 10 KiB, rounded up to the KiB.
// #3127 run 42140111861 measured kucoin 1,681,540 B (guarded futures stop edits and put-back).
// CI size plus 10 KiB, rounded up to the KiB.
// #3128 run 42146728975 measured apexomni 2,002,359 B: amends stream as serial Plans (cancel proof
// and put-back). CI size plus 10 KiB, rounded up to the KiB.
// #3130 run 42154583609 measured backpack 1,926,668 B: trigger amends stream as serial Plans
// (proof read and put-back). CI size plus 10 KiB, rounded up to the KiB.
// #3138 run 42166643605 measured bitrue 1,829,700 B: guarded stop edits, cancel-proof polling and
// put-back. CI size plus 10 KiB, rounded up to the KiB.
// #3199 run 42280800704, job 128112974377 at a527a7ccff0 measured okx 1,973,874 B.
// CI size plus 10 KiB rounded up to the KiB; all OKX variants share the okx family cap.
// #3202 run 42280173727 measured bitget 2,379,303 B; CI size plus 10 KiB rounded up
// to the KiB, bitgetusdc equal to bitget.
// #3198 + #3201 combined on master 6228bbef939 (#3206 CI merge-ref job) measured bitget
// 2,364,891 B; CI size plus 10 KiB rounded up to the KiB, bitgetusdc equal to bitget.
// #3143 run 42181736999 measured hyperliquid 2,126,281 B: the public data watch, error-channel
// refusals and Control::Unwatched. CI size plus 10 KiB, rounded up to the KiB; siblings equal.
// #3152 run 42200843544, job 127838799471 measured bingx 2,063,923 B.
// CI size plus 10 KiB, rounded up to the KiB.
// #3149 run 42197163575 measured hyperliquid 2,143,008 B: activeAssetData scoping, the
// FetchLeverage read and subscription.count. CI size plus 10 KiB, rounded up; siblings equal.
// #3158 run 42215250423, job 127884302751 measured lighter 2,068,697 B.
// CI size plus 10 KiB, rounded up to the KiB; lighter-robinhood equal.
// #3170 on master (merge a5bd5e671b6), run 42318845632, jobs 128238566076 and 128238565994
// measured kucoin 1,791,027 B and kucoinspot 2,346,832 B.
// Independent family caps: CI size plus 10 KiB, rounded up to the KiB.
// #3168 run 42227488436, job 127925185663 measured bybit 2,246,685 B.
// Guarded Partial edits add serial Plans, terminal/lineage reads and one-shot put-back.
// ddi2il standing rule: CI + 10 KiB rounded up; all Bybit variants share this family cap.
// #3145 run 42218930349 job 127896386942 measured hyperliquid 2,153,944 B: trades as last for
// watched coins. CI size plus 10 KiB, rounded up to the KiB; siblings equal.
// #3150 final runtime, same-run comparison 42258955401: PR merge d820d0d012d vs
// exact master first-parent 0115ba8df07. Source head d04b192d1b1; URL/RPC tables removed.
// Only over-cap families receive measured CI bytes + 10 KiB, rounded up to a KiB; HL siblings equal.
// #3187 run 42251340763 measured binance 2,212,299 B: PM algo migration and cancel proof.
// CI size plus 10 KiB, rounded up; all Binance aliases use this single family cap.
// #3178 run 42239151280, job 127965614325: Hyperliquid 2,154,756 B at 81206ca3c02.
// CI size plus 10 KiB rounded up to the KiB; Hyperliquid Spot shares the cap.
// #3188 startup attribution, run 42260192686 at cda38ed52a9: CI + 10 KiB rounded up.
// Only measured over-cap families grow; shared siblings stay equal and larger incoming caps win.
// #3207 run 42279265759, job 128106963965 measured Bitget 2,390,477 B.
// CI size plus 10 KiB rounded up to KiB; BitgetUSDC shares the family cap.
// #3202 on #3207 (merge 62c3b59c872), run 42305638406, job 128195021443 measured Bitget
// 2,409,448 B. CI size plus 10 KiB rounded up to the KiB; BitgetUSDC shares the family cap.
// #3219 drb public on master, source 58c9e8eb9: CI run 42352093025, job 128342498489
// measured web Coinbase at 1,924,268 B. CI + 10 KiB rounded up; Coinbase Spot shares the family cap.
// #3220 KuCoin hosted signing: run 42324881707, job 128258225940 measured 1,827,119 B.
// CI size plus 10 KiB rounded up to KiB; KuCoin Spot retains its existing cap.
// #3210 run 42320236420, job 128244910061 measured Bitget 2,422,853 B at 191de16bd7d.
// CI size plus 10 KiB rounded up to the KiB; BitgetUSDC shares the family cap.
// #3229 hosted Backpack: run 42352281022, job 128343086016 measured 1,994,139 B.
// Backpack keeps that CI size plus 10 KiB, rounded up to the KiB.
// #3229 after master merge: run 42392889181, job 128464301381 measured 2,000,456 B
// at CI merge ba85167cc7c. CI size plus 10 KiB, rounded up to the KiB.
// #3193 at ad4f1d6d5d9: seven CI measurements + 10 KiB, rounded up to KiB.
// Later master caps win per family; shared-family siblings remain equal.
// #3111 historical run 42096897919 measured HL siblings at 2,106,939 B; current larger caps retained.
// #3306 CI3 run 42490856791, merge 331730698740: Bitunix 1,905,582 B; OKX 2,035,284 B.
// CI bytes plus 10 KiB rounded up to KiB; never below either incoming parent cap.
export const WASM_VENUE_OVERRIDES = Object.freeze({
  apexomni: 2_056_192,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 2,187,368 B + 10 KiB, rounded up.
  asterdex: 2_198_528,
  backpack: 2_036_736,
  // #3179 run 42250922133, job 128010105166 measured binance 2,207,171 B: COIN-M algo routing
  // and the trailing put-back. CI size plus 10 KiB, rounded up to the KiB.
  // #3179 on master 160884d020d (merge bd354d2e09e), run 42319187479, job 128239693319 measured
  // binance 2,224,977 B. CI size plus 10 KiB rounded up to the KiB; all Binance aliases share the cap.
  binance: 2_274_304,
  bingx: 2_127_872,
  // #3157 at ea21e32e001, CI run 42399005414, job 128482879551: 2,447,105 B.
  // Standing CI + 10 KiB rounded up to KiB; Bitget siblings share the same module cap.
  bitget: 2_492_416,
  bitgetusdc: 2_492_416,
  bitrue: 1_886_208,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web 1,916,290 B + 10 KiB, rounded up.
  bitunix: 1_927_168,
  blofin: 2_049_024,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 2,316,042 B + 10 KiB, rounded up.
  bybit: 2_326_528,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 1,967,433 B + 10 KiB, rounded up.
  coinbase: 1_978_368,
  cryptocom: 1_803_264,
  // #3129, CI run 42145552491: GRVT measured 1,806,590 B + 10 KiB, rounded up.
  // #3227 hosted signing: CI run 42352873546, job 128345833529 measured 1,944,314 B.
  // CI size plus 10 KiB, rounded up to the KiB.
  grvt: 1_985_536,
  // #3120 run 42241033166, job 127971867963: Hyperliquid 2,167,039 B at c5208b32b0f.
  // CI size plus 10 KiB rounded up to the KiB; Hyperliquid Spot shares the cap.
  // #3120 on master 160884d020d (merge be90b5c6891), run 42319184274, job 128239709172 measured
  // Hyperliquid 2,184,857 B. CI size plus 10 KiB rounded up to the KiB; Hyperliquid Spot shares the cap.
  hyperliquid: 2_231_296,
  hyperliquidspot: 2_231_296,
  // #2351 run 42402512142, jobs 128493440380/128493440356: node/web 2,136,136 B.
  // CI size plus 10 KiB rounded up to KiB; Kraken and Kraken Spot share the cap.
  kraken: 2_172_928,
  krakenspot: 2_172_928,
  kucoin: 1_872_896,
  kucoinspot: 2_440_192,
  lighter: 2_124_800,
  'lighter-robinhood': 2_124_800,
  // #3169, CI run 42228563954: MEXC measured 1,848,283 B + 10 KiB, rounded up.
  mexc: 1_898_496,
  okx: 2_045_952,
  orderly: 1_771_520,
  // #3191, CI run 42249705679: Paradex measured 2,249,779 B + 10 KiB, rounded up.
  paradex: 2_300_928,
  // #3245 at 46378cfab09, CI run 42367120442: 2,079,779 B + 10 KiB, rounded up to KiB.
  // #3249 CI: web Phemex measured 2,079,916 B; CI size plus 10 KiB, rounded up to KiB.
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 2,104,206 B + 10 KiB, rounded up.
  phemex: 2_114_560,
  polymarket: 1_652_736,
  // #3277 TxFlow public-decode crate, CI run 42407352871: 1,896,742 B + 10 KiB, rounded up to KiB.
  txflow: 1_934_336,
  weex: 1_981_440,
  // #3193 at 90750c898c2, CI job 128343606230: 2,417,950 B + 10 KiB, rounded up to KiB.
  // #3221 hosted signing: CI run 42352558360 measured web/node at 2,568,219 B; + 10 KiB, rounded up to KiB.
  // #3221 CI run 42393607294, artifact 13582294491: web/node 2,578,659 B + 10 KiB, rounded up to KiB.
  // #3395 run 42561995598, merge c4b68c461e1f: CI web 2,631,851 B + 10 KiB, rounded up.
  woo: 2_642_944,
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
