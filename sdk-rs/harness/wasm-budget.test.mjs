import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { venueFamily, venueFamilies } from '../hosts/wasm/js/venues.mjs';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

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
  assert.equal(WASM_BUDGETS.combined, 17 * 1024 * 1024);
  for (const profile of Object.keys(WASM_BUDGETS)) {
    assert.equal(wasmBudgetError(WASM_BUDGETS[profile], profile), undefined);
    assert.match(wasmBudgetError(WASM_BUDGETS[profile] + 1, profile), /exceeds/);
  }
  for (const value of [0, -1, Infinity, NaN]) assert.match(wasmBudgetError(value, 'combined'), /invalid/);
});





test('the approved combined ceiling accepts 17 MiB exactly and rejects the next byte', () => {
  for (const venue of [undefined, ...Object.keys(WASM_VENUE_OVERRIDES)]) {
    assert.equal(wasmBudgetError(16_778_652, 'combined', venue), undefined, '#3140 measured size');
    assert.equal(wasmBudgetError(17_825_791, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(17_825_792, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(17_825_793, 'combined', venue), 'combined wasm 17825793 exceeds 17825792 bytes');
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
