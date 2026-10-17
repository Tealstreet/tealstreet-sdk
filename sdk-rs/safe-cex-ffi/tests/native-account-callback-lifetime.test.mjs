import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const addon = fileURLToPath(new URL('../dist/node/safe_cex_ffi.node', import.meta.url));
test('closing a native account releases its proxy-token callback and lets Node exit', { timeout: 15000 }, async () => {
  const source = `
    const native = require(process.argv[1]);
    (async () => {
      const id = await native.nativeAccountOpen({ exchange: 'bitrue', optionsJson: JSON.stringify({
        key: 'test-key', secret: 'test-secret', tealstreetProxy: true,
      }) }, () => {}, () => {}, undefined, async () => 'unused-token');
      await native.nativeAccountClose(id);
      console.log('account closed');
    })().catch(error => { console.error(error); process.exitCode = 1; });
  `;
  const child = spawn(process.execPath, ['-e', source, addon], { stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '', timedOut = false;
  child.stdout.on('data', chunk => { output += chunk; });
  child.stderr.on('data', chunk => { output += chunk; });
  const timeout = setTimeout(() => { timedOut = true; child.kill(); }, 10000);
  try {
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', resolve);
    });
    assert.match(output, /account closed/, output);
    assert.equal(timedOut, false, `closed account kept its JS callback referenced: ${output}`);
    assert.equal(code, 0, output);
  } finally {
    clearTimeout(timeout);
    if (child.exitCode === null && child.signalCode === null) child.kill();
  }
});
