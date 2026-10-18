import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const cases = [
  ['okx', { key: 'native-key', secret: 'native-secret', passphrase: 'native-passphrase' }],
  ['polymarket', { key: `0x${'01'.repeat(32)}`, secret: 'unused' }],
  ['asterdex', { key: 'native-key', secret: 'native-secret' }],
  ['phemex', { key: 'native-key', secret: 'native-secret' }],
  ['woo', { key: 'native-key', secret: 'native-secret' }],
  ['kodiakfi', {
    key: 'native-key', secret: '11111111111111111111111111111111',
    applicationId: 'account-native',
  }],
  ['breakoutprop', { key: 'k', secret: 'p', applicationId: 'default' }],
];

for (const [exchange, options] of cases) {
  test(`${exchange} refuses unavailable proxy routing before HTTP`, { timeout: 10000 }, async t => {
    let opened;
    t.after(async () => {
      if (opened !== undefined) await native.nativeAccountClose(opened);
    });
    const open = native.nativeAccountOpen({
      exchange,
      options: { ...options, tealstreetProxy: true, tealstreetTunnel: false },
    }, () => {}, () => {}).then(id => {
      opened = id;
      return id;
    });
    await assert.rejects(open, /direct transport is active/);
  });
}

for (const [exchange, options] of cases.filter(([name]) => ['okx', 'woo'].includes(name))) {
  test(`${exchange} installs its configured tunnel without direct HTTP`, { timeout: 10000 }, async t => {
    let tokenCalls = 0;
    const id = await native.nativeAccountOpen({
      exchange,
      options: { ...options, tealstreetTunnel: true },
    }, () => {}, () => {}, undefined, async () => {
      tokenCalls += 1;
      throw new Error('construction must not request a proxy token');
    });
    t.after(() => native.nativeAccountClose(id));
    assert.equal(tokenCalls, 0);
  });
}
