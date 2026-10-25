import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

test('combined shipping build and standalone builds retain distinct fixed ceilings', () => {
  assert.equal(WASM_BUDGETS.venue, 1_343_488);
  assert.equal(WASM_BUDGETS.combined, 13.5 * 1024 * 1024);
  for (const profile of Object.keys(WASM_BUDGETS)) {
    assert.equal(wasmBudgetError(WASM_BUDGETS[profile], profile), undefined);
    assert.match(wasmBudgetError(WASM_BUDGETS[profile] + 1, profile), /exceeds/);
  }
  for (const value of [0, -1, Infinity, NaN]) assert.match(wasmBudgetError(value, 'combined'), /invalid/);
});

test('only Bitget, Bybit and AsterDEX carry venue allowances, and only for the venue profile', () => {
  assert.deepEqual(Object.keys(WASM_VENUE_OVERRIDES).sort(), ['asterdex', 'bitget', 'bitgetusdc', 'bybit']);
  assert.equal(WASM_VENUE_OVERRIDES.asterdex, 1_343_488 + 114_688);
  assert.equal(wasmBudgetError(1_458_176, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(1_458_177, 'venue', 'asterdex'), /exceeds 1458176/);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], 1_343_488 + 135_168);
    assert.equal(wasmBudgetError(1_478_656, 'venue', venue), undefined);
    assert.match(wasmBudgetError(1_478_657, 'venue', venue), /exceeds 1478656/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bybit, 1_343_488 + 61_440);
  assert.equal(wasmBudgetError(1_404_928, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(1_404_929, 'venue', 'bybit'), /exceeds 1404928/);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined', 'bybit'), /exceeds 14155776/);
  assert.match(wasmBudgetError(WASM_BUDGETS.venue + 1, 'venue', 'okx'), /exceeds 1343488/);
  assert.match(wasmBudgetError(WASM_BUDGETS.venue + 1, 'venue'), /exceeds 1343488/);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined', 'bitget'), /exceeds 14155776/);
});

test('CI budgets each standalone venue build under its own venue name', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const step = workflow.indexOf('name: Check the wasm size budget');
  assert.ok(step >= 0);
  assert.match(workflow.slice(step, step + 200), /rust:wasm-budget --venue '\$\{\{ matrix\.venue \}\}'/);
});

test('CI budgets the combined artifact immediately after its build before standalone overwrite', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const combined = workflow.indexOf('name: Build the combined wasm engine');
  const budget = workflow.indexOf('name: Check the shipped combined wasm size budget');
  const standalone = workflow.indexOf('name: Build the venue wasm engine');
  assert.ok(combined >= 0 && budget > combined && standalone > budget);
  assert.match(workflow.slice(budget, standalone), /rust:wasm-budget --combined/);
});

test('the Bybit capability comparison budgets ordinary Bybit under its own venue name', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const step = workflow.indexOf('name: Build and measure both capabilities from this checkout');
  const compare = workflow.indexOf('name: Compare measurements and require ordinary Bybit budget');
  assert.ok(step >= 0 && compare > step);
  assert.match(workflow.slice(step, compare), /wasmBudgetError\(bytes\.length, 'venue', 'bybit'\)/);
  assert.match(workflow.slice(step, compare), /budgetBytes: wasmBudgetBytes\('venue', 'bybit'\)/);
});
