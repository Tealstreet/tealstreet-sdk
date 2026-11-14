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
// #3397 run 42552867793 at 2cfc2f3a0ce: Bybit 2,281,554; Coinbase 1,954,575;
// MEXC 1,873,137; Orderly 1,744,951 B. Preserve these CI + 10 KiB floors;
// newer master measurements and larger incoming caps remain authoritative.

// #3281 merge: max(d9b5310e7d1, 6c158931c59) for every parent cap.
// Current master dominates; retain both parents' measurement provenance
// and the #3281 measured-headroom regression (run 42422693183, 5025f1670c7).
  // #3302 merged source 207b88974ec: web/Node each 2,218,910 B, standard AsterDEX build.
  // CI run 42564749959, job 128989112359, merge 0bac20f54f5f: web/Node each 2,219,070 B.
  // Rule 17: measured bytes + 10 KiB, rounded up to KiB; larger parent floors retained.

// #3391 run 43040714236 at 86083b569b50: exact-head failing family sizes + 10 KiB, rounded up.
// Preserve larger existing caps; per-family byte measurements and job IDs are pinned in wasm-budget.test.mjs.
// #3397 fresh merged-tree measurements: Rust 1.99.0, wasm-bindgen 0.2.129,
// Binaryen 132, two -Oz passes via build.sh; all older provenance remains below.

// #3317 merge: max(45302e5486f, cba9c99c82a) per parent cap;
// retain each measurement's provenance, including the current CI WOO floor.
// #3317 merge with afccf1049cc: max(5f3c601be34, afccf1049cc) for each cap;
// both parents' measurement histories are retained below.
export const WASM_VENUE_OVERRIDES = Object.freeze({
  // #3300 post-#3306: run 42557412385, job 128966462873, CI merge 291f65994cda.
  // Optimized web/node measured 2,078,907 B; CI plus 10 KiB rounded up to KiB.
  // #3300 merge c4d663622c01: local optimized web/node 2,092,748 B; measured + 10 KiB rounded up.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh --venue apexomni, twice -Oz.
  // #3302 run 43517974996, source 0375249bc56e, artifact 14081617722: web/Node/replay 2,093,078 B.
  // Pinned Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; measured + 10 KiB rounded up.
  // Retain historical floors and cap+1 refusal; the former cap missed the reserve by 22 B.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14093433805: web/Node/replay 2,094,662 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  apexomni: 2_105_344,
  // #3448 merged head 9b737161757f: CI run 43061512265, artifact 13873588589.
  // Optimized web/node 2,197,821 B + 10 KiB, rounded up to KiB.
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 2,187,368 B + 10 KiB, rounded up.
  // #3302 merge 80d25442f36f: local optimized web/node 2,231,057 B; measured + 10 KiB rounded up.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh --venue asterdex, twice -Oz.
  // #3302 merge 359c4b3e3c4d: local optimized web/node 2,231,732 B; measured + 10 KiB rounded up.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh --venue asterdex, twice -Oz.
  // #3438 merged-head local build.sh --venue asterdex: web/node 2,197,178 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3425 run 43176700451/job 131043125663, CI source cca7b8337ad3:
  // web/Node 2,208,996 B; current measured maximum + 10 KiB, rounded up.
  // Shared engine valuation/native-size policy; retain unknown instead of a partial value.
  // #3448 head 9a736b2a6bf1: run 43198030267/job 131114624468, artifact 13934600864.
  // Optimized web/Node 2,211,230 B + 10 KiB, rounded up to KiB.
  // #3302 run 43319605083, CI merge fb0f9b777241: optimized web/Node 2,245,672 B.
  // Job 131537312211 / artifact 13990380922; current bytes + 10 KiB, rounded up.
  // #3302 web/Node: reviewed 3df8707ddee4 measured 2,256,476 B; isolated a5b5284402c8 measured 2,247,295 B.
  // build.sh --venue asterdex; Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; +10 KiB, retain higher floor.
  // Isolated WASM SHA256 b75f55fc133ad54919521fd09bd1274e4f6ddf54f9d761656b7d62a985830d36.
  // #3397 run 43319466529/job 131536920564, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 2,211,866 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990660547; SHA-256 39ec0ebda4cdf700bf11a73ef9009b5c7ff7f92ecee38d62e2da36096c5a6a89.
  // #3302 merge 1ccd7b26df0b: local optimized web/Node 2,257,016 B; +10 KiB rounded up.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; retain larger parent floors.
  // SHA256 c311efa0fd52b2350179a6aa4bed061d8289aa41ce830316ef1eb05b49c5a575.
  // #3391 run 43204096369/job 131135453773, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,216,296 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43263509956/job 131345282843, artifact 13966760078; CI source 0fd07e1c89f8.
  // Optimized web/Node 2,219,023 B + 10 KiB, rounded up; c7c32191002a + master 0c1b6dc17cd9.
  // Rust 1.99.0 / wasm-bindgen 0.2.129 / Binaryen 132.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,221,326 B.
  // build.sh --venue asterdex; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 merge aa2dc74ee433 + master aa78ef0a06b7: local optimized web/Node 2,231,055 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132, required two -Oz passes; +10 KiB rounded up.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14094386070: web/Node/replay 2,258,086 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  asterdex: 2_269_184,
  // #3438 merged-head local build.sh --venue backpack: web/node 2,036,671 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  backpack: 2_053_120,
  // #3179 run 42250922133, job 128010105166 measured binance 2,207,171 B: COIN-M algo routing
  // and the trailing put-back. CI size plus 10 KiB, rounded up to the KiB.
  // #3179 on master 160884d020d (merge bd354d2e09e), run 42319187479, job 128239693319 measured
  // binance 2,224,977 B. CI size plus 10 KiB rounded up to the KiB; all Binance aliases share the cap.
  // #3438 merged-head local build.sh --venue binance: web/node 2,270,372 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  binance: 2_286_592,
  // #3438 merged-head local build.sh --venue bingx: web/node 2,130,988 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  bingx: 2_142_208,
  // #3157 at ea21e32e001, CI run 42399005414, job 128482879551: 2,447,105 B.
  // Standing CI + 10 KiB rounded up to KiB; Bitget siblings share the same module cap.
  // #3426 run 43187677592, job 131079474589, artifact 13930235036, CI source b3c4cf6212e6.
  // Verified web/Node 2,487,523 B + 10 KiB, rounded up to KiB; both siblings share this cap.
  // #3438 merged-head local build.sh --venue bitget: web/node 2,486,531 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3317 engine/host throughput profile: local optimized web/node 2,754,487 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue bitget.
  // #3317 master merge 20d61cbdf174: run 43517857744, family job 132204972157.
  // Optimized web 2,769,255 B + 10 KiB, rounded up to KiB; both aliases share the module.
  bitget: 2_780_160,
  bitgetusdc: 2_780_160,
  // #3297 run 42556682443 at e5ae4d9a8db measured 1,895,658 B; +10 KiB rounded up.
  // Keep the larger PR-parent cap when merging master; both measurement histories remain pinned.
  // #3301 at 8628807c88b: CI run 42609133754, artifacts 13671350218 / 13670477262
  // measured 1,898,429 B; +10 KiB rounded up to KiB.
  // #3299 merged 2ca7125e8ab: local optimized web/node 1,901,720 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue bitrue.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded up.
  // #3438 merged-head local build.sh --venue bitrue: web/node 1,908,695 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  bitrue: 1_925_120,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web 1,916,290 B + 10 KiB, rounded up.
  // Nullable history output: local optimized web/node 1,927,088 B.
  // SAFE_CEX_WASM_OPT=required build.sh --venue bitunix; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  // Current measured size + 10 KiB, rounded up to KiB (rule 17).
  // #3425 run 43176700451/job 131043125555, CI source cca7b8337ad3:
  // web/Node 1,942,038 B; current measured maximum + 10 KiB, rounded up.
  // Shared native-size policy must retain unknown/conflicting quantity refusals.
  // #3397 run 43319466529/job 131536920580, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 1,942,779 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13989557960; SHA-256 ba9bb52dc22748bca25fcb5242b9cec3dc23470a6f0f56783e6ab4047a07599e.
  // #3438 run 43204071123, artifact 13937985101, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 1,942,715 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3391 run 42561525825 at ec49cdd8159: 1,920,841 B + 10 KiB, rounded up; retain PR-parent cap.
  // #3391 run 43204096369/job 131135453920, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 1,949,515 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13994321229; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 1,950,059 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 1,952,459 B.
  // build.sh --venue bitunix; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14061430806.
  // Optimized web/Node/replay 1,953,128 B; module SHA256 5c895ebe6691538575c669258efdb3a1eee051116489d39d0a11e92045cdb211.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,904 B reserve; shares bitunix family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14093428727: web/Node/replay 1,954,918 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  bitunix: 1_966_080,
  // Nullable history output: local optimized web/node 2,070,435 B.
  // SAFE_CEX_WASM_OPT=required build.sh --venue blofin; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  // Current measured size + 10 KiB, rounded up to KiB (rule 17).
  blofin: 2_085_888,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 2,316,042 B + 10 KiB, rounded up.
  // #3372 merge keeps max(PR 2,295,808, master 2,326,528); prior run 42484598496 measured 2,285,327 B.
  // #3372 restage at merge 41d4292d6f3: build.sh --venue bybit, pinned Binaryen132, web/node 2,323,991 B.
  // Current measured size + 10 KiB, rounded up to KiB; both parent provenance blocks retained.
  // #3372 head 5858609a705: CI run 43052995465, job 130635861554, artifact 13869852410.
  // CI merge cc0145a0f0ba (base d956d25f8b48): optimized web/node/replay each 2,331,744 B.
  // Current CI size + 10 KiB, rounded up to KiB; historical provenance and tests retained.
  // #3397 run 42571903722 attempt 2, job 129025076016: 2,316,667 B + 10 KiB rounded up.
  // #3397 merge fcee82e27df: local optimized web/node 2,329,775 B.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh --venue bybit.
  // Current measured bytes + 10 KiB, rounded up; retain both parent floors.
  // Nullable history output: local optimized web/node 2,329,857 B.
  // SAFE_CEX_WASM_OPT=required build.sh --venue bybit; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  // Current measured size + 10 KiB, rounded up to KiB (rule 17).
  // #3425 run 43176700451/job 131043125392, CI source cca7b8337ad3:
  // web/Node 2,330,713 B; current measured maximum + 10 KiB, rounded up.
  // Nullable valuation plus inverse quote-capacity refusals remain in this family.
  // #3391 run 42553793906 at 20f13e05df1: 2,320,705 B + 10 KiB, rounded up; retain PR-parent cap.
  // #3391 run 43204096369/job 131135453903, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,338,200 B; current measurement + 10 KiB, rounded up.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,341,308 B.
  // build.sh --venue bybit; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3342 merged head 2b623dc76a03: local optimized web/node 2,351,685 B; +10 KiB, rounded up.
  // build.sh --venue bybit; Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132, twice -Oz.
  // #3342 fix round 091d0b980aef: local merged-head optimized web/node 2,354,730 B; +10 KiB rounded up.
  // Same build.sh/toolchain above; SHA256 170c1e44400fd4b682818cdd37b01d7a322d11d6c3b9677f53b638b3165a1167.
  // #3342 fix e1e8114b948c: optimized web/Node 2,355,973 B + 10 KiB, rounded up.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132 twice -Oz; both parent histories retained.
  // #3372 head 48cb0d34c168: CI run 43328678379/job 131568212209, artifact 13995125309.
  // CI merge cff6adaa9fc1 (base 908bedbe6f1a): optimized web/Node/replay 2,338,561 B.
  // Current CI size + 10 KiB, rounded up; historical provenance/tests retained.
  // #3372 reduced head 5429d69bb2c3: run 43341438240/job 131611062815, artifact 14000097059.
  // CI merge fc362e3e5ebd: web/Node/replay 2,338,212 B; the same cap retains 10,844 B.
  // #3472 source f6a275af12092d378f8812f5260b4a3a07b7b0ee: fresh optimized web/node 2,332,186 B.
  // build.sh --venue bybit; Rust1.99, bindgen0.2.129, Binaryen132 twice-Oz.
  // SHA256 2f99b9639875bdb29dfde985745ca9f91b1e3903dcbd3b9052e2a788548d6db2.
  // Generated from wasm-budget-venue.json: ceil((max(web,node) + 10 KiB) / KiB).
  // #3397 run 43364338606/job 131685537135, artifact 14009728045:
  // source ba06b221f45f has head fa06619c7d21's tree; web/Node/replay 2,332,649 B.
  // SHA-256 70cd348f758b71303b71e47a7231223bd06078e79b1e56e095dcca7a7f8d2aa2.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; required -Oz build.
  // Current measured bytes + 10 KiB, rounded up; retain all larger parent floors.
  // #3317 throughput profile at 0c8e04fe5a: optimized local web/node 2,483,240 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue bybit.
  // #3317 run 43060399359, job 130665084604, artifact 13874660310:
  // CI merge 2d1f87d77872 web/node 2,481,896 B; current CI size + 10 KiB, rounded up.
  // #3317 merge 688531f4bfcb + master 159102c8a9a7: optimized local web/Node 2,479,523 B.
  // build.sh --venue bybit; Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz.
  // Fresh minimum is 2,490,368 B; retain the recorded CI floor of 2,492,416 B.
  // #3372 merged head 41026bb9ab40: run 43381322258, artifact 14016926715.
  // CI source 15801feeeb97 has the same tree; all web/Node/replay copies measure 2,338,820 B.
  // SHA-256 106ff1f90f5866c2d654e8534b2eceba9f0be73e9b525e3b729614f1ea54079c.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; required -Oz build.
  // Current measured bytes + 10 KiB, rounded up: 11,260 B reserve; prior provenance retained.
  // #3391 head e5b6f01259b9, run 43454205848/job 131985087682, artifact 14051924992.
  // CI source 81ae648f2f29 has the same tree; all web/Node/replay copies 2,346,773 B.
  // Module SHA256 207116edb828e944a3ef5bc12e9e23bdccd4d3cdcbba1795c9b30b1d43a9d21a.
  // Current CI bytes + 10 KiB, rounded up: 10,475 B reserve; parent histories retained.
  // #3342 run 43452683819/job 131979457461, artifact wasm-family-bybit: web/Node/replay 2,360,107 B.
  // CI source 0d075971b309 has e0b3a4b9f1f9's tree; pinned tools above; +10 KiB rounded up.
  // SHA256 99f3fea9eac4f64b632943b980f506558dacd2a9b769a3717d27ca29b50848cd.
  // #3317 merged 371118f7c19c: run 43461440513, family 132020005385 / replay 132020005877.
  // CI source 433d752a7d43 has the identical tree; optimized web/Node 2,539,197 B.
  // SHA-256 a2d0a3997752168f38b5eea6f5b90bf4389c68494e48efb31645fca60aa319a0.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; current CI + 10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14060117398.
  // Optimized web/Node/replay 2,369,936 B; module SHA256 5bee4b33c2214384e3070aed758134fed412e2536dca4c0becfa76f258711486.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,864 B reserve; shares bybit family.
  // #3317 merge 6a8318ff61c9: CI 43493007722, artifact 14071675570; source b07da9e11b0f has the same tree.
  // Optimized web/Node/replay 2,547,120 B; SHA256 9d1980d78eeb9ee0fd6441997f1fcf864fa3d71e6ae5fd5041549094b9de48a4.
  // Rust1.99, bindgen0.2.129, Binaryen132, required twice -Oz; current bytes + 10 KiB rounded up (10,832 B reserve).
  // Preserve both parent floors and all provenance above.
  bybit: 2_557_952,
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 1,967,433 B + 10 KiB, rounded up.
  // #3397 run 42571903722 attempt 2, job 129025075890: 1,968,376 B + 10 KiB rounded up.
  // #3301 at 7a2f4816ce4: CI run 42555202714, job 128959723881 measured 2,091,319 B.
  // CI size plus 10 KiB, rounded up to the KiB; Coinbase Spot shares this family cap.
  // #3301 merge 42063dd9a97: CI run 42605138812, job 129119693578, artifact 13666589511
  // measured web/node 2,094,041 B; +10 KiB rounded up to KiB, shared with Coinbase Spot.
  // #3301 at 8628807c88b: CI run 42609133754, artifacts 13671350218 / 13670477262
  // measured 2,094,163 B; +10 KiB rounded up to KiB, shared with Coinbase Spot.
  // #3397 merge fcee82e27df: local optimized web/node 2,105,215 B.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh --venue coinbase.
  // Current measured bytes + 10 KiB, rounded up; retain both parent floors.
  // #3438 merged-head local build.sh --venue coinbase: web/node 2,103,909 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,112,360 B.
  // build.sh --venue coinbase; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14059882137.
  // Optimized web/Node/replay 2,112,897 B; module SHA256 c6f6a2330c47317d7e406ef7961eaef84dc6c715fe14dc9c9ba1d1d0de12e336.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,879 B reserve; shares coinbase family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14094520418: web/Node/replay 2,114,517 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  coinbase: 2_124_800,
  // #3438 merged-head local build.sh --venue cryptocom: web/node 1,803,468 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3391 run 43204096369/job 131135453931, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 1,810,902 B; current measurement + 10 KiB, rounded up.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 1,813,558 B.
  // build.sh --venue cryptocom; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3397 run 43319466529/job 131536920523, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 1,804,182 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990122410; SHA-256 fb12f0bb7360254bb91749840faf977e1d5977880f179fd2933dfcf79de15443.
  // #3397 run 43364338606/job 131685537173, artifact 14010191105:
  // source ba06b221f45f has head fa06619c7d21's tree; web/Node/replay 1,804,494 B.
  // SHA-256 7f1d8ee00cedbffb70fce530ff87ee0fd4ad2238bc36790e4098ebbdd9bf88f8.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; required -Oz build.
  // Current measured bytes + 10 KiB, rounded up; retain all larger parent floors.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14093603492: web/Node/replay 1,815,949 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  cryptocom: 1_826_816,
  // #3129, CI run 42145552491: GRVT measured 1,806,590 B + 10 KiB, rounded up.
  // #3227 hosted signing: CI run 42352873546, job 128345833529 measured 1,944,314 B.
  // CI size plus 10 KiB, rounded up to the KiB.
  // #3397 run 43319466529/job 131536920517, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 1,990,869 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13989269627; SHA-256 617f513988a40ee3f14be04338f455aee23972814425c6cadf00f87f30002f4b.
  // #3438 merged-head local build.sh --venue grvt: web/node 1,988,047 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3438 run 43204071123, artifact 13936313617, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 1,990,569 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3317 run 43321890002/job 131545235862, artifact 13990169480; CI source e969a3bf1c92.
  // Web/Node 1,991,790 B; current measured maximum + 10 KiB, rounded up to KiB.
  // #3391 run 43204096369/job 131135453957, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 1,997,577 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13993179391; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 1,998,120 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,000,353 B.
  // build.sh --venue grvt; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14061770193.
  // Optimized web/Node/replay 2,000,978 B; module SHA256 532ef5dc288bab54bd2b8a764d23d0e10040ed7e782ecda1ba5900f099d7fdab.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 11,182 B reserve; shares grvt family.
  // #3317 merge 6a8318ff61c9: CI 43493007722, artifact 14071196267; source b07da9e11b0f has the same tree.
  // Optimized web/Node/replay 2,002,550 B; SHA256 678fde6acd0301529c273e8e582affa5432b34dae8dd9ce40fce992bcc99ce90.
  // Rust1.99, bindgen0.2.129, Binaryen132, required twice -Oz; current bytes + 10 KiB rounded up (10,634 B reserve).
  // Preserve both parent floors and all provenance above.
  grvt: 2_013_184,
  // #3120 run 42241033166, job 127971867963: Hyperliquid 2,167,039 B at c5208b32b0f.
  // CI size plus 10 KiB rounded up to the KiB; Hyperliquid Spot shares the cap.
  // #3120 on master 160884d020d (merge be90b5c6891), run 42319184274, job 128239709172 measured
  // Hyperliquid 2,184,857 B. CI size plus 10 KiB rounded up to the KiB; Hyperliquid Spot shares the cap.
  // #3344 run 42479985723 at 51005ee6d89 measured 2,210,524 B; floor 2,221,056 B.
  // #3306 run 42481130823 at merge 680966203b3 measured 2,220,716 B; floor 2,231,296 B.
  // Keep the larger parent cap for both Hyperliquid aliases.
  // #3344 restage 2cc05051503: optimized web/node 2,225,418 B + 10 KiB, rounded up.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue hyperliquid.
  // Master 05241cd0f4 changes no Hyperliquid build inputs; keep both parent provenance blocks.
  // Current CI run 42565604601, merge ec8633c44e98: web/node 2,225,586 B + 10 KiB, rounded up.
  // #3302 run 42720697962, job 129514220728: CI 2,236,465 B; local merge 06aa3a745fcc web/node 2,238,386 B.
  // build.sh --venue hyperliquid; Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132 twice -Oz.
  // Larger current measurement + 10 KiB, rounded up; retain both parent floors.
  // #3438 run 42745609629, family job 129597324393: web 2,236,724 B.
  // Current CI measurement + 10 KiB, rounded up; shared with Hyperliquid Spot (rule 17).
  // #3438 merged-head local build.sh --venue hyperliquid: web/node 2,238,027 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3429 merged head 1bb3f1352988: local optimized web/node 2,238,274 B; +10 KiB rounded up.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue hyperliquid.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; both aliases share the compiled family.
  // #3438 merge cf112c51b87c + 3f94ec11a27f: optimized web/node 2,238,644 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3433 run 42742356027 at CI merge 972f1282ea16: web/node 2,236,421 B.
  // Current measured size + 10 KiB, rounded up to KiB; both aliases share the family.
  // #3433 branch 1547c3b2e287: optimized local web/node 2,237,725 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue hyperliquid.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; measured + 10 KiB, rounded up.
  // #3433 merge b57d4ab06df2 + master 4d399f716231: optimized local web/node 2,238,332 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue hyperliquid.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded up to KiB.
  // #3317 at 5d04ddb28ab: run 42703052461, family job 129453575087, node artifact 13712976849.
  // CI web/node 2,237,802 B; max + 10 KiB rounded up. CI merge 46c64a76e282 has the identical source tree.
  hyperliquid: 2_249_728,
  // #3438 merged-head local build.sh --venue hyperliquid: web/node 2,238,027 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3438 merge cf112c51b87c + 3f94ec11a27f: optimized web/node 2,238,644 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  hyperliquidspot: 2_249_728,
  // #2351 run 42402512142, jobs 128493440380/128493440356: node/web 2,136,136 B.
  // CI size plus 10 KiB rounded up to KiB; Kraken and Kraken Spot share the cap.
  // #3304 restage run 42558782819 at CI merge a068e6cb0876a3699d46fa843b9310887bd055f4:
  // family job 128970669128, replay jobs 128970669384/128970669374: node/web 2,228,472 B.
  // Current CI size plus 10 KiB, rounded up to KiB; both aliases share the compiled family.
  // #3304 refusal ordering at 0b7e6ccf60f3ac4c202ffb595a2b6185b1f5fb1e: web/node 2,234,008 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue kraken;
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132. Measured + 10 KiB rounded up to KiB.
  // #3438 merged-head local build.sh --venue kraken: web/node 2,247,702 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3425 run 43176700451/job 131043125504, CI source cca7b8337ad3:
  // web/Node 2,260,418 B; current measured maximum + 10 KiB, rounded up.
  // Keep unavailable mark valuation and distinct PF/inverse quantity rules.
  // #3391 run 43204096369/job 131135453763, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,267,706 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13992739463; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 2,268,250 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,270,539 B.
  // build.sh --venue kraken; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3397 run 43319466529/job 131536920531, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 2,261,215 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990910307; SHA-256 5be5d66839f94eb26cb61fcb370e7eb08c7f1e1b03bcb779016c0043b2b59279.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14059659521.
  // Optimized web/Node/replay 2,272,619 B; module SHA256 2145a93ee6b3a3051988d7b20b949ca5d9b33b561a1221f051933febc95ad1a5.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,901 B reserve; shares kraken family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14093868148: web/Node/replay 2,274,234 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  kraken: 2_284_544,
  // #3438 merged-head local build.sh --venue kraken: web/node 2,247,702 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3391 run 43328298773, artifact 13992739463; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 2,268,250 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,270,539 B.
  // build.sh --venue kraken; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14059659521.
  // Optimized web/Node/replay 2,272,619 B; module SHA256 2145a93ee6b3a3051988d7b20b949ca5d9b33b561a1221f051933febc95ad1a5.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,901 B reserve; shares kraken family.
  krakenspot: 2_284_544,
  // #3438 merged-head local build.sh --venue kucoin: web/node 1,866,880 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  kucoin: 1_878_016,
  // #3397 run 43319466529/job 131536920530, source b9f5256efe4c (tree = merge 1663c1a885):
  // web size refusal: 2,464,041 B; separate Node capsule has the same size.
  // Node artifact 13990151877; SHA-256 738c1759de0129c349b8800cb1c77a52c27af034f578bb6dffa12e9366048674.
  // Current measured bytes + 10 KiB, rounded up; retain parent floors.
  // #3438 merged-head local build.sh --venue kucoinspot: web/node 2,461,646 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3438 run 43204071123, artifact 13936822120, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 2,462,504 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3317 run 43321890002/job 131545235949, artifact 13991900630; CI source e969a3bf1c92.
  // Web/Node 2,463,737 B; current measured maximum + 10 KiB, rounded up to KiB.
  // #3391 run 43204096369/job 131135454013, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,471,018 B; current measurement + 10 KiB, rounded up.
  // #3372 head 5f32a22a8c6c: CI run 43303561061/job 131481481601, artifact 13983990723.
  // CI merge c75a34b4bd34 (base dda0e1868f38): optimized web/Node 2,463,988 B.
  // Current measured size + 10 KiB, rounded up; historical provenance/tests retained.
  // That pre-reduction 2,475,008-byte cap is superseded: passive Plan element reports
  // now link only for their Bybit adopter, removing 1,484 B from KuCoin Spot.
  // Head 5429d69bb2c3: run 43341438240/job 131611062986, artifact 13999827413.
  // CI merge fc362e3e5ebd (base 80a802868f09): web/Node/replay each 2,462,504 B.
  // Current measured size + 10 KiB, rounded up: 10,456 B reserve, matching master.
  // Master integration retains its larger 2,475,008 B parent floor; reduction history above remains historical.
  // #3391 head a545cbe2fca0, CI run 43439489621, artifact 14044993539.
  // Source 4f137fb49815 has the reviewed head tree; optimized web/Node/replay 2,472,600 B.
  // Module SHA256 9e08084887ba174d928963d343c88859471fadb398544530632cfb95994810a6.
  // Current measured size + 10 KiB, rounded up; historical floors and overflow controls retained.
  // #3391 merged source df6f095bd740, incorporating master 353a3ab925b0: local web/Node 2,473,416 B.
  // build.sh --venue kucoinspot; Rust1.99, bindgen0.2.129, Binaryen132, required two -Oz passes.
  // Both module SHA256 ce57c8df54582f153ec305930df9fa87fcf763ee80bc9552a27059926cebd28f.
  // Fresh measured size + 10 KiB, rounded up: 10,808 B reserve; all historical floors retained.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14061155780.
  // Optimized web/Node/replay 2,475,797 B; module SHA256 d8d26d9cfb2ec34af49483b3670a419d02bcfe75c33b9d21bdeb213c0dafbc56.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,475 B reserve; shares kucoinspot family.
  // #3317 merge 6a8318ff61c9: CI 43493007722, artifact 14071220644; source b07da9e11b0f has the same tree.
  // Optimized web/Node/replay 2,477,083 B; SHA256 498d7beb845e5568abfd9c028c93a0b657b3d3bff21e161c45e7f8c6ac649d8c.
  // Rust1.99, bindgen0.2.129, Binaryen132, required twice -Oz; current bytes + 10 KiB rounded up (11,237 B reserve).
  // Preserve both parent floors and all provenance above.
  kucoinspot: 2_488_320,
  // #3438 merged-head local build.sh --venue lighter: web/node 2,133,346 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3425 run 43176700451/job 131043125518, CI source cca7b8337ad3:
  // web/Node 2,146,773 B; current measured maximum + 10 KiB, rounded up.
  // Shared engine valuation/native-size policy; no signing or admission expansion.
  // #3397 run 43319466529/job 131536920467, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 2,147,689 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990221484; SHA-256 9a67744fac8df42080c232cc974f1c9bd15c16209c660ce4354992941100f919.
  // #3438 run 43204071123, artifact 13936986915, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 2,147,387 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3391 run 43204096369/job 131135454148, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,154,475 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13994720808; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 2,155,019 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,158,209 B.
  // build.sh --venue lighter; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3397 run 43364338606/job 131685537252, artifact 14010330544:
  // source ba06b221f45f has head fa06619c7d21's tree; web/Node/replay 2,148,847 B.
  // SHA-256 121b8a073e360e125207a6f776d9d6b682ae45a67c6f4df400474d8a27feec2e.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; required -Oz build.
  // Current measured bytes + 10 KiB, rounded up; retain all larger parent floors.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14060640962.
  // Optimized web/Node/replay 2,158,718 B; module SHA256 42f81f3ba0400a925bb311e98021045bf99ac78949badaba9017cdaf06e04376.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 11,138 B reserve; shares lighter family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14094201484: web/Node/replay 2,160,313 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  lighter: 2_170_880,
  // #3438 merged-head local build.sh --venue lighter: web/node 2,133,346 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3438 run 43204071123, artifact 13936986915, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 2,147,387 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3391 run 43328298773, artifact 13994720808; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 2,155,019 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,158,209 B.
  // build.sh --venue lighter; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // Shared Lighter family: #3397 run 43364338606, artifact 14010330544 above.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14060640962.
  // Optimized web/Node/replay 2,158,718 B; module SHA256 42f81f3ba0400a925bb311e98021045bf99ac78949badaba9017cdaf06e04376.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 11,138 B reserve; shares lighter family.
  'lighter-robinhood': 2_170_880,
  // #3169, CI run 42228563954: MEXC measured 1,848,283 B + 10 KiB, rounded up.
  // #3397 run 42571903722 attempt 2, job 129025075967: 1,888,209 B + 10 KiB rounded up.
  // #3299 run 42437005422 at 94211a4edff: 1,893,996 B + 10 KiB, rounded up; larger parent cap retained.
  // #3299 restage run 42558057739 at 5dec79811311, jobs 128968431838/128968431872:
  // MEXC measured 1,914,033 B; current CI size + 10 KiB, rounded up to KiB (rule 17).
  // #3299 merged 2ca7125e8ab: local optimized web/node 1,920,015 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue mexc.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded up.
  // #3438 merged-head local build.sh --venue mexc: web/node 1,927,089 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3433 head 99eed37c13cc: fresh optimized web/node 1,927,409 B + 10 KiB, rounded up to KiB.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue mexc.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132 (two -Oz passes).
  // #3397 merge fcee82e27df: local optimized web/node 1,927,721 B.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh --venue mexc.
  // Current measured bytes + 10 KiB, rounded up, already fits the parent cap.
  // #3438 merge 6ec2b44e3f95 + d956d25f8b48: optimized web/node 1,927,721 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3425 run 43176700451/job 131043125322, CI source cca7b8337ad3:
  // web/Node 1,946,379 B; current measured maximum + 10 KiB, rounded up.
  // Shared native sizing with current master broker behavior retained.
  // #3397 run 43319466529/job 131536920527, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 1,947,286 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13988974079; SHA-256 77119ba064a842b5db5ef5dba4f819ea6affef465a8913949e4767e2dbf7e401.
  // #3438 run 43204071123, artifact 13937850519, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 1,947,010 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3391 run 43204096369/job 131135454171, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 1,953,995 B; current measurement + 10 KiB, rounded up.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 1,956,731 B.
  // build.sh --venue mexc; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14059549866.
  // Optimized web/Node/replay 1,957,413 B; module SHA256 81eb721cb195ebcb55161f365e41701cc269cea2e5270320bc424f8c7c667ca3.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,715 B reserve; shares mexc family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14093863330: web/Node/replay 1,959,030 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  mexc: 1_970_176,
  // Nullable history output: local optimized web/node 2,052,533 B.
  // SAFE_CEX_WASM_OPT=required build.sh --venue okx; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  // Current measured size + 10 KiB, rounded up to KiB (rule 17).
  okx: 2_068_480,
  // #3438 merged-head local build.sh --venue orderly: web/node 1,769,717 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3391 run 43204096369/job 131135454025, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 1,777,164 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13993673114; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 1,777,708 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 1,779,876 B.
  // build.sh --venue orderly; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3397 run 43192535364/job 131096463465: prior web/Node 1,768,995 B (minimum 1,779,712 B).
  // #3397 run 43319466529/job 131536920541, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 1,770,708 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990735425; SHA-256 eb72c086fa0ac4d0fd52405501d4344ba4fe31f48bd04db51ea79928ecf05628.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14061375404.
  // Optimized web/Node/replay 1,780,766 B; module SHA256 a670e67122af53e4e4cc765eaaf2ea14f9345379c42bd1f837c51303b92c2415.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 11,234 B reserve; shares orderly family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14094391124: web/Node/replay 1,782,494 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  orderly: 1_793_024,
  // #3191, CI run 42249705679: Paradex measured 2,249,779 B + 10 KiB, rounded up.
  // #3397 run 43319466529/job 131536920493, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 2,297,964 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990157207; SHA-256 9722d37456ac324db76c00f50ae051735928f51d3f10a19e7e6762596ca9aaf0.
  // #3438 merged-head local build.sh --venue paradex: web/node 2,301,551 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14060841092.
  // Optimized web/Node/replay 2,308,139 B; module SHA256 a8ecfaa5b358c757518ba94c760da9ca3a6d01389f36cd63ff844f539067a063.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 11,221 B reserve; shares paradex family.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14094286290: web/Node/replay 2,309,926 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  paradex: 2_320_384,
  // #3245 at 46378cfab09, CI run 42367120442: 2,079,779 B + 10 KiB, rounded up to KiB.
  // #3249 CI: web Phemex measured 2,079,916 B; CI size plus 10 KiB, rounded up to KiB.
  // #3395 run 42561995598, merge c4b68c461e1f: CI web/node 2,104,206 B + 10 KiB, rounded up.
  // #3438 merged-head local build.sh --venue phemex: web/node 2,118,601 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3429 run 42729630872 at a2be15368b4: CI web/node 2,125,299 B.
  // Shared-helper fix: local web/node 2,127,824 B; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue phemex; measured + 10 KiB, rounded up.
  // #3438 merge cf112c51b87c + 3f94ec11a27f: optimized web/node 2,128,251 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3391 run 43204096369/job 131135454267, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,132,324 B; current measurement + 10 KiB, rounded up.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,135,165 B.
  // build.sh --venue phemex; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3317 merge e25dc312bd2c, run 43545517625, artifact 14093848345: web/Node/replay 2,137,374 B.
  // Measured + 10 KiB, rounded up to KiB; retain parent floors and shared-family aliases.
  phemex: 2_148_352,
  // #3397 run 43319466529/job 131536920519, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 1,652,282 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13990097423; SHA-256 fd43cf0b2b9090eb02a7a9e5220b4aeedf2d40088730a61187302221876e9a99.
  // #3438 merged-head local build.sh --venue polymarket: web/node 1,647,100 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3438 run 43204071123, artifact 13937575821, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 1,651,961 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3317 run 43321890002/job 131545235900, artifact 13992140380; CI source e969a3bf1c92.
  // Web/Node 1,653,005 B; current measured maximum + 10 KiB, rounded up to KiB.
  // #3391 run 43204096369/job 131135454125, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 1,658,851 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13994907058; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 1,659,395 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 1,661,564 B.
  // build.sh --venue polymarket; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14059981907.
  // Optimized web/Node/replay 1,662,490 B; module SHA256 b1ad6f8112f2b4bb4f0e4c2be82d9118e8f03da9b1d9807a3f218d19fba2f295.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,726 B reserve; shares polymarket family.
  // #3317 merge 6a8318ff61c9: CI 43493007722, artifact 14070537614; source b07da9e11b0f has the same tree.
  // Optimized web/Node/replay 1,664,022 B; SHA256 c7c5a3503743c1d3efc30738948fe3cc7993217c44a8b58af597e2a5e2018f42.
  // Rust1.99, bindgen0.2.129, Binaryen132, required twice -Oz; current bytes + 10 KiB rounded up (11,242 B reserve).
  // Preserve both parent floors and all provenance above.
  polymarket: 1_675_264,
  // #3277 TxFlow public-decode crate, CI run 42407352871: 1,896,742 B + 10 KiB, rounded up to KiB.
  // #3314 CI run 42560319271, merge 449511022a8d, job 128975487404: web/node 2,121,120 B.
  // Local build.sh --venue txflow (Binaryen 132): c5c8a213f6a5 2,121,037 B; e2da53cc9f26 2,120,966 B + 10 KiB, rounded up.
  // #3438 merged-head local build.sh --venue txflow: web/node 1,928,662 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3314 merge 9bb43250830d: optimized web/Node 2,147,684 B; SHA256 51d351b7ae0e7fcd244447101618ed26006cbf57c276cc06d6f6b00f967a5bd1.
  // build.sh --venue txflow; Rust1.99.0, bindgen0.2.129, Binaryen132 twice -Oz; +10 KiB rounded to KiB.
  // #3314 refusal fix e959e2a7dc2e: optimized web/Node 2,147,758 B; SHA256 1cb8da3ed26c7486315d8020848de440ce8670fb25ea45fa0fa5e1145e9f88e5.
  // Same pinned build.sh profile; measured +10 KiB still rounds to 2,158,592 B.
  txflow: 2_158_592,
  // #3362 fresh merge f10ca13c112 + master 05241cd0f41: web/node 1,980,557 B.
  // corepack yarn rust:build-wasm --venue weex; CI profiles, Binaryen132 twice-Oz; +10 KiB rounded up.
  // #3302 run 42720697962, job 129514220621: CI 1,991,705 B; local merge 06aa3a745fcc web/node 1,993,745 B.
  // build.sh --venue weex; Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132 twice -Oz.
  // Larger current measurement + 10 KiB, rounded up; retain both parent floors.
  // #3438 run 42745609629, family job 129597324422: web 1,991,983 B.
  // Current CI measurement + 10 KiB, rounded up to KiB (rule 17).
  // #3438 merged-head local build.sh --venue weex: web/node 1,993,495 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3429 merged head 1bb3f1352988: local optimized web/node 1,993,725 B; +10 KiB rounded up.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue weex.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; current measured headroom per rule 17.
  // #3438 merge cf112c51b87c + 3f94ec11a27f: optimized web/node 1,994,023 B.
  // Rust 1.99.0, bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded to KiB.
  // #3433 run 42742356027 at CI merge 972f1282ea16: web/node 1,991,688 B.
  // Current measured size + 10 KiB, rounded up to KiB.
  // #3433 branch 1547c3b2e287: optimized local web/node 1,993,200 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue weex.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; measured + 10 KiB, rounded up.
  // #3429 merged head 1bb3f1352988: local optimized web/node 1,993,725 B; +10 KiB rounded up.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue weex.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; current measured headroom per rule 17.
  // #3433 merge b57d4ab06df2 + master 4d399f716231: optimized local web/node 1,993,711 B.
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue weex.
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; measured + 10 KiB rounded up to KiB.
  // #3425 run 43176700451/job 131043125397, CI source cca7b8337ad3:
  // web/Node 2,010,529 B; current measured maximum + 10 KiB, rounded up.
  // Shared engine valuation/native-size policy; no venue activation or codec change.
  // #3391 run 43204096369/job 131135454191, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,018,048 B; current measurement + 10 KiB, rounded up.
  // #3391 run 43328298773, artifact 13993473438; source 46852c576bd1 (ca7e7ea34bf6 tree).
  // Optimized web 2,018,592 B + 10 KiB, rounded up to KiB.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,021,024 B.
  // build.sh --venue weex; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3397 run 43319466529/job 131536920672, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 2,011,369 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13989832819; SHA-256 62047d7bb3937991ed92bf91642fd8b3b5cc722c48bd96a4350d5f4a87ee4c81.
  // #3317 at 5d04ddb28ab: run 42703052461, family job 129453575116, node artifact 13712317711.
  // CI web/node 1,993,124 B; max + 10 KiB rounded up. CI merge 46c64a76e282 has the identical source tree.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14060691371.
  // Optimized web/Node/replay 2,021,455 B; module SHA256 01ce94a0481a27d6b9dc31ff20ce8d3caed9826d6035822faa8f2f892a34724f.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 11,185 B reserve; shares weex family.
  // #3317 merge 6a8318ff61c9: CI 43493007722, artifact 14070393067; source b07da9e11b0f has the same tree.
  // Optimized web/Node/replay 2,023,034 B; SHA256 f3aa4b5ef0d6c67ac1a332e8438d7dc03c2370333b4747a3ab4834d4e260cf6d.
  // Rust1.99, bindgen0.2.129, Binaryen132, required twice -Oz; current bytes + 10 KiB rounded up (10,630 B reserve).
  // Preserve both parent floors and all provenance above.
  // #3314 composed f6830c3f2466: optimized web/Node 2,028,105 B; SHA256 6e0f61345400027da6944a6e1d5fe80b77e976c159062493e68d785c2c25d204.
  // build.sh --venue weex; Rust1.99.0, bindgen0.2.129, Binaryen132 twice -Oz; +10 KiB rounded to KiB.
  weex: 2_038_784,
  // #3193 at 90750c898c2, CI job 128343606230: 2,417,950 B + 10 KiB, rounded up to KiB.
  // #3221 hosted signing: CI run 42352558360 measured web/node at 2,568,219 B; + 10 KiB, rounded up to KiB.
  // #3221 CI run 42393607294, artifact 13582294491: web/node 2,578,659 B + 10 KiB, rounded up to KiB.
  // #3395 run 42561995598, merge c4b68c461e1f: CI web 2,631,851 B + 10 KiB, rounded up.
  // #3442 CI run 42749390151, job 129609649777 at ded39a35466: web/node 2,654,237 B.
  // Current CI size + 10 KiB, rounded up to KiB.
  // #3397 run 43319466529/job 131536920584, source b9f5256efe4c (tree = merge 1663c1a885):
  // web/Node 2,657,521 B; current measured bytes + 10 KiB, rounded up; retain parent floors.
  // Artifact 13989742912; SHA-256 52bfe24e34c27248e12dc2b0d8b5cdc4de2b8c4b21603b7dbcdf9260c573afe7.
  // #3438 run 43204071123, artifact 13937331538, source 0340fd57a506 (head 7464a40315bc tree).
  // Verified web/Node 2,655,496 B; measured + 10 KiB rounded up; shared aliases equal.
  // #3317 at 8e55e01469b: run 42622329629, job 129178042670, artifact 13676298877.
  // CI WOO measured 2,636,672 B + 10 KiB, rounded up to KiB.
  // #3317 merged with master 4afcd22d45: run 42777931067, job 129701225317, artifact 13744522959.
  // CI merge 26181bafc6b6 (same tree as f6689ec9085): web/node 2,656,862 B + 10 KiB, rounded up to KiB.
  // #3391 run 43204096369/job 131135454115, source d62f6d989c3a (3de5a492d792 tree).
  // Optimized web/Node 2,664,064 B; current measurement + 10 KiB, rounded up.
  // #3391 head 133b901dc12b (master b99d9867a1d8): local optimized web/Node 2,665,054 B.
  // build.sh --venue woo; Rust 1.99.0, bindgen 0.2.129, Binaryen 132; +10 KiB rounded up.
  // #3391 head a545cbe2fca0, CI run 43439489621, artifact 14045298425.
  // Source 4f137fb49815 has the reviewed head tree; optimized web/Node/replay 2,666,111 B.
  // Module SHA256 09297ace386e55b238f24ef0c1d3fdf2a266d10cc1e2822a76834a444c401de9.
  // Current measured size + 10 KiB, rounded up; historical floors and overflow controls retained.
  // #3391 merged source df6f095bd740, incorporating master 353a3ab925b0: local web/Node 2,667,111 B.
  // build.sh --venue woo; Rust1.99, bindgen0.2.129, Binaryen132, required two -Oz passes.
  // Both module SHA256 fa221b247c4c9d795922cf2a195d327c5bfc5b10ff5eac17f604268fb54ab1d8.
  // Fresh measured size + 10 KiB, rounded up: 10,649 B reserve; all historical floors retained.
  // #3391 CI run 43467762505, source c0867d1af0ee (1967f27d62ae tree), artifact 14059749363.
  // Optimized web/Node/replay 2,669,297 B; module SHA256 06acfb1b57dfa76fca4988f3b703e3bc609791496dba1b884f56f66b726d7a44.
  // Rust1.99, bindgen0.2.129, Binaryen132, required -Oz; current bytes + 10 KiB rounded up.
  // Retain historical floors; 10,511 B reserve; shares woo family.
  // #3317 merge 6a8318ff61c9: CI 43493007722, artifact 14070187529; source b07da9e11b0f has the same tree.
  // Optimized web/Node/replay 2,671,977 B; SHA256 4a7b69619d603116b5786c53eb71642564416ffcee2e55a918fc5ff88d13e205.
  // Rust1.99, bindgen0.2.129, Binaryen132, required twice -Oz; current bytes + 10 KiB rounded up (10,903 B reserve).
  // Preserve both parent floors and all provenance above.
  woo: 2_682_880,
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
