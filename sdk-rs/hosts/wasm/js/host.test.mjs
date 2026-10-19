import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:net';
import { EngineHost, nativeTransport } from './host.mjs';

test('command failures preserve partial placement metadata', async () => {
  const details = { message: 'second order rejected', partialOrderIds: ['accepted'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1 };
  const engine = {
    command(json) { const { command } = JSON.parse(json); return JSON.stringify([{ op: 'done', command, error: JSON.stringify(details) }]); },
  };
  const host = new EngineHost(engine, { now: () => 1 });
  await assert.rejects(host.command({ op: 'placeOrders', orders: [] }), (error) => {
    assert.equal(error.message, details.message);
    assert.deepEqual(error.partialOrderIds, details.partialOrderIds);
    assert.equal(error.failedOrderIndex, 1);
    assert.equal(error.totalOrderCount, 2);
    return true;
  });
});

test('invalid command parsing rejects and releases its pending completion', async () => {
  const host = new EngineHost({ command() { throw new Error('invalid caller client order id'); } }, { now: () => 1 });
  await assert.rejects(host.command({ op: 'placeOrders', orders: [] }), /invalid caller/);
  assert.equal(host.commands.size, 0);
});

test('settings preserve independent hedge leverage and decode older frames', async () => {
  const { Frame } = await import('./wire.mjs');
  const settings = (independent) => {
    const buffer = new ArrayBuffer(independent ? 64 : 48);
    new Uint8Array(buffer, 0, 4).set(new TextEncoder().encode('SCXF'));
    const view = new DataView(buffer);
    view.setUint16(4, 1, true); view.setUint16(6, 1, true);
    view.setUint16(16, 12, true); view.setUint32(20, independent ? 40 : 24, true);
    view.setUint32(24, 1, true); view.setUint32(28, 0, true); view.setUint32(32, 2, true);
    view.setFloat64(40, 20, true);
    if (independent) { view.setFloat64(48, 10, true); view.setFloat64(56, 15, true); }
    return new Frame(buffer).settings();
  };
  assert.equal(settings(true).buyLeverage[0], 10);
  assert.equal(settings(true).sellLeverage[0], 15);
  assert.equal(settings(false).buyLeverage[0], 20);
  assert.equal(settings(false).sellLeverage[0], 20);
});

test('required upgrade headers are forwarded only to capable transports', () => {
  for (const capable of [false, true]) {
    const connected = [];
    const closed = [];
    const engine = { wsClose: (...args) => (closed.push(args), '') };
    const transport = {
      webSocketHeaders: capable,
      now: () => 100,
      connect: (...args) => (connected.push(args), { close() {}, send() {} }),
      setTimeout: (fn) => fn(),
    };
    const host = new EngineHost(engine, transport);
    host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test/private', headers: [['ACCESS-SIGN', 'signature']] });
    if (capable) {
      assert.equal(connected.length, 1);
      assert.deepEqual(connected[0][1].headers, { 'ACCESS-SIGN': 'signature' });
      assert.equal(closed.length, 0);
    } else {
      assert.equal(connected.length, 0);
      assert.deepEqual(closed, [[7, 1006, 100]]);
    }
  }
});

for (const closeBeforeOpen of [false, true]) {
  test(`Node socket reports ${closeBeforeOpen ? 'close before open' : 'a refused upgrade'} once without an unhandled error`, async () => {
    const server = createServer();
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const port = server.address().port;
    await new Promise((resolve) => server.close(resolve));
    let closes = 0;
    let timer;
    try {
      await new Promise((resolve, reject) => {
        timer = setTimeout(() => reject(new Error('socket close was not reported')), 3000);
        const socket = nativeTransport().connect(`ws://127.0.0.1:${port}`, {
          onOpen: () => reject(new Error('unexpected open')),
          onMessage() {},
          onClose: () => { closes++; resolve(); },
        });
        if (closeBeforeOpen) socket.close();
      });
      await new Promise(setImmediate);
      assert.equal(closes, 1);
    } finally { clearTimeout(timer); }
  });
}

test('disposing a host cannot schedule a reconnect through close callbacks', () => {
  let closeHandler;
  let closedEvents = 0;
  const host = new EngineHost({ wsClose: () => { closedEvents++; return ''; } }, {
    now: () => 100,
    connect: (_, handlers) => { closeHandler = handlers.onClose; return { close: () => closeHandler(1000) }; },
  });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test' });
  host.dispose();
  closeHandler(1006);
  assert.equal(closedEvents, 0);
  assert.equal(host.sockets.size, 0);
});
