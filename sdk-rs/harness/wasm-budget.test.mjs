import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { WASM_BUDGETS, WASM_VENUE_OVERRIDES, wasmBudgetError } from './wasm-budget.mjs';

test('combined shipping build and standalone builds retain distinct fixed ceilings', () => {
  assert.equal(WASM_BUDGETS.venue, 1_343_488);
  assert.equal(WASM_BUDGETS.combined, 14 * 1024 * 1024);
  for (const profile of Object.keys(WASM_BUDGETS)) {
    assert.equal(wasmBudgetError(WASM_BUDGETS[profile], profile), undefined);
    assert.match(wasmBudgetError(WASM_BUDGETS[profile] + 1, profile), /exceeds/);
  }
  for (const value of [0, -1, Infinity, NaN]) assert.match(wasmBudgetError(value, 'combined'), /invalid/);
});

test('only the declared venue families carry overrides, and only for the venue profile', () => {
  assert.deepEqual(Object.keys(WASM_VENUE_OVERRIDES).sort(), ['asterdex', 'bitget', 'bitgetusdc', 'bybit', 'kraken', 'krakenspot']);
  assert.equal(WASM_VENUE_OVERRIDES.asterdex, 1_353 * 1_024);
  assert.equal(wasmBudgetError(1_385_472, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(1_385_473, 'venue', 'asterdex'), /exceeds 1385472/);
  for (const venue of ['bitget', 'bitgetusdc']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], 1446 * 1024);
    assert.ok(WASM_VENUE_OVERRIDES[venue] <= 1448 * 1024);
    assert.equal(wasmBudgetError(1_480_704, 'venue', venue), undefined);
    assert.match(wasmBudgetError(1_480_705, 'venue', venue), /exceeds 1480704/);
  }
  for (const venue of ['kraken', 'krakenspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], 1367 * 1024);
    assert.equal(wasmBudgetError(1_399_777, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_399_808, 'venue', venue), undefined);
    assert.match(wasmBudgetError(1_399_809, 'venue', venue), /exceeds 1399808/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bybit, 1_343_488 + 102_400);
  assert.equal(wasmBudgetError(1_445_888, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(1_445_889, 'venue', 'bybit'), /exceeds 1445888/);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined', 'bybit'), /exceeds 14680064/);
  assert.match(wasmBudgetError(WASM_BUDGETS.venue + 1, 'venue', 'okx'), /exceeds 1343488/);
  assert.match(wasmBudgetError(WASM_BUDGETS.venue + 1, 'venue'), /exceeds 1343488/);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined', 'bitget'), /exceeds 14680064/);
});

test('the approved combined ceiling accepts 14 MiB exactly and rejects the next byte', () => {
  for (const venue of [undefined, ...Object.keys(WASM_VENUE_OVERRIDES)]) {
    assert.equal(wasmBudgetError(14_680_063, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(14_680_064, 'combined', venue), undefined);
    assert.equal(wasmBudgetError(14_680_065, 'combined', venue), 'combined wasm 14680065 exceeds 14680064 bytes');
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
