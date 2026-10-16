import { execFileSync } from 'node:child_process';
import { cargo, cargoHome, workspace, wasmBindgen, wasmBindgenVersion, requireWasmBindgen } from './tooling.mjs';

try {
  requireWasmBindgen();
} catch {
  execFileSync(cargo, ['install', 'wasm-bindgen-cli', '--version', `=${wasmBindgenVersion()}`, '--locked', '--root', cargoHome, '--force'], { cwd: workspace, stdio: 'inherit' });
  requireWasmBindgen();
}
console.log(`FFI tooling ready: ${wasmBindgen}`);
