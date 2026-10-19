import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import tls from 'node:tls';
import http from 'node:http';
import WebSocket, { WebSocketServer } from 'ws';
import { makeCA, recordingProxy } from './recorder.mjs';
import { streamRelay, streamKey, verifyMappedDrain, verifyMappedTransportDrain } from './stream-relay.mjs';
import { compareSequences, qualifyComparisons } from './stream-compare.mjs';
import { differences } from './compare.mjs';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
test('ordered channels detect loss, duplication, reordering and non-JSON values', () => {
  const sequence = [
    { kind: 'store', data: { tickers: [{ last: 42 }] } },
    { kind: 'candles', data: { close: 42 } },
    { kind: 'candles', data: { close: 43 } },
  ];
  assert.equal(compareSequences(sequence, structuredClone(sequence)).combined.status, 'PASS');
  for (const mutation of [sequence.slice(1), [...sequence, sequence[2]], [sequence[0], sequence[2], sequence[1]]]) {
    assert.equal(compareSequences(sequence, mutation).combined.status, 'DIFF');
  }
  for (const [a, b] of [
    [new Map([['x', 1]]), new Map([['x', 2]])],
    [new Set([1]), new Set([2])],
    [new Date(1), new Date(2)],
    [Buffer.from([1]), Buffer.from([2])],
    [-0, 0],
    [NaN, null],
    [{ x: undefined }, {}],
  ]) {
    assert.ok(differences(a, b).length, 'non-JSON values cannot silently PASS');
  }
});

test('strict REST replay fails closed without invoking upstream', async (t) => {
  const records = [];
  const proxy = await recordingProxy({
    cache: new Map(),
    records,
    implementation: 'rust',
    allowHost: (host) => host === 'api.bybit.com',
    replayOnly: true,
    forward: () => {
      throw new Error('must never reach upstream');
    },
  });
  t.after(() => proxy.close());
  const status = await new Promise((resolve, reject) => {
    const req = http.get(proxy.url, { path: 'https://api.bybit.com/uncaptured' }, (res) => {
      res.resume();
      res.once('end', () => resolve(res.statusCode));
    });
    req.once('error', reject);
  });
  assert.equal(status, 409);
  assert.equal(records[0].replayMiss, true);
});

test(
  'one global multi-socket tape replays twice identically, retaining binary and control frames',
  { timeout: 20000 },
  async (t) => {
    const directory = mkdtempSync(`${tmpdir()}/stream-relay-test-`),
      ca = makeCA(directory),
      tape = {},
      relays = [],
      clients = [];
    const upstream = new WebSocketServer({ host: '127.0.0.1', port: 0 });
    await new Promise((resolve) => upstream.once('listening', resolve));
    let upstreamConnections = 0;
    upstream.on('connection', (socket, request) => {
      upstreamConnections++;
      socket.once('message', () => {
        if (request.url === '/market') {
          socket.send('ticker-one');
          setTimeout(() => socket.send('ticker-two'), 80);
          setTimeout(() => socket.close(), 110);
        } else {
          setTimeout(() => {
            socket.send(Buffer.from([0, 255, 128]), { binary: true });
            socket.ping('health');
          }, 30);
        }
      });
    });
    t.after(async () => {
      for (const c of clients) c.terminate();
      for (const r of relays) await r.close();
      for (const c of upstream.clients) c.terminate();
      await new Promise((resolve) => upstream.close(resolve));
      rmSync(directory, { recursive: true, force: true });
    });
    async function connect(relay, path, output) {
      const socket = new WebSocket(`wss://public.exchange.test${path}`, {
        createConnection: () =>
          tls.connect({
            host: '127.0.0.1',
            port: relay.ports.v4,
            servername: 'public.exchange.test',
            ca: readFileSync(ca.cert),
          }),
      });
      clients.push(socket);
      socket.on('message', (bytes, binary) =>
        output.push({ path, kind: 'message', base64: bytes.toString('base64'), binary }),
      );
      socket.on('ping', (bytes) => output.push({ path, kind: 'ping', base64: bytes.toString('base64') }));
      socket.on('pong', (bytes) => output.push({ path, kind: 'pong', base64: bytes.toString('base64') }));
      socket.on('close', (code, reason) =>
        output.push({ path, kind: 'close', code, base64: reason.toString('base64') }),
      );
      await new Promise((resolve, reject) => {
        socket.once('open', resolve);
        socket.once('error', reject);
      });
      socket.send('subscribe');
      if (path === '/market') socket.ping('client-health');
      return socket;
    }
    const capture = await streamRelay({
      directory,
      ca,
      hosts: ['public.exchange.test'],
      mode: 'capture',
      tape,
      records: [],
      openUpstream: (url) =>
        new WebSocket(`ws://127.0.0.1:${upstream.address().port}${url.pathname}`, { autoPong: false }),
    });
    relays.push(capture);
    const original = [];
    await connect(capture, '/market', original);
    await connect(capture, '/public', original);
    await sleep(180);
    capture.freeze();
    const expected = structuredClone(original);
    await capture.close();
    assert.equal(original.find((f) => f.kind === 'close')?.code, 1005, 'an empty upstream close stays an empty close');
    assert.equal(tape.connections.length, 2);
    assert.ok(tape.frames.some((f) => f.binary === true));
    assert.ok(tape.frames.some((f) => f.kind === 'pong'));
    assert.deepEqual(
      tape.frames.filter((f) => f.kind === 'message').map((f) => f.connection),
      [0, 1, 0],
    );
    const copies = [];
    for (let i = 0; i < 4; i++) {
      const records = [],
        output = [];
      const relay = await streamRelay({
        directory,
        ca,
        hosts: ['public.exchange.test'],
        mode: 'replay',
        tape,
        records,
      });
      relays.push(relay);
      await connect(relay, '/market', output);
      await connect(relay, '/public', output);
      await sleep(20);
      assert.deepEqual(output, [], 'replay does not synthesize unrecorded pongs');
      assert.equal(await relay.ready(), true);
      const receipt = await relay.play();
      await sleep(20);
      assert.equal(receipt.dropped, 0);
      assert.equal(receipt.inputHash, receipt.expectedInputHash);
      assert.equal(receipt.delivered, tape.frames.length);
      copies.push(structuredClone(output));
      await relay.close();
    }
    for (const copy of copies) assert.deepEqual(copy, expected);
    assert.equal(upstreamConnections, 2, 'four replay clients never open a new upstream');
    const missing = await streamRelay({
      directory,
      ca,
      hosts: ['public.exchange.test'],
      mode: 'replay',
      tape,
      records: [],
    });
    relays.push(missing);
    const receipt = await missing.play();
    assert.equal(receipt.dropped, tape.frames.length, 'missing sockets without records cannot pass delivery checks');
    assert.notEqual(receipt.inputHash, receipt.expectedInputHash);
    assert.equal(streamKey('wss://x/ws?b=2&requestId=a&a=1'), streamKey('wss://x/ws?a=1&b=2&requestId=b'));
    assert.notEqual(streamKey('wss://x/ws?a=1'), streamKey('wss://x/ws?a=2'));
  },
);

test('cross-side attribution requires repeatable output and complete inputs/barriers', () => {
  const a = [{ kind: 'candles', data: { close: 42 } }],
    b = [{ kind: 'candles', data: { close: 43 } }];
  const parity = () => compareSequences(a, b);
  const repeats = { ts: compareSequences(a, a), rust: compareSequences(b, b) };
  assert.equal(qualifyComparisons(parity(), repeats).candles.qualifiedStatus, 'DIFF');
  assert.equal(
    qualifyComparisons(parity(), { ...repeats, ts: compareSequences(a, b) }).candles.qualifiedStatus,
    'NONDETERMINISTIC',
  );
  assert.equal(qualifyComparisons(parity(), repeats, { inputDiff: true }).candles.qualifiedStatus, 'INPUT_DIFF');
  assert.equal(qualifyComparisons(parity(), repeats, { incomplete: true }).candles.qualifiedStatus, 'INCOMPLETE');
});

test('duplicate URLs bind by exact subscription rather than network accept order', { timeout: 15000 }, async (t) => {
  const directory = mkdtempSync(`${tmpdir()}/duplicate-dial-test-`),
    ca = makeCA(directory, undefined);
  const key = 'wss://same.public.test/feed';
  const tape = {
    connections: [0, 1].map((id) => ({ id, key, url: key, occurrence: id, openedAtMs: 0 })),
    frames: [0, 1].map((connection) => ({
      connection,
      atMs: connection,
      kind: 'message',
      binary: false,
      base64: Buffer.from(`frame-${connection}`).toString('base64'),
    })),
    durationMs: 2,
  };
  const capturedSockets = [0, 1].map((occurrence) => ({
    key,
    occurrence,
    sent: [{ binary: false, base64: Buffer.from(`subscribe-${occurrence}`).toString('base64') }],
  }));
  const clients = [],
    counters = [],
    got = [],
    records = [];
  const relay = await streamRelay({
    directory,
    ca,
    hosts: ['same.public.test'],
    mode: 'replay',
    tape,
    records,
    capturedSockets,
  });
  t.after(async () => {
    for (const client of clients) client.terminate();
    await relay.close();
    rmSync(directory, { recursive: true, force: true });
  });
  // Construct and accept the logical second socket first.
  for (const logical of [1, 0]) {
    const counter = { url: key, consumed: 0, processed: 0, errors: 0, closeConsumed: 0, closeProcessed: 0 };
    counters.push(counter);
    const client = new WebSocket(key, {
      createConnection: () =>
        tls.connect({
          host: '127.0.0.1',
          port: relay.ports.v4,
          servername: 'same.public.test',
          ca: readFileSync(ca.cert),
        }),
    });
    clients.push(client);
    client.on('message', (bytes) => {
      counter.consumed++;
      got.push({ logical, body: bytes.toString() });
      counter.processed++;
    });
    await new Promise((resolve, reject) => {
      client.once('open', resolve);
      client.once('error', reject);
    });
    client.send(`subscribe-${logical}`);
  }
  assert.equal(await relay.ready(3000), true);
  const receipt = await relay.playControlled({
    advance: async () => {},
    receipt: async () => ({ sockets: counters, pendingJobs: 0 }),
    timeoutMs: 3000,
  });
  assert.deepEqual(got, [
    { logical: 0, body: 'frame-0' },
    { logical: 1, body: 'frame-1' },
  ]);
  assert.equal(receipt.applicationDrainVerified, true);
  assert.equal(receipt.inputHash, receipt.expectedInputHash);
  assert.equal(receipt.dialBindings.parser[`${key}#0`].index, 1);
  assert.equal(receipt.dialBindings.parser[`${key}#1`].index, 0);
  assert.deepEqual(
    records.map((r) => [r.networkOccurrence, r.occurrence]),
    [
      [0, 1],
      [1, 0],
    ],
  );
});

test('duplicate-dial bindings reject extra work, ambiguous vectors and later counter migration', () => {
  const key = 'wss://same.public.test/feed',
    tape = { connections: [0, 1].map((occurrence) => ({ id: occurrence, key, occurrence })) };
  const row = (processed) => ({
    url: key,
    consumed: processed,
    processed,
    errors: 0,
    closeConsumed: 0,
    closeProcessed: 0,
  });
  const expected = new Map([[`${key}#0`, 1]]),
    bindings = new Map();
  assert.equal(
    verifyMappedDrain(tape, expected, { sockets: [row(0), row(1)], pendingJobs: 0 }, new Map(), bindings),
    true,
  );
  assert.equal(bindings.get(`${key}#0`).index, 1);
  assert.equal(
    verifyMappedDrain(tape, expected, { sockets: [row(1), row(0)], pendingJobs: 0 }, new Map(), bindings),
    false,
    'identity never migrates to another receipt index',
  );
  assert.equal(
    verifyMappedDrain(tape, expected, { sockets: [row(1), row(1)], pendingJobs: 0 }, new Map(), bindings),
    false,
    'extra work cannot be hidden in URL totals',
  );
  const ambiguous = new Map();
  assert.equal(
    verifyMappedDrain(
      tape,
      new Map([
        [`${key}#0`, 1],
        [`${key}#1`, 1],
      ]),
      { sockets: [row(1), row(1)], pendingJobs: 0 },
      new Map(),
      ambiguous,
    ),
    false,
  );
  assert.equal(ambiguous.size, 0, 'failed observations do not commit guessed identities');
  const physical = new Map(),
    counts = new Map([
      [`${key}#0`, { message: 1, ping: 0, pong: 0, close: 0 }],
      [`${key}#1`, { message: 0, ping: 0, pong: 0, close: 0 }],
    ]);
  const transport = (n) => ({ url: key, counts: { message: n, ping: 0, pong: 0, close: 0 } });
  assert.equal(verifyMappedTransportDrain(tape, { transport: [transport(0), transport(1)] }, counts, physical), true);
  assert.equal(verifyMappedTransportDrain(tape, { transport: [transport(1), transport(0)] }, counts, physical), false);
});
