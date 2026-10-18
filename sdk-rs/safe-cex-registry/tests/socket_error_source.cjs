const assert = require('node:assert/strict');
const { once } = require('node:events');
const { WebSocketServer } = require('ws');
const { DefaultStore } = require('../../../packages/safe-cex/src/store/store.base.ts');

async function main() {
  const name = process.argv[2];
  const server = new WebSocketServer({ port: 0, host: '127.0.0.1' });
  await once(server, 'listening');
  const url = `ws://127.0.0.1:${server.address().port}`;
  let venue;
  if (name === 'asterdex') {
    const { AsterDEXExchange } = require('../../../packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
    venue = new AsterDEXExchange(name, { tuning: { consoleLogging: false } }, new DefaultStore());
    Object.defineProperty(venue.publicWebsocket, 'connectionURL', { get: () => url });
  } else {
    assert.equal(name, 'bitunix');
    const { BitunixExchange } = require('../../../packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
    const { BASE_WS_URL } = require('../../../packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts');
    BASE_WS_URL.public.livenet = url;
    venue = new BitunixExchange(name, { tuning: { consoleLogging: false } }, new DefaultStore());
  }
  const owner = venue.publicWebsocket;
  const logs = [];
  venue.on('log', message => logs.push(message));
  const accepted = once(server, 'connection');
  owner.connectAndSubscribe();
  const socket = owner.ws;
  const [peer] = await accepted;
  try {
    assert(socket instanceof EventTarget, 'production plain WebSocket dispatch');
    if (socket.readyState !== WebSocket.OPEN) await once(socket, 'open');
    socket.dispatchEvent(Object.assign(new Event('error'), { message: 'native transport failed' }));
    const afterError = owner.onCloseCount;
    assert.equal(afterError, 0, 'base stopImmediatePropagation suppresses later venue error callback');
    assert.equal(logs.filter(message => message.includes('WebSocket error:')).length, 1);
    const closed = once(socket, 'close');
    peer.close(1001, 'server close');
    await closed;
    const afterClose = owner.onCloseCount;
    assert.equal(afterClose, 1, 'real close listener remains registered');
    process.stdout.write(`SOCKET_RESULT ${JSON.stringify({ afterError, afterClose })}\n`);
  } finally {
    venue.dispose();
    socket.close();
    peer.terminate();
    await new Promise(resolve => server.close(resolve));
  }
}
const deadline = setTimeout(() => { console.error('source socket dispatch timed out'); process.exit(1); }, 10_000);
main().catch(error => { console.error(error); process.exitCode = 1; }).finally(() => clearTimeout(deadline));
