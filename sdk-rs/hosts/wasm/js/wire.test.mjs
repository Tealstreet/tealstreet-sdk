import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Frame, SECTION } from './wire.mjs';

function settingsFrame(scalar, sides) {
  const count = scalar.length;
  const data = (4 + count * 8 + 7) & ~7;
  const length = data + count * 8 * (sides ? 3 : 1);
  const buffer = new ArrayBuffer(16 + 8 + length + 8 + 16);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 2, true);
  view.setUint16(16, SECTION.SETTINGS, true);
  view.setUint32(20, length, true);
  view.setUint32(24, count, true);
  for (let i = 0; i < count; i++) {
    view.setUint32(28 + i * 4, i, true);
    view.setUint32(28 + count * 4 + i * 4, 2, true);
    view.setFloat64(24 + data + i * 8, scalar[i], true);
    if (sides) {
      view.setFloat64(24 + data + count * 8 + i * 8, sides[0][i], true);
      view.setFloat64(24 + data + count * 16 + i * 8, sides[1][i], true);
    }
  }
  const status = 24 + length;
  view.setUint16(status, SECTION.STATUS, true);
  view.setUint32(status + 4, 16, true);
  view.setUint32(status + 8, 31, true);
  view.setFloat64(status + 16, 123, true);
  return buffer;
}

test('settings retain the scalar offset and independent side columns after transfer', () => {
  const frame = new Frame(structuredClone(settingsFrame([20, 15], [[20, NaN], [30, NaN]])));
  const settings = frame.settings();
  assert.deepEqual([...settings.leverage], [20, 15]);
  assert.deepEqual([...settings.buyLeverage], [20, NaN]);
  assert.deepEqual([...settings.sellLeverage], [30, NaN]);
  assert.equal(settings.marginType(0), 'cross');
  assert.equal(frame.status().latency, 123);
});

test('legacy settings fall back to scalar without reading the following section', () => {
  for (const values of [[12], [20, 15], []]) {
    const frame = new Frame(settingsFrame(values));
    const settings = frame.settings();
    assert.deepEqual([...settings.buyLeverage], values);
    assert.deepEqual([...settings.sellLeverage], values);
    assert.equal(frame.status().loaded, 31);
  }
});
