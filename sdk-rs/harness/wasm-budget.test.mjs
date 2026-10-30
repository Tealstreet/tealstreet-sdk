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

test('only the declared venue families carry overrides, and only for the venue profile', () => {
  assert.deepEqual(Object.keys(WASM_VENUE_OVERRIDES).sort(), ['asterdex', 'binance', 'bitget', 'bitgetusdc', 'bybit', 'hyperliquid', 'hyperliquidspot', 'kraken', 'krakenspot', 'paradex']);
  // Cap sweep 10-30: measured 1,461,044 B; ceil KiB + 2 KiB.
  assert.equal(WASM_VENUE_OVERRIDES.asterdex, 1_429 * 1_024);
  assert.equal(wasmBudgetError(1_385_869, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_386_496, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_386_952, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_387_520, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_387_670, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_388_544, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_418_240, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_418_676, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_419_317, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_420_288, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_419_038, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_419_264, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_420_478, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_421_312, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_422_195, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_422_336, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_423_639, 'venue', 'asterdex'), undefined);
  // Cutover 10-29: release run 40863850081 measured 1,434,289 B.
  assert.equal(wasmBudgetError(1_434_289, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_436_672, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_453_174, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_456_128, 'venue', 'asterdex'), undefined);
  assert.equal(wasmBudgetError(1_463_296, 'venue', 'asterdex'), undefined);
  assert.match(wasmBudgetError(1_463_297, 'venue', 'asterdex'), /exceeds 1463296/);
  for (const venue of ['bitget', 'bitgetusdc']) {
    // Frame-fed Bitget (COSIGNED writer): 1,522,315 -> 1,549,604 B (+27,289); cap +27 KiB +2 KiB.
    // Settings holds: core 1,549,604 -> 1,558,459, Bitget -> 1,568,357 (+18,753); cap +19 KiB +2 KiB.
    assert.equal(WASM_VENUE_OVERRIDES[venue], 1539 * 1024);
    assert.ok(WASM_VENUE_OVERRIDES[venue] <= 1539 * 1024);
    assert.equal(wasmBudgetError(1_481_804, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_482_752, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_482_944, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_483_776, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_483_869, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_484_800, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_485_945, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_486_848, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_487_624, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_488_071, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_488_896, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_522_283, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_554_432, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_575_936, 'venue', venue), undefined);
    assert.match(wasmBudgetError(1_575_937, 'venue', venue), /exceeds 1575936/);
  }
  for (const venue of ['kraken', 'krakenspot']) {
    assert.equal(WASM_VENUE_OVERRIDES[venue], 1417 * 1024);
    assert.equal(wasmBudgetError(1_399_777, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_399_808, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_399_905, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_400_974, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_401_856, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_402_082, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_404_713, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_404_928, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_405_315, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_405_952, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_407_904, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_408_000, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_408_110, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_409_024, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_409_432, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_410_048, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_410_076, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_411_072, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_448_429, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_451_008, 'venue', venue), undefined);
    assert.match(wasmBudgetError(1_451_009, 'venue', venue), /exceeds 1451008/);
  }
  for (const venue of ['hyperliquid', 'hyperliquidspot']) {
    // Cutover 10-29: release run 40859840322 measured 1,366,837 B; 1344 KiB leaves room for #2362.
    // Frame-fed 10-30: ingest plus the settings guard measured 1,377,242 B; ceil KiB + 2 KiB.
    // Setting holds 10-30: measured 1,386,637 B (master 1,385,259 B); ceil KiB + 2 KiB.
    assert.equal(WASM_VENUE_OVERRIDES[venue], 1357 * 1024);
    assert.equal(wasmBudgetError(1_363_789, 'venue', venue), undefined);
    assert.equal(wasmBudgetError(1_389_568, 'venue', venue), undefined);
    assert.match(wasmBudgetError(1_389_569, 'venue', venue), /exceeds 1389568/);
  }
  assert.equal(WASM_VENUE_OVERRIDES.bybit, 1448 * 1024);
  assert.equal(wasmBudgetError(1_446_912, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_446_915, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_447_936, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_448_312, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_448_960, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_452_004, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_452_032, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_452_035, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_471_488, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_479_891, 'venue', 'bybit'), undefined);
  assert.equal(wasmBudgetError(1_482_752, 'venue', 'bybit'), undefined);
  assert.match(wasmBudgetError(1_482_753, 'venue', 'bybit'), /exceeds 1482752/);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined', 'bybit'), /exceeds 16777216/);
  assert.match(wasmBudgetError(WASM_BUDGETS.venue + 1, 'venue', 'okx'), /exceeds 1343488/);
  assert.match(wasmBudgetError(WASM_BUDGETS.venue + 1, 'venue'), /exceeds 1343488/);
  assert.match(wasmBudgetError(WASM_BUDGETS.combined + 1, 'combined', 'bitget'), /exceeds 16777216/);
});

test('Paradex keyless remote signing carries its ruled family budget', () => {
  // #2367: local optimized Paradex 1,482,511 bytes (device descriptor parser, PublicKey
  // setup, hosted-wallet); ddi ruled ceil KiB plus 4 KiB for local/CI drift, no CI wait.
  // Frame-fed VENUE_STATE (native order rows for amend) measured 1,527,413 B; ceil KiB + 2 KiB.
  assert.equal(WASM_VENUE_OVERRIDES.paradex, 1494 * 1024);
  assert.equal(wasmBudgetError(1_482_511, 'venue', 'paradex'), undefined);
  assert.equal(wasmBudgetError(1_486_848, 'venue', 'paradex'), undefined);
  assert.equal(wasmBudgetError(1_515_321, 'venue', 'paradex'), undefined);
  assert.equal(wasmBudgetError(1_517_568, 'venue', 'paradex'), undefined);
  assert.equal(wasmBudgetError(1_529_856, 'venue', 'paradex'), undefined);
  assert.match(wasmBudgetError(1_529_857, 'venue', 'paradex'), /exceeds 1529856/);
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


test('Binance Options cutover carries the ruled shared-family ceiling', () => {
  // Cap sweep 10-30: measured 1,492,477 B; Binance frame-fed then measured 1,501,037 B.
  assert.equal(WASM_VENUE_OVERRIDES.binance, 1468 * 1024);
  assert.equal(wasmBudgetError(1_448_609, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_460_760, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_463_296, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_484_450, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_486_848, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_492_477, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_501_037, 'venue', 'binance'), undefined);
  assert.equal(wasmBudgetError(1_503_232, 'venue', 'binance'), undefined);
  assert.match(wasmBudgetError(1_503_232 + 1, 'venue', 'binance'), /exceeds 1503232/);
  assert.equal(wasmBudgetError(WASM_BUDGETS.combined, 'combined', 'binance'), undefined);
});
