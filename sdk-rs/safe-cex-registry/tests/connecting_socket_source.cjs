const assert = require('node:assert/strict');
const http = require('node:http');
const { once } = require('node:events');
const { DefaultStore } = require('../../../packages/safe-cex/src/store/store.base.ts');
const { BitgetExchange } = require('../../../packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const { BitgetSecondaryPublicWebsocket } = require('../../../packages/safe-cex/src/exchanges/bitget/bitget.ws-secondary-public.ts');

async function held(run) {
  const server = http.createServer();
  const streams = new Set();
  let received;
  const request = new Promise(resolve => { received = resolve; });
  server.on('upgrade', (message, stream) => {
    assert.equal(message.method, 'GET');
    assert.equal(message.url, '/');
    streams.add(stream);
    stream.on('error', () => {});
    received(); // Deliberately never complete this actual HTTP upgrade.
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  try { return await run(`ws://127.0.0.1:${server.address().port}`, request); }
  finally {
    for (const stream of streams) stream.destroy();
    await new Promise(resolve => server.close(resolve));
  }
}
async function markets() {
  return held(async (url, received) => {
    const venue = new BitgetExchange('bitget', { tuning: { consoleLogging: false } }, new DefaultStore());
    const owner = new BitgetSecondaryPublicWebsocket(venue, 1);
    const create = owner.createWebsocket;
    owner.createWebsocket = () => create.call(owner, url); // Terminal origin only; original core body executes.
    const logs = [];
    venue.on('log', message => logs.push(message));
    try {
      owner.connectAndSubscribe();
      assert.equal(owner.ws.readyState, WebSocket.CONNECTING);
      await received;
      const raw = owner.ws;
      assert.notEqual(raw, undefined, 'CONNECTING handle exists when the market callback starts');
      assert.equal(raw.readyState, WebSocket.CONNECTING);
      const closed = once(raw, 'close');
      owner.onMarketsChanged({ added: [{symbol:'SKIP'}, {symbol:'NEW'}, {symbol:'OTHER'}] });
      assert.equal(raw.readyState, WebSocket.CLOSING);
      assert.deepEqual(Object.keys(owner.topics.ticker), ['NEW']);
      await closed;
      const output = {
        keys: Object.keys(owner.topics.ticker),
        warning: logs.some(message => message.includes('state: CONNECTING, stateCode: 0')),
        reconnectCount: owner.reconnectCount,
        closeCount: owner.onCloseCount,
        closeCode: owner.lastCloseCode,
        errorCount: logs.filter(message => message.includes('WebSocket error:')).length,
      };
      assert.equal(output.warning, true);
      assert.equal(output.reconnectCount, 1);
      assert.equal(output.closeCount, 1);
      assert.equal(output.closeCode, 1006);
      assert.equal(output.errorCount, 1);
      return output;
    } finally { owner.dispose(); venue.dispose(); }
  });
}
async function rawClose() {
  return held(async (url, received) => {
    const socket = new WebSocket(url);
    const events = [];
    socket.addEventListener('open', () => events.push(['open']));
    socket.addEventListener('error', event => events.push(['error', event.message]));
    socket.addEventListener('close', event => events.push(['close', event.code, event.reason]));
    assert.equal(socket.readyState, 0);
    let sendError;
    try { socket.send('while-connecting'); } catch (error) { sendError = {name:error.name, message:error.message}; }
    assert.equal(sendError?.name, 'InvalidStateError');
    await received;
    const closed = once(socket, 'close');
    socket.close();
    const immediate = socket.readyState;
    await closed;
    assert.deepEqual(events.map(event => event[0]), ['error', 'close']);
    assert.deepEqual(events[1], ['close', 1006, '']);
    return {sendError, immediate, ended:socket.readyState, events};
  });
}
const deadline = setTimeout(() => { console.error('connecting source timed out'); process.exit(1); }, 15000);
(async () => {
  const result = {markets:await markets(), rawClose:await rawClose()};
  process.stdout.write(`CONNECTING_RESULT ${JSON.stringify(result)}\n`);
})().catch(error => { console.error(error); process.exitCode=1; }).finally(() => clearTimeout(deadline));
