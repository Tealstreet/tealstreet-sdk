import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineDisposedError, EngineHost } from './host.mjs';

const rig = (applyFrame) => {
  let time = 5;
  const sent = [];
  const engine = {
    command: () => '[]',
    ...(applyFrame ? { applyFrame } : {}),
  };
  const transport = {
    now: () => time,
    setTimeout: () => 1,
    clearTimeout() {},
    request: (request) => sent.push(request),
  };
  const host = new EngineHost(engine, transport, {});
  return { host, sent, tick: (ms) => { time += ms; } };
};

test('applyFrame hands the exact bytes, snapshot flag and clock to the engine', () => {
  const seen = [];
  const r = rig((bytes, snapshot, now) => {
    seen.push({ bytes: [...bytes], snapshot, now });
    return '[]';
  });
  r.host.applyFrame(new Uint8Array([1, 2, 3]).buffer, true);
  r.tick(2);
  r.host.applyFrame(new Uint8Array([4]), false);
  assert.deepEqual(seen, [
    { bytes: [1, 2, 3], snapshot: true, now: 5 },
    { bytes: [4], snapshot: false, now: 7 },
  ]);
});

test('applyFrame runs the actions the ingest releases, such as a held write settling', async () => {
  const r = rig(() => JSON.stringify([{ op: 'done', command: 1, result: '["placed-1"]' }]));
  const placed = r.host.command({ op: 'placeOrders', orders: [] });
  r.host.applyFrame(new Uint8Array([0]), false);
  assert.deepEqual(await placed, ['placed-1']);
});

test('an ingest refusal reaches the caller; a disposed or ingest-less host refuses', () => {
  const r = rig(() => {
    throw new Error('frame market index 9 is not in the source table');
  });
  assert.throws(() => r.host.applyFrame(new Uint8Array([0]), false), /source table/);
  assert.throws(() => rig().host.applyFrame(new Uint8Array([0]), true), /no frame ingest/);
  const disposed = rig(() => '[]');
  disposed.host.dispose();
  assert.throws(() => disposed.host.applyFrame(new Uint8Array([0]), true), EngineDisposedError);
});

test('setSourceClockOffset reaches the engine, and an older engine without it is left alone', () => {
  const offsets = [];
  const r = rig(() => '[]');
  r.host.engine.setSourceClockOffset = (offset) => offsets.push(offset);
  r.host.setSourceClockOffset(1_250);
  r.host.setSourceClockOffset(NaN);
  assert.deepEqual(offsets, [1_250, undefined]);
  assert.doesNotThrow(() => rig(() => '[]').host.setSourceClockOffset(0));
});
