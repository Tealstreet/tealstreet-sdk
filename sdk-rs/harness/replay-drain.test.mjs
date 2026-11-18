import assert from 'node:assert/strict';
import test from 'node:test';
import { gunzipSync } from 'node:zlib';
import { socketMessages } from '../hosts/wasm/js/host.mjs';
import { spotReplayRecords } from './bingxspot-fixtures.mjs';
import { Tape } from './tape.mjs';

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
