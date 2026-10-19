import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';

const wasm = new URL('../hosts/wasm/', import.meta.url);
const manifest = readFileSync(new URL('Cargo.toml', wasm), 'utf8');
const families = [...manifest.matchAll(/^venue-([a-z0-9-]+) = /gm)].map((match) => match[1]).sort();
const budgets = JSON.parse(readFileSync(new URL('wasm-budgets.json', import.meta.url), 'utf8'));
assert.deepEqual(Object.keys(budgets).sort(), families, 'every WASM family requires its own size budget');
for (const family of families) {
  const budget = budgets[family];
  assert.ok(Number.isSafeInteger(budget) && budget > 0, `invalid ${family} budget`);
  for (const target of ['web', 'node']) {
    const bytes = statSync(new URL(`pkg/${target}/${family}/safe_cex_wasm_bg.wasm`, wasm)).size;
    console.log(`${family} ${target}: ${bytes} bytes (budget ${budget})`);
    assert.ok(bytes <= budget, `${family} ${target} exceeded its WASM budget`);
  }
}
