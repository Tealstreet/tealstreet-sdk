import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
async function eventually(predicate, message) {
  for (let i = 0; i < 1000; i++) {
    if (predicate()) return;
    await delay(10);
  }
  assert.fail(typeof message === 'function' ? message() : message);
}

test('a real KuCoin private socket reconnect posts the worker event without data', { timeout: 15000 }, async t => {
  const peers = new Set(), privatePeers = new Set(), events = [], seen = [];
  let opens = 0, privateFrames = 0, origin;
  const server = createServer((request, response) => {
    const url = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://local');
    seen.push(`${request.method} ${url.pathname}`);
    const routes = {
      '/api/v1/contracts/active': [], '/api/v1/allTickers': [], '/api/v1/positions': [], '/api/v1/ticker': [],
      '/api/v1/account-overview': { availableBalance: 0, orderMargin: 0, accountEquity: 0, unrealisedPNL: 0 },
      '/api/v1/orders': { items: [] }, '/api/v1/stopOrders': { items: [] },
      '/api/v1/bullet-public': { token: 'local-public-token', instanceServers: [{ endpoint: `ws://${origin}/public`, pingInterval: 5000 }] },
      '/api/v1/bullet-private': { token: 'local-private-token', instanceServers: [{ endpoint: `ws://${origin}/private`, pingInterval: 5000 }] },
    };
    const exists = Object.hasOwn(routes, url.pathname);
    response.writeHead(exists ? 200 : 500, { 'content-type': 'application/json' });
    response.end(JSON.stringify(exists ? { code: '200000', data: routes[url.pathname] } : { code: 'fixture-error', msg: `unexpected ${request.method} ${url.pathname}` }));
  });
  server.on('upgrade', (request, socket) => {
    peers.add(socket);
    if (request.url.startsWith('/private')) { privatePeers.add(socket); opens++; socket.on('data', () => privateFrames++); }
    socket.on('error', () => {});
    socket.on('close', () => { peers.delete(socket); privatePeers.delete(socket); });
    const accept = createHash('sha1').update(request.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  origin = `127.0.0.1:${server.address().port}`;
  let id;
  t.after(async () => {
    try { if (id !== undefined) await native.nativeAccountClose(id); }
    finally {
      for (const socket of peers) socket.destroy();
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    }
  });
  id = await native.nativeAccountOpen({ exchange: 'kucoin', options: {
    key: 'reconnect-key', secret: 'reconnect-secret', passphrase: 'reconnect-passphrase', overrideProxyUrl: `http://${origin}`,
  } }, event => events.push(event), () => {});
  await native.nativeAccountCallMethod(id, 'disablePublicWebsockets', []);
  assert.notEqual(await native.nativeAccountCallMethod(id, 'start', []), false, JSON.stringify({ seen, events }));
  await eventually(() => opens === 1 && privateFrames > 0, () => `private websocket did not open: ${JSON.stringify({opens, privateFrames, seen, events})}`);
  for (const socket of privatePeers) socket.end(Buffer.from([0x88, 0x02, 0x03, 0xe9]));
  await eventually(() => opens >= 2 && events.some(event => event.event === 'ws-reconnected'), () => `reconnect event did not arrive (opens=${opens}): ${JSON.stringify({seen, events})}`);
  assert.ok(opens >= 2, 'the real native socket reopened before forwarding');
  assert.deepEqual(events.find(event => event.event === 'ws-reconnected'), { event: 'ws-reconnected' });
});
