import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');

async function open(t, hold = false) {
  const seen = [], responses = [];
  const server = createServer((request, response) => {
    const url = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://local');
    assert.equal(url.pathname, '/fapi/v2/leverageBracket');
    seen.push(url.searchParams.get('contractName'));
    response.writeHead(200, { 'content-type': 'application/json' });
    if (hold) responses.push(response);
    else response.end('{"code":"0","data":[]}');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let id = await native.nativeAccountOpen({ exchange: 'bitrue', options: {
    key: 'native-key', secret: 'native-secret', overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
  } }, () => {}, () => {});
  const close = async () => { if (id !== undefined) { await native.nativeAccountClose(id); id = undefined; } };
  t.after(async () => {
    for (const response of responses) {
      if (!response.writableEnded) response.end('{"code":"0","data":[]}');
    }
    try { await close(); }
    finally {
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    }
  });
  return { seen, responses, close, call: symbol => native.nativeAccountCallMethod(id, 'updateSymbolMetadata', [symbol]) };
}
async function eventually(predicate) {
  for (let i = 0; i < 200; i++) { if (predicate()) return; await delay(10); }
  assert.fail('native metadata HTTP observation timed out');
}

test('native metadata calls acknowledge before the real HTTP response settles', async t => {
  const h = await open(t, true);
  let settled = false;
  const pending = h.call('BTCUSDT').then(result => { settled = true; return result; });
  await eventually(() => h.responses.length === 1);
  await delay(50);
  const acknowledged = settled;
  h.responses[0].end('{"code":"0","data":[]}');
  assert.equal(await pending, null);
  assert.equal(acknowledged, true, 'the worker acknowledges its unawaited metadata method immediately');
});

test('native metadata calls coalesce each symbol independently', async t => {
  const h = await open(t);
  await Promise.all([h.call('BTCUSDT'), h.call('BTCUSDT'), h.call('BTCUSDT'), h.call('ETHUSDT')]);
  await eventually(() => h.seen.filter(symbol => symbol === 'BTCUSDT').length >= 2 && h.seen.includes('ETHUSDT'));
  assert.equal(h.seen.filter(symbol => symbol === 'BTCUSDT').length, 2);
  assert.equal(h.seen.filter(symbol => symbol === 'ETHUSDT').length, 1);
});

test('close cancels trailing native metadata work', async t => {
  const h = await open(t);
  await Promise.all([h.call('BTCUSDT'), h.call('BTCUSDT')]);
  await eventually(() => h.seen.length >= 1);
  await h.close();
  await delay(650);
  assert.equal(h.seen.length, 1, 'disposal clears the shared worker metadata throttle');
});
