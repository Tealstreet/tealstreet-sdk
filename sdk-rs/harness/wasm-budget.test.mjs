import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { venueFamily, venueFamilies } from '../hosts/wasm/js/venues.mjs';
import { readWasmCaps } from './artifact-census.mjs';
import { serializeWasmCaps } from './update-wasm-caps.mjs';
import {
  nextWasmCap, reserveTargetCap, WASM_BUDGETS, WASM_CAPS_PATH, WASM_FAMILY_CAPS, WASM_RESERVE_BYTES,
  wasmBudgetBytes, wasmBudgetError, wasmReserveWarning,
} from './wasm-budget.mjs';

// AGT-23 (generated caps) replaced the per-round census tests that pinned each PR's
// measured bytes, SHA256 and exact cap (#2219 through #3640 M48, the M46/M48 tuples).
// Those tuples and the provenance comments now live in git history and
// wasm-caps-history.md; the bot recomputes caps from fresh master measurements, so
// what remains pinned here is the mechanism and the cutover floor below.

const KiB = 1024;
const venueNames = readFileSync(new URL('../hosts/wasm/js/venues.mjs', import.meta.url), 'utf8')
  .match(/^\s+'?[a-z-]+'?: '[a-z]+',$/gm)
  .map((line) => line.trim().split(':')[0].replaceAll("'", ''));

// Every cap at the cutover (master d68c0f8d2053, 2026-11-19). Each earlier census
// floor is at or below these, and caps are never lowered, so this one floor retains
// every historical value. Aliases inherit their family's floor.
const CUTOVER_FLOORS = Object.freeze({
  apexomni: 2_232_320, asterdex: 2_396_160, backpack: 2_204_672, binance: 2_410_496,
  bingx: 2_291_712, bitget: 2_929_664, bitrue: 2_040_832, bitunix: 2_071_552,
  blofin: 2_203_648, bybit: 2_696_192, coinbase: 2_478_080, cryptocom: 1_946_624,
  grvt: 2_171_904, hyperliquid: 2_440_192, kraken: 2_423_808, kucoin: 1_997_824,
  kucoinspot: 2_680_832, lighter: 2_354_176, mexc: 2_084_864, okx: 2_192_384,
  orderly: 1_914_880, paradex: 2_442_240, phemex: 2_253_824, polymarket: 1_797_120,
  txflow: 2_298_880, weex: 2_161_664, woo: 2_885_632,
});
const COMBINED_CUTOVER_FLOOR = 18_087_936;

test('the generated cap file is canonical, schema 1, and keeps the 10 KiB reserve target', () => {
  const source = readFileSync(WASM_CAPS_PATH, 'utf8');
  const caps = readWasmCaps(source);
  assert.equal(serializeWasmCaps(caps), source, 'wasm-caps.json is only ever written by serializeWasmCaps');
  assert.equal(caps.schema, 1);
  assert.equal(WASM_RESERVE_BYTES, 10 * KiB);
  assert.deepEqual(WASM_FAMILY_CAPS, caps.families);
  assert.equal(WASM_BUDGETS.combined, caps.combined);
});

test('every family has exactly one cap and every venue alias resolves to it', () => {
  assert.deepEqual(Object.keys(WASM_FAMILY_CAPS).sort(), [...venueFamilies].sort());
  assert.ok(venueNames.length > venueFamilies.length, 'the alias inventory was read');
  for (const venue of venueNames) {
    const cap = WASM_FAMILY_CAPS[venueFamily(venue)];
    assert.equal(wasmBudgetBytes('venue', venue), cap, venue);
    assert.equal(wasmBudgetError(cap, 'venue', venue), undefined, venue);
    assert.match(wasmBudgetError(cap + 1, 'venue', venue), /exceeds/, venue);
  }
  assert.equal(wasmBudgetBytes('venue', 'unlistedvenue'), WASM_BUDGETS.venue);
  assert.equal(wasmBudgetBytes('venue'), WASM_BUDGETS.venue);
});

test('generated caps never fall below the cutover (historical) floors', () => {
  assert.deepEqual(Object.keys(CUTOVER_FLOORS).sort(), [...venueFamilies].sort());
  for (const [family, floor] of Object.entries(CUTOVER_FLOORS)) assert.ok(WASM_FAMILY_CAPS[family] >= floor, family);
  assert.ok(WASM_BUDGETS.combined >= COMBINED_CUTOVER_FLOOR);
});

test('rule 17 math: measured + 10 KiB, rounded up to the KiB', () => {
  assert.equal(reserveTargetCap(1), 11 * KiB);
  assert.equal(reserveTargetCap(KiB), 11 * KiB);
  assert.equal(reserveTargetCap(KiB + 1), 12 * KiB);
  // #3640 M48 measurements reproduce the caps that were hand-committed for them.
  assert.equal(reserveTargetCap(2_181_378), 2_192_384);
  assert.equal(reserveTargetCap(2_060_309), 2_071_552);
  assert.equal(reserveTargetCap(2_669_596), 2_680_832);
  for (const measured of [1, 1023, 1024, 1025, 2_345_678]) {
    const cap = reserveTargetCap(measured);
    assert.equal(cap % KiB, 0);
    assert.ok(cap - measured >= 10 * KiB && cap - measured < 11 * KiB);
  }
  for (const invalid of [0, -1, 1.5, NaN, Infinity]) assert.throws(() => reserveTargetCap(invalid), /invalid measured/);
});

test('a generated cap is never lowered', () => {
  assert.equal(nextWasmCap(2_929_664, 1_000_000), 2_929_664);
  assert.equal(nextWasmCap(2_048 * KiB, 2_048 * KiB - 10 * KiB), 2_048 * KiB);
  assert.equal(nextWasmCap(2_048 * KiB, 2_048 * KiB - 10 * KiB + 1), 2_049 * KiB);
  for (const [family, cap] of Object.entries(WASM_FAMILY_CAPS)) assert.equal(nextWasmCap(cap, cap - 10 * KiB), cap, family);
});

test('the PR gate enforces only the byte ceiling; a reserve shortfall is advisory', () => {
  for (const [family, cap] of Object.entries(WASM_FAMILY_CAPS)) {
    assert.equal(wasmBudgetError(cap, 'venue', family), undefined, 'inclusive cap');
    assert.equal(wasmBudgetError(cap - 1, 'venue', family), undefined, 'shortfall does not fail');
    assert.match(wasmBudgetError(cap + 1, 'venue', family), /exceeds/);
    assert.match(wasmReserveWarning(cap - 1, 'venue', family), new RegExp(`${family} family wasm .* short of the 10240 B rule-17 reserve`));
    assert.equal(wasmReserveWarning(cap - 10 * KiB, 'venue', family), undefined, 'exact reserve');
    assert.equal(wasmReserveWarning(cap + 1, 'venue', family), undefined, 'over cap is an error, not a warning');
  }
  assert.match(wasmReserveWarning(WASM_BUDGETS.combined, 'combined'), /^combined wasm/);
});

test('combined shipping build and standalone builds retain distinct ceilings', () => {
  assert.equal(WASM_BUDGETS.venue, 1_343_488);
  for (const profile of Object.keys(WASM_BUDGETS)) {
    assert.equal(wasmBudgetError(WASM_BUDGETS[profile], profile), undefined);
    assert.match(wasmBudgetError(WASM_BUDGETS[profile] + 1, profile), /exceeds/);
  }
  for (const value of [0, -1, Infinity, NaN]) assert.match(wasmBudgetError(value, 'combined'), /invalid/);
  assert.match(wasmBudgetError(1, 'unknownprofile'), /invalid/);
});

test('CI budgets each standalone venue build under its own venue name', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const step = workflow.indexOf('name: Check family isolation, every alias and the shipped size');
  assert.ok(step >= 0);
  assert.match(workflow.slice(step, step + 220), /RUST_TEST_FAMILY: \$\{\{ matrix\.family \}\}/);
  assert.match(workflow.slice(step, step + 220), /rust:test-families/);
  const familyCheck = readFileSync(new URL('./families.integration.mjs', import.meta.url), 'utf8');
  assert.match(familyCheck, /wasmBudgetError\(size, 'venue', family\)/);
  assert.match(familyCheck, /wasmReserveWarning\(size, 'venue', family\)/);
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
