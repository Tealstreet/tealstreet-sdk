import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Actual compiler diagnostics pin the reason; rustdoc annotations cannot.
const crate = fileURLToPath(new URL('../', import.meta.url));
const dialOptions = process.argv.includes('--dial-options');
const source = readFileSync(path.join(crate, dialOptions ? 'src/socket.rs' : 'src/routing.rs'), 'utf8');
const expectedCode = dialOptions ? 'E0451' : 'E0308';
const doc = source.match(/\/\/\/ ```compile_fail\n([\s\S]*?)\/\/\/ ```/);
assert.ok(doc, 'typed site-id compile-fail example must be present');
const snippet = doc[1].replace(/^\/\/\/ ?/gm, '');
const build = spawnSync(process.env.CARGO ?? 'cargo', ['build', '--locked', '--manifest-path', path.join(crate, 'Cargo.toml'), '--lib', '--message-format=json'], { encoding: 'utf8' });
assert.equal(build.status, 0, build.error?.message ?? build.stderr);
const artifact = build.stdout.split('\n').filter(Boolean).map(line => JSON.parse(line)).find(message => message.reason === 'compiler-artifact' && message.target.name === 'safe_cex_transport');
const library = artifact?.filenames.find(name => name.endsWith('.rlib'));
assert.ok(library, 'Cargo must provide the actual transport library artifact');
const directory = mkdtempSync(path.join(tmpdir(), 'site-id-diagnostic-'));
try {
  const input = path.join(directory, 'probe.rs');
  writeFileSync(input, `fn main() {\n${snippet}\n}\n`);
  const result = spawnSync(process.env.RUSTC ?? 'rustc', ['--edition=2024', '--emit=metadata', '--error-format=json', '--extern', `safe_cex_transport=${library}`, '-L', `dependency=${path.dirname(library)}`, '-L', `dependency=${path.join(path.dirname(library), 'deps')}`, '--out-dir', directory, input], { encoding: 'utf8' });
  assert.equal(result.status, 1, result.error?.message ?? 'the site-id example must be a compile error');
  const diagnostics = result.stderr.split('\n').filter(Boolean).map(line => JSON.parse(line));
  const errors = diagnostics.filter(message => message.level === 'error' && message.code);
  assert.deepEqual(errors.map(message => message.code.code), [expectedCode], `committed example must fail specifically with ${expectedCode}`);
  assert.ok(errors[0].message.includes(dialOptions ? 'private' : 'mismatched types'));
  if (dialOptions) {
    assert.ok(errors[0].message.includes('headers'));
    assert.ok(errors[0].message.includes('alpn_protocols'));
  }
  console.log(`${dialOptions ? 'Dial policy' : 'Site ID'}: committed doctest fails specifically with ${expectedCode}.`);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
