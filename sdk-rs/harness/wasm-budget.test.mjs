import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { venueFamily, venueFamilies } from '../hosts/wasm/js/venues.mjs';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

// #2301 shared Venue declaration, exact source fce94fa3c1000811e23e5e520dcb9dcec71d934c.
// Full dispatch44370048905; raw optimized web/Node/replay modules and pinned manifests verified.
// Artifacts14459236107 (Hyperliquid),14458384002 (Coinbase). Keep all older floors below.
for (const [family, aliases, measured, previousFloor, requiredCap] of [
  ['hyperliquid', ['hyperliquid', 'hyperliquidspot'], 2_309_110, 2_317_312, 2_319_360],
  ['coinbase', ['coinbase', 'coinbasespot'], 2_349_208, 2_359_296, 2_360_320],
]) {
  test(`#2301 shared-input ${family} artifact retains current reserve and historical floors`, () => {
    assert.equal(requiredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= Math.max(requiredCap, previousFloor));
    assert.ok(cap - measured >= 10 * 1024);
    for (const venue of aliases) {
      assert.equal(venueFamily(venue), family);
      if (Object.hasOwn(WASM_VENUE_OVERRIDES, venue)) {
        assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
      }
    }
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

// #2301 alias-retention source08d18516706c, CI44275298247/job134703677883.
// Artifact14414113480: optimized web/Node/replay SHA25690538a5261dd88c12a5381a6a1bb40fbfa0a6ebef06a9c1b3305cb6e9faf2434.
test('#2301 retained caller aliases preserve current measured reserve and prior floors', () => {
  const measured = 2_014_499;
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bitunix;
  assert.equal(requiredCap, 2_025_472);
  assert.ok(cap >= Math.max(requiredCap, 2_023_424));
  assert.ok(cap - measured >= 10 * 1024);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bitunix'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bitunix'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bitunix'), /exceeds/);
});

// #2301 CI44266617073/jobs134673305255+134673305783, source b1ac2260570b.
// Optimized web/Node 2,012,442 B, SHA256 6dfd360519771817ea5e071168dca2a462a67654ec08592d134783345f9b524c.
test('#2301 managed Bitunix sockets retain measured reserve and the historical floor', () => {
  const measured = 2_012_442;
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bitunix;
  assert.equal(requiredCap, 2_023_424);
  assert.ok(cap >= Math.max(requiredCap, 1_967_104));
  assert.ok(cap - measured >= 10 * 1024);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bitunix'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bitunix'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bitunix'), /exceeds/);
});

// #3669 run 44325589048, artifact 14437511989: optimized Hyperliquid 2,309,206 B.
// Coordinator approved measured +10 KiB rounded up, retaining all parent floors.
for (const venue of ['hyperliquid', 'hyperliquidspot']) {
  test(`#3669 ${venue} diagnostics retain measured reserve and strict boundaries`, () => {
    const measured = 2_309_206;
    const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(requiredCap, 2_320_384);
    assert.ok(cap >= Math.max(2_317_312, requiredCap));
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

// #3643 CI44132069302/job134212792364, composition9fd6c31b5299 (PR36a0f89ed4f2 + mastercd4052047a9f).
// Optimized web/Node 2,292,953 B, SHA256 ef43266ef637108ab003a62b16674079df413c8db0a6fe1c10edaa277281ef53.
// Coordinator approved measured +10 KiB rounded up on 2026-11-18; all earlier floors remain.
test('#3643 Kraken row diagnostics retain measured reserve and shared alias floors', () => {
  const measured = 2_292_953;
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(requiredCap, 2_304_000);
  for (const venue of ['kraken', 'krakenspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= Math.max(2_299_904, requiredCap), `${venue}: retain the measured reserve and parent floor`);
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

// #3647 pinned Rust1.99/bindgen0.2.129/Binaryen132 twice-Oz web/Node measurements.
// 80274d2: 2,066,659 B, SHA11835d00d138a4c751944336d8de85c449e1c85cd14e64fdfee963805b05a3d6.
// b309848: 2,063,838 B, SHA6dee6055e89e1884781a44f0abddb92b1c463b2b5c6cc2629376bbdca6c31eef.
test('#3647 OKX diagnostics retain both measured reserves and the historical floor', () => {
  const measurements = [2_066_659, 2_063_838];
  const minimumCap = Math.ceil((Math.max(...measurements) + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.okx;
  assert.equal(minimumCap, 2_077_696);
  assert.ok(cap >= Math.max(minimumCap, 2_069_504));
  for (const measured of measurements) {
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'okx'), undefined);
  }
  assert.equal(wasmBudgetError(cap, 'venue', 'okx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'okx'), /exceeds/);
});

// #3579 CI43782227341/jobs133022239190+133022239183: web/Node 2,305,365 B.
// Local pinned build at b323ddf25c4a: web/Node 2,306,669 B; retain the larger reserve.
// Both aliases share one family; historical 2,279,424 B floor remains authoritative.
test('#3579 Hyperliquid admission retains both current measurements plus 10 KiB', () => {
  const measured = Math.max(2_305_365, 2_306_669);
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(requiredCap, 2_317_312);
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= Math.max(requiredCap, 2_279_424));
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

// #3566 pinned build.sh --venue backpack: Rust1.99, bindgen0.2.129, Binaryen132 twice-Oz.
// Local optimized web/Node SHA256 a5336b7d6336e82ac8c8d8c45633be984b4b56e38928662799d0a37e858f381d.
// CI43755040219/job132937508086 web2076180; only Backpack retains default device dispatch.
test('#3566 device admission retains the measured Backpack reserve and historical floor', () => {
  const measured = Math.max(2_082_554, 2_076_180);
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.backpack;
  assert.equal(requiredCap, 2_093_056);
  assert.ok(cap >= Math.max(requiredCap, 2_060_288));
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'backpack'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'backpack'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'backpack'), /exceeds/);
});

// #3266 merge 2d0fdf629b84: equal optimized web/Node artifacts, build.sh --venue mexc.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz.
// Module SHA256 e8939820ec914ac233aafc4ef82692d2fda4478800a10f85a7597073810af27f.
test('#3266 clock-safety composition retains the measured MEXC 10 KiB reserve', () => {
  const measured = 1_960_367;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.mexc;
  assert.equal(minimumCap, 1_971_200);
  assert.ok(cap >= minimumCap, `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'mexc'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'mexc'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'mexc'), /exceeds/);
});

// #3266 merge 2d0fdf629b84: equal optimized web/Node artifacts, build.sh --venue polymarket.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz.
test('#3266 clock-safety composition retains the measured Polymarket 10 KiB reserve', () => {
  const measured = 1_665_253;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.polymarket;
  assert.equal(minimumCap, 1_676_288);
  assert.ok(cap >= minimumCap, `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'polymarket'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'polymarket'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'polymarket'), /exceeds/);
});

// #3513 run 43622387718, artifact 14125280270; source 0a27110da82221143bc3f51377130ae5f98c22ea.
// sdk-rs tree d1ae9a6dfa82f578465f85541feb6ae013823bc5 equals reviewed b5231048d4.
// Optimized web/Node SHA256 bd2760d7eaa57e7ef051fa344dc7ff5cb7b23b4791bbd0a971b550087b0b6c41.
test('#3513 protective preflight retains its measured CI reserve for every Bybit alias', () => {
  const measured = 2_557_426;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_568_192);
  for (const venue of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader']) {
    const family = venueFamily(venue);
    assert.equal(family, 'bybit');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

// Owner-fence fix round: build.sh --venue bybit, Rust1.99, bindgen0.2.129, Binaryen132 twice -Oz.
// Optimized web/Node SHA256 9c8836adf04d0a678f0725d213ab3a5e6ecad2c62cdf1a98003b23fa6d3b9834.
test('#3513 owner-fence composition retains its fresh local reserve and higher CI floor', () => {
  const measured = 2_555_348;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.equal(minimumCap, 2_566_144);
  assert.ok(cap >= Math.max(minimumCap, 2_568_192));
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

// #3513 run43629571410, source280b4708aa45963ab4ade5e29b20b9b2754841b0: only a Kraken test differs from21acae318c.
// Verified identical web/Node/replay and manifest hashes; Rust1.99, bindgen0.2.129, Binaryen132 twice -Oz.
// Preserve every parent floor and measured +10 KiB rounded up, including live cap/cap+1 refusal.
const ownerMetadataCertificates = [
  ['apexomni', 14127810819, 2_101_409, 2_112_512, 2_111_488, '9d4e40a935dc8bd57ba529ce0c356ae1eebdf9b5916b4820b043f3cd709dc18a'],
  ['asterdex', 14127303101, 2_265_275, 2_276_352, 2_275_328, '77d3af865052d9be5a9b965415671b2ed7d43819e4d38e67ed1d311e85ee3a71'],
  ['backpack', 14128096165, 2_049_388, 2_060_288, 2_059_264, '39027694b3f5dd6d5ca8480f1680403cdad7710eb63ad937effded72e80edb47'],
  ['binance', 14127681155, 2_280_227, 2_290_688, 2_290_688, '87ead58e27e28fa012a122f2a2f1bc978dd30a48098d391a553ae235394e5484'],
  ['bingx', 14127507348, 2_144_095, 2_154_496, 2_174_976, 'dc5edd65730e7cf1e65d5e0c06494cd1f3fffc7ff8be1243c99574cc7144f858'],
  ['bitget', 14127327967, 2_770_055, 2_781_184, 2_781_184, 'c4ac95e28f3477d54a85da19d802edf8c3de4176e49d534283e96fc78a944026'],
  ['bitrue', 14127875766, 1_910_052, 1_921_024, 1_925_120, '8b927653347cba75fee36e33b61b90af5c2b7cf95e45bde58e04df00e0a306c2'],
  ['bitunix', 14127976229, 1_956_569, 1_967_104, 1_967_104, 'c171528627df8f391a6cbf17fae7c2f31e70769a0fb30e16ef368e402a619981'],
  ['blofin', 14127810932, 2_072_958, 2_083_840, 2_085_888, 'd5342804d9e1fce4ec59701c8d2806f758fd9abeb4b1640227637b94792c49d6'],
  ['bybit', 14127507376, 2_557_502, 2_568_192, 2_568_192, '20b8de4a1824fa7a49b307375da6a167da27618219c89e1ad47fd9e8ac7070bd'],
  ['coinbase', 14127333141, 2_221_333, 2_232_320, 2_232_320, 'bc02f877d2a6d7d5da8692f6d91b61d34752151e80196f99928472260e007825'],
  ['cryptocom', 14127945791, 1_817_571, 1_827_840, 1_827_840, 'e3fdbb36e0c6a3e8e7fa8fe46061c69240f69bee149cd3b234cd419fcfcdee29'],
  ['grvt', 14127521281, 2_004_367, 2_015_232, 2_014_208, 'a0d4892c8427ef329242e28a5ea955af655730cfb0bd431138198093b013c475'],
  ['hyperliquid', 14127288589, 2_268_301, 2_279_424, 2_278_400, 'a8bec6534efe4913528b46c9147e2f4c971aaad97462bbdb00884c2f41e31eab'],
  ['kraken', 14126954553, 2_277_017, 2_287_616, 2_287_616, '02e18fe1f43ba2befe3d90d9f1d7fa39b5c4a60db57cadf6dec774559a147b99'],
  ['kucoin', 14127716690, 1_866_642, 1_876_992, 1_878_016, '48cc562dad88e11c9255a5ced3b56d1097d7233c951f790172756dd4f9707ec1'],
  ['kucoinspot', 14127981482, 2_478_273, 2_489_344, 2_488_320, 'f03c7ff8686b83f221b908ab9fab8c45cd23f1906cdc0521bfa1fdb1df5241bb'],
  ['lighter', 14127840849, 2_187_365, 2_198_528, 2_197_504, '50b7838a11a59a2d6c8209554e57161e10e76a475f4237c8093277361e7fd46c'],
  ['mexc', 14127791747, 1_960_743, 1_971_200, 1_971_200, 'e9f97f2812047091633b3b0da133e8f681ba4eda06704f9c458f8920eb413d57'],
  ['okx', 14126639830, 2_058_368, 2_069_504, 2_068_480, 'ece7c50d45d5662aa88322192db8c0a2b39060ee84172742b1d2233a995b7317'],
  ['orderly', 14127487221, 1_784_084, 1_795_072, 1_794_048, '66a7196b556875b1b3e4e31bc9859125b62e11df8105a6d323a6648f1307151a'],
  ['paradex', 14127751729, 2_311_734, 2_322_432, 2_322_432, '6e0a3a6c6f905f285ef7e9200ce7b4a174779cab13a879631a79897c1c1a028d'],
  ['phemex', 14127457235, 2_145_381, 2_156_544, 2_155_520, '6d32c08265abb4f62cb6aedaa7e136bed639ebd185fffde4b3d0cf058bf0ad91'],
  ['polymarket', 14127785957, 1_665_383, 1_676_288, 1_676_288, '0696ebe921b2e6e0f2f4f891fa12182157ac7859bb3261fc404ad3ab37d8650a'],
  ['txflow', 14127288519, 2_166_747, 2_177_024, 2_177_024, '30d722886500bbde7610e69048ddf7239ae5afa2fedd13c1f03edff4ed8a6d6e'],
  ['weex', 14128200821, 2_030_211, 2_040_832, 2_043_904, '03fa189b0710d4e49029c00e7ff1629b818b9aaae25554b426ae61aaee829f75'],
  ['woo', 14127680983, 2_681_664, 2_692_096, 2_691_072, '61b2119b17289f18981a36d7b551c9c64871c55f4f7c59abb6a46c398d978318'],
];
for (const [family, artifact, measured, minimumCap, parentCap, sha256] of ownerMetadataCertificates) {
  test(`#3513 owner metadata: ${family} CI artifact ${artifact} ${sha256} retains reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= Math.max(minimumCap, parentCap), `${family}: ${cap} < ${Math.max(minimumCap, parentCap)}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}
test('#3513 owner metadata certificates cover every shipped family', () => {
  assert.deepEqual(ownerMetadataCertificates.map(([family]) => family).sort(), [...venueFamilies].sort());
});

// #3317 run 43457064180 checks integration source 05d55b30c6f7, not just the PR tree.
// Family job 131994663823 and replay job 131994664428 independently refused these bytes.
// Current run 43461440513: family 132020005385 and replay 132020005877 agree;
// integration 433d752a7d43 has merged head 371118f7c19c's tree and build-input fingerprint.
for (const [source, measured, minimumCap] of [
  ['CI integration 05d55b30c6f7', 2_516_764, 2_527_232],
  ['CI composed master 433d752a7d43', 2_539_197, 2_549_760],
]) {
  test(`#3317 composed Bybit ${source} retains measured reserve for every alias`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const venue of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader']) {
      const family = venueFamily(venue);
      assert.equal(family, 'bybit');
      const cap = WASM_VENUE_OVERRIDES[family];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
    }
  });
}

// Recorded CI caps are provenance floors; later PRs may raise the live cap.
// Boundary checks always exercise the live cap and the byte immediately above it.

test('#3424 merged local Bybit artifact retains measured headroom', () => {
  const measured = 2_332_681;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

// #3391 merge aa2dc74ee433 + master aa78ef0a06b7; local optimized web/Node.
// Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132, required two -Oz passes.
// Both modules SHA256 b317a14e05a5e75e26c659a226c1e316f50301d81045fe51f14d520ba574c7e8.
test('#3391 master merge retains fresh AsterDEX measured 10 KiB reserve', () => {
  const measured = 2_231_055;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.equal(minimumCap, 2_241_536);
  assert.ok(cap >= minimumCap, `asterdex: ${cap} must retain ${minimumCap} bytes`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});

// #3397 run 43364338606, source ba06b221f45fdacf7e90bd48cb4f4c93e5be9d2d:
// tree equals head fa06619c7d21f13db5d6acd05f2cc2609fcdf9f8.
// Verified artifact ZIP digests, replay manifests and identical web/Node/replay bytes.
// Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; required -Oz build.
for (const [venue, family, artifact, measured, minimumCap] of [
  ['bybit', 'bybit', 14009728045, 2_332_649, 2_342_912],
  ['cryptocom', 'cryptocom', 14010191105, 1_804_494, 1_815_552],
  ['lighter', 'lighter', 14010330544, 2_148_847, 2_159_616],
  ['lighter-robinhood', 'lighter', 14010330544, 2_148_847, 2_159_616],
]) {
  test(`#3397 merged fa06619c: ${venue} artifact ${artifact} retains fresh measured headroom`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(venueFamily(venue), family);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(cap, WASM_VENUE_OVERRIDES[family]);
    assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

test('#3397 Orderly CI run 43192535364 retains measured 10 KiB headroom', () => {
  // Job 131096463465, artifact 13932495107; identical optimized web/Node modules.
  // SHA-256 e249271d883327d158591bfa298c49a23b60d4d25e105938ed86adfbc9927b3d.
  const measured = 1_768_995;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.orderly;
  assert.equal(minimumCap, 1_779_712);
  assert.ok(cap >= minimumCap, `orderly: ${cap} must retain ${minimumCap} bytes`);
  assert.equal(wasmBudgetError(measured, 'venue', 'orderly'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'orderly'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'orderly'), /exceeds/);
});

// Fresh CI run 43319466529, composition b9f5256efe4c5a0370478f9d2309a84c35d3750b:
// tree equals merge 1663c1a885eeb476556fb6a957e351f8a9354b66.
// Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132 (two -Oz passes).
// Web/Node and replay copies match for 26 family artifacts. KuCoin Spot's
// web-size refusal (job 131536920530) matches the verified Node capsule size
// (artifact 13990151877); its family artifact is not uploaded after the refusal.
for (const [venue, measured, minimumCap] of [
  ['apexomni', 2_082_984, 2_094_080],
  ['asterdex', 2_211_866, 2_223_104],
  ['backpack', 2_029_683, 2_040_832],
  ['binance', 2_260_506, 2_271_232],
  ['bingx', 2_119_360, 2_129_920],
  ['bitget', 2_470_849, 2_481_152],
  ['bitrue', 1_896_666, 1_907_712],
  ['bitunix', 1_942_779, 1_953_792],
  ['blofin', 2_053_666, 2_064_384],
  ['bybit', 2_331_426, 2_341_888],
  ['coinbase', 2_103_069, 2_113_536],
  ['cryptocom', 1_804_182, 1_814_528],
  ['grvt', 1_990_869, 2_001_920],
  ['hyperliquid', 2_225_121, 2_235_392],
  ['kraken', 2_261_215, 2_272_256],
  ['kucoin', 1_853_689, 1_864_704],
  ['kucoinspot', 2_464_041, 2_475_008],
  ['lighter', 2_147_689, 2_158_592],
  ['mexc', 1_947_286, 1_957_888],
  ['okx', 2_038_817, 2_050_048],
  ['orderly', 1_770_708, 1_781_760],
  ['paradex', 2_297_964, 2_309_120],
  ['phemex', 2_125_631, 2_136_064],
  ['polymarket', 1_652_282, 1_662_976],
  ['txflow', 1_916_148, 1_927_168],
  ['weex', 2_011_369, 2_022_400],
  ['woo', 2_657_521, 2_668_544],
]) {
  test(`#3397 merged 1663c1a885: ${venue} retains fresh measured headroom`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const aliases = Object.keys(WASM_VENUE_OVERRIDES).filter((name) => venueFamily(name) === venue);
    assert.ok(aliases.includes(venue));
    for (const alias of aliases) {
      const cap = WASM_VENUE_OVERRIDES[alias];
      assert.ok(cap >= minimumCap, `${alias}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured, 'venue', alias), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', alias), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', alias), /exceeds/);
    }
  });
}

// Local optimized web/node bytes match at merge fcee82e27df; Rust 1.99.0,
// wasm-bindgen 0.2.129 and Binaryen 132. Preserve both parent floors.
for (const [venue, measured, minimumCap] of [
  ['bybit', 2_329_775, 2_340_864],
  ['coinbase', 2_105_215, 2_115_584],
  ['mexc', 1_927_721, 1_938_432],
]) {
  test(`#3397 merged fcee82e27df: ${venue} retains current measured headroom`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

test('#3397 exact-head CI (run 42552867793 at 2cfc2f3a0ce) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_281_554, 2_292_736],
    ['coinbase', 1_954_575, 1_965_056],
    ['mexc', 1_873_137, 1_884_160],
    ['orderly', 1_744_951, 1_756_160],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, venue);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

// CI merge be4ca1f8a99c matches head f4223be642f5 exactly; web/node hashes match.
for (const [venue, measured, minimumCap, job] of [
  ['bybit', 2_316_667, 2_327_552, '129025076016'],
  ['coinbase', 1_968_376, 1_979_392, '129025075890'],
  ['mexc', 1_888_209, 1_898_496, '129025075967'],
]) {
  test(`#3397 run 42571903722 attempt 2 job ${job}: ${venue} retains measured headroom`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, venue);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

// #3317 run 43321890002, attempt 1; CI source e969a3bf1c92 has the head's sdk-rs tree.
// Node artifacts 13990169480, 13991900630 and 13992140380; family logs match the web sizes.
for (const [family, measured, minimumCap] of [
  ['grvt', 1_991_790, 2_002_944],
  ['kucoinspot', 2_463_737, 2_473_984],
  ['polymarket', 1_653_005, 1_664_000],
]) {
  test(`#3317 current composed ${family} build retains measured plus 10 KiB headroom`, () => {
    assert.equal(venueFamily(family), family);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${family}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

test('#3317 Bybit throughput profile retains measured headroom for every alias', () => {
  // Run 43060399359/job 130665084604, sealed artifact 13874660310.
  const web = 2_481_896, node = 2_481_896;
  const minimumCap = 2_492_416;
  assert.equal(minimumCap, Math.ceil((Math.max(web, node) + 10 * 1024) / 1024) * 1024);
  for (const venue of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader']) {
    const family = venueFamily(venue);
    assert.equal(family, 'bybit');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(web, 'venue', family), undefined);
    assert.equal(wasmBudgetError(node, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

test('#3317 Bitget throughput profile retains measured headroom for every alias', () => {
  // #3317 engine/host throughput build, pinned Rust 1.99.0/bindgen 0.2.129/Binaryen 132.
  const web = 2_754_487, node = 2_754_487;
  const minimumCap = 2_764_800;
  assert.equal(minimumCap, Math.ceil((Math.max(web, node) + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES.bitgetusdc, WASM_VENUE_OVERRIDES.bitget);
  for (const venue of ['bitget', 'bitgetusdc', 'bitgetinverse']) {
    const family = venueFamily(venue);
    assert.equal(family, 'bitget');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(web, 'venue', family), undefined);
    assert.equal(wasmBudgetError(node, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

// #3317 run 42703052461 at 5d04ddb28ab (CI merge 46c64a76e282 has the identical tree).
// Web sizes come from family jobs; node sizes were re-measured from sealed CI artifacts.
for (const [family, aliases, web, node, minimumCap] of [
  ['hyperliquid', ['hyperliquid', 'hyperliquidspot'], 2_237_802, 2_237_802, 2_248_704],
  ['weex', ['weex'], 1_993_124, 1_993_124, 2_003_968],
  ['woo', ['woo'], 2_656_709, 2_656_709, 2_667_520],
]) {
  test(`#3317 current CI ${family} web/node retains measured wasm headroom`, () => {
    const measured = Math.max(web, node);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const venue of aliases) {
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
      assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

test('#3268 (run 42404306341 at fca7d275e3a) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1863967, 1874944],
    ['bybit', 2270429, 2281472],
    ['kraken', 2136627, 2147328],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3268 (run 42422775913 at 1af9adc3527) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['cryptocom', 1_740_870, 1_752_064],
    ['lighter', 2_093_401, 2_104_320],
    ['paradex', 2_266_161, 2_277_376],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3268 restage (run 42563664495 at d83accc10298) retains current measured headroom', () => {
  for (const [family, measured] of [
    ['apexomni', 2_041_066],
    ['asterdex', 2_185_503],
    ['backpack', 2_024_787],
    ['binance', 2_252_168],
    ['bingx', 2_110_452],
    ['bitget', 2_474_128],
    ['bitrue', 1_867_797],
    ['bitunix', 1_914_440],
    ['blofin', 2_038_821],
    ['bybit', 2_314_201],
    ['coinbase', 1_965_618],
    ['cryptocom', 1_790_943],
    ['grvt', 1_976_043],
    ['hyperliquid', 2_216_378],
    ['kraken', 2_163_221],
    ['kucoin', 1_854_448],
    ['kucoinspot', 2_433_114],
    ['lighter', 2_120_209],
    ['mexc', 1_885_764],
    ['okx', 2_036_943],
    ['orderly', 1_756_808],
    ['paradex', 2_289_098],
    ['phemex', 2_102_363],
    ['polymarket', 1_634_988],
    ['txflow', 1_916_025],
    ['weex', 1_968_079],
    ['woo', 2_627_455],
  ]) {
    const siblings = Object.keys(WASM_VENUE_OVERRIDES).filter(venue => venueFamily(venue) === family);
    assert.ok(siblings.length > 0, family);
    for (const venue of siblings) {
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= Math.ceil((measured + 10 * 1024) / 1024) * 1024, venue);
      assert.equal(cap, WASM_VENUE_OVERRIDES[family], venue);
      assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  }
  assert.ok(WASM_BUDGETS.combined >= 16_941_885 + 10 * 1024);
  assert.equal(wasmBudgetError(16_941_885, 'combined'), undefined);
});

test('combined shipping build and standalone builds retain distinct fixed ceilings', () => {
  assert.equal(WASM_BUDGETS.venue, 1_343_488);
  // #3640 raised the combined ceiling from 17 MiB to 17.25 MiB (native store cutover; see wasm-budget.mjs).
  assert.equal(WASM_BUDGETS.combined, 17.25 * 1024 * 1024);
  for (const profile of Object.keys(WASM_BUDGETS)) {
    assert.equal(wasmBudgetError(WASM_BUDGETS[profile], profile), undefined);
    assert.match(wasmBudgetError(WASM_BUDGETS[profile] + 1, profile), /exceeds/);
  }
  for (const value of [0, -1, Infinity, NaN]) assert.match(wasmBudgetError(value, 'combined'), /invalid/);
});





// The 17 MiB ceiling (#3140) moved to 17.25 MiB in #3640; its measured sizes stay accepted.
test('the approved combined ceiling accepts 17.25 MiB exactly and rejects the next byte', () => {
  for (const venue of [undefined, ...Object.keys(WASM_VENUE_OVERRIDES)]) {
    assert.equal(wasmBudgetError(16_778_652, 'combined', venue), undefined, '#3140 measured size');
    assert.equal(wasmBudgetError(17_856_202, 'combined', venue), undefined, '#3640 measured size');
    assert.equal(wasmBudgetError(18_087_935, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(18_087_936, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(18_087_937, 'combined', venue), 'combined wasm 18087937 exceeds 18087936 bytes');
  }
});

test('shared broker-role setup caps retain CI headroom and reject the next byte', () => {
  for (const [venue, measured, minimumCap] of [
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('KuCoin 400002 resync caps retain the approved CI headroom and exact boundary', () => {
  for (const [venue, measured, minimumCap] of [
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('master 41783084277 caps retain the ruled headroom and shared-family boundary', () => {
  for (const [venue, measured, minimumCap] of [
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('CI budgets each standalone venue build under its own venue name', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const step = workflow.indexOf('name: Check family isolation, every alias and the shipped size');
  assert.ok(step >= 0);
  assert.match(workflow.slice(step, step + 220), /RUST_TEST_FAMILY: \$\{\{ matrix\.family \}\}/);
  assert.match(workflow.slice(step, step + 220), /rust:test-families/);
  const familyCheck = readFileSync(new URL('./families.integration.mjs', import.meta.url), 'utf8');
  assert.match(familyCheck, /wasmBudgetError\(size, 'venue', family\)/);
});

test('CI budgets the combined artifact immediately after its build before standalone overwrite', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const combined = workflow.indexOf('name: Build the combined wasm engine');
  const budget = workflow.indexOf('name: Check the shipped combined wasm size budget');
  const archive = workflow.indexOf('name: Retain built combined module for reuse');
  assert.ok(combined >= 0 && budget > combined && archive > budget);
  assert.match(workflow.slice(budget, archive), /rust:wasm-budget --combined/);
});

test('the Bybit capability comparison budgets ordinary Bybit under its own venue name', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const step = workflow.indexOf('name: Build and measure both capabilities from this checkout');
  const compare = workflow.indexOf('name: Compare measurements and require ordinary Bybit budget');
  assert.ok(step >= 0 && compare > step);
  assert.match(workflow.slice(step, compare), /wasmBudgetError\(bytes\.length, 'venue', 'bybit'\)/);
  assert.match(workflow.slice(step, compare), /budgetBytes: wasmBudgetBytes\('venue', 'bybit'\)/);
});

test('every venue carries its LOCAL-algos family cap, inclusive, and rejects the next byte', () => {
  const caps = {
    apexomni: 1_866_752,
    asterdex: 1_999_872,
    backpack: 1_780_736,
    binance: 2_041_856,
    bingx: 1_866_752,
    bitget: 2_150_400,
    bitgetusdc: 2_150_400,
    bitrue: 1_719_296,
    bitunix: 1_727_488,
    blofin: 1_824_768,
    bybit: 2_032_640,
    coinbase: 1_720_320,
    cryptocom: 1_612_800,
    grvt: 1_686_528,
    hyperliquid: 1_964_032,
    hyperliquidspot: 1_964_032,
    kraken: 2_002_944,
    krakenspot: 2_002_944,
    kucoin: 1_494_016,
    kucoinspot: 1_969_152,
    lighter: 1_864_704,
    'lighter-robinhood': 1_864_704,
    mexc: 1_634_304,
    okx: 1_797_120,
    orderly: 1_632_256,
    paradex: 2_102_272,
    phemex: 1_901_568,
    polymarket: 1_509_376,
    txflow: 1_736_704,
    weex: 1_824_768,
    woo: 2_238_464,
  };
  for (const [venue, minimumCap] of Object.entries(caps)) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('read-cover batch caps (cut 41796230942 at 3b9f6b0fe2b) retain headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
  assert.equal(WASM_VENUE_OVERRIDES.lighter, WASM_VENUE_OVERRIDES['lighter-robinhood']);
});

test('#3150 final CI 42258955401 cap refresh: refresh-ask caps (PR run 41804469904 at 466b16cd11e) retain headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
    ['coinbase', 1_816_299, 1_826_816],
    ['txflow', 1_839_762, 1_850_368],
    ['weex', 1_925_507, 1_936_384],
    ['woo', 2_402_361, 2_413_568],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3193 coinbase CI job 128343606114 retains measured headroom and the exact boundary', () => {
  const measured = 1_841_539;
  const minimumCap = 1_852_416;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.coinbase;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES.coinbase >= cap);
  for (const alias of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(alias);
    assert.equal(family, 'coinbase');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3148 protective edit live fixes (run 42196761441) retain headroom and siblings match', () => {
  const measured = 2_348_153;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_359_296);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3202 plan cancel confirmation (run 42280173727) retains headroom and siblings match', () => {
  const measured = 2_379_303;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_390_016);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3198 + #3201 combined on master 6228bbef939 retain Bitget headroom and siblings match', () => {
  const measured = 2_364_891;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_375_680);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3018 after merging 78bc1ae5a39 (run 41870743418 at c9d4de158d5) retains headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
    ['kraken', 2_125_098, 2_136_064],
    ['krakenspot', 2_125_098, 2_136_064],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#2351 managed ingress (run 42402512142) retains Kraken headroom and equal siblings', () => {
  const measured = 2_136_136;
  const minimumCap = 2_147_328;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.kraken;
  assert.ok(cap >= minimumCap);
  for (const venue of ['kraken', 'krakenspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3304 restage (run 42558782819 at merge a068e6cb0876) retains Kraken CI headroom and equal siblings', () => {
  // Family job 128970669128 and replay jobs 128970669384/128970669374 agree on web/node size.
  const measured = 2_228_472;
  const minimumCap = 2_239_488;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['kraken', 'krakenspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap + 1, 'venue', venue), `venue wasm ${cap + 1} exceeds ${cap} bytes`);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3304 refusal ordering at 0b7e6ccf60f3 retains optimized Kraken headroom and equal siblings', () => {
  // SAFE_CEX_WASM_OPT=required bash sdk-rs/harness/build.sh --venue kraken;
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132: web/node both 2,234,008 B.
  const measured = 2_234_008;
  const minimumCap = 2_244_608;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['kraken', 'krakenspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap + 1, 'venue', venue), `venue wasm ${cap + 1} exceeds ${cap} bytes`);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3259 KuCoin/Kraken ambiguous writes (run 42401545356 at f9ec901182f) retain headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
    ['kraken', 2_136_165, 2_147_328],
    ['krakenspot', 2_136_165, 2_147_328],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3018 protective-amend caps (run 41854021043 at 824ff543b6d) retain the ruled headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['polymarket', 1_590_031, 1_600_512],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3150 final CI 42258955401 cap refresh: refresh-ask caps after #3039 (PR run 41845929983 at 9bf12da7671) retain headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_850_459, 1_861_632],
    ['cryptocom', 1_727_568, 1_738_752],
    ['kucoinspot', 2_253_260, 2_264_064],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.lighter, WASM_VENUE_OVERRIDES['lighter-robinhood']);
});

test('#3150 final CI 42258955401 cap refresh: Phemex read-cover caps (PR run 41868980693 at d8899b7a3e0) retain headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
    ['phemex', 2_066_964, 2_077_696],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3245 Phemex CI 42367120442 at 46378cfab09 retains the ruled headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['phemex', 2_079_779, 2_091_008],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3249 monotonic engine clock retains measured Phemex headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['phemex', 2_079_916, 2_091_008],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3018 after merging d171fbb0e31 (run 41893192741 at b94d76fa368) retains the ruled headroom', () => {
  for (const [venue, measured, cap] of [
    ['orderly', 1_721_849, 1_732_608],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const currentCap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(currentCap >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(currentCap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(currentCap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3193 orderly provenance at ad4f1d6d5d9 keeps the larger composed cap', () => {
  const measured = 1_734_176;
  const measuredCap = 1_744_896;
  const incomingCap = 1_732_608;
  const minimumCap = 1_744_896;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(minimumCap, Math.max(incomingCap, measuredCap));
  const cap = WASM_VENUE_OVERRIDES.orderly;
  assert.ok(cap >= minimumCap);
  for (const name of ['orderly', 'kodiakfi', 'mode', 'what', 'woofi']) {
    const family = venueFamily(name);
    assert.equal(family, 'orderly');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3179 COIN-M algo routing (run 42250922133) retains the ruled binance headroom', () => {
  const [measured, minimumCap] = [2_207_171, 2_217_984];
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.binance;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES.binance >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'binance'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['binance'] + 1, 'venue', 'binance'), /exceeds/);
});

test('#3179 on master 160884d020d (run 42319187479) uses CI plus 10 KiB for every Binance alias', () => {
  const measured = 2_224_977;
  const cap = 2_235_392;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'binance');
    const currentCap = WASM_VENUE_OVERRIDES[family];
    assert.ok(currentCap >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(currentCap, 'venue', family), undefined);
    assert.match(wasmBudgetError(currentCap + 1, 'venue', family), /exceeds/);
  }
});

test('master b0d2122742d release cut (run 41901638294) retains the ruled binance headroom', () => {
  const [measured, minimumCap] = [2_195_793, 2_206_720];
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.binance;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES.binance >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'binance'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['binance'] + 1, 'venue', 'binance'), /exceeds/);
});

test('#3187 PM algo migration (run 42251340763) retains headroom for every Binance alias', () => {
  const [measured, minimumCap] = [2_212_299, 2_223_104];
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.binance;
  assert.ok(cap >= minimumCap);
  for (const venue of ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'binance');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3193 binance provenance at ad4f1d6d5d9 keeps the larger composed cap', () => {
  const measured = 2_225_818;
  const measuredCap = 2_236_416;
  const incomingCap = 2_235_392;
  const minimumCap = 2_236_416;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(minimumCap, Math.max(incomingCap, measuredCap));
  const cap = WASM_VENUE_OVERRIDES.binance;
  assert.ok(cap >= minimumCap);
  for (const name of ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot']) {
    const family = venueFamily(name);
    assert.equal(family, 'binance');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3073 cancel confirmation (run 41931095253) retains headroom and siblings match', () => {
  const measured = 2_334_811;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_345_984);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3094 batch acknowledgements (run 42062090938) retain Bybit headroom', () => {
  const measured = 2_214_254;
  const minimumCap = 2_225_152;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['bybit'] + 1, 'venue', 'bybit'), /exceeds/);
});

test('#3168 guarded Partial edits (run 42227488436) retain measured headroom for every Bybit alias', () => {
  const measured = 2_246_685;
  const minimumCap = 2_257_920;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES.bybit >= cap);
  for (const venue of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader']) {
    const family = venueFamily(venue);
    assert.equal(family, 'bybit');
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3106 critical reads (run 42097859473) retain measured headroom and equal HL siblings', () => {
  const measured = 2_108_435;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_119_680);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3158 Lighter guarded edits (run 42215250423, job 127884302751) retain headroom and siblings match', () => {
  const measured = 2_068_697;
  const minimumCap = 2_079_744;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.lighter;
  assert.ok(cap >= minimumCap);
  for (const venue of ['lighter', 'lighter-robinhood']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(2_034_142, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3118 Aster protection guard (run 42112075305) retains measured headroom', () => {
  const [venue, measured, minimumCap] = ['asterdex', 2_139_837, 2_150_400];
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
});

test('#3114 cold-path sharing (run 42104530272) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_106_596;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_117_632);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3143 public data watch (run 42181736999) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_126_281;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_137_088);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3149 activeAssetData scoping (run 42197163575) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_143_008;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_153_472);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3120 protective recovery (run 42241033166, job 127971867963) retains equal HL headroom', () => {
  const measured = 2_167_039;
  const measuredCap = 2_178_048;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3120 on master 160884d020d (run 42319184274) uses CI plus 10 KiB and keeps HL siblings equal', () => {
  const measured = 2_184_857;
  const minimumCap = 2_195_456;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= minimumCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3150 final CI 42258955401 cap refresh: #3145 trades as last (run 42218930349 job 127896386942) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_166_092;
  const minimumCap = 2_177_024;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= minimumCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3150 final CI 42258955401 cap refresh: #3122 protective stop edits (run 42126229513) retain Paradex and BloFin headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['paradex', 2_252_369, 2_263_040],
    ['blofin', 1_979_025, 1_989_632],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});


test('#3129 guarded GRVT edits (run 42145552491) retain measured headroom', () => {
  const [venue, measured, cap] = ['grvt', 1_806_590, 1_817_600];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const currentCap = WASM_VENUE_OVERRIDES[venue];
  assert.ok(currentCap >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(currentCap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(currentCap + 1, 'venue', venue), /exceeds/);
});

test('#3227 GRVT hosted signing (run 42352873546, job 128345833529) keeps CI plus 10 KiB rounded up to the KiB', () => {
  const measured = 1_944_314;
  const cap = 1_954_816;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.ok(WASM_VENUE_OVERRIDES.grvt >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'grvt'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'grvt'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['grvt'] + 1, 'venue', 'grvt'), /exceeds/);
});

test('#3193 grvt provenance at ad4f1d6d5d9 keeps the larger composed cap', () => {
  const measured = 1_819_801;
  const measuredCap = 1_830_912;
  const incomingCap = 1_817_600;
  const minimumCap = 1_830_912;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(minimumCap, Math.max(incomingCap, measuredCap));
  const cap = WASM_VENUE_OVERRIDES.grvt;
  assert.ok(cap >= minimumCap);
  for (const name of ['grvt']) {
    const family = venueFamily(name);
    assert.equal(family, 'grvt');
    const currentCap = WASM_VENUE_OVERRIDES[family];
    assert.ok(currentCap >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(currentCap, 'venue', family), undefined);
    assert.match(wasmBudgetError(currentCap + 1, 'venue', family), /exceeds/);
  }
});

test('#3170 managed KuCoin protective edits on master (run 42318845632) retain measured family headroom', () => {
  for (const [venue, measured, cap] of [
    ['kucoin', 1_791_027, 1_802_240],
    ['kucoinspot', 2_346_832, 2_357_248],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const currentCap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(currentCap >= cap, `${venue} retains at least the earlier measured headroom`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(currentCap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(currentCap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3193 kucoinspot provenance at ad4f1d6d5d9 keeps the larger composed cap', () => {
  const measured = 2_278_175;
  const measuredCap = 2_288_640;
  const incomingCap = 2_357_248;
  const minimumCap = 2_357_248;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(minimumCap, Math.max(incomingCap, measuredCap));
  const cap = WASM_VENUE_OVERRIDES.kucoinspot;
  assert.ok(cap >= minimumCap);
  for (const name of ['kucoinspot']) {
    const family = venueFamily(name);
    assert.equal(family, 'kucoinspot');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3193 kucoin provenance at ad4f1d6d5d9 keeps the larger composed cap', () => {
  const measured = 1_696_803;
  const measuredCap = 1_708_032;
  const incomingCap = 1_802_240;
  const cap = WASM_VENUE_OVERRIDES.kucoin;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.ok(cap >= Math.max(incomingCap, measuredCap));
  for (const name of ['kucoin']) {
    const family = venueFamily(name);
    assert.equal(family, 'kucoin');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3169 MEXC managed protective edits (run 42228563954) retain measured headroom', () => {
  const [venue, measured, cap] = ['mexc', 1_848_283, 1_858_560];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const currentCap = WASM_VENUE_OVERRIDES[venue];
  assert.ok(currentCap >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(currentCap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(currentCap + 1, 'venue', venue), /exceeds/);
});

test('#3193 mexc provenance at ad4f1d6d5d9 keeps the larger composed cap', () => {
  const measured = 1_862_399;
  const measuredCap = 1_872_896;
  const incomingCap = 1_858_560;
  const minimumCap = 1_872_896;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(minimumCap, Math.max(incomingCap, measuredCap));
  const cap = WASM_VENUE_OVERRIDES.mexc;
  assert.ok(cap >= minimumCap);
  for (const name of ['mexc']) {
    const family = venueFamily(name);
    assert.equal(family, 'mexc');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3128 ApeX guarded amends (run 42146728975) retain ApeX headroom', () => {
  for (const [venue, measured, minimumCap] of [['apexomni', 2_002_359, 2_013_184]]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3130 Backpack guarded trigger amends (run 42154583609) retain Backpack headroom', () => {
  for (const [venue, measured, minimumCap] of [['backpack', 1_926_668, 1_937_408]]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3138 Bitrue guarded stop edits (run 42166643605) retain Bitrue headroom', () => {
  for (const [venue, measured, minimumCap] of [['bitrue', 1_829_700, 1_840_128]]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3199 chaser refusals (run 42280800704) retain OKX headroom and all siblings share the cap', () => {
  const measured = 1_973_874;
  const minimumCap = 1_984_512;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.okx;
  assert.ok(cap >= minimumCap);
  for (const venue of ['okx', 'okxeu', 'okxdemo', 'okxeudemo']) {
    const family = venueFamily(venue);
    assert.equal(family, 'okx');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3152 BingX protective edits (run 42200843544) retain measured headroom and the exact boundary', () => {
  const measured = 2_063_923;
  const minimumCap = 2_074_624;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bingx;
  assert.ok(cap >= minimumCap);
  assert.equal(venueFamily('bingx'), 'bingx');
  assert.ok(WASM_VENUE_OVERRIDES.bingx >= cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['bingx'] + 1, 'venue', 'bingx'), /exceeds/);
});

test('#3190 engine de-monomorphisation (run 42248954523) retains WOO and Paradex headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['woo', 2_399_392, 2_410_496],
    ['paradex', 2_249_770, 2_260_992],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3193 woo CI job 128343606230 retains measured headroom and the exact boundary', () => {
  const measured = 2_417_950;
  const minimumCap = 2_428_928;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.woo;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES.woo >= cap);
  for (const alias of ['woo']) {
    const family = venueFamily(alias);
    assert.equal(family, 'woo');
    assert.ok(WASM_VENUE_OVERRIDES[family] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[family] + 1, 'venue', family), /exceeds/);
  }
});

test('#3178 intent cleanup (run 42239151280, job 127965614325) retains HL headroom and equal siblings', () => {
  const measured = 2_154_756;
  const measuredCap = 2_165_760;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3188 startup causes (run 42260192686 at cda38ed52a9) retain ruled headroom and exact boundaries', () => {
  for (const [venue, measured, incomingCap, measuredCap] of [
    ['asterdex', 2_150_517, 2_150_400, 2_161_664],
    ['binance', 2_208_976, 2_223_104, 2_223_104],
    ['bitunix', 1_853_221, 1_849_344, 1_863_680],
    ['bybit', 2_259_153, 2_257_920, 2_270_208],
    ['cryptocom', 1_730_308, 1_727_488, 1_740_800],
    ['hyperliquid', 2_169_080, 2_165_760, 2_180_096],
    ['kucoinspot', 2_266_287, 2_264_064, 2_277_376],
    ['lighter', 2_081_957, 2_079_744, 2_093_056],
    ['paradex', 2_254_964, 2_260_992, 2_266_112],
    ['phemex', 2_068_959, 2_065_408, 2_079_744],
    ['polymarket', 1_601_181, 1_600_512, 1_611_776],
    ['txflow', 1_842_121, 1_838_080, 1_852_416],
    ['woo', 2_406_393, 2_410_496, 2_416_640],
  ]) {
    assert.equal(measuredCap, Math.max(incomingCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024));
    // A later merge may keep a larger cap for the family (#3170 kucoinspot).
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= measuredCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  for (const [family, sibling] of [['hyperliquid', 'hyperliquidspot'], ['lighter', 'lighter-robinhood']]) {
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(WASM_VENUE_OVERRIDES[sibling] >= cap);
    assert.equal(wasmBudgetError(cap, 'venue', sibling), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[sibling] + 1, 'venue', sibling), /exceeds/);
  }
  assert.equal(wasmBudgetError(16_830_742, 'combined'), undefined);
});

test('#3150 merged-tree caps retain the approved exact boundaries', () => {
  for (const [venue, minimumCap] of [
    ['apexomni', 2_024_448],
    ['bingx', 2_085_888],
    ['bitrue', 1_851_392],
    ['blofin', 2_001_920],
    ['coinbase', 1_852_416],
    ['okx', 1_986_560],
    ['weex', 1_948_672],
  ]) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3219 drb public consumers (run 42352093025) retain CI headroom for Coinbase and Spot', () => {
  const measured = 1_924_268;
  const cap = 1_935_360;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    const current = WASM_VENUE_OVERRIDES[family];
    assert.ok(current >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(current, 'venue', family), undefined);
    assert.match(wasmBudgetError(current + 1, 'venue', family), /exceeds/);
  }
});

test('#3207 telemetry (run 42279265759) uses CI plus 10 KiB and keeps Bitget siblings equal', () => {
  const measured = 2_390_477;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_401_280);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3202 on #3207 (run 42305638406) uses CI plus 10 KiB and keeps Bitget siblings equal', () => {
  const measured = 2_409_448;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_419_712);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3210 admitted close split (run 42320236420) uses CI plus 10 KiB and keeps Bitget siblings equal', () => {
  const measured = 2_422_853;
  const minimumCap = 2_434_048;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= minimumCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3157 protective edit guards (run 42399005414) retain measured Bitget headroom and equal siblings', () => {
  const measured = 2_447_105;
  const minimumCap = 2_457_600;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= minimumCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3191 run 42249705679 retains measured Paradex headroom', () => {
  const measured = 2_249_779;
  const minimumCap = 2_260_992;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.paradex;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'paradex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'paradex'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['paradex'] + 1, 'venue', 'paradex'), /exceeds/);
});


test('#3229 Backpack hosted signing (run 42352281022, job 128343086016) retains CI headroom', () => {
  const measured = 1_994_139;
  const minimumCap = 2_004_992;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const current = WASM_VENUE_OVERRIDES.backpack;
  assert.ok(current >= minimumCap, 'Backpack retains at least CI size plus 10 KiB, rounded up to the KiB');
  assert.equal(wasmBudgetError(measured, 'venue', 'backpack'), undefined);
  assert.equal(wasmBudgetError(current, 'venue', 'backpack'), undefined);
  assert.match(wasmBudgetError(current + 1, 'venue', 'backpack'), /exceeds/);
});

test('#3229 master merge (run 42392889181, job 128464301381) retains rounded CI headroom', () => {
  const measured = 2_000_456;
  const minimumCap = 2_011_136;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const current = WASM_VENUE_OVERRIDES.backpack;
  assert.ok(current >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'backpack'), undefined);
  assert.equal(wasmBudgetError(current, 'venue', 'backpack'), undefined);
  assert.match(wasmBudgetError(current + 1, 'venue', 'backpack'), /exceeds/);
});

test('#3220 KuCoin hosted signing (run 42324881707, job 128258225940) keeps measured headroom', () => {
  const measured = 1_827_119;
  const minimumCap = 1_838_080;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.kucoin;
  assert.ok(cap >= minimumCap);
  assert.ok(WASM_VENUE_OVERRIDES.kucoin >= cap);
  assert.ok(WASM_VENUE_OVERRIDES.kucoinspot >= 2_357_248);
  assert.equal(wasmBudgetError(measured, 'venue', 'kucoin'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'kucoin'), undefined);
  assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES['kucoin'] + 1, 'venue', 'kucoin'), /exceeds/);
});

test('#3111 candle repair readiness (run 42096897919) retains headroom and siblings match', () => {
  const measured = 2_106_939;
  const minimumCap = 2_117_632;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= minimumCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.ok(WASM_VENUE_OVERRIDES[venue] >= cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3221 hosted signing (run 42352558360) rounds web/node CI plus 10 KiB up to KiB', () => {
  const measured = 2_568_219;
  const minimumCap = 2_579_456;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.woo;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(WASM_VENUE_OVERRIDES['woo'] + 1, 'venue', 'woo'), `venue wasm ${cap + 1} exceeds ${cap} bytes`);
});

test('#3221 run 42393607294 artifact 13582294491 retains measured Woo headroom', () => {
  const measured = 2_578_659;
  const minimumCap = 2_589_696;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.woo;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(WASM_VENUE_OVERRIDES['woo'] + 1, 'venue', 'woo'), `venue wasm ${cap + 1} exceeds ${cap} bytes`);
});

test('#3264 Kraken Spot ambiguous-write fence (run 42403521233 at d1f8b3c2e40) retains headroom and siblings match', () => {
  for (const [venue, measured, minimumCap] of [
    ['kraken', 2_136_521, 2_147_328],
    ['krakenspot', 2_136_521, 2_147_328],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3270 (run 42404311752 at 46e5a6918ec) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['coinbase', 1943133, 1953792],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3277 TxFlow public decode (run 42407352871, job 128508214838) retains CI headroom', () => {
  const measured = 1_896_742;
  const minimumCap = 1_907_712;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const current = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(current >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(current, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(current + 1, 'venue', 'txflow'), /exceeds/);
});

test('#3296 (run 42430126386 at f6a7386a89f) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_270_215, 2_281_472],
    ['kucoinspot', 2_395_123, 2_405_376],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3301 merged head (run 42555202714 at 7a2f4816ce4) retains measured wasm headroom', () => {
  const measured = 2_091_319;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.coinbase;
  assert.ok(cap >= minimumCap, `coinbase: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured, 'venue', 'coinbase'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'coinbase'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'coinbase'), /exceeds/);
});

test('#3301 (run worker-reported at 8124fb265ab) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['coinbase', 2_069_925, 2_080_768],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3303 (run 42429733509 at 986f7b8f915) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_270_210, 2_281_472],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3317 (run 42449143312 at c10e09e0df6) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_864_844, 1_875_968],
    ['cryptocom', 1_741_327, 1_752_064],
    ['lighter', 2_093_819, 2_104_320],
    ['paradex', 2_266_717, 2_277_376],
    ['polymarket', 1_612_406, 1_623_040],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3298 (run 42433363173 at a42fcf9deaf) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_270_210, 2_281_472],
    ['cryptocom', 1_768_685, 1_779_712],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3316 (run 42449269046, job 128635008984) retains measured BingX headroom', () => {
  const measured = 2_087_229;
  const minimumCap = 2_098_176;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bingx;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bingx'), /exceeds/);
});

test('#3263 (run 42434407009 at e78c6873b82) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_863_786, 1_874_944],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3263 (run 42451527276 at 1b863010e9c) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['lighter', 2_093_063, 2_104_320],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(WASM_VENUE_OVERRIDES[venue] + 1, 'venue', venue), /exceeds/);
  }
});

test('#3348 (run 42456685425 at 0571f899890) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['binance', 2_236_853, 2_247_680],
    ['bingx', 2_089_856, 2_100_224],
    ['blofin', 2_004_606, 2_015_232],
    ['kucoin', 1_839_459, 1_850_368],
    ['lighter', 2_098_243, 2_109_440],
    ['okx', 1_989_276, 1_999_872],
    ['weex', 1_952_089, 1_963_008],
    ['woo', 2_600_295, 2_611_200],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3348 (run 42459036702 at 2c9d859698b) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['apexomni', 2_027_105, 2_037_760],
    ['asterdex', 2_164_612, 2_174_976],
    ['bitrue', 1_853_598, 1_864_704],
    ['grvt', 1_956_582, 1_967_104],
    ['hyperliquid', 2_198_659, 2_209_792],
    ['kraken', 2_149_207, 2_159_616],
    ['kucoinspot', 2_412_630, 2_423_808],
    ['paradex', 2_271_265, 2_282_496],
    ['polymarket', 1_616_976, 1_628_160],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3345 (run 42456627405 at ca5c784d486) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['apexomni', 2_024_940, 2_035_712],
    ['asterdex', 2_162_720, 2_173_952],
    ['bingx', 2_087_973, 2_099_200],
    ['bitrue', 1_851_860, 1_862_656],
    ['bitunix', 1_867_317, 1_878_016],
    ['blofin', 2_003_028, 2_014_208],
    ['hyperliquid', 2_197_214, 2_207_744],
    ['kucoinspot', 2_409_913, 2_420_736],
    ['lighter', 2_096_714, 2_107_392],
    ['okx', 2_003_000, 2_014_208],
    ['paradex', 2_269_444, 2_280_448],
    ['polymarket', 1_615_226, 1_626_112],
    ['weex', 1_950_564, 1_960_960],
    ['woo', 2_597_896, 2_609_152],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3395 restage (run 42561995598 at merge c4b68c461e1f) retains current CI headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['asterdex', 2_187_368, 2_198_528],
    ['bitunix', 1_916_290, 1_927_168],
    ['bybit', 2_316_042, 2_326_528],
    ['coinbase', 1_967_433, 1_978_368],
    ['cryptocom', 1_792_792, 1_803_264],
    ['mexc', 1_887_614, 1_898_496],
    ['orderly', 1_758_659, 1_769_472],
    ['phemex', 2_104_206, 2_114_560],
    ['woo', 2_631_851, 2_642_944],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3306 (run 42481130823 at merge 680966203b3) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['apexomni', 2_045_121, 2_056_192],
    ['asterdex', 2_182_426, 2_193_408],
    ['backpack', 2_025_534, 2_036_736],
    ['binance', 2_263_342, 2_274_304],
    ['bingx', 2_116_644, 2_127_872],
    ['bitget', 2_481_473, 2_492_416],
    ['bitgetusdc', 2_481_473, 2_492_416],
    ['bitrue', 1_875_844, 1_886_208],
    ['bitunix', 1_894_388, 1_904_640],
    ['blofin', 2_038_156, 2_049_024],
    ['bybit', 2_309_712, 2_320_384],
    ['cryptocom', 1_792_567, 1_803_264],
    ['grvt', 1_975_052, 1_985_536],
    ['hyperliquid', 2_220_716, 2_231_296],
    ['hyperliquidspot', 2_220_716, 2_231_296],
    ['kraken', 2_162_286, 2_172_928],
    ['krakenspot', 2_162_286, 2_172_928],
    ['kucoin', 1_861_899, 1_872_896],
    ['kucoinspot', 2_429_328, 2_440_192],
    ['lighter', 2_113_576, 2_124_800],
    ['lighter-robinhood', 2_113_576, 2_124_800],
    ['mexc', 1_888_055, 1_898_496],
    ['okx', 2_023_450, 2_034_688],
    ['orderly', 1_760_283, 1_771_520],
    ['paradex', 2_290_302, 2_300_928],
    ['polymarket', 1_641_496, 1_652_736],
    ['txflow', 1_923_737, 1_934_336],
    ['weex', 1_970_870, 1_981_440],
    ['woo', 2_618_127, 2_628_608],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3306 repair (run 42484190726 at merge ac99b43383f) retains Coinbase and Phemex headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['coinbase', 1_961_227, 1_972_224],
    ['phemex', 2_100_869, 2_111_488],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

for (const [venue, measured, minimumCap] of [
  ['bitunix', 1_905_582, 1_915_904],
  ['okx', 2_035_284, 2_045_952],
]) {
  test(`#3306 CI3 ${venue} (run 42490856791 at merge 331730698740) retains measured headroom`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

test('#3383 (run 42493810742 at 1e85d49cc13) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['backpack', 2_011_875, 2_022_400],
    ['bitget', 2_459_146, 2_469_888],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3317 current CI (run 42622329629, job 129178042670, artifact 13676298877 at 8e55e01469b) retains WOO headroom', () => {
  const measured = 2_636_672;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_647_040);
  const cap = WASM_VENUE_OVERRIDES.woo;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(cap + 1, 'venue', 'woo'), `venue wasm ${cap + 1} exceeds ${cap} bytes`);
});

test('#3344 (run 42479985723 at 51005ee6d89) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['hyperliquid', 2_210_524, 2_221_056],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3373 (run 42483155921 at f62df98ad32) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_877_328, 1_888_256],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3297 (run 42428983328 at 934311ce7af) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitrue', 1_876_650, 1_887_232],
    ['bybit', 2_270_329, 2_281_472],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3297 (run 42449468684 at 93ba489bb3c) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_863_777, 1_874_944],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3297 (run 42457496953 at 4c117b1f686) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['polymarket', 1_611_838, 1_623_040],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3297 post-#3306 (run 42556682443 at e5ae4d9a8db) retains current measured Bitrue headroom', () => {
  // WASM family job 128964311836 and replay job 128964312205 measured 1,895,658 B.
  const measured = 1_895_658;
  const minimumCap = 1_906_688;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bitrue;
  assert.ok(cap >= minimumCap, 'Bitrue must retain current measured size plus 10 KiB, rounded up');
  assert.equal(wasmBudgetError(measured, 'venue', 'bitrue'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bitrue'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bitrue'), /exceeds/);
});

test('#3344 restage (optimized head 2cc05051503) retains measured headroom for both aliases', () => {
  const measured = 2_225_418;
  const minimumCap = 2_236_416;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3344 current CI (run 42565604601 at merge ec8633c44e98) retains both alias floors', () => {
  const measured = 2_225_586;
  const minimumCap = 2_236_416;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3281 (run 42422693183 at 5025f1670c7) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_863_889, 1_874_944],
    ['bybit', 2_270_581, 2_281_472],
    ['lighter', 2_093_087, 2_104_320],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3377 master merge a4b99b8ded4 retains measured Phemex headroom', () => {
  const measured = 2_105_874;
  const minimumCap = 2_116_608;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.phemex;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'phemex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'phemex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'phemex'), /exceeds/);
});

test('#3362 (run 42485221909 at c224f7a9c4b) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['weex', 1_967_049, 1_977_344],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3362 fresh merge f10ca13c112 + master 05241cd0f41 retains measured wasm headroom', () => {
  const measured = 1_980_557;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.weex;
  assert.equal(minimumCap, 1_991_680);
  assert.ok(cap >= minimumCap, `weex: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured, 'venue', 'weex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'weex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'weex'), /exceeds/);
});

test('#3280 (run 42422666520 at 2cfa45a3b1f) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_270_273, 2_281_472],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3280 merged-head CI (run 42559068674 at 5492575dead5) retains measured wasm headroom', () => {
  const measured = 2_313_532;
  const minimumCap = 2_324_480;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

test('#3299 (run 42432952035 at 9e6863a2b86) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_270_312, 2_281_472],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3299 (run 42437005422 at 94211a4edff) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['mexc', 1_893_996, 1_904_640],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3299 restage (run 42558057739 at 5dec79811311) retains current measured wasm headroom', () => {
  const measured = 1_914_033;
  const minimumCap = 1_925_120;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.mexc;
  assert.ok(cap >= minimumCap, `mexc: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured, 'venue', 'mexc'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'mexc'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'mexc'), /exceeds/);
});

test('#3299 merged head 2ca7125e8ab retains measured MEXC and Bitrue headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['mexc', 1_920_015, 1_931_264],
    ['bitrue', 1_901_720, 1_912_832],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3351 (run 42479504663 at 0ff5614cf2f) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['backpack', 2_014_116, 2_024_448],
    ['bitget', 2_462_609, 2_472_960],
    ['bitunix', 1_876_636, 1_887_232],
    ['bybit', 2_283_982, 2_294_784],
    ['coinbase', 1_956_448, 1_967_104],
    ['cryptocom', 1_782_301, 1_793_024],
    ['kucoinspot', 2_433_271, 2_444_288],
    ['mexc', 1_878_087, 1_889_280],
    ['orderly', 1_749_651, 1_760_256],
    ['phemex', 2_093_612, 2_104_320],
    ['txflow', 1_910_376, 1_921_024],
    ['woo', 2_621_408, 2_631_680],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3351 (run 42483561301 at 68e9acc3e5d) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['asterdex', 2_177_526, 2_188_288],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3351 fresh merge d3f3f8ea2c4 + master 05241cd0f41 retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['apexomni', 2_047_375, 2_058_240],
    ['asterdex', 2_190_560, 2_201_600],
    ['backpack', 2_029_946, 2_040_832],
    ['binance', 2_257_960, 2_269_184],
    ['bingx', 2_117_036, 2_127_872],
    ['bitget', 2_480_076, 2_490_368],
    ['bitgetusdc', 2_480_076, 2_490_368],
    ['bitrue', 1_873_699, 1_884_160],
    ['bitunix', 1_919_740, 1_930_240],
    ['blofin', 2_044_697, 2_055_168],
    ['bybit', 2_320_022, 2_330_624],
    ['coinbase', 1_970_954, 1_981_440],
    ['cryptocom', 1_796_893, 1_807_360],
    ['grvt', 1_981_398, 1_991_680],
    ['hyperliquid', 2_221_358, 2_232_320],
    ['hyperliquidspot', 2_221_358, 2_232_320],
    ['kraken', 2_169_192, 2_180_096],
    ['krakenspot', 2_169_192, 2_180_096],
    ['kucoin', 1_860_274, 1_870_848],
    ['kucoinspot', 2_452_039, 2_462_720],
    ['lighter', 2_125_882, 2_137_088],
    ['lighter-robinhood', 2_125_882, 2_137_088],
    ['mexc', 1_891_672, 1_902_592],
    ['okx', 2_042_123, 2_053_120],
    ['orderly', 1_762_742, 1_773_568],
    ['paradex', 2_294_926, 2_306_048],
    ['phemex', 2_108_005, 2_118_656],
    ['polymarket', 1_640_307, 1_650_688],
    ['txflow', 1_921_952, 1_932_288],
    ['weex', 1_973_943, 1_984_512],
    ['woo', 2_645_372, 2_656_256],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3343 (run 42483662882 at 25b0c17bb30) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['blofin', 2_025_074, 2_035_712],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3343 restage at 46491209e7f (local build.sh --venue blofin) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['blofin', 2_056_571, 2_067_456],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3343 signature fix at e546df5771e4 retains current optimized Blofin headroom', () => {
  // SAFE_CEX_WASM_OPT=required build.sh --venue blofin;
  // Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132: web/node 2,062,295 B.
  const measured = 2_062_295;
  const minimumCap = 2_072_576;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.blofin;
  assert.ok(cap >= minimumCap, `blofin: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'blofin'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'blofin'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'blofin'), /exceeds/);
});

test('#3300 restage (run 42557412385 at CI merge 291f65994cda) retains measured ApeX Omni headroom', () => {
  const measured = 2_078_907;
  const minimumCap = 2_089_984;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.apexomni;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'apexomni'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'apexomni'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'apexomni'), /exceeds/);
});

test('#3300 pinned master merge c4d663622c01 retains measured ApeX Omni headroom', () => {
  const measured = 2_092_748;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.apexomni;
  assert.equal(minimumCap, 2_103_296);
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'apexomni'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'apexomni'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'apexomni'), /exceeds/);
});

for (const venue of ['bitget', 'bitgetusdc']) {
  test(`#3426 ${venue} current CI artifact retains measured family reserve`, () => {
    // Rust SDK run 43187677592, job 131079474589, artifact 13930235036.
    // CI source b3c4cf6212e6: verified equal web/Node modules of 2,487,523 bytes.
    const measured = 2_487_523;
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(minimumCap, 2_498_560);
    assert.equal(venueFamily(venue), 'bitget');
    assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
  });
}


test('nullable history output retains current measured family headroom', () => {
  // Pinned local build.sh --venue FAMILY: Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_329_857, 2_340_864],
    ['okx', 2_052_533, 2_063_360],
    ['blofin', 2_070_435, 2_080_768],
    ['bitunix', 1_927_088, 1_937_408],

  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3438 nullable history retains measured shared-family headroom', () => {
  for (const [venue, measured] of [
    ['hyperliquid', 2_236_724],
    ['hyperliquidspot', 2_236_724],
    ['weex', 1_991_983],
  ]) {
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3438 merged-head measurements cover every wasm family', () => {
  // Optimized web/node builds: Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  const measurements = [
    ['apexomni', 2_092_636],
    ['asterdex', 2_197_178],
    ['backpack', 2_036_671],
    ['binance', 2_270_372],
    ['bingx', 2_130_988],
    ['bitget', 2_486_531],
    ['bitrue', 1_908_695],
    ['bitunix', 1_926_520],
    ['blofin', 2_069_819],
    ['bybit', 2_329_362],
    ['coinbase', 2_103_909],
    ['cryptocom', 1_803_468],
    ['grvt', 1_988_047],
    ['hyperliquid', 2_238_027],
    ['kraken', 2_247_702],
    ['kucoin', 1_866_880],
    ['kucoinspot', 2_461_646],
    ['lighter', 2_133_346],
    ['mexc', 1_927_089],
    ['okx', 2_051_925],
    ['orderly', 1_769_717],
    ['paradex', 2_301_551],
    ['phemex', 2_118_601],
    ['polymarket', 1_647_100],
    ['txflow', 1_928_662],
    ['weex', 1_993_495],
    ['woo', 2_655_191],
  ];
  assert.deepEqual(measurements.map(([family]) => family).sort(), [...venueFamilies].sort());
  const aliases = { bitget: ['bitgetusdc'], hyperliquid: ['hyperliquidspot'], kraken: ['krakenspot'], lighter: ['lighter-robinhood'] };
  const covered = measurements.flatMap(([family]) => [family, ...(aliases[family] ?? [])]);
  assert.deepEqual(covered.sort(), Object.keys(WASM_VENUE_OVERRIDES).sort());
  for (const [family, measured] of measurements) {
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    for (const venue of [family, ...(aliases[family] ?? [])]) {
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  }
});

test('#3429 Phemex current-head size has measured plus 10 KiB headroom', () => {
  // CI run 42729630872, head a2be15368b4: web/node measured 2,125,299 bytes.
  // Same pinned build after the shared-helper fix: web/node 2,127,824 bytes.
  const measured = Math.max(2_125_299, 2_127_824);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_138_112);
  const cap = WASM_VENUE_OVERRIDES.phemex;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'phemex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'phemex'), /exceeds/);
});

test('#3429 master restage retains current measured Hyperliquid and Weex headroom', () => {
  // Local merged head 1bb3f1352988; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
  for (const [venue, measured] of [
    ['hyperliquid', 2_238_274], ['hyperliquidspot', 2_238_274], ['weex', 1_993_725],
  ]) {
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
});


test('#3342 (run 42480192430 at 188604decf7) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_299_152, 2_310_144],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    // A later integration can retain a larger measured parent floor.
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < historical floor ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3342 master merge e817ea1fa46 retains measured Bybit headroom', () => {
  const measured = 2_336_508;
  const minimumCap = 2_347_008;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

test('#3342 merge 2b623dc76a03 retains current optimized Bybit headroom', () => {
  const measured = 2_351_685;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, `bybit: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

test('#3342 proof-diagnostic fix retains fresh merged-head Bybit headroom', () => {
  // Local 091d0b980aef build.sh --venue bybit; Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz.
  // Web/Node SHA256 170c1e44400fd4b682818cdd37b01d7a322d11d6c3b9677f53b638b3165a1167.
  const measured = Math.max(2_354_730, 2_354_730);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_365_440);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, `bybit: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

test('#3448 changed AsterDEX artifact retains measured plus 10 KiB reserve', () => {
  // Merged-head CI run 43061512265, artifact 13873588589: optimized web/node 2,197,821 bytes.
  const measured = 2_197_821;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap >= minimumCap, `asterdex: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});

test('#3448 master merge 9a736b2a6bf1 retains fresh AsterDEX artifact reserve', () => {
  // CI run 43198030267, artifact 13934600864; source tree matches head 9a736b2a6bf1.
  // Optimized web and Node modules both measure 2,211,230 bytes.
  const measured = Math.max(2_211_230, 2_211_230);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_222_080);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap >= minimumCap, `asterdex: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});

// #3425 run 43176700451; CI source cca7b8337ad3 has the dbff6fc75827 merge tree.
// Web and Node bytes come from the same original-attempt job per family.
for (const [family, aliases, web, node, minimumCap] of [
  // Replay job 131043125663.
  ['asterdex', ['asterdex'], 2_208_996, 2_208_996, 2_220_032],
  // Replay job 131043125555.
  ['bitunix', ['bitunix'], 1_942_038, 1_942_038, 1_952_768],
  // Replay job 131043125392.
  ['bybit', ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'], 2_330_713, 2_330_713, 2_341_888],
  // Replay job 131043125504.
  ['kraken', ['kraken', 'krakenspot'], 2_260_418, 2_260_418, 2_271_232],
  // Replay job 131043125518.
  ['lighter', ['lighter', 'lighter-robinhood'], 2_146_773, 2_146_773, 2_157_568],
  // Replay job 131043125322.
  ['mexc', ['mexc'], 1_946_379, 1_946_379, 1_956_864],
  // Replay job 131043125397.
  ['weex', ['weex'], 2_010_529, 2_010_529, 2_021_376],
]) {
  test(`#3425 ${family} current CI web/Node retains measured headroom`, () => {
    const measured = Math.max(web, node);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${family}: ${cap} < ${minimumCap}`);
    for (const alias of aliases) {
      assert.equal(venueFamily(alias), family);
      const admittedCap = WASM_VENUE_OVERRIDES[alias] ?? cap;
      assert.equal(admittedCap, cap, alias);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
    }
  });
}


test('#3372 reduced CI run 43341438240 retains current KuCoin Spot reserve', () => {
  // Previous run 43303561061 measured 2,463,988 B and required the superseded 2,475,008 cap.
  // Head 5429d69bb2c3, CI merge fc362e3e5ebd, job 131611062986, artifact 13999827413.
  const measured = 2_462_504;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_472_960);
  const cap = WASM_VENUE_OVERRIDES.kucoinspot;
  assert.ok(cap >= minimumCap, 'KuCoin Spot retains the current CI measurement plus 10 KiB');
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'kucoinspot'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'kucoinspot'), /exceeds/);
});

test('#3372 CI run 43328678379 retains current Bybit reserve', () => {
  // Head 48cb0d34c168, CI merge cff6adaa9fc1, job 131568212209, artifact 13995125309.
  const measured = 2_338_561;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_349_056);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, 'Bybit retains the current CI measurement plus 10 KiB');
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});
test('#3472 merged-head Bybit retains fresh measured plus 10 KiB reserve', () => {
  // Source f6a275af12092d378f8812f5260b4a3a07b7b0ee, build.sh --venue bybit.
  // Rust1.99, bindgen0.2.129, Binaryen132 twice-Oz: web/node each 2,332,186 B.
  // SHA256 2f99b9639875bdb29dfde985745ca9f91b1e3903dcbd3b9052e2a788548d6db2.
  const measured = Math.max(2_332_186, 2_332_186);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_342_912);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, `bybit: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

// #3438 run 43204071123; source 0340fd57a506 has head 7464a40315bc's exact tree.
// Original-attempt artifacts: verified ZIP digests, manifest hashes and equal web/Node bytes.
for (const [family, artifact, measured] of [
  ['bitunix', 13937985101, 1_942_715],
  ['grvt', 13936313617, 1_990_569],
  ['kucoinspot', 13936822120, 2_462_504],
  ['lighter', 13936986915, 2_147_387],
  ['mexc', 13937850519, 1_947_010],
  ['polymarket', 13937575821, 1_651_961],
  ['woo', 13937331538, 2_655_496],
]) {
  test(`#3438 ${family} artifact ${artifact} retains integrated-build headroom`, () => {
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${family}: ${cap} < ${minimumCap}`);
    const aliases = family === 'lighter' ? [family, 'lighter-robinhood'] : [family];
    for (const venue of aliases) {
      assert.equal(venueFamily(venue), family);
      assert.equal(WASM_VENUE_OVERRIDES[venue], cap, venue);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

test('#3391 (run 42553793906 at 20f13e05df1) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_320_705, 2_331_648],
    ['lighter', 2_126_674, 2_137_088],
    ['woo', 2_634_532, 2_644_992],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3391 (run 42561525825 at ec49cdd8159) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bitunix', 1_920_841, 1_931_264],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3391 run 43040714236 at 86083b569b50 retains exact-head measured wasm headroom', () => {
  for (const [family, measured, job] of [
    ['asterdex', 2_202_568, '130592610683'],
    ['backpack', 2_042_111, '130592610542'],
    ['binance', 2_275_818, '130592610355'],
    ['bitrue', 1_914_290, '130592610293'],
    ['bitunix', 1_931_905, '130592610314'],
    ['blofin', 2_074_912, '130592610589'],
    ['bybit', 2_334_515, '130592610614'],
    ['coinbase', 2_109_666, '130592610708'],
    ['cryptocom', 1_809_034, '130592610731'],
    ['grvt', 1_993_609, '130592610826'],
    ['kucoinspot', 2_469_164, '130592610751'],
    ['lighter', 2_138_905, '130592610584'],
    ['mexc', 1_932_648, '130592610566'],
    ['okx', 2_057_769, '130592610711'],
    ['orderly', 1_775_270, '130592610851'],
    ['paradex', 2_307_308, '130592610816'],
  ]) {
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const siblings = Object.keys(WASM_VENUE_OVERRIDES).filter(venue => venueFamily(venue) === family);
    assert.ok(siblings.length > 0, family);
    for (const venue of siblings) {
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= required, `${venue}: CI job ${job} measured ${measured}; needs ${required}`);
      assert.equal(cap, WASM_VENUE_OVERRIDES[family], venue);
      assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  }
});



// #3391 run 43204096369: source d62f6d989c3a35cacf7b20375a4b5edc334e3323,
// tree 23178e76bbafb6f557c77ffd0431aee4cdeea9e2 (also 3de5a492d792).
// Rust 1.99.0 / wasm-bindgen 0.2.129 / Binaryen 132, required -Oz.
// Polymarket web comes from the failing family job; Node from ci-wasm-polymarket.
for (const [family, web, node, minimumCap] of [
  // Family job 131135453880; artifact 13936582318.
  ['apexomni', 2_089_661, 2_089_661, 2_100_224],
  // Family job 131135453773; artifact 13936588049.
  ['asterdex', 2_216_296, 2_216_296, 2_227_200],
  // Family job 131135453549; artifact 13936982114.
  ['backpack', 2_036_366, 2_036_366, 2_046_976],
  // Family job 131135453620; artifact 13937780468.
  ['binance', 2_267_072, 2_267_072, 2_277_376],
  // Family job 131135453731; artifact 13936523131.
  ['bingx', 2_126_140, 2_126_140, 2_137_088],
  // Family job 131135453683; artifact 13935794958.
  ['bitget', 2_477_459, 2_477_459, 2_488_320],
  // Family job 131135453687; artifact 13937850329.
  ['bitrue', 1_903_365, 1_903_365, 1_913_856],
  // Family job 131135453920; artifact 13936642823.
  ['bitunix', 1_949_515, 1_949_515, 1_959_936],
  // Family job 131135453711; artifact 13935909796.
  ['blofin', 2_060_408, 2_060_408, 2_071_552],
  // Family job 131135453903; artifact 13936642978.
  ['bybit', 2_338_200, 2_338_200, 2_349_056],
  // Family job 131135453743; artifact 13937425727.
  ['coinbase', 2_109_592, 2_109_592, 2_120_704],
  // Family job 131135453931; artifact 13937106766.
  ['cryptocom', 1_810_902, 1_810_902, 1_821_696],
  // Family job 131135453957; artifact 13937815232.
  ['grvt', 1_997_577, 1_997_577, 2_008_064],
  // Family job 131135453858; artifact 13937311047.
  ['hyperliquid', 2_231_852, 2_231_852, 2_242_560],
  // Family job 131135453763; artifact 13936588037.
  ['kraken', 2_267_706, 2_267_706, 2_278_400],
  // Family job 131135453832; artifact 13936248817.
  ['kucoin', 1_860_316, 1_860_316, 1_870_848],
  // Family job 131135454013; artifact 13936787868.
  ['kucoinspot', 2_471_018, 2_471_018, 2_482_176],
  // Family job 131135454148; artifact 13936772862.
  ['lighter', 2_154_475, 2_154_475, 2_164_736],
  // Family job 131135454171; artifact 13937321670.
  ['mexc', 1_953_995, 1_953_995, 1_965_056],
  // Family job 131135454062; artifact 13936912504.
  ['okx', 2_045_606, 2_045_606, 2_056_192],
  // Family job 131135454025; artifact 13937476728.
  ['orderly', 1_777_164, 1_777_164, 1_787_904],
  // Family job 131135454051; artifact 13936218205.
  ['paradex', 2_304_689, 2_304_689, 2_315_264],
  // Family job 131135454267; artifact 13937386078.
  ['phemex', 2_132_324, 2_132_324, 2_143_232],
  // Family job 131135454125; artifact 13936288104.
  ['polymarket', 1_658_851, 1_658_851, 1_669_120],
  // Family job 131135454016; artifact 13937271386.
  ['txflow', 1_922_889, 1_922_889, 1_933_312],
  // Family job 131135454191; artifact 13937520671.
  ['weex', 2_018_048, 2_018_048, 2_028_544],
  // Family job 131135454115; artifact 13938010042.
  ['woo', 2_664_064, 2_664_064, 2_674_688],
]) {
  test(`#3391 integrated ${family} retains current measured headroom`, () => {
    const measured = Math.max(web, node);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${family}: ${cap} < ${minimumCap}`);
    const aliases = Object.keys(WASM_VENUE_OVERRIDES).filter(venue => venueFamily(venue) === family);
    for (const venue of aliases) {
      assert.equal(WASM_VENUE_OVERRIDES[venue], cap, venue);
      assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}


test('#3391 merged artifact run 43254034225 retains AsterDEX measured reserve', () => {
  // Family job 131308189778, artifact 13960543088; web and Node both 2,218,535 B.
  const measured = 2_218_535;
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


test('#3391 round 2 current CI merge retains AsterDEX measured reserve', () => {
  // Run 43263509956/job 131345282843, artifact 13966760078; source 0fd07e1c89f8.
  // CI merges c7c32191002a with master 0c1b6dc17cd9; optimized web and Node match.
  const measured = Math.max(2_219_023, 2_219_023);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_230_272);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});

// Run 43328298773: CI source 46852c576bd1 has ca7e7ea34bf6's exact tree.
// The original-attempt optimized web modules independently establish these reserve floors.
for (const [family, artifact, measured] of [
  ['bitunix', 13994321229, 1_950_059],
  ['grvt', 13993179391, 1_998_120],
  ['kraken', 13992739463, 2_268_250],
  ['lighter', 13994720808, 2_155_019],
  ['orderly', 13993673114, 1_777_708],
  ['polymarket', 13994907058, 1_659_395],
  ['weex', 13993473438, 2_018_592],
]) {
  test(`#3391 reviewed ${family} artifact ${artifact} retains measured reserve`, () => {
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const aliases = Object.keys(WASM_VENUE_OVERRIDES).filter(venue => venueFamily(venue) === family);
    assert.ok(aliases.length > 0, family);
    for (const venue of aliases) {
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
      assert.equal(cap, WASM_VENUE_OVERRIDES[family], venue);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

// #3391 fresh integrated local build at 133b901dc12b; master b99d9867a1d8.
// build.sh --venue FAMILY, Rust 1.99.0 / bindgen 0.2.129 / Binaryen 132, twice -Oz.
for (const [family, measured, minimumCap] of [
  ['apexomni', 2_092_245, 2_103_296],
  ['asterdex', 2_221_326, 2_232_320],
  ['backpack', 2_039_182, 2_050_048],
  ['binance', 2_269_830, 2_280_448],
  ['bingx', 2_129_010, 2_140_160],
  ['bitget', 2_479_808, 2_490_368],
  ['bitrue', 1_906_065, 1_916_928],
  ['bitunix', 1_952_459, 1_963_008],
  ['blofin', 2_063_570, 2_074_624],
  ['bybit', 2_341_308, 2_352_128],
  ['coinbase', 2_112_360, 2_122_752],
  ['cryptocom', 1_813_558, 1_824_768],
  ['grvt', 2_000_353, 2_011_136],
  ['hyperliquid', 2_234_627, 2_245_632],
  ['kraken', 2_270_539, 2_281_472],
  ['kucoin', 1_862_963, 1_873_920],
  ['kucoinspot', 2_471_856, 2_482_176],
  ['lighter', 2_158_209, 2_168_832],
  ['mexc', 1_956_731, 1_967_104],
  ['okx', 2_048_578, 2_059_264],
  ['orderly', 1_779_876, 1_790_976],
  ['paradex', 2_307_385, 2_318_336],
  ['phemex', 2_135_165, 2_146_304],
  ['polymarket', 1_661_564, 1_672_192],
  ['txflow', 1_925_609, 1_936_384],
  ['weex', 2_021_024, 2_031_616],
  ['woo', 2_665_054, 2_675_712],
]) {
  test(`#3391 fresh integrated ${family} retains measured reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const aliases = Object.keys(WASM_VENUE_OVERRIDES).filter(venue => venueFamily(venue) === family);
    assert.ok(aliases.length > 0, family);
    for (const venue of aliases) {
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
      assert.equal(cap, WASM_VENUE_OVERRIDES[family], venue);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

test('#3372 merged CI run 43381322258 retains current Bybit reserve', () => {
  // Head 41026bb9ab40 and CI source 15801feeeb97 have the same compiled-source tree.
  // Artifact 14016926715: all web/Node/replay copies measure 2,338,820 B.
  const measured = 2_338_820;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_350_080);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, 'Bybit retains the current merged CI size plus 10 KiB, rounded up');
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});


test('#3438 master merge retains newly measured conflict-family headroom', () => {
  // Merge cf112c51b87c + 3f94ec11a27f; pinned build.sh web/node sizes.
  for (const [venue, measured, minimumCap, resolvedCap = minimumCap] of [
    ['phemex', 2_128_251, 2_139_136],
    ['hyperliquid', 2_238_644, 2_249_728],
    ['hyperliquidspot', 2_238_644, 2_249_728],
    ['weex', 1_994_023, 2_004_992, 2_021_376],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    // A later integration can retain a larger measured parent floor.
    assert.ok(cap >= resolvedCap, `${venue}: ${cap} < historical floor ${resolvedCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

// #3391 head a545cbe2fca0; run 43439489621, CI source 4f137fb49815.
// CI source and reviewed head share tree 1bf68bddecd71587299f2eccaeb255030753fac1.
// Original optimized web/Node/replay measurements; Rust1.99, bindgen0.2.129, Binaryen132.
for (const [family, artifact, measured, minimumCap] of [
  ['woo', 14045298425, 2_666_111, 2_676_736],
  ['kucoinspot', 14044993539, 2_472_600, 2_483_200],
]) {
  test(`#3391 reviewed ${family} artifact ${artifact} keeps current 10 KiB reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${family}: ${cap} < current measured floor ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}
test('#3342 Hyro Unknown presentation retains fresh merged-head Bybit reserve', () => {
  // Source e1e8114b948c55955c6499f8b26feda7df9fbee1; build.sh --venue bybit, pinned tools, twice -Oz.
  // Web/Node SHA256 b63cd566f882b1310ac05dcccd3d09dd48e075e5862aa6bb46028b294905eec2.
  const measured = 2_355_973;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_366_464);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, `bybit: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});


test('#3372 (run 42484598496 at 44f62e31028) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['bybit', 2_285_327, 2_295_808],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});


test('#3372 restage (optimized merge 41d4292d6f3) retains current Bybit headroom', () => {
  const measured = 2_323_991;
  const minimumCap = 2_334_720;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, 'Bybit retains the current measured size plus 10 KiB, rounded up');
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});


test('#3372 current CI run 43052995465 retains measured Bybit headroom', () => {
  // Head 5858609a705, CI merge cc0145a0f0ba (base d956d25f8b48), artifact 13869852410.
  // Optimized web/node/replay each measured 2,331,744 B; earlier provenance pins stay intact.
  const measured = 2_331_744;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_342_912);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, 'Bybit retains the current CI size plus 10 KiB, rounded up');
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});


// #3391 fresh composition df6f095bd740 (master 353a3ab925b0).
// build.sh --venue FAMILY; Rust1.99 / bindgen0.2.129 / Binaryen132, required twice -Oz.
for (const [family, measured, minimumCap, moduleSha256] of [
  ['woo', 2_667_111, 2_677_760, 'fa221b247c4c9d795922cf2a195d327c5bfc5b10ff5eac17f604268fb54ab1d8'],
  ['kucoinspot', 2_473_416, 2_484_224, 'ce57c8df54582f153ec305930df9fa87fcf763ee80bc9552a27059926cebd28f'],

]) {
  test(`#3391 fresh merged ${family} retains measured 10 KiB reserve`, () => {
    assert.match(moduleSha256, /^[0-9a-f]{64}$/);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= minimumCap, `${family}: ${cap} < fresh merged minimum ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

// #3391 new-head CI run 43454205848, source 81ae648f2f29ec50fc44519308d91a72132d8e37.
// Source tree equals e5b6f01259b9; artifact 14051924992 archive/module digests independently verified.
// All five optimized web/Node/replay copies: SHA256 207116edb828e944a3ef5bc12e9e23bdccd4d3cdcbba1795c9b30b1d43a9d21a.
test('#3391 fresh CI composition retains Bybit measured 10 KiB reserve', () => {
  const measured = 2_346_773;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_357_248);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, `bybit: ${cap} < current CI minimum ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});


test('#3342 latest master composition retains fresh CI Bybit reserve', () => {
  // Run 43452683819/job 131979457461; wasm-family-bybit, CI source 0d075971b309.
  // Source tree equals e0b3a4b9f1f9af2ff541d4a564b28adf2da4bf06; all five WASM copies agree.
  // SHA256 99f3fea9eac4f64b632943b980f506558dacd2a9b769a3717d27ca29b50848cd.
  const measured = 2_360_107;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_370_560);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap, `bybit: ${cap} < ${minimumCap}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

// #3391 current-head CI run 43467762505, source c0867d1af0eeccdc4717d57a07901a941d218cd8.
// Source tree equals 1967f27d62ae541cbd8964e698205b1fea8acc71 (5328ccccabc22cee2f66ac6d84ae4995c839ec69).
// All 27 artifact ZIP digests, manifests and 135 optimized web/Node/replay copies verified.
// Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; required -Oz, no RUSTFLAGS override.
// Minimums are fresh measurements + 10 KiB, rounded up; larger historical floors remain.
for (const [family, artifact, measured, minimumCap] of [
  ['apexomni', 14060776179, 2_093_044, 2_103_296],
  ['asterdex', 14060047456, 2_222_212, 2_233_344],
  ['backpack', 14059882145, 2_041_072, 2_052_096],
  ['binance', 14060551817, 2_270_604, 2_281_472],
  ['bingx', 14060212106, 2_130_222, 2_141_184],
  ['bitget', 14061755419, 2_483_140, 2_493_440],
  ['bitrue', 14061700421, 1_906_792, 1_917_952],
  ['bitunix', 14061430806, 1_953_128, 1_964_032],
  ['blofin', 14059484696, 2_064_208, 2_074_624],
  ['bybit', 14060117398, 2_369_936, 2_380_800],
  ['coinbase', 14059882137, 2_112_897, 2_123_776],
  ['cryptocom', 14059976968, 1_814_333, 1_824_768],
  ['grvt', 14061770193, 2_000_978, 2_012_160],
  ['hyperliquid', 14061765222, 2_235_561, 2_246_656],
  ['kraken', 14059659521, 2_272_619, 2_283_520],
  ['kucoin', 14061140935, 1_863_423, 1_873_920],
  ['kucoinspot', 14061155780, 2_475_797, 2_486_272],
  ['lighter', 14060640962, 2_158_718, 2_169_856],
  ['mexc', 14059549866, 1_957_413, 1_968_128],
  ['okx', 14061255578, 2_049_215, 2_060_288],
  ['orderly', 14061375404, 1_780_766, 1_792_000],
  ['paradex', 14060841092, 2_308_139, 2_319_360],
  ['phemex', 14059729555, 2_135_702, 2_146_304],
  ['polymarket', 14059981907, 1_662_490, 1_673_216],
  ['txflow', 14059867229, 1_926_244, 1_937_408],
  ['weex', 14060691371, 2_021_455, 2_032_640],
  ['woo', 14059749363, 2_669_297, 2_679_808],
]) {
  const venues = Object.keys(WASM_VENUE_OVERRIDES).filter((venue) => venueFamily(venue) === family);
  for (const venue of venues) {
    test(`#3391 CI 43467762505: ${venue} artifact ${artifact} retains current measured reserve`, () => {
      assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.equal(cap, WASM_VENUE_OVERRIDES[family]);
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    });
  }
}

// #3317 merge 6a8318ff61c9 + master 7600a9a56787, CI run 43493007722.
// Source b07da9e11b0f has the same tree; all six artifact ZIP digests verified.
// All five optimized web/Node/replay copies agree for each family.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132; required two -Oz passes.
for (const [family, artifact, measured, minimumCap, moduleSha256] of [
  ['bybit', 14071675570, 2_547_120, 2_557_952, '9d1980d78eeb9ee0fd6441997f1fcf864fa3d71e6ae5fd5041549094b9de48a4'],
  ['kucoinspot', 14071220644, 2_477_083, 2_488_320, '498d7beb845e5568abfd9c028c93a0b657b3d3bff21e161c45e7f8c6ac649d8c'],
  ['grvt', 14071196267, 2_002_550, 2_013_184, '678fde6acd0301529c273e8e582affa5432b34dae8dd9ce40fce992bcc99ce90'],
  ['polymarket', 14070537614, 1_664_022, 1_675_264, 'c7c5a3503743c1d3efc30738948fe3cc7993217c44a8b58af597e2a5e2018f42'],
  ['weex', 14070393067, 2_023_034, 2_033_664, 'f3aa4b5ef0d6c67ac1a332e8438d7dc03c2370333b4747a3ab4834d4e260cf6d'],
  ['woo', 14070187529, 2_671_977, 2_682_880, '4a7b69619d603116b5786c53eb71642564416ffcee2e55a918fc5ff88d13e205'],
]) {
  test(`#3317 CI 43493007722: ${family} artifact ${artifact} retains fresh 10 KiB reserve`, () => {
    assert.match(moduleSha256, /^[0-9a-f]{64}$/);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const venues = family === 'bybit' ? ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'] : [family];
    for (const venue of venues) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[family];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
    }
  });
}

// #3317 master merge 20d61cbdf174: run 43517857744, family job 132204972157 measured web 2,769,255 B.
// Both aliases share one Bitget module; require the measured reserve and retain the hard boundary.
for (const venue of ['bitget', 'bitgetusdc']) {
  test(`#3317 fresh Bitget composition retains measured reserve for ${venue}`, () => {
    const measured = 2_769_255;
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    assert.equal(venueFamily(venue), 'bitget');
    assert.equal(minimumCap, 2_780_160);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}



test('#3302 CI run 42564749959 at 0bac20f54f5f retains current measured wasm headroom', () => {
  const measured = 2_219_070;
  const minimumCap = 2_230_272;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap - measured >= 10 * 1024);
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


test('#3302 merged 207b88974ec retains current measured wasm headroom', () => {
  const measured = 2_218_910;
  const minimumCap = 2_229_248;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


test('#3302 (run 42435433699 at 080ed4b51ae) retains measured wasm headroom', () => {
  for (const [venue, measured, minimumCap] of [
    ['asterdex', 2_193_230, 2_203_648],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});


test('#3302 pinned master merge 80d25442f36f retains measured asterdex headroom', () => {
  const measured = 2_231_057;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.equal(minimumCap, 2_241_536);
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


test('#3302 pinned master merge 359c4b3e3c4d retains measured asterdex headroom', () => {
  const measured = 2_231_732;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.equal(minimumCap, 2_242_560);
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


test('#3302 CI job 129514220621 retains merged-head weex family headroom', () => {
  const measured = Math.max(1_991_705, 1_993_745);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_004_992);
  for (const venue of ['weex']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});


test('#3302 CI job 129514220728 retains merged-head hyperliquid family headroom', () => {
  const measured = Math.max(2_236_465, 2_238_386);
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_248_704);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});


// #3302 run 43319605083, CI merge fb0f9b777241; pinned build inputs match a3275d876624.
// AsterDEX web: job 131537312211; Node: artifact 13990380922.
// Hyperliquid/WEEX web and Node: artifacts 13989992769 / 13989702631.
for (const [venue, measured] of [
  ['asterdex', 2_245_672],
  ['hyperliquid', 2_224_643],
  ['hyperliquidspot', 2_224_643],
  ['weex', 2_010_797],
]) {
  test(`#3302 current merged ${venue} artifact retains measured plus 10 KiB reserve`, () => {
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap, `${venue}: ${cap} < ${minimumCap}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}


// sam-mbp2/codex-4t8vx1: build.sh --venue asterdex at 3df8707ddee4.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; equal web/Node bytes.
test('#3302 reviewed merge retains the measured AsterDEX reserve', () => {
  const measured = 2_256_476;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.equal(minimumCap, 2_267_136);
  assert.ok(cap >= minimumCap, `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


// Isolated build.sh --venue asterdex at a5b5284402c8: equal web/Node bytes.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz.
test('#3302 fresh merged build keeps current reserve and the reviewed higher floor', () => {
  const measured = 2_247_295;
  const currentMinimum = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const reviewedMinimum = Math.ceil((2_256_476 + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.equal(currentMinimum, 2_257_920);
  assert.ok(cap >= Math.max(currentMinimum, reviewedMinimum));
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});


// Standard isolated build at merge 1ccd7b26df0b: identical web/Node WASM bytes.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz.
test('#3302 J6 identity master merge keeps the measured AsterDEX reserve', () => {
  const measured = 2_257_016;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.equal(minimumCap, 2_268_160);
  assert.ok(cap >= Math.max(minimumCap, 2_267_136));
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'asterdex'), /exceeds/);
});

test('#3302 run 43517974996 retains merged-head ApeX reserve and exact boundaries', () => {
  const measured = 2_093_078;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.apexomni;
  assert.equal(minimumCap, 2_104_320);
  assert.ok(cap >= minimumCap, `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'apexomni'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'apexomni'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'apexomni'), /exceeds/);
});

// #3499 head 22149d1131cb, run 43550434029, job 132311748258, artifact 14095922408.
// CI test merge e4514ec78d69 has the identical tree; optimized web/Node/replay
// SHA256 d69ef94f3c4beee62d2de9192d101670a0c8f6e21b1dbaff99fee7ad2de2fada.
test('#3499 BingX public-reference preflight retains measured 10 KiB reserve', () => {
  const measured = 2_135_090;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bingx;
  assert.equal(minimumCap, 2_146_304);
  assert.ok(cap >= 2_142_208, 'retain the historical #3438 cap floor');
  assert.ok(cap >= minimumCap, `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bingx'), /exceeds/);
});

// #3317 merge e25dc312bd2c: Rust SDK run 43545517625, source b1876cea63af.
// All 27 family ZIP digests, manifests and equal web/Node/replay hashes verified.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; retain every parent floor.
for (const [family, artifact, measured, minimumCap, aliases] of [
  ["apexomni", 14093433805, 2_094_662, 2_105_344, ["apexomni"]],
  ["asterdex", 14094386070, 2_258_086, 2_269_184, ["asterdex"]],
  ["backpack", 14094356166, 2_042_682, 2_053_120, ["backpack"]],
  ["binance", 14093817488, 2_272_209, 2_282_496, ["binance"]],
  ["bingx", 14094256309, 2_131_831, 2_142_208, ["bingx"]],
  ["bitget", 14093533458, 2_769_289, 2_780_160, ["bitget","bitgetusdc"]],
  ["bitrue", 14094226353, 1_908_397, 1_918_976, ["bitrue"]],
  ["bitunix", 14093428727, 1_954_918, 1_966_080, ["bitunix"]],
  ["blofin", 14093732852, 2_065_825, 2_076_672, ["blofin"]],
  ["bybit", 14093717922, 2_547_145, 2_557_952, ["bybit"]],
  ["coinbase", 14094520418, 2_114_517, 2_124_800, ["coinbase"]],
  ["cryptocom", 14093603492, 1_815_949, 1_826_816, ["cryptocom"]],
  ["grvt", 14094296177, 2_002_596, 2_013_184, ["grvt"]],
  ["hyperliquid", 14093254117, 2_237_152, 2_247_680, ["hyperliquid","hyperliquidspot"]],
  ["kraken", 14093868148, 2_274_234, 2_284_544, ["kraken","krakenspot"]],
  ["kucoin", 14093782761, 1_865_033, 1_875_968, ["kucoin"]],
  ["kucoinspot", 14093883114, 2_477_103, 2_488_320, ["kucoinspot"]],
  ["lighter", 14094201484, 2_160_313, 2_170_880, ["lighter","lighter-robinhood"]],
  ["mexc", 14093863330, 1_959_030, 1_970_176, ["mexc"]],
  ["okx", 14094291240, 2_050_864, 2_061_312, ["okx"]],
  ["orderly", 14094391124, 1_782_494, 1_793_024, ["orderly"]],
  ["paradex", 14094286290, 2_309_926, 2_320_384, ["paradex"]],
  ["phemex", 14093848345, 2_137_374, 2_148_352, ["phemex"]],
  ["polymarket", 14094421012, 1_664_056, 1_675_264, ["polymarket"]],
  ["txflow", 14093284013, 1_927_903, 1_938_432, ["txflow"]],
  ["weex", 14093538474, 2_023_069, 2_033_664, ["weex"]],
  ["woo", 14094331296, 2_672_011, 2_682_880, ["woo"]],
]) {
  test(`#3317 CI 43545517625: ${family} artifact ${artifact} retains merged reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const venue of aliases) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

// #3290 merge 86ea9140052b, Rust SDK run 43561789464: optimized web/Node modules.
// Replay jobs 132347235888/132347235976 (Hyperliquid) and 132347235852 (TxFlow).
for (const [family, measured, minimumCap, aliases] of [
  ['hyperliquid', 2_266_210, 2_277_376, ['hyperliquid', 'hyperliquidspot']],
  ['txflow', 1_957_203, 1_968_128, ['txflow']],
]) {
  test(`#3290 CI 43561789464: ${family} preserves the measured reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const venue of aliases) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

// #3290 source 8043e2bb11e5: CI 43563699705 builds cb6f9af848f2 with the identical tree.
// Archive digests and equal web/Node/replay module hashes verified; retain every historical floor.
for (const [family, artifact, measured, minimumCap, aliases] of [
  ["apexomni", 14101309480, 2_099_376, 2_110_464, ["apexomni"]],
  ["asterdex", 14101975996, 2_263_190, 2_274_304, ["asterdex"]],
  ["backpack", 14101806150, 2_047_591, 2_058_240, ["backpack"]],
  ["binance", 14101567436, 2_278_309, 2_288_640, ["binance"]],
  ["bingx", 14101497823, 2_137_074, 2_147_328, ["bingx"]],
  ["bitget", 14101418607, 2_769_289, 2_780_160, ["bitget", "bitgetusdc"]],
  ["bitrue", 14101906115, 1_908_397, 1_918_976, ["bitrue"]],
  ["bitunix", 14101901190, 1_954_918, 1_966_080, ["bitunix"]],
  ["blofin", 14101213436, 2_071_746, 2_082_816, ["blofin"]],
  ["bybit", 14101452734, 2_552_589, 2_563_072, ["bybit"]],
  ["coinbase", 14102120122, 2_114_517, 2_124_800, ["coinbase"]],
  ["cryptocom", 14101901106, 1_815_949, 1_826_816, ["cryptocom"]],
  ["grvt", 14101941136, 2_002_596, 2_013_184, ["grvt"]],
  ["hyperliquid", 14101826265, 2_266_273, 2_277_376, ["hyperliquid", "hyperliquidspot"]],
  ["kraken", 14101856260, 2_275_348, 2_286_592, ["kraken", "krakenspot"]],
  ["kucoin", 14101357803, 1_865_034, 1_875_968, ["kucoin"]],
  ["kucoinspot", 14102045777, 2_477_104, 2_488_320, ["kucoinspot"]],
  ["lighter", 14101492956, 2_166_230, 2_177_024, ["lighter", "lighter-robinhood"]],
  ["mexc", 14101831371, 1_959_030, 1_970_176, ["mexc"]],
  ["okx", 14101741392, 2_057_167, 2_067_456, ["okx"]],
  ["orderly", 14101238198, 1_782_494, 1_793_024, ["orderly"]],
  ["paradex", 14101377600, 2_309_926, 2_320_384, ["paradex"]],
  ["phemex", 14101582159, 2_144_151, 2_154_496, ["phemex"]],
  ["polymarket", 14101801194, 1_664_056, 1_675_264, ["polymarket"]],
  ["txflow", 14102000849, 1_957_266, 1_968_128, ["txflow"]],
  ["weex", 14101910992, 2_028_911, 2_039_808, ["weex"]],
  ["woo", 14101841020, 2_680_145, 2_691_072, ["woo"]],

]) {
  test(`#3290 CI 43563699705: ${family} artifact ${artifact} preserves the merged reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const venue of aliases) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

// #3499 merge f2a87132c7771a459267ff33f270f2efc8deb929: local optimized web/Node bytes.
// Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; build.sh twice -Oz, -j2.
// Both targets have the pinned SHA256; keep all inherited floors and boundary checks.
for (const [family, measured, minimumCap, sha256, venues] of [
  ["apexomni", 2_096_110, 2_106_368, "567beee34b576e856051058c0806272d33ee88ef52e6d1f6fb18ec085313b278", [["apexomni", 2105344]]],
  ["asterdex", 2_259_398, 2_270_208, "f386784c50b3aa5696602a41a12c6a87025ed52ee4afd98405b345ac4e5be9c4", [["asterdex", 2269184]]],
  ["backpack", 2_044_034, 2_055_168, "92668e3b4ca517c6190b4127c58f5991ee54c910d9446a160e1f4bf559f18d29", [["backpack", 2053120]]],
  ["binance", 2_273_578, 2_284_544, "5d7f01edc0b07f1d04eaf6b920ac03015b2190ecb27e681969ff66382effde68", [["binance", 2286592]]],
  ["bingx", 2_138_113, 2_149_376, "38497edda9c26b968fdfd7894f38c1f5e2c7d4fa6d2144093dd5e2349a23e8c2", [["bingx", 2146304]]],
  ["bitget", 2_770_480, 2_781_184, "b513162cc6aacd4da661791769bed30765f078c1b441c801e464b55cc4f3368c", [["bitget", 2780160], ["bitgetusdc", 2780160]]],
  ["bitrue", 1_909_654, 1_920_000, "276947e8095d040d5db49f11f7fb3b8aa28f6e19ca4a479d83c5731661d2c2ea", [["bitrue", 1925120]]],
  ["bitunix", 1_956_174, 1_967_104, "75529c2d03ed8b533ea09d7f464bbb672f7a173417921be9090fdb99365d6b33", [["bitunix", 1966080]]],
  ["blofin", 2_067_785, 2_078_720, "3c587c778e336c7e9cf028dc23e4ab05d4cc36e5605f7247b3224bbf03d091ab", [["blofin", 2085888]]],
  ["bybit", 2_548_513, 2_558_976, "2ceb757a47ef52ae107f4747a634c506b93fa5c3c0099be289da092cb1975854", [["bybit", 2557952]]],
  ["coinbase", 2_115_717, 2_126_848, "8f1e756380cf3aa131c182b0b20c169dc159538d91f2943e33c31e592a7f97ae", [["coinbase", 2124800]]],
  ["cryptocom", 1_817_206, 1_827_840, "61dcbdc6521d5487f64955622b4665061dcf624477c4f28b6beae9ed91f869c4", [["cryptocom", 1826816]]],
  ["grvt", 2_003_900, 2_014_208, "cc51562f8946a8803f173598bfd7ffd85e414b2d33d08e724ec0deed2b99560c", [["grvt", 2013184]]],
  ["hyperliquid", 2_238_456, 2_248_704, "9eeb077d38fef93cd1d55f9c7930cf0edfd771fb1d72989a1ea71a6276878b3d", [["hyperliquid", 2249728], ["hyperliquidspot", 2249728]]],
  ["kraken", 2_275_565, 2_286_592, "28c4ded8f737bf6afedbb9d0dbb56c74ca913797532b6bf7a232660c2983ec5c", [["kraken", 2284544], ["krakenspot", 2284544]]],
  ["kucoin", 1_866_297, 1_876_992, "15c8988c4da51594105b86b7d14d58257f9ed7a90317f72405a53e649c73cf37", [["kucoin", 1878016]]],
  ["kucoinspot", 2_477_934, 2_488_320, "0e75b89916e645323d17fcac9f6b67e2457c545857ef48eaa551392e8ee8e256", [["kucoinspot", 2488320]]],
  ["lighter", 2_161_641, 2_171_904, "a68ba37b7bb5cf5626c8aef8e706dc63a25accc78e1d7d234e4528cf15e5541d", [["lighter", 2170880], ["lighter-robinhood", 2170880]]],
  ["mexc", 1_960_367, 1_971_200, "e8939820ec914ac233aafc4ef82692d2fda4478800a10f85a7597073810af27f", [["mexc", 1970176]]],
  ["okx", 2_052_230, 2_063_360, "e3fb37a419ff55b0d2642c40c1aa198babd76b6273fdddcc9cebf2858eac068c", [["okx", 2068480]]],
  ["orderly", 1_783_731, 1_794_048, "8bf9bcd08805790ec750277a4c18b63006df2a616d353b1b77273675106b8743", [["orderly", 1793024]]],
  ["paradex", 2_311_054, 2_321_408, "d09b9350c4291ebb92b009a175f6d5e8cf1375b5f8762913fe992925981f5620", [["paradex", 2320384]]],
  ["phemex", 2_138_706, 2_149_376, "468e75b79a6df9a2af00e8254dbc7c45e1348fafea89af406e5d78a4a8a822ad", [["phemex", 2148352]]],
  ["polymarket", 1_665_253, 1_676_288, "3ce71a13dd298e47164f6f1389a2d17c19e93e29e5dfe04e2ec67c0b11545f9c", [["polymarket", 1675264]]],
  ["txflow", 1_929_231, 1_940_480, "6aef7bdc32905fa546b7613b9dd31ccdfdfc923838384d6cb66cbe47c09fe901", [["txflow", 1939456]]],
  ["weex", 2_024_581, 2_035_712, "8e8c2d751f909d1a76344fb3b2ddd608d0370f99a0fe8587753e06a6c25394b7", [["weex", 2033664]]],
  ["woo", 2_673_019, 2_683_904, "3db40d4e7470855ee11eb59695246ea78fe34bc8fa07de3f3333af4988feef30", [["woo", 2682880]]],
]) {
  test(`#3499 local merged-head: ${family} ${sha256} retains measured reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const [venue, parentFloor] of venues) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= parentFloor, `${venue}: ${cap} must retain parent floor ${parentFloor}`);
      assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

test('#3225 private drb CI 42367168155 retains measured headroom and the exact boundary', () => {
  const measured = 2_022_882;
  const minimumCap = 2_033_664;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.coinbase;
  assert.ok(cap >= minimumCap);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    assert.equal(WASM_VENUE_OVERRIDES[family], cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});


test('#3225 refreshed private DRB artifacts retain 10 KiB and both Coinbase family boundaries', () => {
  const measured = 2_222_017;
  const minimumCap = 2_232_320;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    const cap = WASM_VENUE_OVERRIDES.coinbase;
    assert.ok(cap >= Math.max(minimumCap, 2_231_296, 2_228_224, 2_126_848));
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

for (const [venue, measured, required] of [
  ['txflow', 2_148_377, 2_159_616],
  ['weex', 2_028_106, 2_038_784],
]) test(`#3314 merge f19209120d28: ${venue} retains fresh measured reserve`, () => {
  assert.equal(required, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES[venue];
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});

// #3290 master refresh 78d46f0574ef, CI 43569184535, shipped family artifact 14104151569.
test('#3290 merged BingX module retains the current measured reserve', () => {
  const measured = 2_142_207;
  const minimumCap = 2_152_448;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bingx;
  assert.ok(cap >= minimumCap, `${cap} must retain ${minimumCap} bytes`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bingx'), /exceeds/);
});

// #3290 lookup candidate 11456fb4a3c2, CI 43579592827; optimized web/Node/replay maximum.
// CI source 2dd9066eb21ffb1d9902f6d3dc19a585b4f18748; Rust 1.99.0, bindgen 0.2.129, Binaryen 132.
// Binance job 132401623226; BingX job 132401623277. Retain all historical floors and boundaries.
for (const [venue, measured, minimumCap, parentFloor, artifact, sha256] of [
  ['binance', 2_278_530, 2_289_664, 2_288_640, 14107991476, 'ec6b62ed8987dd234703da1c20aae4b6ae401c54b6ab6b648ac72cd92f0e7a6b'],
  ['bingx', 2_142_427, 2_153_472, 2_152_448, 14107034718, '3135a5a3ea736112947776cb213111ccb5c0c1b75d625339bf348cf8cf12b3d3'],
]) {
  test(`#3290 lookup CI: ${venue} artifact ${artifact} ${sha256} retains measured reserve`, () => {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(venueFamily(venue), venue);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= parentFloor, `${venue}: ${cap} must retain parent floor ${parentFloor}`);
    assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

// Local build.sh --venue lighter at d28424d3b6bf51619760093b2004e8a357da1524.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132, required two -Oz passes; web/Node
// SHA256 a39817bd77ca65696cbf3dc95cabbe15f8fd0cd9f17e0daece68173c0568fe82.
for (const venue of ['lighter', 'lighter-robinhood']) {
  test(`Lighter protective-reference preflight: ${venue} retains measured 10 KiB reserve`, () => {
    const measured = 2_178_471;
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(minimumCap, 2_189_312);
    assert.equal(venueFamily(venue), 'lighter');
    assert.equal(cap, WASM_VENUE_OVERRIDES.lighter);
    assert.ok(cap >= 2_170_880, 'retain all historical parent floors');
    assert.ok(cap >= minimumCap, `${venue}: ${cap} leaves less than 10 KiB above ${measured}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

// #3501 run 43559467764/job 132340062664, artifact 14100190688 (CI merge bd10840552fe).
// All five optimized web/Node/replay modules measured 2,179,692 B, SHA256
// a010cf7ae0beec4410505d5ecfa2ca86e4b907c26b4591136d6a08ee6a8cd65b.
for (const venue of ['lighter', 'lighter-robinhood']) {
  test(`Lighter CI composition: ${venue} retains measured 10 KiB reserve`, () => {
    const measured = 2_179_692;
    const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(minimumCap, 2_190_336);
    assert.equal(venueFamily(venue), 'lighter');
    assert.equal(cap, WASM_VENUE_OVERRIDES.lighter);
    assert.ok(cap >= 2_189_312, 'retain all historical parent floors');
    assert.ok(cap >= minimumCap, `${venue}: ${cap} leaves less than 10 KiB above ${measured}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

// #3501 merge parents 12b1102ecc65 + c3eaaf96d355; optimized local web/Node artifacts.
// build.sh --venue <family>; Rust1.99.0, bindgen0.2.129, Binaryen132, two required -Oz passes.
// Keep both parents' measured floors as well as the fresh merged composition's 10 KiB reserve.
for (const [venue, family, measured, minimumCap, sha256, parentFloor] of [
  [
    "lighter",
    "lighter",
    2178874,
    2189312,
    "385b00264366418823c013a7d137285df257ca48427c6cc184c8af63c9bc1050",
    2190336
  ],
  [
    "lighter-robinhood",
    "lighter",
    2178874,
    2189312,
    "385b00264366418823c013a7d137285df257ca48427c6cc184c8af63c9bc1050",
    2190336
  ],
  [
    "bingx",
    "bingx",
    2157930,
    2168832,
    "a5d293a7feef4cfede2bda1e20d3cb59ea26e0f32e4515a0bbfbc4cbc2b972cd",
    2149376
  ]
]) {
  test(`#3501 master merge: ${venue} retains measured reserve and both parent floors`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(venueFamily(venue), family);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= parentFloor, `${venue}: ${cap} must retain parent floor ${parentFloor}`);
    assert.ok(cap >= minimumCap, `${venue}: ${cap} leaves less than 10 KiB above ${measured}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    if (family === 'lighter') assert.equal(cap, WASM_VENUE_OVERRIDES['lighter-robinhood']);
  });
}

// #3226 run 43581762045/job 132408167613, CI merge 32c729814fe1b342b7184ac06c871397a662dd74.
// Optimized web/Node 2,314,538 B, SHA256
// 5c2d9ff5f7489b48ad64307a289aad14b80f40224689fac3bdc9999150b59bc1.
// Rust1.99.0, bindgen0.2.129, Binaryen132, two required -Oz passes.
// Fresh composed head 3527907c2249, run 43588966237/job 132429829237/artifact 14111067968:
// optimized web/Node/replay 2,314,622 B, same profile; the existing measured floor remains valid.
test('#3226 DRB trading retains current measured reserve and both parent floors', () => {
  const measured = 2_314_622;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_325_504);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= 2_126_848, 'retain master and PR-parent floors');
    assert.ok(cap >= minimumCap, `${venue}: less than 10 KiB above current CI artifact`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});
// #3290 merge bbc40c3cd8ad, CI 43582722154/job 132414117890, family artifact 14109099497.
// Source 2cf28d853f16058cb6802e0aec598c43f132cd78; web/Node/replay SHA ca71afa4470420325dfcd035152511607e2f288559cffb6fc25a0e32c66168c4.
// Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132; required two -Oz passes.
for (const venue of ['lighter', 'lighter-robinhood']) {
  test(`#3290 merged Lighter artifact 14109099497: ${venue} retains fresh measured reserve`, () => {
    const measured = 2_185_562;
    const minimumCap = 2_196_480;
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(venueFamily(venue), 'lighter');
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= 2_190_336, `${venue}: ${cap} must retain both parent floors`);
    assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
    assert.equal(cap, WASM_VENUE_OVERRIDES.lighter);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

test('#3314 (run 42560319271 at merge 449511022a8d) retains measured TxFlow headroom', () => {
  const measured = 2_121_120;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_131_968);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

test('#3314 repaired c5c8a213f6a5 retains current optimized TxFlow headroom', () => {
  const measured = 2_121_037;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_131_968);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

test('#3314 agent setup e2da53cc9f26 retains measured TxFlow headroom', () => {
  const measured = 2_120_966;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_131_968);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= minimumCap);
  assert.equal(wasmBudgetError(measured, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});


test('#3314 refreshed merge 9bb43250830d retains measured TxFlow reserve', () => {
  const measured = 2_147_684;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(required, 2_158_592);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});


test('#3314 refusal fix e959e2a7dc2e retains fresh optimized TxFlow reserve', () => {
  const measured = 2_147_758;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(required, 2_158_592);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});


test('#3314 composed WEEX f6830c3f2466 retains measured optimized reserve', () => {
  const measured = 2_028_105;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(required, 2_038_784);
  const cap = WASM_VENUE_OVERRIDES.weex;
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'weex'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'weex'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'weex'), /exceeds/);
});


test('#3314 recoverable authorization reads retain freshly measured TxFlow reserve', () => {
  const measured = 2_148_376;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(required, 2_159_616);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});


for (const [venue, measured, required] of [
  ['txflow', 2_148_377, 2_159_616],
  ['weex', 2_028_106, 2_038_784],
]) test(`#3314 merge f19209120d28: ${venue} retains fresh measured reserve`, () => {
  assert.equal(required, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES[venue];
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});


test('#3314 serial placement source1c2a4f8232c4 retains measured TxFlow reserve', () => {
  const measured = 2_149_614;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(required, 2_160_640);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= required);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

// #3290 CI 43591113196, source97507acf9371 (head738ee1289): pinned optimized profile.
// Verified26family archive digests/manifests and equal web/Node/replay modules;
// TxFlow cap refusal retains current web/Node bytes+hashes in strict-replay artifact14113250022.
for (const [venue, family, artifact, measured, minimumCap] of [
  ['apexomni', 'apexomni', 14112672780, 2_100_887, 2_111_488],
  ['asterdex', 'asterdex', 14112563027, 2_264_745, 2_275_328],
  ['backpack', 'backpack', 14112672710, 2_048_859, 2_059_264],
  ['binance', 'binance', 14112821349, 2_279_707, 2_290_688],
  ['bybit', 'bybit', 14112214193, 2_553_690, 2_564_096],
  ['hyperliquid', 'hyperliquid', 14112936192, 2_267_769, 2_278_400],
  ['hyperliquidspot', 'hyperliquid', 14112936192, 2_267_769, 2_278_400],
  ['kraken', 'kraken', 14112762396, 2_276_461, 2_287_616],
  ['krakenspot', 'kraken', 14112762396, 2_276_461, 2_287_616],
  ['lighter', 'lighter', 14112777448, 2_186_833, 2_197_504],
  ['lighter-robinhood', 'lighter', 14112777448, 2_186_833, 2_197_504],
  ['paradex', 'paradex', 14112204213, 2_311_211, 2_322_432],
  ['phemex', 'phemex', 14112821350, 2_144_862, 2_155_520],
  ['weex', 'weex', 14112856212, 2_029_690, 2_040_832],
  ['txflow', 'txflow', 14113250022, 2_164_428, 2_174_976],
]) {
  test(`#3290 receipt fix CI43591113196: ${venue} artifact${artifact} retains measured reserve`, () => {
    assert.equal(venueFamily(venue), family);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(cap, WASM_VENUE_OVERRIDES[family]);
    assert.ok(cap >= minimumCap, `${venue}: ${cap} must retain ${minimumCap} bytes`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}


// #3226 c548d1eb3 fix + master b86a48664; head89f7496f69aca.
// CI43598275562/job132457674114, source1108664e5141: optimized web/Node2344277B.
// SHA256864e305c0b2edc517397404b064cf2255d429d2041d02cd679264201f2ac2d05.
// Rust1.99.0, bindgen0.2.129, Binaryen132, two required -Oz passes.
test('#3226 per-identity census and queued intent keep current CI reserve', () => {
  const measured = 2_344_277;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_355_200);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= 2_325_504, 'retain all measured parent floors');
    assert.ok(cap >= minimumCap, `${venue}: less than 10 KiB above current CI module`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

// #3226 master #3225 composition7b87c02999ae, CI43603165570/job132472467519.
// Artifact14117401283: web/Node/replay2346763B, SHA256
// 4c1200b004b8fb86924bacdc68a0199e67a9710fa7664aae3f330124f1ac9c67.
// Rust1.99.0, bindgen0.2.129, Binaryen132 and two required -Oz passes.
test('#3226 causal-order composition retains the current CI reserve', () => {
  const measured = 2_346_763;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_357_248);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= 2_355_200, 'retain all measured parent floors');
    assert.ok(cap >= minimumCap, `${venue}: less than 10 KiB above current CI module`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

// #3273 run 43563376371, artifact 14101234552; source d420622af083 has merge 18f33911d's tree.
// Rust 1.99.0, bindgen 0.2.129, Binaryen 132 twice -Oz; equal web/Node/replay bytes.
// Preserve measured + 10 KiB, parent floors, the inclusive cap and cap+1 refusal.
test('#3273 merged decoder retains TxFlow measured reserve', () => {
  const measured = 1_929_688;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.equal(minimumCap, 1_940_480);
  assert.ok(cap >= minimumCap, `txflow: ${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

// #3273 merge parents babeb9fb7769 + 4c199abd5d2b: optimized local web/Node 1,944,289 B.
// Rust1.99.0, bindgen0.2.129, Binaryen132, two required -Oz passes; build.sh --venue txflow.
// Module SHA256 5fe84613c4f05a4801114df78fc68cdca49fceffcbd29687d8676b0dad054f3e.
test('#3273 current master merge retains measured TxFlow reserve and parent floors', () => {
  const measured = 1944289;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.equal(minimumCap, 1954816);
  assert.ok(cap >= 1940480, 'retain both parent floors');
  assert.ok(cap >= minimumCap, `txflow: ${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

// #3273 second merge: current master 829f80ec68cb artifacts, run 43588166333.
// API ZIP digests, source manifests and all 135 optimized web/Node/replay copies verified.
// Other-family dependency graphs exclude the changed TxFlow crates; retain both parent floors.
for (const [family, artifact, measured, minimumCap, sha256, aliases] of [
  ["apexomni", 14111526939, 2095953, 2106368, "1c519563b5d1e333070814fdc8811754222128fc82e0e4bf8c369e48a8c83212", [["apexomni", 2106368]]],
  ["asterdex", 14111846355, 2259309, 2270208, "5402f6414a501e2980c2725f9236c04ecb5bacb2c3ec7983c91b01950c2b8f92", [["asterdex", 2270208]]],
  ["backpack", 14111252370, 2043730, 2054144, "f5b6da17f80d75caeeb191f1f718687f152c648421c215c271b6c6e7759db95e", [["backpack", 2055168]]],
  ["binance", 14111601646, 2273321, 2284544, "747794467cf3881e48edbfe58a552bbec74f3947a12ccc5552aa62f42e4a0b8b", [["binance", 2286592]]],
  ["bingx", 14111511818, 2138102, 2148352, "a4e96fd88d4e7a3a8ed7f121b1f849301306f7374783ed1c134ad4fa4d4a361d", [["bingx", 2168832]]],
  ["bitget", 14111441144, 2769063, 2780160, "2de457cce27593076890f2bbd9f85192e456493279c224ccb06c4117ddbb52ff", [["bitget", 2781184], ["bitgetusdc", 2781184]]],
  ["bitrue", 14111765680, 1909531, 1920000, "eb2ab9feb465017fad932b05f7dfb11a4603cefb710db4813fa13e3b643705cc", [["bitrue", 1925120]]],
  ["bitunix", 14111402010, 1956040, 1967104, "b6af001dc4435c60516b68c86e75b6b88c420f608d82ede31d36b46f66ead7e4", [["bitunix", 1967104]]],
  ["blofin", 14111851118, 2066952, 2077696, "db8bcf5c7ae3ec1638a4617c29aa068affb6f82b16f5c67dfeb11332c74b3b7b", [["blofin", 2085888]]],
  ["bybit", 14111222513, 2548241, 2558976, "7a6bc4d25640a679bbc9b63ef6b86d07c45d5a30d81e9a3b2717298b39d413e0", [["bybit", 2558976]]],
  ["coinbase", 14111031867, 2115685, 2126848, "0cf88295d47549348b0add2c5816abdd1e88cc36cf4caf08b4b23fd423150161", [["coinbase", 2126848]]],
  ["cryptocom", 14110957125, 1817046, 1827840, "52bec97b9324c5f41210a4662bc7e26ebe6d1fd439f3bf7b6c2f6a45a3957b6a", [["cryptocom", 1827840]]],
  ["grvt", 14111995603, 2003844, 2014208, "07912964d0644b0635ea9c1a6d60db30d3c09213f2b3957b74a995f066bb3dff", [["grvt", 2014208]]],
  ["hyperliquid", 14111406114, 2238730, 2249728, "65261f9e03b87d8f4a934e8291d4681130b67f021afc8bb095b8389d7d77bffb", [["hyperliquid", 2249728], ["hyperliquidspot", 2249728]]],
  ["kraken", 14111436859, 2275474, 2286592, "e5b7c7ba291f6ad190060271c211a9d21bc8fe9521be4eb198dc1718b18e98e0", [["kraken", 2286592], ["krakenspot", 2286592]]],
  ["kucoin", 14110992905, 1866114, 1876992, "1ad012bd0fe40a07e12838644a17a8a654d71a564f752b2b89414e803cb7306e", [["kucoin", 1878016]]],
  ["kucoinspot", 14111726490, 2477288, 2488320, "7e23b4e1ee5370c16482b45fb9810c0d1744ed0442847eb0b65aa1dc235b805e", [["kucoinspot", 2488320]]],
  ["lighter", 14111990891, 2181366, 2192384, "0dfe47e19ebc5371e3c2aae972a24a77aed4867500fb63f38039d472bd0f17be", [["lighter", 2190336], ["lighter-robinhood", 2190336]]],
  ["mexc", 14111566854, 1960214, 1971200, "a6f84772449bfc43ed3d026256d272539d77d6c3473b17673b9bb7927b9c801d", [["mexc", 1971200]]],
  ["okx", 14111706505, 2051981, 2062336, "2de1c842c358ce8dec96be38cbda161cc5bb13ebd68df8163bbb37f9e447174f", [["okx", 2068480]]],
  ["orderly", 14110943325, 1783559, 1794048, "049724bf0ca1017ad5190ca594d6a2abb5bce1195d9b4921e4a4f0f659439345", [["orderly", 1794048]]],
  ["paradex", 14112065076, 2311211, 2322432, "b4e1bbf2bc6d2e11bd71089d28dc34f4447e0461384281ae6ea1743635e3c412", [["paradex", 2321408]]],
  ["phemex", 14111157390, 2138503, 2149376, "4816cb2a50041f3bf8b5e28a458da63bba108f778cc859c05381005cba704adb", [["phemex", 2149376]]],
  ["polymarket", 14111920487, 1664855, 1675264, "e2dae46043567fa8b03196d881b3ae77437842d4df61c4a6134f38b030073133", [["polymarket", 1676288]]],
  ["txflow", 14111501911, 2138512, 2149376, "5af8092888a666e615673e71658a236289289196dba1447b9ce57e79feb42aab", [["txflow", 2160640]]],
  ["weex", 14111331283, 2024284, 2034688, "9c1161f2847fb35bcb1d7b61d08d88e9c3e79bd2aeb111799741f90e92c821bf", [["weex", 2038784]]],
  ["woo", 14111551715, 2672565, 2682880, "6fdc9bd7ee7f74074d922fea96d33b3e11100f2369e38559d1362c38976097e4", [["woo", 2683904]]],
]) {
  test(`#3273 second merge: ${family} CI artifact ${artifact} retains measured reserve`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const [venue, parentFloor] of aliases) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= parentFloor, `${venue}: ${cap} must retain ${parentFloor}`);
      assert.ok(cap >= minimumCap, `${venue}: ${cap} leaves less than 10 KiB above ${measured}`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

// #3273 second merge, parents 73b97124a92d + 829f80ec68cb: freshly optimized local web/Node.
// build.sh --venue <family>; Rust1.99.0, bindgen0.2.129, Binaryen132, two required -Oz passes.
// Retain current-master CI measurements and all parent floors as well as this exact composition.
for (const [family, measured, minimumCap, sha256, parentFloor] of [
  ["txflow", 2151457, 2162688, "b621258fa939e3a7460d60d33fbb5f56493b5a5a9d2c3b7e3b9aac08ba09df7c", 2160640],
  ["bingx", 2159071, 2169856, "4c640bbcfbdc675bce5e758e1c412e4ee3313bd807a57705fbc026c4e908f14d", 2168832],
  ["weex", 2028146, 2038784, "2be7bb0ed10887820ed63aa9e091cad08cbc381b670d07edf0c1b458c0e16e6f", 2038784],
]) {
  test(`#3273 second merge: ${family} retains fresh local merged reserve`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.ok(cap >= parentFloor, `${family}: ${cap} must retain ${parentFloor}`);
    assert.ok(cap >= minimumCap, `${family}: ${cap} leaves less than 10 KiB above ${measured}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

// #3273 third merge: master b86a48664ac8, run 43595404845; all 27 archives verified.
// Source/tool/required-Oz metadata, API ZIP digests and all 135 module copies checked.
// Other selected family graphs exclude changed TxFlow crates; preserve both parent floors.
for (const [family, artifact, measured, minimumCap, sha256, aliases] of [
  ["apexomni", 14114900156, 2100887, 2111488, "77668ba3c8e46c6365627540656151f3d8a78531c2fef79cbf897c68ef040088", [["apexomni", 2111488]]],
  ["asterdex", 14114385814, 2264745, 2275328, "0dda31c9b2d4b80e09af5654ce3bb096647fd5c6a153037acda1564c7e90efb4", [["asterdex", 2275328]]],
  ["backpack", 14114755523, 2048859, 2059264, "2630ae63a9a7ef9d382ac49c264ff62e92a8d232df0a91e10533a2dd0242db60", [["backpack", 2059264]]],
  ["binance", 14114034496, 2279707, 2290688, "2336f164dfa6ad6f617a50ae7e14937aeaf4bed3efab735764fb771009c0e6cd", [["binance", 2290688]]],
  ["bingx", 14114161496, 2143567, 2154496, "52d674ffd7ef1228db95b0c3256ffe8219e23f3b66002f0c2475e56ea0549c18", [["bingx", 2169856]]],
  ["bitget", 14114805469, 2769063, 2780160, "ec7adffd8921d45a38665513311269d8b61dfc3bf0465b4ef921a1e5114c823e", [["bitget", 2781184], ["bitgetusdc", 2781184]]],
  ["bitrue", 14114236341, 1909531, 1920000, "6fbc523e688787c8a8781519f130e97d959eb2152d91ba4a78abea62eae8aec9", [["bitrue", 1925120]]],
  ["bitunix", 14114815471, 1956040, 1967104, "588519f45253910d0637298e83d3e07417a17ff63e5e360a98c812922053d53b", [["bitunix", 1967104]]],
  ["blofin", 14114665891, 2072437, 2082816, "d040bd3ebbedddd1effc4f73fed2dbcfa0d6fc40edfbc7b0b9ef1ee3db09683a", [["blofin", 2085888]]],
  ["bybit", 14114446538, 2553690, 2564096, "0174ad6c8a3475d0b410e1405d6a5e9dc6beddd8ef15baa8dc3c8b0bceae3160", [["bybit", 2564096]]],
  ["coinbase", 14114925255, 2115685, 2126848, "69e3b0df45a63cd5b2946211b7ac7d73fc75a87276ae781f03d6b16f7e145535", [["coinbase", 2126848]]],
  ["cryptocom", 14114166529, 1817046, 1827840, "c8afbe1012c5b1b3eced62ec21400edc85856c5ce004a11c5c1516de33a85c6f", [["cryptocom", 1827840]]],
  ["grvt", 14114850211, 2003844, 2014208, "b4b13b7501777db4f84e92892db50982f4738c2394cc2ecede80c42b5488b393", [["grvt", 2014208]]],
  ["hyperliquid", 14114770613, 2267769, 2278400, "b13c7edfa378a4fd12266d66a4085d34aeb4e3cf43b1e2a25032c0137ed941e9", [["hyperliquid", 2278400], ["hyperliquidspot", 2278400]]],
  ["kraken", 14114044309, 2276461, 2287616, "1b7414e8645ad6a26eb3f35af0bb79308f5ba29b391615303e7ec624bd8e45b8", [["kraken", 2287616], ["krakenspot", 2287616]]],
  ["kucoin", 14114024316, 1866114, 1876992, "7f891c6d24bfdc3c4d933212031b5690bb75dca0859253518058a8c5e39becd6", [["kucoin", 1878016]]],
  ["kucoinspot", 14114206477, 2477288, 2488320, "95dd52feb06729357578b1aeeb97e49987010974f42f772c22aac6c115725ec6", [["kucoinspot", 2488320]]],
  ["lighter", 14114561215, 2186833, 2197504, "325adefbb1e7b16e138d05c4d6aef82064041c526ee68156757eb5eafdc722f3", [["lighter", 2197504], ["lighter-robinhood", 2197504]]],
  ["mexc", 14115020297, 1960214, 1971200, "dc76f6f36a6bc847e935b1b162fb66853886b9fd6f9696dc9faa452503f38d9a", [["mexc", 1971200]]],
  ["okx", 14114990085, 2057840, 2068480, "77071b439ebb252dba3beee597bfbc20d4c7643549a6e0eca8713ee0b979fc69", [["okx", 2068480]]],
  ["orderly", 14114890158, 1783559, 1794048, "c90323dcde1815b0b8f63f7f53bd97d325dabfe4299338241cfe404d14166e28", [["orderly", 1794048]]],
  ["paradex", 14114506220, 2311211, 2322432, "5b06471f86a183e59e0f01177a20baad87ae96849c177b00ad7e07253f9ccfef", [["paradex", 2322432]]],
  ["phemex", 14114401408, 2144862, 2155520, "f8edfa4fc31b825f067684c1580a6ce69d152cb6d8461f18ca8fc11d78158155", [["phemex", 2155520]]],
  ["polymarket", 14114595450, 1664855, 1675264, "42d2e26e23ad5f376b6ac7ba747a018b91d8070ade4679ead5c4727636dbead5", [["polymarket", 1676288]]],
  ["txflow", 14114401698, 2164428, 2174976, "822331413210853200886da6485268b8f5bc732c4b42d486bd6ef8de507a2d1c", [["txflow", 2174976]]],
  ["weex", 14114510649, 2029690, 2040832, "21491682ad7c79e45e8d8c77384d231d1aff0b67897696d07b5cdaf5e1742960", [["weex", 2040832]]],
  ["woo", 14114860248, 2680679, 2691072, "654fb9be8ff3a3a25019d373519f0db95f1d18ab8fef49b4a4e8111870867f6f", [["woo", 2691072]]],
]) {
  test(`#3273 third merge: ${family} CI artifact ${artifact} retains measured reserve`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const [venue, parentFloor] of aliases) {
      assert.equal(venueFamily(venue), family);
      const cap = WASM_VENUE_OVERRIDES[venue];
      assert.ok(cap >= parentFloor, `${venue}: ${cap} must retain ${parentFloor}`);
      assert.ok(cap >= minimumCap, `${venue}: ${cap} leaves less than 10 KiB above ${measured}`);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    }
  });
}

// #3273 third merge, parents 47a277eb4a6b + b86a48664ac8: fresh optimized local web/Node.
// build.sh --venue <family>; Rust1.99.0, bindgen0.2.129, Binaryen132, two required -Oz passes.
// Keep all CI observations and historical floors in addition to this resolved composition.
for (const [family, measured, minimumCap, sha256, parentFloor] of [
  ["txflow", 2164535, 2174976, "d71dc47f60671601eee77073110473dd4d1882b8c551c92ba3f89170538a273a", 2174976],
  ["bingx", 2164521, 2174976, "b8b941a07f752d798e3731d562024632682ddde559ed417baec2c589741bb7a7", 2169856],
  ["weex", 2033603, 2043904, "8717aeec21f0b2953b00eff9c2ce490b032a88b5ec352db18eedb7908f00e827", 2040832],
]) {
  test(`#3273 third merge: ${family} retains fresh local merged reserve`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.ok(cap >= parentFloor, `${family}: ${cap} must retain ${parentFloor}`);
    assert.ok(cap >= minimumCap, `${family}: ${cap} leaves less than 10 KiB above ${measured}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

// #3273 exact-head CI run 43601014849/job 132465891849, artifact 14116513084.
// CI source 7d78dc7dc091 and head c0afb09f78af have identical tree ef945e2e1373; pinned required-Oz build.
// Five optimized web/Node/replay copies: SHA256 0f9fa7d7b84f4cefa7f6804515cf072938c4cc23aa34cd76351d96c3fc80c4d5.
test('#3273 exact-head CI TxFlow artifact retains measured reserve', () => {
  const measured = 2_166_223;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.equal(venueFamily('txflow'), 'txflow');
  assert.equal(minimumCap, 2_177_024);
  assert.ok(cap >= 2_174_976, 'retain every historical parent floor');
  assert.ok(cap >= minimumCap, `txflow: ${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

// #3226 review fixes b4bedc2279 + master b991dbd548; clean merge f0660fb9387f.
// CI43610609451/job132495348457, artifact14120177731, sourceb29eaa51e93a.
// CI and merged branch share tree40bfb8ec80e2c13f9ae0229229e5e2fa4de6aab6.
// Five optimized web/Node/replay copies SHA256
// 9793c41c170e9ee0ce010699f0ba2d0e63a03f18aabdef2cfe293aff22a65f5d.
test('#3226 reviewed reconciliation CI artifact retains 10 KiB for the Coinbase family', () => {
  const measured = 2_348_687;
  const minimumCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(minimumCap, 2_359_296);
  for (const venue of ['coinbase', 'coinbasespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'coinbase');
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.ok(cap >= 2_357_248, 'retain every historical parent floor');
    assert.ok(cap >= minimumCap, `coinbase: ${cap} leaves less than 10 KiB above ${measured}`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

// #3520 run43648572739/job132612114429, artifact14135771071: all five module copies match.
// Source5de35a110cf1 has eecfabbc5d6e's tree; Rust1.99/bindgen0.2.129/Binaryen132 required twice-Oz.
test('#3520 TxFlow margin-mode guard retains measured 10 KiB reserve', () => {
  const measured = 2_167_082;
  const required = 2_178_048;
  const parentFloor = 2_177_024;
  const sha256 = '4a3fd2ef9c8134095b97d1db4897701e5d0ee1aeb161cc86681a3e339a5b5f01';
  assert.match(sha256, /^[0-9a-f]{64}$/);
  assert.equal(required, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.txflow;
  assert.ok(cap >= Math.max(parentFloor, required));
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'txflow'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'txflow'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'txflow'), /exceeds/);
});

// #3530 CI43669178292/job132677533254, source7c31d4ae7f57 (PR f04bd59c3bf2).
// Optimized web/Node 2,289,128 B; SHA256 de63f605ca567bf3441c5cf9f18b381c1f599414dde70cea3f3655cf33269b65.
// The measured composition and PR head share build-input fingerprint 4c6055d606b45c78e1a922ffee43c112260c14c1004db827a05b74c1c6d02f5d.
test('#3530 typed Kraken diagnostics retain measured reserve and historical alias floors', () => {
  const measured = 2_289_128;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const parentFloor = 2_287_616;
  for (const venue of ['kraken', 'krakenspot']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= Math.max(parentFloor, required), `${venue} retains the measured 10 KiB reserve`);
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

// GRVT scalar admission at 53f1649d9f2e09d8777127ea28eb53b4d13e75b3.
// build.sh --venue grvt; Rust1.99.0, bindgen0.2.129, Binaryen132 twice -Oz.
// Optimized web/Node SHA256 19b58267327bb19c7b508b01625e5b50423edd96e37efb96f8c30e3802f2c02b.
test('GRVT scalar admission retains its measured 10 KiB reserve and historical floor', () => {
  const measured = Math.max(2_043_354, 2_004_367);
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.grvt;
  assert.equal(requiredCap, 2_054_144);
  assert.ok(cap >= Math.max(requiredCap, 2_015_232));
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'grvt'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'grvt'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'grvt'), /exceeds/);
});

// #3576 refreshed CI43806604286/job133099107519 measures Lighter with hosted-join-proof.
// Source a86de005afb4, Rust1.99/bindgen0.2.129/Binaryen132, required twice-Oz.
test('#3576 Lighter admission dispatcher retains measured reserve and alias floors', () => {
  const measured = 2_227_410;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  for (const venue of ['lighter', 'lighter-robinhood']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= Math.max(2_198_528, required), `${venue}: retain measured reserve and historical floor`);
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

// #3579 source 873c3c199134737b5fd59b0aab5da073f1564fff; Rust1.99/bindgen0.2.129/Binaryen132 twice-Oz.
// Fresh web/Node 2,228,730 B, also reproduced byte-identically at parent 711dc03e942.
// SHA256 abd2e36ac525defc7673fc51d3ca7170d7d3f784589016c821c95de511b6f8f9.
test('#3579 merged Lighter family retains its current measured 10 KiB reserve', () => {
  const measured = 2_228_730;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(required, 2_239_488);
  for (const venue of ['lighter', 'lighter-robinhood']) {
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= Math.max(2_238_464, required), `${venue}: retain fresh reserve and parent floors`);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.lighter, WASM_VENUE_OVERRIDES['lighter-robinhood']);
});

// Fresh master 63f700d2 plus the ordinary BingX command codec gates.
// Rust1.99/bindgen0.2.129/Binaryen132, canonical build.sh --venue bingx, two -Oz passes.
test('BingX shared command codecs retain the measured optimized reserve', () => {
  const measured = 2_168_354; // Equal web/Node maxima; SHA256 pinned below.
  const sha256 = 'aeab98147da32f70379dbe9be6670fd523d92e292967e04bc9206483b5a6ab2e';
  assert.match(sha256, /^[0-9a-f]{64}$/);
  const minimum = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.bingx;
  assert.ok(cap >= 2_174_976, 'retain the previous BingX floor');
  assert.ok(cap >= minimum, `BingX codecs need ${minimum} bytes including reserve`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bingx'), /exceeds/);
});

// Rule 5 / #3640: remeasure all27 families after native allocation/chaser fixes.
// CI 44266062446 source 283acdd2f583259fa40113dc88f58b54adac610d, native inputs match c0113a933d.
// SAFE_CEX_WASM_OPT=required corepack yarn rust:build-wasm --venue FAMILY
// Rust1.99.0 / bindgen0.2.129 / Binaryen132; web/Node identical; two -Oz passes.
// Follow-up replaces bb6590f792 measurement provenance without amending earlier commits.
for (const [family, measured, required, sha256, aliases] of [
  [
    "apexomni",
    2220751,
    2231296,
    "dfb0fa2d787dc732c417206db14e03399e764d9e844e674edb00eab89a3a710e",
    [
      "apexomni"
    ]
  ],
  [
    "asterdex",
    2385027,
    2396160,
    "e6a1adab7ecb17c1b16b0bb17851c5b82e835cb7a2d3e1995fc00fb8fa57986b",
    [
      "asterdex"
    ]
  ],
  [
    "backpack",
    2193919,
    2204672,
    "ad6e6816230105ad7f2435df6b22dc5608606cc318103446fdb3463704792019",
    [
      "backpack"
    ]
  ],
  [
    "binance",
    2399175,
    2409472,
    "f79444b14168342f3462fded8fe412578394a2c973459e30d492caabe42078eb",
    [
      "binance"
    ]
  ],
  [
    "bingx",
    2270745,
    2281472,
    "ef586380c8b6bd8e20a3e2987a26a4bcd870a6ba5b88f8b2f800540686c9b6cd",
    [
      "bingx"
    ]
  ],
  [
    "bitget",
    2918561,
    2929664,
    "6ef022237a7baafe51b3e464a41bbedd96dc3e1f45fcb96cde38caadfab8a53d",
    [
      "bitget",
      "bitgetusdc"
    ]
  ],
  [
    "bitrue",
    2030087,
    2040832,
    "f3b2090b13b49cd15916715fb11eb187d2de992b9412bb9231c32fdc3462e9d5",
    [
      "bitrue"
    ]
  ],
  [
    "bitunix",
    2059916,
    2070528,
    "e7cb693567ce7dc6770d0d0dde0e770202580fb660d45849bf917078b7f69369",
    [
      "bitunix"
    ]
  ],
  [
    "blofin",
    2192734,
    2203648,
    "01cdb77227b07f8f0959ef5c1cf4bf4983e1227fa49184a44f3d4a5a78f36c49",
    [
      "blofin"
    ]
  ],
  [
    "coinbase",
    2466856,
    2478080,
    "7693e6784c74e9eb700c549b79b84c44db4f066502a48f41b27414063ce28ecf",
    [
      "coinbase"
    ]
  ],
  [
    "cryptocom",
    1935411,
    1946624,
    "e1246b54134ca1c1a5beb9469744a0a93fe0fdb66a4eb3ad218c463594e5ed0f",
    [
      "cryptocom"
    ]
  ],
  [
    "grvt",
    2160946,
    2171904,
    "a2590f52a24aed1b433b5375c632ff011b98c3f61a648923ceb25a79684ec294",
    [
      "grvt"
    ]
  ],
  [
    "hyperliquid",
    2428631,
    2439168,
    "aff893505edf698ccd1997148fa611524361b7437a58fd2810ff116d5bbdacdd",
    [
      "hyperliquid",
      "hyperliquidspot"
    ]
  ],
  [
    "kraken",
    2412386,
    2422784,
    "73e7014a9a87236911d331855a7d009b13f52e295b6cde8a655da61556f3d2ea",
    [
      "kraken",
      "krakenspot"
    ]
  ],
  [
    "kucoin",
    1986956,
    1997824,
    "858ee1d21f1e443f4813e21b37db68130e799698537722b4cbd18f0402b266f5",
    [
      "kucoin"
    ]
  ],
  [
    "kucoinspot",
    2669219,
    2679808,
    "8d4260591c80f6697cc783fdb58a6327273da3d59d4364207ebe3f0168307bc4",
    [
      "kucoinspot"
    ]
  ],
  [
    "lighter",
    2342879,
    2353152,
    "6a182aacc16009af4c4bee6864a8bbd600a3b1c78abca6096f90973bc6ab8cd3",
    [
      "lighter",
      "lighter-robinhood"
    ]
  ],
  [
    "mexc",
    2071600,
    2082816,
    "c15059b683588685e1ecabc96097db1178d9c094ea54175a7659b8c5d1527965",
    [
      "mexc"
    ]
  ],
  [
    "okx",
    2179779,
    2190336,
    "e1f3bd28dee0fa45690fc91885e827f3358855b2e15edc43011e4a3a9a2081a9",
    [
      "okx"
    ]
  ],
  [
    "orderly",
    1903948,
    1914880,
    "88bef4319c6e2c2ae3379a5bfe72cc5da91f4ed567fcb3c82b883e3e5220bdb8",
    [
      "orderly"
    ]
  ],
  [
    "paradex",
    2431053,
    2442240,
    "efcd0c03fed2fde15ab17a82b7c7fc98423561624a6183600a51a77986d03644",
    [
      "paradex"
    ]
  ],
  [
    "phemex",
    2242920,
    2253824,
    "ae359d4830fe1038d31920bc81946424bc4c1c2bd62644661ce4ff8eefae6362",
    [
      "phemex"
    ]
  ],
  [
    "polymarket",
    1786332,
    1797120,
    "b92937150ddece67d6026cc604c8ec945a0d521bbd21b1b7c3462e8c2d238e51",
    [
      "polymarket"
    ]
  ],
  [
    "txflow",
    2287526,
    2297856,
    "3456d8fa9d7865e00bcfa4197c4084bd9a21b0cb4bf9eb51957c59f96ffc2c10",
    [
      "txflow"
    ]
  ],
  [
    "weex",
    2150534,
    2161664,
    "90438d8fe568e089b91eb9ec9082a3d0b79e4bf71acf7ce3ebb79e2cc22d50f4",
    [
      "weex"
    ]
  ],
  [
    "woo",
    2873004,
    2883584,
    "6816124c90b7f4afdbbeabcadbf59a3e043f30aa9cfc9b2918b7b186a0428957",
    [
      "woo"
    ]
  ]
]) {
  test(`#3640 final native-store rule 17: ${family} reserve and aliases`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.equal(required, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    for (const alias of aliases) {
      assert.equal(venueFamily(alias), family);
      const cap = WASM_VENUE_OVERRIDES[alias];
      // This earlier composition remains a floor after later measured reserve refreshes.
      assert.ok(cap >= required, 'retain the original native-store measured floor');
      assert.equal(cap, WASM_VENUE_OVERRIDES[family]);
      assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', alias), undefined);
      assert.equal(wasmBudgetError(cap, 'venue', alias), undefined);
      assert.match(wasmBudgetError(cap + 1, 'venue', alias), /exceeds/);
    }
  });
}

test('#3640 family sweep preserves separately approved Bybit and combined ceilings', () => {
  assert.equal(WASM_VENUE_OVERRIDES.bybit, 2_696_192);
  assert.equal(WASM_BUDGETS.combined, 18_087_936);
});

// #3638 Rule 5: Rust SDK run 44220118208, artifact 14390662738.
// Exact source 06c5b32372b7eb047af0039b861e3b58dc8bc0cd; equal optimized web/Node/replay modules.
// Coordinator: measured + 10 KiB rounded up, preserving historical floors.
test('#3638 diagnostic composition retains the fresh MEXC reserve and boundary', () => {
  const measured = 1_962_854;
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.mexc;
  assert.equal(requiredCap, 1_973_248);
  assert.ok(cap >= Math.max(requiredCap, 1_971_200), `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'mexc'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'mexc'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'mexc'), /exceeds/);
});

// #3638 object envelopes: source c2455ca0e799, run 44255752536, artifact 14406113023.
// Five optimized web/Node/replay copies: 1,963,086 B; SHA256 b13413acd4d81fae3b088fb69ad69776d0ae0c513d657f49565f4399e2e67b0f.
// Rule 17: fresh measured + 10 KiB rounded up, retaining the earlier approved floor.
test('#3638 object-envelope guard retains the fresh MEXC reserve and boundaries', () => {
  const measured = 1_963_086;
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.mexc;
  assert.equal(requiredCap, 1_974_272);
  assert.ok(cap >= Math.max(requiredCap, 1_973_248), `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'mexc'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'mexc'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'mexc'), /exceeds/);
});

// #3639 Rule 5: Rust SDK run 44220127210, artifact 14390881543.
// Exact source 85a85576995b511750553a16337a8ea4d53dfcc5; equal optimized web/Node/replay modules.
// Coordinator: measured + 10 KiB rounded up, preserving historical floors.
test('#3639 diagnostic composition retains the fresh WOO reserve and boundary', () => {
  const measured = 2_683_302;
  const requiredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  const cap = WASM_VENUE_OVERRIDES.woo;
  assert.equal(requiredCap, 2_694_144);
  assert.ok(cap >= Math.max(requiredCap, 2_692_096), `${cap} leaves less than 10 KiB above ${measured}`);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'woo'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'woo'), /exceeds/);
});

// #3646 d10ba2f3c500, CI44135644635 jobs134225245837/134225246062: web/Node bytes.
// Rust1.99/bindgen0.2.129/Binaryen132 twice-Oz; shared family and replay budget.
// SHA256 26a96f1c6e97109a715a8c86a562f22619192492b97c5e75799d2c82c7be0b90.
for (const venue of ['bitget', 'bitgetusdc']) {
  test(`#3646 ${venue} login clock recovery retains measured reserve and strict boundaries`, () => {
    const measured = 2_782_976;
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(required, 2_793_472);
    assert.ok(cap >= required, 'retain the original clock-recovery measured floor');
    assert.ok(cap >= 2_781_184, 'retain the historical floor');
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
    assert.equal(venueFamily(venue), 'bitget');
    assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
  });
}

// #3646 M44 head7bd8f25, CI44224839389/job134535952644/artifact14393041086.
// All five web/Node/capsule modules: 2,784,464 B, SHA49aade25741062f3c2b963dcff1c3012fa565d52f67d1176971a638dfab1fe9a.
// A passing old absolute cap leaves only 9,008 B; fresh reserve is independently required.
for (const venue of ['bitget', 'bitgetusdc']) {
  test(`#3646 M44 ${venue} retains fresh composed reserve and strict boundaries`, () => {
    const measured = 2_784_464;
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.equal(required, 2_795_520);
    assert.ok(cap >= required, 'retain the M44 measured reserve alongside the native-store floor');
    assert.ok(cap >= 2_793_472, 'retain the approved pre-merge floor');
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  });
}

// Rule 5 / #3640: final allocation/chaser inputs; separately approved ceilings remain fixed.
// Bybit CI44266062446; combined local c0113a933d, optimized web/Node paired hashes agree.
test('#3640 final Bybit and combined fit approved ceilings', () => {
  assert.equal(wasmBudgetError(2670297, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(17866681, 'combined'), undefined);
});

// #3640 post-master rule 17: CI run 44302321301, PR head 7e9d5723fad5e8820242a18b205c068dd8eb851f.
// Built source 1fdd2db8279b2d4ab697720f2169cd0b6941a078 has the identical tree c2bf155c4e89dbcceb92c77982e18bd23824118d.
// All 27 wasm-family artifacts: Rust1.99.0 / bindgen0.2.129 / Binaryen132, required two -Oz passes.
// Sealed build.json source/run/input fingerprint and JS/package/module hashes verified; optimized web/Node bytes and hashes match.
// Each tuple pins family, measured bytes, parent cap, paired module SHA256, all routed aliases and the CI artifact ID.
const postMasterReserveMeasurements = [
  ['apexomni', 2_221_403, 2_231_296, '193bb6035b6b8ec007bc59426a3c8d041a9078544a141f942f71cfd9f1eaaa97', ['apexomni'], 14426802575],
  ['asterdex', 2_385_297, 2_396_160, '66ebbf601d8c04a3734d2a5e8d088fd1a83e38bb1a82bf6dcfa8a1547e97e135', ['asterdex'], 14425539944],
  ['backpack', 2_194_170, 2_204_672, '24cc7d61cb47a50dbc2bb7fa556f963c549a9a85b2b02b127c5c89cec059467a', ['backpack'], 14427485937],
  ['binance', 2_399_452, 2_409_472, '1b0d30cbaca03f7b322134826f216aabd1173dc04c021febbb14213a6caf0146', ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot'], 14425857970],
  ['bingx', 2_270_996, 2_281_472, '54c0e1759dd66c6d76032f3ce18c84d370b3ee5be1fcafc76d2f1ef287bf452b', ['bingx', 'bingxspot'], 14427600380],
  ['bitget', 2_918_770, 2_929_664, '2400a09e2d6dd51c3e3347531c273929fbae524ee34b336cc9904fe4ebc47a94', ['bitget', 'bitgetinverse', 'bitgetusdc'], 14426037766],
  ['bitrue', 2_030_338, 2_040_832, '86ede9f124e6e069737701c24da13bf2868209851f0487aacc11b7975d02f863', ['bitrue'], 14426777326],
  ['bitunix', 2_060_167, 2_070_528, '5cbfe1c812b152be9fc40242e95804d5069baac7ceb99c38205184fb0922ed9f', ['bitunix'], 14426103396],
  ['blofin', 2_192_985, 2_203_648, 'e4dfb89a75b3575807fd6825877435ec3fbd6cf20238f1878bff436c4dab8486', ['blofin'], 14425927809],
  ['bybit', 2_670_556, 2_696_192, '70cb1cae66bf86108ad153ec0b16420d5c46db04a9dc756316217652fa8e04cb', ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'], 14427266218],
  ['coinbase', 2_467_135, 2_478_080, '2bd68345e2bb7d0f1075ae6607d84af066607319e5f010dd973619668a92bc7e', ['coinbase', 'coinbasespot'], 14427027314],
  ['cryptocom', 1_935_686, 1_946_624, '67755d278a5d797e02803aa1abbf2fd4389b5b25ef20ca3834609ba13fcc38ca', ['cryptocom'], 14426732045],
  ['grvt', 2_161_229, 2_171_904, '36969f3edb0b3ea15dd0ec595590d62f20d6976b8be1b8e2ce6a290bdfcf5e56', ['grvt'], 14427455470],
  ['hyperliquid', 2_428_882, 2_439_168, 'c6ea5abfc697f796283241073552d13751164810cf38264d28ff072898681f66', ['hyperliquid', 'hyperliquidspot'], 14427207778],
  ['kraken', 2_412_637, 2_422_784, 'f30e01e9c371b3d226937806f76f3edb0e6dc5c9e3cc42ff500fd08fa0e1b804', ['kraken', 'krakenspot'], 14427176717],
  ['kucoin', 1_987_229, 1_997_824, '99372e6eb8201352dd7eaad8ccba0c02ce8c55a6dcff88731459aafedbf8d5d1', ['kucoin'], 14427171520],
  ['kucoinspot', 2_669_453, 2_679_808, '8b61e04e3dfc65e5ec76d41aabe64de593cf7be4ceff0dc6e55bc94fbb0117f3', ['kucoinspot'], 14427760047],
  ['lighter', 2_343_161, 2_353_152, '62df49b7977246384829fa61c649af35886b647bd6b15dad6bfa4bd17500f63b', ['lighter', 'lighter-robinhood'], 14426338781],
  ['mexc', 2_071_869, 2_082_816, '360adfcc45a1041d38f9d5f33344bec72d7e24a814474d2e59f85a7167538008', ['mexc'], 14427017973],
  ['okx', 2_180_034, 2_190_336, 'ce7fb2dfc16cbc0d1fef22330a1df3d4bcee7cd5bd21966002ab7b8024ef15a7', ['okx', 'okxdemo', 'okxeu', 'okxeudemo'], 14426628131],
  ['orderly', 1_904_199, 1_914_880, 'bffe4b93f606b1947b5bee66ffe0e4be0531f6b423630bf8b008c69e98ba9e78', ['orderly', 'kodiakfi', 'mode', 'what', 'woofi'], 14427530436],
  ['paradex', 2_431_304, 2_442_240, '245be56095673b49375ab47c5a8ce0d2cb59ce4fdc5f4584e7ea489158a7844d', ['paradex'], 14426877553],
  ['phemex', 2_243_190, 2_253_824, '4b2a7b5afab6d2c20ec9a49cfc83f23626c5f65a4d341cb044bd28a15a873bee', ['phemex'], 14426463443],
  ['polymarket', 1_786_592, 1_797_120, 'b86fea8262408335339cc44331924c7ad7a33d3ea2eb189087bee53643db9cc3', ['polymarket'], 14425908933],
  ['txflow', 2_287_781, 2_297_856, '46995b6b4097494d4e38c59b9878b702cdd8ba447bb82c458b1af8ab1f23b66c', ['txflow'], 14426828486],
  ['weex', 2_150_775, 2_161_664, '775aec55ab7962fb28d7a4557a08eb32618f3223f90a4cb06a19c5e387dbd036', ['weex'], 14425818798],
  ['woo', 2_875_093, 2_883_584, '45358e66399d0a26b69c0a4ca86d62cd023beaac12ca53986743d0f25177ca61', ['woo'], 14427037266],
];

for (const [family, measured, parentCap, sha256, aliases, artifact] of postMasterReserveMeasurements) {
  test(`#3640 post-master rule 17: ${family} complete reserve and alias audit`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.ok(Number.isSafeInteger(artifact) && artifact > 0);
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[family];
    // Historical composition stays a floor; the complete M46 census below pins the exact current cap.
    assert.ok(cap >= Math.max(parentCap, required), 'retain every earlier measured reserve and parent floor');
    assert.ok(cap - measured >= 10 * 1024, `${family}: ${cap - measured} bytes of reserve`);
    for (const alias of aliases) {
      assert.equal(venueFamily(alias), family);
      if (Object.hasOwn(WASM_VENUE_OVERRIDES, alias)) {
        assert.equal(WASM_VENUE_OVERRIDES[alias], cap);
        assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', alias), undefined);
        assert.equal(wasmBudgetError(cap, 'venue', alias), undefined);
        assert.match(wasmBudgetError(cap + 1, 'venue', alias), /exceeds/);
      }
    }
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

test('#3640 post-master reserve census covers every family and budget alias', () => {
  assert.deepEqual(postMasterReserveMeasurements.map(([family]) => family).sort(), [...venueFamilies].sort());
  const aliases = postMasterReserveMeasurements.flatMap(([, , , , names]) => names);
  assert.equal(aliases.length, 49, 'all routed names at the pinned CI source');
  assert.equal(aliases.length, new Set(aliases).size);
  const budgetAliases = aliases.filter((alias) => Object.hasOwn(WASM_VENUE_OVERRIDES, alias)).sort();
  assert.deepEqual(budgetAliases, Object.keys(WASM_VENUE_OVERRIDES).sort());
});

// CI 44302321301 combined artifact 14427602030, source/tree as above; optimized web/Node modules match.
// SHA256 b7693720dffd5c3878c09ea239e9949532c43d110e4883080aa07b476ad482a3, measured 17,869,568 B.
test('#3640 post-master combined build retains its measured reserve and parent ceiling', () => {
  const measured = 17_869_568;
  const parentCap = 18_087_936;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(WASM_BUDGETS.combined, Math.max(parentCap, required));
  assert.equal(WASM_BUDGETS.combined - measured, 218_368);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'combined'), undefined);
  assert.equal(wasmBudgetError(WASM_BUDGETS.combined, 'combined'), undefined);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined'), /exceeds/);
});

// #3640 vetoable M46 rule 17: complete CI run 44319661954, head d931514bb9750d82e2f538ed454b0d24edd0db5f.
// Built source de42fedbb4021e94d1162e4420b8583d0d20906a has identical tree 96034a1dc92a991b43673ee6023cd915815b42fe.
// All 31 measurement jobs completed successfully before this census; all 27 sealed family artifacts verified.
// Rust1.99.0 / bindgen0.2.129 / Binaryen132, required two -Oz passes; optimized web/Node hashes match.
// Tuples: family, measured bytes, parent cap, paired module SHA256, all routed aliases, artifact ID.
const m46ReserveMeasurements = [
  ['apexomni', 2_221_499, 2_232_320, '10b21908c4228d0eeb8da7859b3ca2d837715531c6aa0acce9c0af576f911b88', ['apexomni'], 14434253983],
  ['asterdex', 2_385_366, 2_396_160, '15131a05b946f46cfa0869bbd99b1d96de28295dc97c546222287079b2228602', ['asterdex'], 14434307727],
  ['backpack', 2_194_243, 2_204_672, 'e4ac1ad98e2718829dbee4b7429d3b1b21478a5b4e3594250f1a51dbfe43e9eb', ['backpack'], 14434746254],
  ['binance', 2_399_548, 2_410_496, '8f65a33e378620a67ae85e629a5e0f69d7f780b49ddb2e1ee9ac4eb9fc080c9d', ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot'], 14435345088],
  ['bingx', 2_280_540, 2_281_472, '43b6deab11cf21d7566a4f257fc13e849d294d41b98e1a3e30e461c5205021e9', ['bingx', 'bingxspot'], 14434571947],
  ['bitget', 2_918_911, 2_929_664, '239158cbf1ca4b1d776832a02338759f13447e5015149d4d59a940e4f3ebc38a', ['bitget', 'bitgetinverse', 'bitgetusdc'], 14435500072],
  ['bitrue', 2_030_425, 2_040_832, '7d3a567671e51508d0577947d5eb9999fd941fb2f56dbc333be42026dd2fd6aa', ['bitrue'], 14435400462],
  ['bitunix', 2_060_267, 2_070_528, 'c1bf013f44d3fb0d1ea98d44592a92ae3448890fdef959c5b765ee3e37d3b051', ['bitunix'], 14434103149],
  ['blofin', 2_193_054, 2_203_648, '6f053daba3682ad2375ce7a48b08636fd4802e9df7892c2e991e4b8f8d5e755a', ['blofin'], 14433119064],
  ['bybit', 2_670_652, 2_696_192, '76fbaf355e73bf6b5292f784b5479a9f2a7ab5d9988c88ebc3700d02d30301dc', ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'], 14434357021],
  ['coinbase', 2_467_231, 2_478_080, '5df8c70d8ba2597306f3264e82e032ad0c541738e160f464201b80aeb52c4598', ['coinbase', 'coinbasespot'], 14435110703],
  ['cryptocom', 1_935_754, 1_946_624, '5295ac8606d45645e0e34980f60e993e5906ae089c25990c1fdff3efba4e7dce', ['cryptocom'], 14433549166],
  ['grvt', 2_161_325, 2_171_904, '7ab9c27e4f5416ed0230d6a12167ca4f2c0b3b3d6b438132edbed8c7ce5b4819', ['grvt'], 14435005799],
  ['hyperliquid', 2_428_978, 2_439_168, 'f242e0ade5f65c8cf9fd93bd0f211b02668d1f69f381221b1452a1d6fabf53ff', ['hyperliquid', 'hyperliquidspot'], 14435086069],
  ['kraken', 2_412_733, 2_423_808, '6cafd9893661512013b26bb853f170caeda6f93b4149f92a0660a342ebc61adc', ['kraken', 'krakenspot'], 14435035761],
  ['kucoin', 1_987_325, 1_997_824, 'd5e348710e013476abe057e7b935688e71016c090371aff560991247db93e886', ['kucoin'], 14435081529],
  ['kucoinspot', 2_669_540, 2_679_808, '93e27408a62c1de0476a6b90b4f5ae20d690996485e7f4702602eda6a80950c6', ['kucoinspot'], 14434612501],
  ['lighter', 2_343_151, 2_354_176, 'acda5b0d152fdb44552fa4500f3920cfc22b4100d438fdec05a1bdbeafc54069', ['lighter', 'lighter-robinhood'], 14434940914],
  ['mexc', 2_074_423, 2_082_816, '0c71123e5adb6b43bc1b70488dd43674e758935f6ab404826f5aa22419ba5bcf', ['mexc'], 14433734049],
  ['okx', 2_180_126, 2_190_336, 'c32226e5e573aebd0beca89567131ce7ce719369a730f7cfa0e5c763525a34fa', ['okx', 'okxdemo', 'okxeu', 'okxeudemo'], 14435011270],
  ['orderly', 1_904_286, 1_914_880, 'e7f1a44d85c2ec939a33e1705e71999e6850dfdaa963d5918c5e6d682f2f3e1b', ['orderly', 'kodiakfi', 'mode', 'what', 'woofi'], 14433868603],
  ['paradex', 2_431_400, 2_442_240, '892d6da913e90c29ddf6c9138d9c9486c682bec36a846183199a69b2d366201f', ['paradex'], 14435400501],
  ['phemex', 2_243_263, 2_253_824, '6a138d96809233b17c89f263ec26c3fcb78e92e191028877b9a5de350f12b7d7', ['phemex'], 14435275349],
  ['polymarket', 1_786_678, 1_797_120, '5e8e6545f4969be065ff5eefaa2183c38c099c300e9a180652ed1d6ebede2ef8', ['polymarket'], 14434626467],
  ['txflow', 2_287_868, 2_298_880, 'ef49fce8d93fff542a4b2a0b15105d75e6587d209fc6db9f7bdc663950e77a32', ['txflow'], 14435150538],
  ['weex', 2_150_848, 2_161_664, '0f9b9cf33f198eb13dc44eb883a2f70cb36e752076b297cb0f29c5d254e94833', ['weex'], 14434032692],
  ['woo', 2_875_192, 2_885_632, '9595296a78bd8053db61b6ab9899ab2a2a4d5fc64e8da389eee34c7940451e5a', ['woo'], 14435596190],
];

for (const [family, measured, parentCap, sha256, aliases, artifact] of m46ReserveMeasurements) {
  test(`#3640 M46 rule 17: ${family} complete reserve and alias audit`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.ok(Number.isSafeInteger(artifact) && artifact > 0);
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[family];
    // Historical composition stays a floor; the complete M48 census below pins the exact current cap.
    assert.ok(cap >= Math.max(parentCap, required), 'retain every earlier measured reserve and parent floor');
    assert.ok(cap - measured >= 10 * 1024, `${family}: ${cap - measured} bytes of reserve`);
    for (const alias of aliases) {
      assert.equal(venueFamily(alias), family);
      // Routed aliases consume their parent family; explicit budget aliases must share its cap.
      if (Object.hasOwn(WASM_VENUE_OVERRIDES, alias)) {
        assert.equal(WASM_VENUE_OVERRIDES[alias], cap);
        assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', alias), undefined);
        assert.equal(wasmBudgetError(cap, 'venue', alias), undefined);
        assert.match(wasmBudgetError(cap + 1, 'venue', alias), /exceeds/);
      }
    }
    assert.equal(wasmBudgetError(measured + 10 * 1024, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

test('#3640 M46 reserve census covers every family and budget alias', () => {
  assert.deepEqual(m46ReserveMeasurements.map(([family]) => family).sort(), [...venueFamilies].sort());
  const aliases = m46ReserveMeasurements.flatMap(([, , , , names]) => names);
  assert.equal(aliases.length, 49, 'all routed names at the pinned CI source');
  assert.equal(aliases.length, new Set(aliases).size);
  const budgetAliases = aliases.filter((alias) => Object.hasOwn(WASM_VENUE_OVERRIDES, alias)).sort();
  assert.deepEqual(budgetAliases, Object.keys(WASM_VENUE_OVERRIDES).sort());
});

// CI44319661954 combined artifact 14435287450; same source/tree as above.
// Optimized web/Node 17,876,808 B, SHA256 12320b94f86e8c4561366588bc255edc0d1b438fdcd3c8989d9e22714cc44cee.
test('#3640 M46 combined build retains its measured reserve and parent ceiling', () => {
  const measured = 17_876_808;
  const parentCap = 18_087_936;
  const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(WASM_BUDGETS.combined, Math.max(parentCap, required));
  assert.equal(WASM_BUDGETS.combined - measured, 211_128);
  assert.equal(wasmBudgetError(measured + 10 * 1024, 'combined'), undefined);
  assert.equal(wasmBudgetError(WASM_BUDGETS.combined, 'combined'), undefined);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined'), /exceeds/);
});

// #3640 vetoable M48 rule 17: complete CI run 44389135230, head d123c9b38c4559ab38b5df8142981c7fdcc008b5.
// Built source 42e792c614d768737151090feb06c4da4558855a (GitHub merge of master 04a2f14e5b) has the identical tree; all 27 family artifacts verified.
// Tuples: family, measured bytes, parent cap, paired module SHA256, all routed aliases, artifact ID.
const m48ReserveMeasurements = [
  ['apexomni', 2_221_419, 2_232_320, 'ef17b39c836a3ec30ca7d3cfcd96fe324b8b4cc9b58d4a4277824a727294aa91', ['apexomni'], 14468375144],
  ['asterdex', 2_385_133, 2_396_160, '541bddd20f80d03653e79c1fadb200bdeec0698d00a909ac3b7b319c6b257958', ['asterdex'], 14466874389],
  ['backpack', 2_194_252, 2_204_672, '9e4b86a0f71fe0f213ee2db58a6796e3a678a06aac921a96b289d35f106eb91e', ['backpack'], 14467900321],
  ['binance', 2_399_396, 2_410_496, 'd6837f05f2f802fabfb6540ae1f442f62da4f934e67a1f299b0879330fa122d6', ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot'], 14467173971],
  ['bingx', 2_280_457, 2_291_712, '272f2ae883b996e884840be09f0d55fcdba756b49e1892112470ce4023d991c4', ['bingx', 'bingxspot'], 14467099060],
  ['bitget', 2_919_109, 2_929_664, '4b4a6a08d1dce0a40edd21b32cd59e59f59c4d4bbbec232f94b68ef09fac8ecb', ['bitget', 'bitgetinverse', 'bitgetusdc'], 14468855063],
  ['bitrue', 2_030_311, 2_040_832, '6a6f2007b2e78457d33e36db142b57c892282d3fb12157ec5a56548ca6f76314', ['bitrue'], 14467318646],
  ['bitunix', 2_060_309, 2_070_528, '259d7f7c42217927e3b78aa3d5dbe73a58ea575a8eeb4614a66223eec463a07a', ['bitunix'], 14467079162],
  ['blofin', 2_193_059, 2_203_648, 'b6858d2b82fc868e589b5e80455ddf4f443bb1b9ac9065f1b1a133d52aed6846', ['blofin'], 14467990385],
  ['bybit', 2_676_941, 2_696_192, 'e0131d879a3ab7055fd3415e898c95ab4c2ef88a83f19ce6e7e49165b9c00785', ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'], 14467398587],
  ['coinbase', 2_467_165, 2_478_080, '4bc75a578c84de8e311878e198cb9c03627cefc3d13d7acf80710f0001f59690', ['coinbase', 'coinbasespot'], 14467820449],
  ['cryptocom', 1_935_691, 1_946_624, '2179663ac7ec334f11dd737a18e3e6004a974ec9547a84a440648d4f9ec4588f', ['cryptocom'], 14468805079],
  ['grvt', 2_161_326, 2_171_904, 'ff82bd79aa884e7bd2c761ba5e1f7f6207c29060bb2885457fc5d0110f9d075d', ['grvt'], 14468015358],
  ['hyperliquid', 2_428_911, 2_440_192, '1cf92064c945d5addbe45b8d3e7d7e8a0938a4bdb841e95c4d1dbc7919caa818', ['hyperliquid', 'hyperliquidspot'], 14467550679],
  ['kraken', 2_412_529, 2_423_808, '6babfbee8410ed263527da399bb25c94dea98b834f030f7500881b2db1f59465', ['kraken', 'krakenspot'], 14468655220],
  ['kucoin', 1_987_327, 1_997_824, '312cfa5c38a46076c1a4c085456e7f261d46673e785090a2969e2672d52914bb', ['kucoin'], 14467670522],
  ['kucoinspot', 2_669_596, 2_679_808, 'b34c4c82215750a4fc7031143cac366ed5d7cfc82399ddc96ab13deab4f520eb', ['kucoinspot'], 14468325467],
  ['lighter', 2_343_190, 2_354_176, 'b45e012230d54875a5e7183a64e40a5ca1e9b4ca01579c9c34cd646e7391d442', ['lighter', 'lighter-robinhood'], 14466919402],
  ['mexc', 2_074_432, 2_084_864, '21fe260572fc9a6efa99f898a9bd380fb50ae6d17e203d1f01ff85bed6857a41', ['mexc'], 14466764890],
  ['okx', 2_181_378, 2_191_360, '5754641f60a4094992f8fc052990303ebbfc4188421a7938655fe5ff3e4a504a', ['okx', 'okxdemo', 'okxeu', 'okxeudemo'], 14467433385],
  ['orderly', 1_904_311, 1_914_880, '74922b4fcb3dee097143f7f22be5a868f654753972442f2ada6deeba087d8f4c', ['orderly', 'kodiakfi', 'mode', 'what', 'woofi'], 14468310213],
  ['paradex', 2_431_392, 2_442_240, 'ee4714b2e2bd71a89d24f139d5c2405af49238936542a8609f17e7535c52ad6d', ['paradex'], 14466804576],
  ['phemex', 2_243_042, 2_253_824, '95fd0cb08905951516047c40d41ab947b99b130a68f0d7e4b44fa47fe7a191bb', ['phemex'], 14466814441],
  ['polymarket', 1_786_673, 1_797_120, 'fbf879c883e9dfd8c8956c92ebdc0dfbcde371ea9e5d47f51d098b678a83a677', ['polymarket'], 14467655373],
  ['txflow', 2_287_899, 2_298_880, 'fb3f1df78c15cda35023c16db94dcdd78416e9800cfe41ddefff58924f7fcabc', ['txflow'], 14468515168],
  ['weex', 2_150_790, 2_161_664, 'b6007d2d6f23d3063812c2fef30a0a045605090c66abab1ffa052e28d9129c60', ['weex'], 14468055458],
  ['woo', 2_875_139, 2_885_632, '3fc66068aa915daa0fba4d680d20da7193ee570e7e56592799b4045c0e80af48', ['woo'], 14468885150],
];

for (const [family, measured, parentCap, sha256, aliases, artifact] of m48ReserveMeasurements) {
  test(`#3640 M48 rule 17: ${family} complete reserve and alias audit`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.ok(Number.isSafeInteger(artifact) && artifact > 0);
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[family];
    // The post-#2301 merge census below supersedes Bitunix; preserve this historical floor.
    if (family === 'bitunix') assert.ok(cap >= Math.max(parentCap, required));
    else assert.equal(cap, Math.max(parentCap, required), 'raise only to measured reserve; never lower a parent cap');
    assert.ok(cap - measured >= 10 * 1024, `${family}: ${cap - measured} bytes of reserve`);
    for (const alias of aliases) {
      assert.equal(venueFamily(alias), family);
      if (Object.hasOwn(WASM_VENUE_OVERRIDES, alias)) assert.equal(WASM_VENUE_OVERRIDES[alias], cap);
    }
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}

test('#3640 M48 reserve census covers every family', () => {
  assert.deepEqual(m48ReserveMeasurements.map(([family]) => family).sort(), [...venueFamilies].sort());
});

// CI44389135230 combined package: optimized web/Node 17,880,947 B, SHA256 07d9c9d787961a7cafa778e2104edd1804385e05532d46184d72b0e2e2571f15.
test('#3640 M48 combined build keeps at least 10 KiB under its ceiling', () => {
  assert.ok(WASM_BUDGETS.combined - 17_880_947 >= 10 * 1024);
});

// #2301 post-#3640 merged source69e878301580a8fe5ad4ce1907fb04cec066bcc5.
// Built source d4767ecfa103f692fa8cfe1d518395b49cca6e32 has identical tree.
// 26 sealed families from PR44440728793; Bitunix paired raw modules from diagnostic44441640567.
// All optimized web/Node hashes, pinned tools, source/run and input fingerprint verified.
// Tuples: family, measured bytes, parent cap, paired module SHA256, artifact ID.
const bitunixMergedReserveCensus = [
  ['apexomni', 2_221_419, 2_232_320, '6d26d5920aa3bc61ee4892bb33eee347848c533a29f01cc520952756e34cdaa4', 14490184157],
  ['asterdex', 2_385_133, 2_396_160, 'f94e49992f271a29bac17aa5d68b52c23e641486bc8c79e9ac288101c21f9f55', 14490983611],
  ['backpack', 2_194_252, 2_204_672, '218b3ba1862faa71ac65834484f8f7958fc6076af1ab7cd79fc2250ebc3a46f8', 14490129156],
  ['binance', 2_399_396, 2_410_496, '8535270f88b84bf63b30e31a8111d795d1365962ae21c597cb620d31bf7110d0', 14490358538],
  ['bingx', 2_280_457, 2_291_712, '1365cc6e766ba19248a51829302b8e8a110dd85f8902b07edb78cff175e451eb', 14491183034],
  ['bitget', 2_919_109, 2_929_664, '8a1501c8377f2a2a6c9fd8e9b9b96660ba783a9f3e981a5440d4ff7119929710', 14490004270],
  ['bitrue', 2_030_311, 2_040_832, '86bdf5607c669819bb13e917cb2b3f6d7dc4efd82e8ca9517d5c189a65e47d90', 14491700122],
  ['bitunix', 2_118_250, 2_071_552, '6b5270d93a83bd84da5d5e6a5cfc0e5508c019633555a43c83a2240d5bfd1c55', 14491204173],
  ['blofin', 2_193_059, 2_203_648, '3fce5028d857578ac806996fc4e5e16b183faa0c74293832941e2131e13cd712', 14490753464],
  ['bybit', 2_676_941, 2_696_192, 'e1648a5827d06365f688047ec47644434886421b55ddcff53ae663daef331c2c', 14490573327],
  ['coinbase', 2_467_165, 2_478_080, '348a23dd42d99a9910097f8063dabebea3ed564c0c26e42ed36b8added63f91d', 14491327264],
  ['cryptocom', 1_935_691, 1_946_624, '08a2a40340ba30cf6fe39e1b6f01c5f677fe9b5b5dbad4e8345216dc8ea40a31', 14490294946],
  ['grvt', 2_161_326, 2_171_904, 'd6bda6dd5263f2c282dc578bf40032af0216b8a3405270ec04a6fe17024242b6', 14491342993],
  ['hyperliquid', 2_428_911, 2_440_192, '88b594ed66796ea0985de75440b790957cd0d842e169720c753a26562296f2ca', 14490289675],
  ['kraken', 2_412_529, 2_423_808, '540ab10c67f81d023da4453bfd6b1001801842316c88bcd09afc0abc877dc0bf', 14490952693],
  ['kucoin', 1_987_327, 1_997_824, '1203b6df6a0ca6356f221bf9c771bf9e7ba4b1253946554bc687523abada9dd0', 14490698794],
  ['kucoinspot', 2_669_596, 2_680_832, 'b32a0b854c520ecec4935145db367bc15e6328e1ae8a2bc4656471827039a821', 14491455989],
  ['lighter', 2_343_190, 2_354_176, 'c6604e487fedf9f86f587cfc659fb6345459ef52aff36475493c03f776124ae6', 14490469680],
  ['mexc', 2_074_432, 2_084_864, '2f221db134f985fb2c6ca1b86cd33204820d906476a86a7b0caee2d9f8f7ea85', 14490618623],
  ['okx', 2_181_378, 2_192_384, 'df813420e5511cd7faf515be4ff1f83e121b0970538ed272d38ad3c6260892b8', 14491143347],
  ['orderly', 1_904_311, 1_914_880, '9c47aa9423e1ff253de3741dcefe50a386ac577f5cbb0ba232a13a947677c635', 14490084363],
  ['paradex', 2_431_392, 2_442_240, '4454ef22d3474eb62f0c66abe696a4ab5c864637b3c5813bb2e89f8275564cb3', 14491068779],
  ['phemex', 2_243_042, 2_253_824, 'f4880972673d744f277380c0312085d56f0a2680b6067ad40077d1e21bfd228b', 14490404480],
  ['polymarket', 1_786_673, 1_797_120, '459b9b331cfedd8b1c4aeb404ce180d04c5413cc9328cbd1c54c998cb1ee5c96', 14490233843],
  ['txflow', 2_287_899, 2_298_880, 'a894425761b63fbc4fec799b8831ff8d48cc05dd392fc62dd4199ab0d3b82800', 14491501172],
  ['weex', 2_150_790, 2_161_664, 'c84af2827932aed0ccb72ed40db4e97ec8c3ab82d1bed7639c7fb1a859bf8eab', 14490604498],
  ['woo', 2_875_139, 2_885_632, 'b11c1d41b25d0f8bce9d33a4e1d6ed38bad6a8f25c54a5b18a2e4ca7efe05bc1', 14490673781],
];
for (const [family, measured, parentCap, sha256, artifact] of bitunixMergedReserveCensus) {
  test(`#2301 merged rule 17: ${family} measured reserve and retained floors`, () => {
    assert.match(sha256, /^[0-9a-f]{64}$/);
    assert.ok(Number.isSafeInteger(artifact) && artifact > 0);
    const required = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
    const cap = WASM_VENUE_OVERRIDES[family];
    // The scoped review fix supersedes Bitunix's measurement; retain this prior floor.
    if (family === 'bitunix') assert.ok(cap >= Math.max(parentCap, required));
    else assert.equal(cap, Math.max(parentCap, required));
    assert.ok(cap - measured >= 10 * 1024);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  });
}
test('#2301 merged reserve census covers every canonical family', () => {
  assert.deepEqual(bitunixMergedReserveCensus.map(([family]) => family), [...venueFamilies].sort());
});

// #2301 review fix05427522bc0a27bb656345e9b4cb25e31693b294; PR44444577344.
// Identical-tree built merge0cd4327943dfbac4c4aa60ce2589c52f8f6fb816, artifact14492118639.
// Optimized web/Node/replay SHA256 e13e8e2e27327fe7c9e51d034f6c8ee8d315c906d331571a03af3226864c6baf.
test('#2301 direct-intent and frame-fed fix retains fresh Bitunix reserve', () => {
  const measured = 2_119_780;
  const previousFloor = 2_128_896;
  const cap = WASM_VENUE_OVERRIDES.bitunix;
  assert.equal(cap, Math.max(previousFloor, Math.ceil((measured + 10 * 1024) / 1024) * 1024));
  assert.ok(cap - measured >= 10 * 1024);
  assert.equal(wasmBudgetError(cap, 'venue', 'bitunix'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bitunix'), /exceeds/);
});
