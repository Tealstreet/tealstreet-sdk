import { createHash } from 'node:crypto';
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { readWasmCaps } from './artifact-census.mjs';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

// Cap numbers live in the generated wasm-caps.json; the WASM caps workflow raises
// them from master CI measurements (never lowering). Provenance: wasm-caps-history.md.
export const WASM_CAPS_PATH = fileURLToPath(new URL('./wasm-caps.json', import.meta.url));
const caps = readWasmCaps(readFileSync(WASM_CAPS_PATH, 'utf8'));

export const WASM_RESERVE_BYTES = caps.reserveBytes;
export const WASM_FAMILY_CAPS = Object.freeze({ ...caps.families });
// `venue` is only the fallback for a name outside the family map; it is not generated.
export const WASM_BUDGETS = Object.freeze({ venue: 1_343_488, combined: caps.combined });

const familyOf = (venue) => {
  try { return venueFamily(venue); } catch { return undefined; }
};
export const wasmBudgetBytes = (profile, venue) => {
  const family = profile === 'venue' && venue !== undefined ? familyOf(venue) : undefined;
  return family !== undefined && Object.hasOwn(WASM_FAMILY_CAPS, family) ? WASM_FAMILY_CAPS[family] : WASM_BUDGETS[profile];
};
// The PR gate: a module may never exceed its current cap. Reserve is not checked here.
export const wasmBudgetError = (bytes, profile, venue) => {
  if (!Object.hasOwn(WASM_BUDGETS, profile) || !Number.isSafeInteger(bytes) || bytes <= 0) return 'invalid wasm budget input';
  const budget = wasmBudgetBytes(profile, venue);
  return bytes > budget ? `${profile} wasm ${bytes} exceeds ${budget} bytes` : undefined;
};

// Rule 17: a cap keeps WASM_RESERVE_BYTES above the measured module, rounded up to KiB.
export const reserveTargetCap = (measured, reserve = WASM_RESERVE_BYTES) => {
  if (!Number.isSafeInteger(measured) || measured <= 0) throw new Error(`invalid measured wasm size: ${measured}`);
  return Math.ceil((measured + reserve) / 1024) * 1024;
};
export const nextWasmCap = (currentCap, measured, reserve = WASM_RESERVE_BYTES) =>
  Math.max(currentCap, reserveTargetCap(measured, reserve));

// Advisory only: a shortfall never fails a PR; the master cap bot restores the reserve.
export const wasmReserveWarning = (bytes, profile, venue) => {
  if (wasmBudgetError(bytes, profile, venue)) return undefined;
  const budget = wasmBudgetBytes(profile, venue);
  if (budget - bytes >= WASM_RESERVE_BYTES) return undefined;
  const label = profile === 'venue' ? `${venue} family` : profile;
  return `${label} wasm ${bytes} B leaves ${budget - bytes} B under its ${budget} B cap, short of the ` +
    `${WASM_RESERVE_BYTES} B rule-17 reserve; the WASM caps bot raises it after merge (not a failure)`;
};
export const reportReserveWarning = (message, env = process.env) => {
  if (!message) return;
  console.log(`::warning title=WASM reserve::${message}`);
  if (env.GITHUB_STEP_SUMMARY) appendFileSync(env.GITHUB_STEP_SUMMARY, `- ${message}\n`);
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
  for (const artifact of artifacts) reportReserveWarning(wasmReserveWarning(artifact.bytes, profile, venue));
  if (artifacts.some((artifact) => artifact.error)) process.exit(1);
}
