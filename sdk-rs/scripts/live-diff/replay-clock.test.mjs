import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { verifyDrain, verifyRawTaskDrain, verifyTransportDrain, streamKey } from './stream-relay.mjs';

test('replay clock controls Date, performance, timer ordering and cancellation in two fresh processes', () => {
  const code = `
    import { replayClock } from ${JSON.stringify(new URL('./replay-clock.mjs', import.meta.url).href)};
    import assert from 'node:assert/strict';
    const clock = replayClock(1800000000000);
    const events = [];
    clock.begin();
    const cancelled = setTimeout(() => { throw new Error('cancelled callback'); }, 200);
    clearTimeout(cancelled);
    for (const [id, delay] of [[1,250],[2,100],[3,250]]) {
      setTimeout(() => events.push([id, Date.now(), new Date().valueOf(), performance.now()]), delay);
    }
    await clock.advance(99.5);
    assert.deepEqual(events, []);
    await clock.advance(250.75);
    assert.equal(Date.now(), 1800000000250);
    assert.equal(performance.now(), 250.75);
    assert.deepEqual(events, [[2,1800000000100,1800000000100,100],[1,1800000000250,1800000000250,250],[3,1800000000250,1800000000250,250]]);
    await assert.rejects(clock.advance(200));
    process.stdout.write(JSON.stringify(events));
  `;
  const runs = [0, 1].map(() =>
    spawnSync(process.execPath, ['--input-type=module', '-e', code], { encoding: 'utf8', timeout: 10000 }),
  );
  for (const run of runs) assert.equal(run.status, 0, run.stderr);
  assert.equal(runs[0].stdout, runs[1].stdout);
});

test('drain refuses queued, missing, duplicate, extra socket and unread control frames', () => {
  const url = 'wss://public.exchange.test/ticker',
    key = streamKey(url);
  const tape = {
    connections: [{ id: 0, key, occurrence: 0 }],
    frames: [
      { connection: 0, kind: 'message' },
      { connection: 0, kind: 'ping' },
    ],
  };
  const expected = new Map([[`${key}#0`, 1]]);
  const receipt = {
    pendingJobs: 0,
    rawTasks: {
      version: 1,
      scope: 'exchange-runtime',
      unknown: 0,
      finite: 0,
      ready: 0,
      waitingTimers: 2,
      waitingSockets: 1,
      promiseDrivers: 1,
      blocking: 0,
    },
    sockets: [{ url, consumed: 1, processed: 1 }],
    transport: [{ url, counts: { message: 1, ping: 1, pong: 0, close: 0 } }],
  };
  assert.equal(verifyDrain(tape, expected, receipt), true);
  assert.equal(verifyDrain(tape, expected, { ...receipt, rawTasks: undefined }), false);
  assert.equal(verifyDrain(tape, expected, { ...receipt, pendingJobs: undefined }), false);
  assert.equal(verifyDrain(tape, expected, { ...receipt, pendingJobs: 1 }), false);
  assert.equal(verifyTransportDrain(tape, receipt), true);
  for (const [consumed, processed] of [
    [1, 0],
    [0, 0],
    [2, 2],
  ]) {
    assert.equal(verifyDrain(tape, expected, { sockets: [{ url, consumed, processed }] }), false);
  }
  assert.equal(verifyDrain(tape, expected, { sockets: [...receipt.sockets, ...receipt.sockets] }), false);
  assert.equal(verifyDrain(tape, expected, { sockets: [] }), false);
  assert.equal(
    verifyTransportDrain(tape, {
      ...receipt,
      transport: [{ url, counts: { message: 1, ping: 0, pong: 0, close: 0 } }],
    }),
    false,
  );
});

test('native raw-task drain rejects legacy, unknown, finite, awake and malformed receipts', () => {
  const tasks = {
    version: 1,
    scope: 'exchange-runtime',
    unknown: 0,
    finite: 0,
    ready: 0,
    waitingTimers: 2,
    waitingSockets: 1,
    promiseDrivers: 1,
    blocking: 0,
  };
  assert.equal(verifyRawTaskDrain(tasks), true);
  for (const invalid of [
    undefined,
    {},
    { ...tasks, version: 2 },
    { ...tasks, scope: 'all' },
    { ...tasks, unknown: 1, blocking: 1 },
    { ...tasks, finite: 1, blocking: 1 },
    { ...tasks, ready: 1, blocking: 1 },
    { ...tasks, unknown: 1 },
    { ...tasks, finite: -1 },
    { ...tasks, ready: NaN },
    { ...tasks, waitingTimers: 0.5 },
  ]) {
    assert.equal(verifyRawTaskDrain(invalid), false);
  }
});

test(
  'controlled TLS replay waits for handlers and close before advancing the next clock input',
  { timeout: 10000 },
  async (t) => {
    const { mkdtempSync, readFileSync, rmSync } = await import('node:fs');
    const { tmpdir } = await import('node:os');
    const tls = await import('node:tls');
    const { default: WebSocket } = await import('ws');
    const { makeCA } = await import('./recorder.mjs');
    const { streamRelay } = await import('./stream-relay.mjs');
    const directory = mkdtempSync(`${tmpdir()}/controlled-stream-test-`),
      ca = makeCA(directory);
    const url = 'wss://public.exchange.test/market';
    const tape = {
      connections: [{ id: 0, key: streamKey(url), occurrence: 0, url }],
      durationMs: 40,
      frames: [
        { connection: 0, kind: 'message', atMs: 10.25, base64: Buffer.from('one').toString('base64'), binary: false },
        { connection: 0, kind: 'message', atMs: 10.75, base64: Buffer.from([0, 255]).toString('base64'), binary: true },
        { connection: 0, kind: 'ping', atMs: 11.25, base64: Buffer.from('health').toString('base64') },
        { connection: 0, kind: 'close', atMs: 20.25, code: 1005, base64: '' },
      ],
    };
    const relay = await streamRelay({
      directory,
      ca,
      hosts: ['public.exchange.test'],
      mode: 'replay',
      tape,
      records: [],
    });
    let socket;
    t.after(async () => {
      socket?.terminate();
      await relay.close();
      rmSync(directory, { recursive: true, force: true });
    });
    socket = new WebSocket(url, {
      createConnection: () =>
        tls.connect({
          host: '127.0.0.1',
          port: relay.ports.v4,
          servername: 'public.exchange.test',
          ca: readFileSync(ca.cert),
        }),
    });
    const counts = { message: 0, ping: 0, pong: 0, close: 0 };
    const received = { url, consumed: 0, processed: 0, closeConsumed: 0, closeProcessed: 0 };
    let now = 0;
    const outputs = [];
    socket.on('message', (bytes, binary) => {
      counts.message++;
      received.consumed++;
      setTimeout(() => {
        outputs.push([now, bytes.toString('base64'), binary]);
        received.processed++;
      }, 10);
    });
    socket.on('ping', () => counts.ping++);
    socket.on('close', () => {
      counts.close++;
      received.closeConsumed++;
      received.closeProcessed++;
    });
    await new Promise((resolve, reject) => {
      socket.once('open', resolve);
      socket.once('error', reject);
    });
    const result = await relay.playControlled({
      advance: async (at) => {
        assert.equal(received.consumed, received.processed);
        now = at;
      },
      receipt: async () => ({
        pendingJobs: 0,
        rawTasks: {
          version: 1,
          scope: 'exchange-runtime',
          unknown: 0,
          finite: 0,
          ready: 0,
          waitingTimers: 0,
          waitingSockets: 1,
          promiseDrivers: 0,
          blocking: 0,
        },
        sockets: [structuredClone(received)],
        transport: [{ url, counts: structuredClone(counts) }],
      }),
      timeoutMs: 1000,
    });
    assert.equal(result.applicationDrainVerified, true);
    assert.equal(result.nativeTransportVerified, true);
    assert.deepEqual(outputs, [
      [10.25, Buffer.from('one').toString('base64'), false],
      [10.75, Buffer.from([0, 255]).toString('base64'), true],
    ]);
    assert.equal(result.delivered, 4);
  },
);

test('TS HTTP completion receipt waits for response processing', () => {
  const code = `
    import { httpReceipts } from ${JSON.stringify(new URL('./replay-clock.mjs', import.meta.url).href)};
    import assert from 'node:assert/strict';
    import http from 'node:http';
    import { setImmediate as turn } from 'node:timers';
    const pending = httpReceipts();
    const server = http.createServer((req,res) => res.end('snapshot'));
    await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
    let processed = false;
    await new Promise((resolve, reject) => {
      const req = http.request({ host:'127.0.0.1', port:server.address().port }, res => {
        res.resume();
        res.on('end', () => { processed = true; resolve(); });
      });
      req.once('error',reject);
      req.end();
      assert.equal(pending(),1);
    });
    await new Promise(turn);
    assert.equal(processed,true);
    assert.equal(pending(),0);
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  `;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr);
});

test('TS WebSocket receipts include returned async listener work and preserve removal', () => {
  const code = `
    import { socketReceipts } from ${JSON.stringify(new URL('./replay-clock.mjs', import.meta.url).href)};
    import assert from 'node:assert/strict';
    import { WebSocketServer } from 'ws';
    import { setImmediate as turn } from 'node:timers';
    const receipts = socketReceipts();
    const server = new WebSocketServer({host:'127.0.0.1',port:0});
    await new Promise(resolve => server.once('listening',resolve));
    let peer;
    server.on('connection', socket => {peer=socket;});
    const socket = new WebSocket('ws://127.0.0.1:'+server.address().port);
    await new Promise(resolve => socket.addEventListener('open',resolve,{once:true}));
    const removed = () => {throw new Error('removed listener ran');};
    socket.addEventListener('message',removed);
    socket.removeEventListener('message',removed);
    let finish;
    socket.addEventListener('message',async () => await new Promise(resolve => {finish=resolve;}));
    peer.send('snapshot');
    while (!finish) await new Promise(turn);
    assert.equal(receipts.sockets()[0].consumed,1);
    assert.equal(receipts.pendingJobs(),1);
    finish();
    await new Promise(turn);
    assert.equal(receipts.pendingJobs(),0);
    assert.equal(receipts.sockets()[0].processed,1);
    socket.close();
    await new Promise(resolve => socket.addEventListener('close',resolve,{once:true}));
    await new Promise(turn);
    assert.equal(receipts.sockets()[0].closeProcessed,1);
    await new Promise(resolve => server.close(resolve));
  `;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr);
});
