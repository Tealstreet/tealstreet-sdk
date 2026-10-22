// Every family package built by `yarn rust:build-wasm --families` loads in a
// fresh web initialization, constructs each of its venues, refuses every
// other family's venues and fits the single-venue size budget.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { venueFamilies, venueFamily } from '../hosts/wasm/js/venues.mjs';
import { wasmBudgetError } from './wasm-budget.mjs';
import { probeCapabilityCommand, requireCapabilityEngine } from './capability-conformance.mjs';

const pkg = new URL('../hosts/wasm/pkg/', import.meta.url);
const venues = readFileSync(new URL('../hosts/wasm/js/venues.mjs', import.meta.url), 'utf8')
  .match(/^\s+'?[a-z-]+'?: '[a-z]+',$/gm)
  .map((line) => line.trim().split(':')[0].replaceAll("'", ''));

test('the venue list is read from the family map', () => {
  assert.equal(venues.length, 50);
  for (const venue of venues) assert.doesNotThrow(() => venueFamily(venue), venue);
});

for (const family of venueFamilies) {
  test(`${family} family package`, async () => {
    for (const target of ['web', 'node']) {
      const size = statSync(new URL(`${target}/${family}/safe_cex_wasm_bg.wasm`, pkg)).size;
      // One source of truth for the single-venue budget: wasm-budget.mjs.
      const error = wasmBudgetError(size, 'venue');
      assert.equal(error, undefined, `${target} ${family}: ${error}`);
    }
    const engine = await import(new URL(`web/${family}/safe_cex_wasm.js`, pkg));
    await engine.default({ module_or_path: readFileSync(new URL(`web/${family}/safe_cex_wasm_bg.wasm`, pkg)) });
    for (const venue of venues) {
      let error;
      let instance;
      try {
        instance = new engine.WasmEngine(venue, JSON.stringify({ publicDataOnly: true }));
      } catch (thrown) {
        error = thrown;
      }
      try {
        if (venueFamily(venue) === family) {
          requireCapabilityEngine(() => { if (error) throw error; return instance; }, `${venue} is missing from ${family}`);
          if (instance && JSON.parse(instance.capabilities()).orderBookQuery) {
            // All-feature native tests can hide a missing standalone JSON decoder.
            // This is sans-I/O: returned requests are never dispatched.
            probeCapabilityCommand(instance, {
              op: 'fetchOrderBook', command: 1, symbol: '__rest_book_ingress_probe__', limit: 1,
            }, `${venue} advertises a REST book query but its family decoder rejects it`);
          }
          if (instance && JSON.parse(instance.capabilities()).scheduleCancel) {
            // Public-only construction refuses signing after decoding; no transport runs.
            probeCapabilityCommand(instance, {
              op: 'scheduleCancel', command: 2, time: 5_000,
            }, `${venue} advertises scheduleCancel but its family decoder rejects it`);
          }
        } else assert.match(String(error?.message), /unsupported venue/, `${family} also contains ${venue}`);
      } finally {
        instance?.free();
      }
    }
  });
}
