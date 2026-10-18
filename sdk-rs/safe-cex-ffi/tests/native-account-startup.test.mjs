import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const market = { id: 'BTCUSDT', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
  precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 20 } } };
const ticker = { id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 9.9, ask: 10.1, last: 10, mark: 10, index: 10,
  percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 1, quoteVolume: 10 };
function textFrame(value) {
  const body = Buffer.from(JSON.stringify(value));
  const header = Buffer.alloc(body.length < 126 ? 2 : 4);
  header[0] = 0x81;
  if (body.length < 126) header[1] = body.length;
  else { header[1] = 126; header.writeUInt16BE(body.length, 2); }
  return Buffer.concat([header, body]);
}
function within(pending, ms, message) {
  return new Promise((resolve, reject) => {
    const deadline = setTimeout(() => reject(Error(message)), ms);
    pending.then(value => { clearTimeout(deadline); resolve(value); }, error => { clearTimeout(deadline); reject(error); });
  });
}
async function opening(t, options = {}, sign) {
  const sockets = new Set();
  let acceptGateway;
  const connected = new Promise(resolve => { acceptGateway = resolve; });
  const server = createServer((request, response) => {
    response.writeHead(500, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ code: -1, msg: `unexpected startup request ${request.method} ${request.url}` }));
  });
  server.on('upgrade', (request, socket) => {
    sockets.add(socket);
    socket.on('error', () => {});
    socket.on('close', () => sockets.delete(socket));
    const accept = createHash('sha1').update(request.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    socket.once('data', acceptGateway);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `127.0.0.1:${server.address().port}`;
  const events = [], stores = [];
  let id;
  t.after(async () => {
    try { if (id !== undefined) await native.nativeAccountClose(id); }
    finally {
      for (const socket of sockets) socket.destroy();
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    }
  });
  id = await native.nativeAccountOpen({ exchange: 'binance', accountName: 'startup-account', options: {
    key: 'startup-key', secret: 'startup-secret', publicDataOnly: true,
    overrideProxyUrl: `http://${origin}`,
    externalPublicData: { type: 'websocket', url: `ws://${origin}/gateway`, exchangeId: 'binance' },
    ...options,
  } }, event => events.push(event), store => stores.push(store), sign);
  const call = (method, ...args) => native.nativeAccountCallMethod(id, method, args);
  const started = call('start');
  started.catch(() => {});
  await within(connected, 2000, 'real native gateway connection did not open');
  await delay(30);
  return { id, call, started, events, stores, release(channels = ['market', 'tickers']) {
    for (const socket of sockets) {
      for (const [channel, data] of [['market', market], ['tickers', ticker]]) {
        if (channels.includes(channel)) socket.write(textFrame({ type: 'data', exchangeId: 'binance', channel, data: [data] }));
      }
    }
  } };
}

for (const method of ['dispose', 'close']) test(`${method} settles pending startup as false without a late full snapshot`, { timeout: 5000 }, async t => {
  const h = await opening(t);
  if (method === 'close') await native.nativeAccountClose(h.id);
  else await h.call('dispose');
  assert.equal(await within(h.started, 500, 'disposed startup remained pending'), false);
  const count = h.stores.length;
  h.release();
  await delay(150);
  assert.equal(h.stores.length, count, 'stale startup must not force a post-dispose Store snapshot');
});

test('successful native startup resolves true after the forced full Store post', { timeout: 5000 }, async t => {
  const h = await opening(t);
  h.release();
  assert.equal(await within(h.started, 2000, 'native startup did not complete'), true);
  await delay(30);
  const snapshot = h.stores.at(-1);
  assert.equal(snapshot.markets?.[0]?.symbol, 'BTCUSDT');
  assert.equal(snapshot.tickers?.[0]?.last, 10);
  assert.equal(snapshot.loaded.markets, true);
  assert.equal(snapshot.loaded.tickers, true);
});

test('startup errors resolve false and post the worker startup error context', { timeout: 5000 }, async t => {
  const h = await opening(t, { publicDataOnly: false, secret: '' }, async () => { throw Error('start signer refused'); });
  h.release();
  assert.equal(await within(h.started, 2000, 'failing native startup did not complete'), false);
  await delay(30);
  const event = h.events.find(event => event.event === 'error' && event.data?.[0]?.startsWith('Error starting exchange: binance'));
  assert.ok(event, JSON.stringify(h.events));
  assert.match(event.data[0], /start signer refused/);
  assert.equal(event.data[1].sentryData.tags.worker_error_phase, 'startup');
  assert.equal(event.data[1].sentryData.tags.account, 'startup-account');
});

test('the actual 30-second startup deadline logs visibly and resolves false', { timeout: 35000 }, async t => {
  const h = await opening(t);
  h.release(['market']);
  await delay(350);
  assert.equal(await within(h.started, 32000, 'startup deadline did not settle'), false);
  await delay(30);
  const event = h.events.find(event => event.event === 'error' && event.data?.[0] === 'Error starting exchange: binance: Exchange start timed out after 30s');
  assert.ok(event, JSON.stringify(h.events));
  assert.equal(event.data[1].sentryData.error.message, 'Exchange start timed out after 30s');
  assert.equal(event.data[1].sentryData.tags.worker_error_phase, 'startup');
  assert.equal(event.data[1].sendToSentry, true);
  assert.equal(event.data[1].suppressConsoleLog, false);
  const fullPosts = () => h.stores.filter(store => ['markets', 'tickers', 'orders', 'positions'].every(key => Object.hasOwn(store, key))).length;
  const count = fullPosts();
  h.release(['tickers']);
  await delay(350);
  assert.equal(fullPosts(), count, 'timed-out startup must not force a late full Store snapshot');
});

test('cancelled native startup writes the suppression warning', { timeout: 10000 }, async () => {
  const { stdout, stderr } = await promisify(execFile)(process.execPath, ['--test', '--test-name-pattern=^(dispose|close) settles', new URL(import.meta.url).pathname], { env: Object.fromEntries(Object.entries(process.env).filter(([key]) => key !== 'NODE_TEST_CONTEXT')) });
  assert.match(stdout + stderr, /Error starting exchange \(disposed, suppressing\): binance/);
});
