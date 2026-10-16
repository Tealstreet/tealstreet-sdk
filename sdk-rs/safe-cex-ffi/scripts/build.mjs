import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { crate, workspace, cargo, wasmBindgen, requireWasmBindgen } from './tooling.mjs';

const host = process.argv[2];
if (!['node', 'wasm'].includes(host)) throw new Error('expected node or wasm');
if (host === 'wasm') requireWasmBindgen();
const args = ['build', '-p', 'safe-cex-ffi', '--no-default-features', '--features', host];
if (host === 'wasm') args.push('--target', 'wasm32-unknown-unknown');
const messages = execFileSync(cargo, [...args, '--message-format=json'], { cwd: workspace, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }).trim().split('\n').map(line => JSON.parse(line));
const artifact = messages.find(message => message.reason === 'compiler-artifact' && message.target.name === 'safe_cex_ffi' && message.target.crate_types.includes('cdylib'));
if (!artifact) throw new Error('Cargo did not emit the FFI artifact');
const output = join(crate, 'dist', host);
mkdirSync(output, { recursive: true });
if (host === 'node') {
  const library = { darwin: 'libsafe_cex_ffi.dylib', linux: 'libsafe_cex_ffi.so', win32: 'safe_cex_ffi.dll' }[process.platform];
  if (!library) throw new Error(`unsupported native platform: ${process.platform}`);
  const native = artifact.filenames.find(path => path.endsWith(library));
  if (!native) throw new Error('Cargo did not emit the native library');
  copyFileSync(native, join(output, 'safe_cex_ffi.node'));
} else {
  execFileSync(wasmBindgen, [artifact.filenames.find(path => path.endsWith('.wasm')), '--target', 'web', '--out-dir', output], { stdio: 'inherit' });
}
