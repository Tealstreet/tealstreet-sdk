import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

// Recorded CI caps are provenance floors; later PRs may raise the live cap.
// Boundary checks always exercise the live cap and the byte immediately above it.

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
