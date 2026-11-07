import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

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
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.hyperliquid, WASM_VENUE_OVERRIDES.hyperliquidspot);
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3148 protective edit live fixes (run 42196761441) retain headroom and siblings match', () => {
  const measured = 2_348_153;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_359_296);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
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
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
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
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
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
  for (const [venue, measured, minimumCap] of [
    ['polymarket', 1_590_031, 1_600_512],
  ]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
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
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
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
  const [measured, minimumCap] = [2_195_793, 2_206_720];
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.binance;
  assert.ok(cap >= minimumCap);
  assert.equal(WASM_VENUE_OVERRIDES.binance, cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'binance'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'binance'), /exceeds/);
});

test('#3187 PM algo migration (run 42251340763) retains headroom for every Binance alias', () => {
  const [measured, cap] = [2_212_299, 2_223_104];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['binance', 'binanceinverse', 'binancepm', 'binanceoptions', 'binancespot']) {
    const family = venueFamily(venue);
    assert.equal(family, 'binance');
    assert.equal(WASM_VENUE_OVERRIDES[family], cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

test('#3073 cancel confirmation (run 41931095253) retains headroom and siblings match', () => {
  const measured = 2_334_811;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_345_984);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bitget, WASM_VENUE_OVERRIDES.bitgetusdc);
});

test('#3094 batch acknowledgements (run 42062090938) retain Bybit headroom', () => {
  const measured = 2_214_254;
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(wasmBudgetError(measured, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bybit'), /exceeds/);
});

test('#3168 guarded Partial edits (run 42227488436) retain measured headroom for every Bybit alias', () => {
  const measured = 2_246_685;
  const minimumCap = 2_257_920;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bybit;
  assert.ok(cap >= minimumCap);
  assert.equal(WASM_VENUE_OVERRIDES.bybit, cap);
  for (const venue of ['bybit', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader']) {
    const family = venueFamily(venue);
    assert.equal(family, 'bybit');
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

test('#3106 critical reads (run 42097859473) retain measured headroom and equal HL siblings', () => {
  const measured = 2_108_435;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_119_680);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3158 Lighter guarded edits (run 42215250423, job 127884302751) retain headroom and siblings match', () => {
  const measured = 2_068_697;
  const minimumCap = 2_079_744;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.lighter;
  assert.ok(cap >= minimumCap);
  for (const venue of ['lighter', 'lighter-robinhood']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(2_034_142, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3118 Aster protection guard (run 42112075305) retains measured headroom', () => {
  const [venue, measured, minimumCap] = ['asterdex', 2_139_837, 2_150_400];
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.asterdex;
  assert.ok(cap >= minimumCap);
  assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});

test('#3114 cold-path sharing (run 42104530272) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_106_596;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_117_632);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3143 public data watch (run 42181736999) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_126_281;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_137_088);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3149 activeAssetData scoping (run 42197163575) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_143_008;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_153_472);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3150 final CI 42258955401 cap refresh: #3145 trades as last (run 42218930349 job 127896386942) retains Hyperliquid headroom and siblings match', () => {
  const measured = 2_166_092;
  const minimumCap = 2_177_024;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= minimumCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
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

test('#3169 MEXC managed protective edits (run 42228563954) retain measured headroom', () => {
  const [venue, measured, cap] = ['mexc', 1_848_283, 1_858_560];
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
  assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
});

test('#3128 ApeX guarded amends (run 42146728975) retain ApeX headroom', () => {
  for (const [venue, measured, minimumCap] of [['apexomni', 2_002_359, 2_013_184]]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3130 Backpack guarded trigger amends (run 42154583609) retain Backpack headroom', () => {
  for (const [venue, measured, minimumCap] of [['backpack', 1_926_668, 1_937_408]]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3138 Bitrue guarded stop edits (run 42166643605) retain Bitrue headroom', () => {
  for (const [venue, measured, minimumCap] of [['bitrue', 1_829_700, 1_840_128]]) {
    assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    const cap = WASM_VENUE_OVERRIDES[venue];
    assert.ok(cap >= minimumCap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('BreakoutProp native protective edits retain measured headroom and the exact boundary', () => {
  const measured = 1_554_817;
  const cap = 1_565_696;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES.breakoutprop, cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'breakoutprop'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'breakoutprop'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'breakoutprop'), /exceeds/);
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
    assert.equal(WASM_VENUE_OVERRIDES[family], cap);
    assert.equal(wasmBudgetError(measured, 'venue', family), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
  }
});

test('#3152 BingX protective edits (run 42200843544) retain measured headroom and the exact boundary', () => {
  const measured = 2_063_923;
  const minimumCap = 2_074_624;
  assert.equal(minimumCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.bingx;
  assert.ok(cap >= minimumCap);
  assert.equal(venueFamily('bingx'), 'bingx');
  assert.equal(WASM_VENUE_OVERRIDES.bingx, cap);
  assert.equal(wasmBudgetError(measured, 'venue', 'bingx'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'bingx'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'bingx'), /exceeds/);
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
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3178 intent cleanup (run 42239151280, job 127965614325) retains HL headroom and equal siblings', () => {
  const measured = 2_154_756;
  const measuredCap = 2_165_760;
  assert.equal(measuredCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  const cap = WASM_VENUE_OVERRIDES.hyperliquid;
  assert.ok(cap >= measuredCap);
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3188 startup causes (run 42260192686 at cda38ed52a9) retain ruled headroom and exact boundaries', () => {
  for (const [venue, measured, incomingCap, cap] of [
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
    assert.equal(cap, Math.max(incomingCap, Math.ceil((measured + 10 * 1024) / 1024) * 1024));
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
  for (const [family, sibling] of [['hyperliquid', 'hyperliquidspot'], ['lighter', 'lighter-robinhood']]) {
    const cap = WASM_VENUE_OVERRIDES[family];
    assert.equal(WASM_VENUE_OVERRIDES[sibling], cap);
    assert.equal(wasmBudgetError(cap, 'venue', sibling), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', sibling), /exceeds/);
  }
  assert.equal(wasmBudgetError(16_830_742, 'combined'), undefined);
});

test('#3150 merged-tree caps retain the approved exact boundaries', () => {
  for (const [venue, cap] of [
    ['apexomni', 2_024_448],
    ['backpack', 1_948_672],
    ['bingx', 2_085_888],
    ['bitrue', 1_851_392],
    ['blofin', 2_001_920],
    ['coinbase', 1_838_080],
    ['okx', 1_986_560],
    ['weex', 1_948_672],
  ]) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3207 telemetry (run 42279265759) uses CI plus 10 KiB and keeps Bitget siblings equal', () => {
  const measured = 2_390_477;
  const measuredCap = Math.ceil((measured + 10 * 1024) / 1024) * 1024;
  assert.equal(measuredCap, 2_401_280);
  const cap = WASM_VENUE_OVERRIDES.bitget;
  assert.ok(cap >= measuredCap);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('#3202 on #3207 (run 42305638406) uses CI plus 10 KiB and keeps Bitget siblings equal', () => {
  const measured = 2_409_448;
  const cap = 2_419_712;
  assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});
