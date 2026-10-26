import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Single-venue family bundles: 1.28125 MiB (RUST_WASM_FAMILIES.md), raised from
// 1.25 MiB for FetchAccount decoding in every identity venue. The combined ceiling caps
// total package size; overseer 2026-10-21 raised it to 13.5 MiB for the Bitunix family.
export const WASM_BUDGETS = Object.freeze({ venue: 1_343_488, combined: 14_155_776 });
// Bitget history Read custody: CI family size rounded up to KiB (overseer ceiling 1448 KiB).
// AsterDEX: the venue crate at opt-level "s" (#2178) measured 1,326,024 in CI. Per the overseer's
// ruling the cap is that plus 4 KiB, below the shared venue budget. It can only go down.
// Bybit managed-socket adoption: overseer ruling (ddi2il) 2026-10-26, 1412 KiB, replacing the
// Plan-adoption allowance; it removes Bybit's legacy socket supervisor and inventory sites.
export const WASM_VENUE_OVERRIDES = Object.freeze({ bitget: 1_479_680, bitgetusdc: 1_479_680, bybit: 1_445_888, asterdex: 1_330_120 });
export const wasmBudgetBytes = (profile, venue) =>
  profile === 'venue' && venue !== undefined && Object.hasOwn(WASM_VENUE_OVERRIDES, venue) ? WASM_VENUE_OVERRIDES[venue] : WASM_BUDGETS[profile];
export const wasmBudgetError = (bytes, profile, venue) => {
  if (!Object.hasOwn(WASM_BUDGETS, profile) || !Number.isSafeInteger(bytes) || bytes <= 0) return 'invalid wasm budget input';
  const budget = wasmBudgetBytes(profile, venue);
  return bytes > budget ? `${profile} wasm ${bytes} exceeds ${budget} bytes` : undefined;
};

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const { values } = parseArgs({ options: { combined: { type: 'boolean', default: false }, venue: { type: 'string' } } });
  const profile = values.combined ? 'combined' : 'venue';
  const { venue } = values;
  const artifacts = ['web', 'node'].map((target) => {
    const path = `sdk-rs/hosts/wasm/pkg/${target}/safe_cex_wasm_bg.wasm`;
    const bytes = readFileSync(path);
    return { path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex'), error: wasmBudgetError(bytes.length, profile, venue) };
  });
  const report = { profile, venue, budgetBytes: wasmBudgetBytes(profile, venue), artifacts };
  mkdirSync('sdk-rs/harness/out', { recursive: true });
  writeFileSync(`sdk-rs/harness/out/wasm-budget-${profile}.json`, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
  if (artifacts.some((artifact) => artifact.error)) process.exit(1);
}
