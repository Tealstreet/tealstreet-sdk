import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Users download one 1.25 MiB-capped family (#1421, #1393); the combined ceiling,
// checked before standalone builds overwrite it, caps total package size. Overseer
// 2026-10-21: 13 MiB -> 13.5 MiB for the added Bitunix family (RUST_GATE_HONESTY.md).
export const WASM_BUDGETS = Object.freeze({ venue: 1_310_720, combined: 14_155_776 });
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
