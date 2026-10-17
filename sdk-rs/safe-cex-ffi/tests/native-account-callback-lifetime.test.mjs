import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const addon = fileURLToPath(new URL('../dist/node/safe_cex_ffi.node', import.meta.url));
for (const pending of [false, true]) test(`closing a native account releases its ${pending ? 'pending' : 'idle'} proxy-token callback and lets Node exit`, { timeout: 15000 }, async () => {
  const source = `
    const native = require(process.argv[1]);
    (async () => {
      let entered;
      const called = new Promise(resolve => { entered = resolve; });
      const token = () => { entered(); return new Promise(() => {}); };
      const id = await native.nativeAccountOpen({ exchange: 'bitrue', options: {
        key: 'test-key', secret: 'test-secret', tealstreetProxy: true, overrideProxyUrl: 'http://127.0.0.1:1',
      } }, () => {}, () => {}, undefined, token);
      let outcome;
      if (${pending}) {
        outcome = native.nativeAccountCallMethod(id, 'validateAccount', []).then(
          () => { throw Error('unexpected request success'); },
          error => { if (!error.message.includes('native account callback is closed')) throw error; },
        );
        await called;
      }
      await native.nativeAccountClose(id);
      console.log('account closed');
      if (outcome) { await outcome; console.log('pending call settled'); }
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
    if (pending && !timedOut) assert.match(output, /pending call settled/, output);
    assert.equal(timedOut, false, `closed account kept its JS callback referenced: ${output}`);
    assert.equal(code, 0, output);
  } finally {
    clearTimeout(timeout);
    if (child.exitCode === null && child.signalCode === null) child.kill();
  }
});
