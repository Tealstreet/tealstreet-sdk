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
  apexomni: 1_876_992,
  asterdex: 2_010_112,
  backpack: 1_790_976,
  binance: 2_052_096,
  bingx: 1_876_992,
  bitget: 2_148_352,
  bitgetusdc: 2_148_352,
  bitrue: 1_729_536,
  bitunix: 1_737_728,
  blofin: 1_835_008,
  breakoutprop: 1_430_528,
  bybit: 2_042_880,
  coinbase: 1_730_560,
  cryptocom: 1_623_040,
  grvt: 1_696_768,
  hyperliquid: 1_974_272,
  hyperliquidspot: 1_974_272,
  kraken: 2_013_184,
  krakenspot: 2_013_184,
  kucoin: 1_504_256,
  kucoinspot: 1_971_200,
  lighter: 1_874_944,
  'lighter-robinhood': 1_874_944,
  mexc: 1_644_544,
  okx: 1_807_360,
  orderly: 1_642_496,
  paradex: 2_112_512,
  phemex: 1_911_808,
  polymarket: 1_519_616,
  txflow: 1_746_944,
  weex: 1_835_008,
  woo: 2_240_512,
};
  assert.deepEqual(Object.keys(WASM_VENUE_OVERRIDES).sort(), Object.keys(caps).sort());
  for (const [venue, cap] of Object.entries(caps)) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], cap, venue);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined, venue);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), new RegExp(`exceeds ${cap}`), venue);
  }
});
