import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// The standalone budget is unchanged. The shipped combined artifact has its own
// fixed 13 MiB ceiling (raised from 12 MiB for the 2026-10-20 audit fixes),
// checked before any standalone build replaces it.
export const WASM_BUDGETS = Object.freeze({ venue: 1_258_291, combined: 13_631_488 });
export const wasmBudgetError = (bytes, profile) => {
  if (!Object.hasOwn(WASM_BUDGETS, profile) || !Number.isSafeInteger(bytes) || bytes <= 0) return 'invalid wasm budget input';
  return bytes > WASM_BUDGETS[profile] ? `${profile} wasm ${bytes} exceeds ${WASM_BUDGETS[profile]} bytes` : undefined;
};

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const { values } = parseArgs({ options: { combined: { type: 'boolean', default: false } } });
  const profile = values.combined ? 'combined' : 'venue';
  const artifacts = ['web', 'node'].map((target) => {
    const path = `sdk-rs/hosts/wasm/pkg/${target}/safe_cex_wasm_bg.wasm`;
    const bytes = readFileSync(path);
    return { path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex'), error: wasmBudgetError(bytes.length, profile) };
  });
  const report = { profile, budgetBytes: WASM_BUDGETS[profile], artifacts };
  mkdirSync('sdk-rs/harness/out', { recursive: true });
  writeFileSync(`sdk-rs/harness/out/wasm-budget-${profile}.json`, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
  if (artifacts.some((artifact) => artifact.error)) process.exit(1);
}
