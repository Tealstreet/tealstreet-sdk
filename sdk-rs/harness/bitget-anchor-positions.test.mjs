import assert from 'node:assert/strict';
import test from 'node:test';
import { Frame, SECTION } from '../hosts/wasm/js/wire.mjs';
import { replayPositions } from './bitget-anchor-positions.mjs';

// FrameWriter's canonical empty POSITIONS payload: count + zero alignment word.
function positions(revision = 1, count = 0, length = 8, padding = 0) {
  const buffer = new ArrayBuffer(24 + length), view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setUint16(16, SECTION.POSITIONS, true);
  view.setUint16(18, revision, true);
  view.setUint32(20, length, true);
  if (length >= 4) view.setUint32(24, count, true);
  if (length >= 8) view.setUint32(28, padding, true);
  return new Frame(buffer);
}

test('frozen anchor empty positions decode like current empty positions in public Bitget replay', () => {
  const legacy = positions(), current = positions(2);
  assert.throws(() => legacy.positions(), /unsupported POSITIONS revision/);
  const before = Buffer.from(legacy.buffer).toString('hex');
  assert.equal(replayPositions(legacy, 'bitget', false).count, current.positions().count);
  assert.equal(Buffer.from(legacy.buffer).toString('hex'), before);
  assert.equal(replayPositions(current, 'bitget', false).market.buffer, current.buffer);
});

test('compatibility cannot admit private, other venue, nonempty or malformed legacy positions', () => {
  for (const [venue, privateTape] of [['bitget', true], ['bitget', undefined], ['bybit', false], ['bitgetusdc', false]]) {
    assert.throws(() => replayPositions(positions(), venue, privateTape), /unsupported POSITIONS revision/);
  }
  for (const frame of [positions(0), positions(1, 1), positions(1, 0, 4),
    positions(1, 0, 16), positions(1, 0, 8, 1)]) {
    assert.throws(() => replayPositions(frame, 'bitget', false), /unsupported POSITIONS revision/);
  }
  const absent = positions();
  absent.sections = [];
  assert.equal(replayPositions(absent, 'bitget', false), undefined);
  assert.throws(() => replayPositions(positions(2, 1), 'bitget', false), /truncated section/);
});
