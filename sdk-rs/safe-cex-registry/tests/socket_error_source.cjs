const assert = require('node:assert/strict');
const http = require('node:http');
const { once } = require('node:events');
const { WebSocketServer } = require('ws');
const { DefaultStore } = require('../../../packages/safe-cex/src/store/store.base.ts');

function createVenue(name, url) {
  if (name === 'asterdex') {
    const { AsterDEXExchange } = require('../../../packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
    const venue = new AsterDEXExchange(name, { tuning: { consoleLogging: false } }, new DefaultStore());
    Object.defineProperty(venue.publicWebsocket, 'connectionURL', { get: () => url });
    return venue;
  }
  assert.equal(name, 'bitunix');
  const { BitunixExchange } = require('../../../packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
  const { BASE_WS_URL } = require('../../../packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts');
  BASE_WS_URL.public.livenet = url;
  return new BitunixExchange(name, { tuning: { consoleLogging: false } }, new DefaultStore());
}

async function observe(name, failedUpgrade) {
  const server = failedUpgrade
    ? http.createServer((_, response) => response.writeHead(400).end())
    : new WebSocketServer({ port: 0, host: '127.0.0.1' });
  if (failedUpgrade) {
    server.on('upgrade', (_, socket) => socket.end('HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n'));
    server.listen(0, '127.0.0.1');
  }
  await once(server, 'listening');
  const venue = createVenue(name, `ws://127.0.0.1:${server.address().port}`);
  const owner = venue.publicWebsocket;
  const logs = [];
  venue.on('log', message => logs.push(message));
  const accepted = failedUpgrade ? null : once(server, 'connection');
  owner.connectAndSubscribe();
  const socket = owner.ws;
  let peer;
  let afterError;
  try {
    assert(socket instanceof EventTarget, 'production plain WebSocket dispatch');
    if (failedUpgrade) {
      const [event] = await once(socket, 'close');
      assert.equal(event.code, 1006, 'actual failed upgrade emits abnormal close');
    } else {
      [peer] = await accepted;
      if (socket.readyState !== WebSocket.OPEN) await once(socket, 'open');
      socket.dispatchEvent(Object.assign(new Event('error'), { message: 'native transport failed' }));
      afterError = owner.onCloseCount;
      assert.equal(afterError, 0, 'base listener suppresses the later venue error callback');
      assert(!logs.some(message => message.includes('Reconnecting in')));
      const closed = once(socket, 'close');
      peer.close(1001, 'server close');
      await closed;
    }
    assert.equal(logs.filter(message => message.includes('WebSocket error:')).length, 1);
    assert.equal(owner.onCloseCount, 1, 'one actual close, independently of error dispatch');
    const closed = {
      count: owner.onCloseCount,
      lastCloseCode: owner.lastCloseCode ?? null,
      lifetimeCloseCount: owner.lifetimeCloseCount,
      reconnectScheduled: logs.some(message => message.includes('Reconnecting in')),
    };
    assert(closed.reconnectScheduled, 'actual close schedules recovery');
    return failedUpgrade ? closed : { afterError, closed };
  } finally {
    venue.dispose();
    socket.close();
    peer?.terminate();
    await new Promise(resolve => server.close(resolve));
  }
}
const deadline = setTimeout(() => { console.error('source socket dispatch timed out'); process.exit(1); }, 10_000);
(async () => {
  const name = process.argv[2];
  const invalid = createVenue(name, 'invalid');
  const invalidLogs = [];
  invalid.on('log', message => invalidLogs.push(message));
  let constructorError;
  try { invalid.publicWebsocket.connectAndSubscribe(); }
  catch (error) { constructorError = error; }
  if (name === 'bitunix') assert.equal(constructorError?.name, 'SyntaxError');
  else assert.equal(constructorError, undefined);
  const invalidURL = {
    count: invalid.publicWebsocket.onCloseCount,
    reconnectScheduled: invalidLogs.some(message => message.includes('Reconnecting in')),
  };
  assert.deepEqual(invalidURL, { count: 0, reconnectScheduled: false });
  invalid.dispose();
  const result = { invalidURL, live: await observe(name, false), failedUpgrade: await observe(name, true) };
  process.stdout.write(`SOCKET_RESULT ${JSON.stringify(result)}\n`);
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(() => clearTimeout(deadline));
