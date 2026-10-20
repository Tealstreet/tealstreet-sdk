import { cpSync, mkdtempSync, rmSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

// Copy only release-owned files outside the repo. A source/workspace fallback
// must not make a broken npm artifact appear to work.
const directory = mkdtempSync(join(tmpdir(), 'exchange-mcp-rust-'));
try {
  cpSync(new URL('../dist/rust-probe.js', import.meta.url), join(directory, 'probe.mjs'));
  cpSync(new URL('../dist/wasm', import.meta.url), join(directory, 'wasm'), { recursive: true });
  const result = spawnSync(process.execPath, [join(directory, 'probe.mjs')], {
    cwd: directory,
    timeout: 30000,
    encoding: 'utf8',
    env: { ...process.env, NODE_PATH: '' },
  });
  if (
    result.error ||
    result.status !== 0 ||
    !result.stdout.includes('rust-smoke: native Coinbase market loaded') ||
    !result.stdout.includes('rust-smoke: isolated Binance and Coinbase families initialized')
  ) {
    throw new Error(`Isolated Rust artifact failed: ${result.error ?? result.stderr ?? result.stdout}`);
  }
  console.log(result.stdout.trim());
  // A missing selected family must fail even if a workspace or other families exist.
  unlinkSync(join(directory, 'wasm/rust/binance/safe_cex_wasm_bg.wasm'));
  const missing = spawnSync(process.execPath, [join(directory, 'probe.mjs')], {
    cwd: directory,
    timeout: 30000,
    encoding: 'utf8',
    env: { ...process.env, NODE_PATH: '' },
  });
  if (missing.error || missing.status === 0 || !missing.stderr.includes('safe_cex_wasm_bg.wasm'))
    throw new Error('Missing packaged family did not fail at its owned WASM asset');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
