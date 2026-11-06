import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

test('combined shipping build and standalone builds retain distinct fixed ceilings', () => {
  assert.equal(WASM_BUDGETS.venue, 1_343_488);
  assert.equal(WASM_BUDGETS.combined, 16 * 1024 * 1024);
  for (const profile of Object.keys(WASM_BUDGETS)) {
    assert.equal(wasmBudgetError(WASM_BUDGETS[profile], profile), undefined);
    assert.match(wasmBudgetError(WASM_BUDGETS[profile] + 1, profile), /exceeds/);
  }
  for (const value of [0, -1, Infinity, NaN]) assert.match(wasmBudgetError(value, 'combined'), /invalid/);
});





test('the approved combined ceiling accepts 16 MiB exactly and rejects the next byte', () => {
  for (const venue of [undefined, ...Object.keys(WASM_VENUE_OVERRIDES)]) {
    assert.equal(wasmBudgetError(16_777_215, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(16_777_216, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(16_777_217, 'combined', venue), 'combined wasm 16777217 exceeds 16777216 bytes');
  }
});

test('shared broker-role setup caps retain CI headroom and reject the next byte', () => {
  for (const [venue, measured, cap] of [
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('KuCoin 400002 resync caps retain the approved CI headroom and exact boundary', () => {
  for (const [venue, measured, cap] of [
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('master 41783084277 caps retain the ruled headroom and shared-family boundary', () => {
  for (const [venue, measured, cap] of [
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
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
    breakoutprop: 1_420_288,
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
});

test('read-cover batch caps (cut 41796230942 at 3b9f6b0fe2b) retain headroom and siblings match', () => {
  for (const [venue, measured, cap] of [
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
  assert.equal(WASM_VENUE_OVERRIDES.lighter, WASM_VENUE_OVERRIDES['lighter-robinhood']);
});

test('refresh-ask caps (PR run 41804469904 at 466b16cd11e) retain headroom and siblings match', () => {
  for (const [venue, measured, cap] of [
    ['breakoutprop', 1_501_277, 1_512_448],
    ['coinbase', 1_816_299, 1_826_816],
    ['txflow', 1_827_125, 1_838_080],
    ['weex', 1_925_507, 1_936_384],
    ['woo', 2_388_474, 2_399_232],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3073 cancel confirmation (run 41931095253) retains headroom and siblings match', () => {
  const measured = 2_334_811;
  const cap = 2_345_984;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3018 after merging 78bc1ae5a39 (run 41870743418 at c9d4de158d5) retains headroom and siblings match', () => {
  for (const [venue, measured, cap] of [
    ['kraken', 2_125_098, 2_136_064],
    ['krakenspot', 2_125_098, 2_136_064],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3018 protective-amend caps (run 41854021043 at 824ff543b6d) retain the ruled headroom', () => {
  for (const [venue, measured, cap] of [
    ['polymarket', 1_590_031, 1_600_512],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('refresh-ask caps after #3039 (PR run 41845929983 at 9bf12da7671) retain headroom and siblings match', () => {
  for (const [venue, measured, cap] of [
    ['bitunix', 1_838_517, 1_849_344],
    ['cryptocom', 1_716_657, 1_727_488],
    ['kucoinspot', 2_253_260, 2_264_064],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.lighter, WASM_VENUE_OVERRIDES['lighter-robinhood']);
});

test('Phemex read-cover caps (PR run 41868980693 at d8899b7a3e0) retain headroom and siblings match', () => {
  for (const [venue, measured, cap] of [
    ['phemex', 2_054_401, 2_065_408],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.kraken, WASM_VENUE_OVERRIDES.krakenspot);
});

test('#3018 after merging d171fbb0e31 (run 41893192741 at b94d76fa368) retains the ruled headroom', () => {
  for (const [venue, measured, cap] of [
    ['orderly', 1_721_849, 1_732_608],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('master b0d2122742d release cut (run 41901638294) retains the ruled binance headroom', () => {
  const [measured, cap] = [2_195_793, 2_206_720];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES.binance, cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'binance'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'binance'), /exceeds/);
});

test('#3094 batch acknowledgements (run 42062090938) retain Bybit headroom', () => {
  const measured = 2_214_254;
  const cap = 2_225_152;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES.bybit, cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

test('#2314 Lighter managed sockets (job 127434185656) retain headroom and siblings match', () => {
  const measured = 2_034_142;
  const cap = 2_044_928;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES.lighter, cap);
  assert.equal(WASM_VENUE_OVERRIDES['lighter-robinhood'], cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'lighter'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'lighter'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'lighter'), /exceeds/);
});

test('#3118 Aster protection guard (run 42112075305) retains measured headroom', () => {
  const [venue, measured, cap] = ['asterdex', 2_139_837, 2_150_400];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});

test('#3143 public data watch (run 42181736999) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_126_281;
  const cap = 2_137_088;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3122 protective stop edits (run 42126229513) retain Paradex and BloFin headroom', () => {
  for (const [venue, measured, cap] of [
    ['paradex', 2_239_351, 2_249_728],
    ['blofin', 1_979_025, 1_989_632],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});


test('#3129 guarded GRVT edits (run 42145552491) retain measured headroom', () => {
  const [venue, measured, cap] = ['grvt', 1_806_590, 1_817_600];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});

test('#3127 KuCoin guarded stop edits (run 42140111861) retain KuCoin headroom', () => {
  for (const [venue, measured, cap] of [['kucoin', 1_681_540, 1_692_672]]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3131 MEXC protective edits (run 42150228180) retain measured headroom', () => {
  const [venue, measured, cap] = ['mexc', 1_777_946, 1_788_928];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});

test('#3128 ApeX guarded amends (run 42146728975) retain ApeX headroom', () => {
  for (const [venue, measured, cap] of [['apexomni', 2_002_359, 2_013_184]]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3130 Backpack guarded trigger amends (run 42154583609) retain Backpack headroom', () => {
  for (const [venue, measured, cap] of [['backpack', 1_926_668, 1_937_408]]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3138 Bitrue guarded stop edits (run 42166643605) retain Bitrue headroom', () => {
  for (const [venue, measured, cap] of [['bitrue', 1_829_700, 1_840_128]]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3121 protective amendments (run 42122802638) retain OKX headroom and all siblings share the cap', () => {
  const measured = 1_962_129;
  const cap = 1_973_248;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['okx', 'okxeu', 'okxdemo', 'okxeudemo']) {
    const family = venueFamily(venue);
    assert.equal(family, 'okx');
    assert.equal(WASM_VENUE_OVERRIDES[family], cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

test('#3152 BingX protective edits (run 42200843544) retain measured headroom and the exact boundary', () => {
  const measured = 2_063_923;
  const cap = 2_074_624;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(venueFamily('bingx'), 'bingx');
  assert.equal(WASM_VENUE_OVERRIDES.bingx, cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bingx'), /exceeds/);
});
