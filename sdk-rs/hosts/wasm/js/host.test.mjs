import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:net';
import { EngineHost, nativeTransport } from './host.mjs';

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

for (const continuation of ['chunk', 'end', 'error']) {
  test(`a body awaiting ${continuation} cannot call a freed engine after disposal`, async () => {
    let resume;
    const waiting = new Promise((resolve) => { resume = resolve; });
    let delivered;
    const firstChunk = new Promise((resolve) => { delivered = resolve; });
    let freed = false;
    let closed = false;
    const calls = [];
    const record = (name) => {
      assert.equal(freed, false, `${name} called a freed engine`);
      calls.push(name);
      return '';
    };
    const engine = {
      httpStart: () => record('start'),
      httpChunk: () => { delivered(); return record('chunk'); },
      httpEnd: () => record('end'),
      httpFailed: () => record('error'),
    };
    const host = new EngineHost(engine, { now: () => 100 });
    const body = async function* () {
      try {
        yield Uint8Array.of(1);
        await waiting;
        if (continuation === 'chunk') yield Uint8Array.of(2);
        if (continuation === 'error') throw new Error('late body failure');
      } finally { closed = true; }
    };
    const receiving = host.receive(1, { status: 200, body: body() });
    await firstChunk;
    host.dispose();
    freed = true;
    resume();
    await receiving;
    assert.deepEqual(calls, ['start', 'chunk']);
    assert.equal(closed, true);
  });
}

test('late read and write request failures cannot reach a disposed engine or retry', async () => {
  for (const method of ['GET', 'POST']) {
    let reject;
    let requests = 0;
    let failures = 0;
    const host = new EngineHost({ httpFailed: () => { failures++; return ''; } }, {
      now: () => 100,
      request: () => { requests++; return new Promise((_, fail) => { reject = fail; }); },
    });
    host.execute({ op: 'http', request: 1, method, url: 'https://example.test', headers: [] });
    host.dispose();
    reject(new Error('late request failure'));
    await new Promise(setImmediate);
    assert.equal(failures, 0);
    assert.equal(requests, 1);
  }
});

test('already queued timers cannot enter the engine after disposal', () => {
  const callbacks = [];
  let calls = 0;
  const host = new EngineHost({
    timer: () => { calls++; return ''; },
    wsClose: () => { calls++; return ''; },
    start: () => { calls++; return ''; },
    takeFrame: () => { calls++; },
    snapshotFrame: () => { calls++; },
  }, {
    now: () => 100,
    setTimeout: (fn) => { callbacks.push(fn); return callbacks.length; },
    clearTimeout() {},
  });
  host.execute({ op: 'setTimer', timer: 1, at: 101 });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test', headers: [['Auth', 'secret']] });
  host.dispose();
  for (const callback of callbacks) callback();
  host.start();
  host.flush();
  assert.throws(() => host.snapshot(), /disposed/);
  assert.equal(calls, 0);
});
