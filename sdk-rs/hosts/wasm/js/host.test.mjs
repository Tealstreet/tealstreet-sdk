import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EngineHost } from './host.mjs';

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
