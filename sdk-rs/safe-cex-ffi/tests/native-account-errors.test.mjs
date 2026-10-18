import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
async function open(t, exchange = 'binance', options = {}) {
  const id = await native.nativeAccountOpen({
    exchange, options: { key: 'native-key', secret: 'native-secret', ...options },
  }, () => {}, () => {});
  t.after(() => native.nativeAccountClose(id));
  return id;
}
function plainError(message) {
  return error => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'Error');
    assert.equal(error.message, message);
    assert.equal(Object.hasOwn(error, 'code'), false);
    return true;
  };
}

test('registry method errors retain plain name/message and omit absent code', async t => {
  const id = await open(t);
  await assert.rejects(native.nativeAccountCallMethod(id, 'noSuchMethod', []),
    plainError('Unknown method noSuchMethod'));
  await assert.rejects(native.nativeAccountCallMethod(id, 'fetchAccountGreeks', []),
    plainError('Account greeks are not supported on binance'));
});

test('native lifecycle errors use the same error fields', async () => {
  const id = 0xffffffff;
  await assert.rejects(native.nativeAccountCallMethod(id, 'fetchOrders', []),
    plainError(`no native account ${id}`));
  await assert.rejects(native.nativeAccountClose(id), plainError(`no native account ${id}`));
});

test('account construction errors are plain Error objects', async () => {
  await assert.rejects(native.nativeAccountOpen({ exchange: 'no-such-exchange', options: {} },
    () => {}, () => {}), error => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'Error');
    assert.match(error.message, /unknown variant|Unknown exchange/);
    assert.equal(error.message.startsWith('{'), false);
    assert.equal(Object.hasOwn(error, 'code'), false);
    return true;
  });
});

test('real native transport failures preserve AxiosError name and ECONNREFUSED code', async t => {
  const listener = createServer();
  await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${listener.address().port}`;
  await new Promise(resolve => listener.close(resolve));
  const id = await open(t, 'bitrue', { overrideProxyUrl: origin });
  await assert.rejects(native.nativeAccountCallMethod(id, 'validateAccount', []), error => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'AxiosError');
    assert.equal(error.code, 'ECONNREFUSED');
    assert.match(error.message, /^connect ECONNREFUSED http:\/\/127\.0\.0\.1:/);
    return true;
  });
});
