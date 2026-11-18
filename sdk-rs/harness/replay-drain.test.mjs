import assert from 'node:assert/strict';
import test from 'node:test';
import { gunzipSync } from 'node:zlib';
import { socketMessages } from '../hosts/wasm/js/host.mjs';
import { spotReplayRecords } from './bingxspot-fixtures.mjs';
import { expectedCheckpointPlan, Tape } from './tape.mjs';
import { replaySocketMessages } from './replay-messages.mjs';

const turn = () => new Promise(setImmediate);
const deferred = () => Promise.withResolvers();
const first = spotReplayRecords().find(frame => frame.binary !== undefined);
const compressed = Buffer.from(first.binary, 'base64');
const decoded = gunzipSync(compressed).toString('utf8');

function holdDecompression(t) {
  const original = globalThis.DecompressionStream;
  const gate = deferred();
  const entered = deferred();
  globalThis.DecompressionStream = class {
    constructor(format) {
      const stream = new original(format);
      this.writable = stream.writable;
      this.readable = stream.readable.pipeThrough(new TransformStream({
        async transform(chunk, controller) {
          entered.resolve();
          await gate.promise;
          controller.enqueue(chunk);
        },
      }));
    }
  };
  t.after(() => { gate.resolve(); globalThis.DecompressionStream = original; });
  return { entered: entered.promise, release: gate.resolve };
}

for (const boundary of ['initial checkpoint', 'tick', 'checkpoint', 'finalization']) {
  test(`held gzip drains before ${boundary}`, { timeout: 3000 }, async t => {
    const hold = holdDecompression(t);
    const done = deferred();
    const messages = [];
    let pending;
    const receive = socketMessages(value => {
      messages.push(value);
      if (value === 'after gzip') done.resolve();
    }, done.reject);
    const enqueue = () => { pending = done.promise; receive(compressed); receive('after gzip'); };
    const tape = new Tape([{ type: 'wait', t: 1 }], {}, false, 1000);
    const observed = [];
    let completed = false;
    const work = tape.play(
      () => {
        if (['checkpoint', 'finalization'].includes(boundary)) enqueue();
        if (boundary === 'tick') observed.push([...messages]);
      },
      undefined, undefined,
      { start() {}, advance() { if (boundary === 'tick') enqueue(); } },
      () => { if (boundary === 'initial checkpoint') enqueue(); },
      point => {
        if ((boundary === 'initial checkpoint' && point.ordinal === 0)
          || (boundary === 'checkpoint' && point.ordinal === 1)) observed.push([...messages]);
      },
      () => pending ?? Promise.resolve(),
    ).then(value => {
      completed = true;
      if (boundary === 'finalization') observed.push([...messages]);
      return value;
    });
    // Ordinary event-loop yields must not stand in for decoder completion.
    try {
      await hold.entered;
      for (let i = 0; i < 20; i++) await turn();
    } finally {
      hold.release();
      await work;
      await turn();
    }
    assert.equal(completed, true);
    assert.deepEqual(observed, [[decoded, 'after gzip']]);
  });
}

test('ordinary text and tapes without a drain retain their observations', async () => {
  const messages = [];
  const receive = socketMessages(value => messages.push(value), error => { throw error; });
  receive('synchronous');
  assert.deepEqual(messages, ['synchronous']);
  const tape = new Tape([{ type: 'wait', t: 1 }]);
  const observations = [];
  assert.equal(await tape.play(() => observations.push('tick'), undefined, undefined,
    undefined, undefined, point => observations.push(point.ordinal)), 1);
  assert.deepEqual(observations, [0, 'tick', 1]);
});

test('receipts preserve synchronous text and await handler completion', async () => {
  const messages = [];
  const handler = deferred();
  const receive = replaySocketMessages(value => { messages.push(value); return handler.promise; });
  let settled = false;
  const receipt = receive('text').then(() => { settled = true; });
  assert.deepEqual(messages, ['text']);
  await turn();
  assert.equal(settled, false);
  handler.resolve();
  await receipt;
  assert.equal(settled, true);
});

test('all 241 Spot frames drain in order at the original checkpoint ordinals', { timeout: 3000 }, async t => {
  const hold = holdDecompression(t);
  const records = spotReplayRecords();
  const frames = records.filter(frame => frame.type === 'ws');
  const expected = frames.map(frame => frame.binary === undefined
    ? frame.text : gunzipSync(Buffer.from(frame.binary, 'base64')).toString('utf8'));
  const messages = [];
  const pending = new Set();
  let failure;
  const track = promise => {
    pending.add(promise);
    promise.then(() => pending.delete(promise), error => { pending.delete(promise); failure = error; });
  };
  const read = deferred();
  const receive = replaySocketMessages(value => {
    messages.push(value);
    if (messages.length === 1) track(read.promise);
  });
  const drain = async () => {
    while (pending.size) await Promise.all(pending);
    if (failure) throw failure;
  };
  const tape = new Tape(records, {}, false, 1000, 'ts');
  tape.connect(first.url, () => {}, value => track(receive(value)), () => {});
  const points = [];
  const ticks = [];
  const work = tape.play(() => ticks.push(messages.length), undefined, undefined, undefined,
    undefined, point => {
      points.push(point);
      assert.deepEqual(messages, expected.slice(0, point.ordinal));
    }, drain);
  try {
    await hold.entered;
    for (let i = 0; i < 20; i++) await turn();
    assert.deepEqual(ticks, []);
    hold.release();
    // Decoder completion also waits for account work it starts.
    await turn();
    assert.deepEqual(ticks, []);
  } finally {
    hold.release();
    read.resolve();
    await work;
  }
  assert.equal(frames.length, 241);
  assert.deepEqual(messages, expected);
  assert.deepEqual(ticks, frames.map((_, index) => index + 1));
  assert.deepEqual(points, expectedCheckpointPlan(records));
  assert.equal(pending.size, 0);
});

for (const kind of ['malformed UTF-8', 'handler throw', 'handler rejection']) {
  test(`decoder receipts reject pending and future work on ${kind}`, async () => {
    const messages = [];
    const failure = new Error(kind);
    const receive = replaySocketMessages(value => {
      messages.push(value);
      if (kind === 'handler throw') throw failure;
      if (kind === 'handler rejection') return Promise.reject(failure);
    });
    const receipts = [
      receive(kind === 'malformed UTF-8' ? new Uint8Array([0xff]) : compressed),
      receive('queued text'),
    ];
    const results = await Promise.allSettled(receipts);
    assert(results.every(result => result.status === 'rejected'));
    const error = results[0].reason;
    if (kind === 'malformed UTF-8') assert(error instanceof TypeError);
    else assert.equal(error, failure);
    assert.equal(results[1].reason, error);
    await assert.rejects(receive('after failure'), candidate => candidate === error);
    assert.deepEqual(messages, kind === 'malformed UTF-8' ? [] : [decoded]);
  });
}

test('a stalled decoder drain fails within the existing replay deadline', async () => {
  const tape = new Tape([], {}, false, 10);
  let observed = false;
  await assert.rejects(tape.play(undefined, undefined, undefined, undefined, undefined,
    () => { observed = true; }, () => new Promise(() => {})), /replay timeout: Replay pending messages/);
  assert.equal(observed, false);
});
