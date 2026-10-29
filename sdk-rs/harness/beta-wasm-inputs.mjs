// The build-input fingerprint of the Rust-only beta wasm release. The release
// workflow computes it over the SDK checkout it builds; tealstreet-next's beta
// prebuild recomputes it over its Copybara-mapped sdk-rs/ (mapped 1:1) and
// refuses a pin whose inputs differ, so a stale pin or cache can never ship.
//   node sdk-rs/harness/beta-wasm-inputs.mjs <repo root> '<toolchain json>'
import { createHash } from 'node:crypto';
import { lstatSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Generated or local-only trees: build outputs, installed packages, harness runs.
// Server-side crates never link into the browser wasm (the wasm host depends only on
// the engine and registry), so their changes must not invalidate the web pin.
const EXCLUDED = new Set([
  'target',
  'node_modules',
  '.git',
  'sdk-rs/hosts/wasm/pkg',
  'sdk-rs/harness/out',
  'sdk-rs/algos',
  'sdk-rs/hosts/server',
  'sdk-rs/hosts/candle-server',
  'sdk-rs/hosts/remote-signer',
  'sdk-rs/hosts/native',
]);

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

const walk = (root, dir, files) => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    const rel = relative(root, path).split(sep).join('/');
    if (EXCLUDED.has(entry.name) || EXCLUDED.has(rel)) continue;
    if (entry.isSymbolicLink()) throw new Error(`symlink in beta wasm inputs: ${rel}`);
    if (entry.isDirectory()) walk(root, path, files);
    else if (entry.isFile()) files.push([rel, sha256(readFileSync(path))]);
  }
};

/**
 * Inventory every input under `<root>/sdk-rs` (path and SHA-256), plus the
 * toolchain and profile the release builds with, and fingerprint them together.
 */
export const betaWasmInputs = (root, toolchain) => {
  for (const key of ['wasmBindgen', 'wasmOpt', 'profile', 'target']) {
    if (typeof toolchain?.[key] !== 'string' || !toolchain[key]) throw new Error(`beta wasm toolchain lacks ${key}`);
  }
  if (lstatSync(join(root, 'sdk-rs')).isSymbolicLink()) throw new Error('sdk-rs is a symlink');
  const files = [];
  walk(root, join(root, 'sdk-rs'), files);
  files.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  const record = JSON.stringify({
    toolchain: Object.fromEntries(Object.entries(toolchain).sort(([a], [b]) => a.localeCompare(b))),
    files,
  });
  return { fingerprint: sha256(record), files: files.length, toolchain };
};

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const [root, toolchain] = process.argv.slice(2);
  if (!root || !toolchain) {
    console.error('usage: beta-wasm-inputs.mjs <repo root> <toolchain json>');
    process.exit(2);
  }
  console.log(JSON.stringify(betaWasmInputs(root === '.' ? fileURLToPath(new URL('../../', import.meta.url)) : root, JSON.parse(toolchain))));
}
