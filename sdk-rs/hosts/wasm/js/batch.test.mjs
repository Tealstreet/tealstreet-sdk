import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const { WasmEngine } = createRequire(import.meta.url)('../pkg/node/okx/safe_cex_wasm.js');
const pack = (socket, now, text) => {
  const bytes = new TextEncoder().encode(text);
  const batch = new Uint8Array(16 + bytes.length);
  const view = new DataView(batch.buffer);
  view.setUint32(0, socket, true);
  view.setUint32(4, bytes.length, true);
  view.setFloat64(8, now, true);
  batch.set(bytes, 16);
  return batch;
};

const initialized = () => {
  const engine = new WasmEngine('okx', '{"publicDataOnly":true}');
  try {
    let actions = JSON.parse(engine.start(100));
    for (let i = 0; i < 2; i++) {
      const { request } = actions.find((a) => a.op === 'http');
      engine.httpStart(request, 200, '', 100);
      engine.httpChunk(request, new TextEncoder().encode('{"code":"0","data":[]}'), 100);
      actions = JSON.parse(engine.httpEnd(request, 100));
    }
    const { socket } = actions.find((a) => a.op === 'wsConnect' && a.url.endsWith('/public'));
    engine.wsOpen(socket, 101);
    return { engine, socket };
  } catch (error) {
    engine.free();
    throw error;
  }
};

test('real wasm single and batched delivery preserve Unicode actions and snapshot bytes', () => {
  const single = initialized();
  let batch;
  try {
    batch = initialized();
    const payloads = [
      '',
      '{"event":"error",\n"code":"60001","msg":"雪😀line"}',
      '{"event":"error","code":"60001","msg":"raw-high:\ud800"}',
      '{"event":"error","code":"60001","msg":"raw-low:\udfff"}',
      '{"event":"error","code":"60001","msg":"雪\ud800😀\udfff"}',
    ];
    const actions = (json) => json ? JSON.parse(json) : [];
    for (const [index, text] of payloads.entries()) {
      const now = 102 + index;
      const direct = actions(single.engine.wsMessage(single.socket, text, now));
      const encoded = actions(batch.engine.wsMessages(pack(batch.socket, now, text)));
      assert.deepEqual(direct, encoded, `actions for payload ${index}`);
      assert.deepEqual(single.engine.snapshotFrame(now), batch.engine.snapshotFrame(now),
        `snapshot bytes for payload ${index}`);
      if (index >= 2) {
        assert(text.includes('\ud800') || text.includes('\udfff'), 'the JS input contains a raw lone surrogate');
        assert.match(JSON.stringify(direct), /�/, 'both UTF-8 crossings replace the raw surrogate');
      }
      if (index === 1) assert.match(JSON.stringify(direct), /雪😀/);
    }
  } finally {
    single.engine.free();
    batch?.engine.free();
  }
});

test('real wasm rejects malformed batches atomically and preserves single delivery actions', () => {
  const { engine, socket } = initialized();
  try {
    const text = JSON.stringify({ event: 'error', code: '60001', msg: '雪😀' });
    const good = pack(socket, 102, text);
    const malformed = [
      new Uint8Array([0]),
      pack(socket, NaN, text),
      good.subarray(0, good.length - 1),
      Uint8Array.from([...pack(socket, 102, ''), 0]),
      (() => { const b = pack(socket, 102, 'a'); b[16] = 255; return b; })(),
    ];
    for (const tail of malformed) {
      const batch = new Uint8Array(good.length + tail.length);
      batch.set(good);
      batch.set(tail, good.length);
      assert.throws(() => engine.wsMessages(batch), /batch|arrival/);
      const one = JSON.parse(engine.wsMessages(good));
      assert.equal(one.length, 1, 'a rejected batch must not leave actions from its valid prefix');
      assert.deepEqual(one, JSON.parse(engine.wsMessage(socket, text, 102)));
    }
    assert.equal(engine.wsMessages(new Uint8Array()), '');
  } finally { engine.free(); }
});
