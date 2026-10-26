import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Single-venue family bundles: 1.28125 MiB (RUST_WASM_FAMILIES.md), raised from
// 1.25 MiB for FetchAccount decoding in every identity venue. The combined ceiling caps
// total package size; Sam approved the combined 14 MiB ceiling on 2026-10-26.
export const WASM_BUDGETS = Object.freeze({ venue: 1_343_488, combined: 14_680_064 });
// #2219 Base64Url/EvmHex signing on the #2208 tip: CI Bitget 1,482,166 rounded up to KiB (overseer ceiling 1448 KiB);
// #2224 serial call pacing: CI 1,482,920, +1 KiB under the 2 KiB shared-code rule.
// AsterDEX: managed sockets (#2212) measured 1,384,485 in CI, within the standing 5% venue
// allowance over the opt-s 1,330,120 cap; #2219 signing encodings: CI 1,385,845, then #2224 serial call
// pacing: CI 1,387,161, rounded up to the KiB.
// Bybit managed-socket adoption: overseer ruling (ddi2il) 2026-10-26, 1412 KiB, replacing the
// Plan-adoption allowance; it removes Bybit's legacy socket supervisor and inventory sites.
// #2219 shared signing encodings: CI 1,445,978, 1413 KiB by overseer ruling.
// Kraken on the managed-sockets gate (#2200) CI: 1,405,096 bytes, rounded up to 1373 KiB.
// Kraken beta error custody CI: 1,404,713 bytes, rounded up under the standing
// 5% venue allowance; unrelated venue overrides remain unchanged.
// AsterDEX host address setup (#2233): CI 1,417,307, rounded up to 1385 KiB under the
// standing venue allowance; the larger of it and the incoming cap.
export const WASM_VENUE_OVERRIDES = Object.freeze({ bitget: 1_483_776, bitgetusdc: 1_483_776, bybit: 1_446_912, asterdex: 1_418_240, kraken: 1_405_952, krakenspot: 1_405_952 });
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
