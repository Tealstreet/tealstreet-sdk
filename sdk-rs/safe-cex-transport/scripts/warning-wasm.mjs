import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const cargo = process.env.CARGO ?? 'cargo';
const cli = path.join(process.env.CARGO_HOME ?? path.join(homedir(), '.cargo'), 'bin', 'wasm-bindgen');
const version = readFileSync(path.join(workspace, 'Cargo.lock'), 'utf8').match(/^name = "wasm-bindgen"\nversion = "([^"]+)"/m)?.[1];
assert.equal(execFileSync(cli, ['--version'], { encoding: 'utf8' }).trim(), `wasm-bindgen ${version}`, 'run yarn ffi:setup for the locked CLI');
execFileSync(cargo, ['build', '--locked', '-p', 'safe-cex-transport', '--example', 'warning_wasm', '--target', 'wasm32-unknown-unknown'], { cwd: workspace, stdio: 'inherit' });
const metadata = JSON.parse(execFileSync(cargo, ['metadata', '--locked', '--no-deps', '--format-version', '1'], { cwd: workspace, encoding: 'utf8' }));
const output = mkdtempSync(path.join(tmpdir(), 'safe-cex-warning-'));
try {
  execFileSync(cli, [path.join(metadata.target_directory, 'wasm32-unknown-unknown/debug/examples/warning_wasm.wasm'), '--target', 'nodejs', '--out-dir', output]);
  const { probe_warning, probe_relay_overlay, probe_tunnel_endpoints } = createRequire(import.meta.url)(path.join(output, 'warning_wasm.js'));
  const tunnelPath = process.env.NEXT_PUBLIC_TRADETUNNEL_STAGING === '1' ? '/tunnel-staging' : '/tunnel';
  assert.deepEqual(JSON.parse(probe_tunnel_endpoints()), ['jp', 'asia'].map(region => 'wss://fsjnpy143pbz.' + region + '.tealstreet.io' + tunnelPath));
  assert.deepEqual(JSON.parse(probe_relay_overlay()), { requiredRejected: true, dials: 1, protocols: [] });
  const messages = [];
  const previousWarn = console.warn;
  console.warn = (message) => {
    messages.push(message);
    // Real JS reentry would deadlock/panic if the Rust dedup lock were still held.
    probe_warning('bybit', true);
  };
  const direct = (exchange, enabled) => {
    const route = JSON.parse(probe_warning(exchange, enabled));
    assert.equal(route.restProxyHost, '');
    assert.equal(route.howEgress, 'sg');
    for (const channel of ['wsPublic', 'wsPrivate', 'wsBusiness']) assert.deepEqual(route[channel], { tunnelled: false });
  };
  try {
    direct('bybit', false);
    assert.equal(messages.length, 0, 'disabled routing must not warn');
    direct('bybit', true);
    direct('bybit', true);
    assert.equal(messages.length, 1, 'wasm console must observe one warning per exchange');
    direct('grvt', true);
    assert.equal(messages.length, 2, 'another exchange must emit its own warning');
    assert.match(messages[0], /^\[bybit\] tealstreetWsRoute requested but the direct transport is active;/);
    assert.match(messages[1], /^\[grvt\] tealstreetWsRoute requested but the direct transport is active;/);
  } finally {
    console.warn = previousWarn;
  }
  console.log('Wasm warning: console output, deduplication, reentry, and direct routes verified.');
} finally {
  rmSync(output, { recursive: true, force: true });
}
