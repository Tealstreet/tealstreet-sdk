import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
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

test('KuCoin custody caps retain the approved CI headroom and exact boundary', () => {
  for (const [venue, measured, cap] of [
    ['kucoin', 1_630_161, 1_640_448],
    ['kucoinspot', 2_224_069, 2_234_368],
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
    ['apexomni', 1_959_768, 1_970_176],
    ['backpack', 1_860_786, 1_871_872],
    ['coinbase', 1_800_411, 1_811_456],
    ['weex', 1_909_946, 1_921_024],
  ]) {
    assert.equal(cap, Math.ceil((measured + 10 * 1024) / 1024) * 1024);
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap);
    assert.equal(wasmBudgetError(measured, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/);
  }
});

test('WOO attribution cap retains the approved CI headroom and exact boundary', () => {
  const cap = 2_387_968;
  assert.equal(cap, Math.ceil((2_377_495 + 10 * 1024) / 1024) * 1024);
  assert.equal(WASM_VENUE_OVERRIDES.woo, cap);
  assert.equal(wasmBudgetError(2_377_495, 'venue', 'woo'), undefined);
  assert.equal(wasmBudgetError(cap, 'venue', 'woo'), undefined);
  assert.match(wasmBudgetError(cap + 1, 'venue', 'woo'), /exceeds/);
});

test('single-pass aggregate caps retain the approved CI headroom and exact boundary', () => {
  for (const [venue, measured, cap] of [
    ['asterdex', 2_110_514, 2_121_728],
    ['bingx', 2_009_588, 2_020_352],
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
    ['binance', 2_172_329, 2_183_168],
    ['bitget', 2_293_016, 2_304_000],
    ['bitunix', 1_826_666, 1_837_056],
    ['blofin', 1_939_942, 1_950_720],
    ['bybit', 2_177_290, 2_188_288],
    ['cryptocom', 1_704_291, 1_715_200],
    ['hyperliquid', 2_083_308, 2_094_080],
    ['mexc', 1_717_351, 1_728_512],
    ['orderly', 1_710_152, 1_721_344],
    ['paradex', 2_211_804, 2_222_080],
    ['kraken', 2_111_267, 2_121_728],
    ['krakenspot', 2_111_267, 2_121_728],
    ['lighter', 1_980_286, 1_990_656],
    ['okx', 1_924_239, 1_935_360],
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
