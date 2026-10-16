import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const crate = dirname(dirname(fileURLToPath(import.meta.url)));
export const workspace = dirname(crate);
export const cargo = process.env.CARGO || 'cargo';
export const cargoHome = resolve(process.env.CARGO_HOME || join(homedir(), '.cargo'));
export const wasmBindgen = join(cargoHome, 'bin', process.platform === 'win32' ? 'wasm-bindgen.exe' : 'wasm-bindgen');

export function wasmBindgenVersion() {
  const lock = readFileSync(join(workspace, 'Cargo.lock'), 'utf8');
  const match = lock.match(/^name = "wasm-bindgen"\r?\nversion = "([^"]+)"/m);
  if (!match) throw new Error('Cannot find wasm-bindgen version in sdk-rs/Cargo.lock');
  return match[1];
}

export function requireWasmBindgen() {
  const expected = wasmBindgenVersion();
  let actual;
  try {
    actual = execFileSync(wasmBindgen, ['--version'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  } catch {
    throw new Error(`wasm-bindgen-cli ${expected} is missing or unusable at ${wasmBindgen}. Run yarn ffi:setup.`);
  }
  if (actual !== `wasm-bindgen ${expected}`) {
    throw new Error(`Expected wasm-bindgen ${expected}, got ${actual}. Run yarn ffi:setup.`);
  }
}

export function packageVersion() {
  const metadata = JSON.parse(execFileSync(cargo, ['metadata', '--locked', '--no-deps', '--format-version', '1'], { cwd: workspace, encoding: 'utf8' }));
  const pkg = metadata.packages.find(pkg => pkg.name === 'safe-cex-ffi');
  if (!pkg?.version) throw new Error('Cargo metadata does not contain a safe-cex-ffi package version');
  return pkg.version;
}
