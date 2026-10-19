import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const addon = fileURLToPath(new URL('../dist/node/safe_cex_ffi.node', import.meta.url));
const accountHelper = new URL('./support/native-account.mjs', import.meta.url).href;
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

test('closing the native host releases both notification callbacks', { timeout: 15000 }, async () => {
  const source = `
    const assert = require('node:assert/strict');
    const native = require(process.argv[1]);
    (async () => {
      let event = () => {}, store = () => {};
      const references = [new WeakRef(event), new WeakRef(store)];
      const id = await native.nativeAccountOpen({ exchange: 'binance', options: {
        key: '', secret: '', publicDataOnly: true,
      } }, event, store);
      event = store = null;
      await native.nativeAccountClose(id);
      for (let i = 0; i < 100; i++) {
        await new Promise(resolve => setImmediate(resolve));
        global.gc();
        if (references.every(reference => reference.deref() === undefined)) {
          console.log('both notification callbacks released');
          return;
        }
      }
      assert.fail('closed native account retained a notification callback');
    })().catch(error => { console.error(error); process.exitCode = 1; });
  `;
  const child = spawn(process.execPath, ['--expose-gc', '-e', source, addon], { stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '', timedOut = false;
  child.stdout.on('data', chunk => { output += chunk; });
  child.stderr.on('data', chunk => { output += chunk; });
  const timeout = setTimeout(() => { timedOut = true; child.kill(); }, 10000);
  try {
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', resolve);
    });
    assert.equal(timedOut, false, output);
    assert.equal(code, 0, output);
    assert.match(output, /both notification callbacks released/, output);
  } finally {
    clearTimeout(timeout);
    if (child.exitCode === null && child.signalCode === null) child.kill();
  }
});

test('notification callbacks retain arity, raw returns and original thrown errors', { timeout: 20000 }, async () => {
  const source = `
    const assert = require('node:assert/strict');
    (async () => {
      const { openStartedAccount } = await import(process.argv[1]);
      const cleanups = [];
      const original = Object.assign(new TypeError('callback \\ud800 failure'), { code: 'CALLBACK_SENTINEL' });
      const hostile = Object.create(null);
      for (const key of ['name', 'message', 'stack', 'cause']) Object.defineProperty(hostile, key, {
        get() { throw Error('notification exception getter must not run: ' + key); },
      });
      const exceptions = [original, Symbol('raw callback failure'), hostile];
      let thrown = 0, caught = 0, events = 0, stores = 0;
      process.on('uncaughtException', error => {
        assert.equal(error, exceptions[caught]);
        if (caught === 0) {
          assert.equal(error.code, 'CALLBACK_SENTINEL');
          assert.equal(error.stack, original.stack);
        }
        caught++;
      });
      const callbacks = {
        event: function (value) {
          assert.equal(arguments.length, 1);
          assert.equal(typeof value.event, 'string');
          events++;
          return Symbol('ignored event result');
        },
        store: function (value) {
          assert.equal(arguments.length, 1);
          assert.equal(typeof value, 'object');
          stores++;
          if (thrown < exceptions.length) throw exceptions[thrown++];
          return () => { throw Error('callback return must not be invoked'); };
        },
      };
      const host = await openStartedAccount({ after: callback => cleanups.push(callback) }, {}, callbacks);
      try {
        const deadline = Date.now() + 5000;
        while ((caught < exceptions.length || stores < 4 || !events) && Date.now() < deadline) {
          host.sendGateway({ type: 'data', exchangeId: 'binance', channel: 'tickers', data: [{
            id: 'BTCUSDT', symbol: 'BTCUSDT', last: 11 + stores, bid: 9.9, ask: 10.1,
            mark: 10, index: 10, percentage: 0, openInterest: 0, fundingRate: 0,
            fundingTime: 0, volume: 1, quoteVolume: 10,
          }] });
          await new Promise(resolve => setTimeout(resolve, 25));
        }
        assert.equal(caught, exceptions.length, 'each original callback exception must reach the host');
        assert.ok(events > 0 && stores >= 4, 'later notifications must survive thrown callbacks');
        console.log('notification callback contract preserved');
      } finally {
        for (const cleanup of cleanups.reverse()) await cleanup();
      }
    })().catch(error => { console.error(error); process.exitCode = 1; });
  `;
  const child = spawn(process.execPath, ['--force-node-api-uncaught-exceptions-policy', '-e', source, accountHelper], { stdio: ['ignore', 'pipe', 'pipe'] });
  let output = '', timedOut = false;
  child.stdout.on('data', chunk => { output += chunk; });
  child.stderr.on('data', chunk => { output += chunk; });
  const timeout = setTimeout(() => { timedOut = true; child.kill(); }, 15000);
  try {
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', resolve);
    });
    assert.equal(timedOut, false, output);
    assert.equal(code, 0, output);
    assert.match(output, /notification callback contract preserved/, output);
  } finally {
    clearTimeout(timeout);
    if (child.exitCode === null && child.signalCode === null) child.kill();
  }
});
