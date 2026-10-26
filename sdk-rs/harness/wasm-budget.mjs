import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Single-venue family bundles: 1.28125 MiB (RUST_WASM_FAMILIES.md), raised from
// 1.25 MiB for FetchAccount decoding in every identity venue. The combined ceiling caps
// total package size; Sam approved the combined 14 MiB ceiling on 2026-10-26.
export const WASM_BUDGETS = Object.freeze({ venue: 1_343_488, combined: 14_680_064 });
// Bitget allowance: 1,478,656 less 2,108 measured bytes from #2142/#2145;
// overseer-approved shrink-only follow-up under Sam's no-TS-fallback directive.
// Bybit: Sam approved 2026-10-25 (venue + 60 KiB) for the Plan-adoption stack, whose top measured
// 1,398,660 on 9b05c9ac452; it can only go down.
// AsterDEX: overseer ruling 2026-10-26, 1424 KiB for managed trade plans (funnel slice 3), about half
// of it engine plan machinery. Follow-up (claude-19pt8s): the venue opt-s lever, CPU gate passing; if it
// wins, lower this to the new measured value plus 4 KiB. It can only go down.
export const WASM_VENUE_OVERRIDES = Object.freeze({ bitget: 1_476_548, bitgetusdc: 1_476_548, bybit: 1_404_928, asterdex: 1_458_176 });
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
