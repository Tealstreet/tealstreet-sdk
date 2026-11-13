import assert from 'node:assert/strict';
import { describe, expect, it, vi } from 'vitest';
import start from './store.webworker';
import { isWorkerMessage, isWorkerWriteMarks } from './workerMessage';

describe('worker routing envelope', () => {
  it.each([null, undefined, 1, 'init', [], {}, { op: 1 }, { op: 'unrecognized' }, { op: 'dispose', instanceId: 7 }])(
    'ignores an unusable routing envelope (%j)', value => {
      expect(isWorkerMessage(value)).toBe(false);
    },
  );

  it('retains optional generation IDs and leaves operation arguments with their owner', () => {
    expect(isWorkerMessage({ op: 'dispose' })).toBe(true);
    expect(isWorkerMessage({ op: 'callMethod', instanceId: 'generation', data: { args: [{}] } })).toBe(true);
    expect(isWorkerMessage({ op: 'exec' })).toBe(true);
    expect(isWorkerMessage({ op: '__workerConfig', data: { hostSpecificSetting: true } })).toBe(true);
  });

  it('checks init routing fields without resolving venue or credential policy', () => {
    const data = { exchangeName: 'bybit', options: {}, accountName: 'connection:account' };
    expect(isWorkerMessage({ op: 'init', instanceId: 'generation', data })).toBe(true);
    for (const invalid of [undefined, [], {}, { ...data, accountName: 1 }, { ...data, exchangeName: 1 },
      { ...data, options: null }, { ...data, settings: [] }, { ...data, metadata: false }, { ...data, store: 3 }]) {
      expect(isWorkerMessage({ op: 'init', data: invalid })).toBe(false);
    }
    expect(isWorkerMessage({ op: '__workerConfig', data: [] })).toBe(false);
  });

  it('narrows write timing without changing valid marks or accepting nonnumeric stages', () => {
    const marks = { called: 0, sent: 15, resolved: undefined };
    expect(isWorkerWriteMarks(marks)).toBe(true);
    expect(isWorkerWriteMarks({})).toBe(true);
    for (const value of [null, [], 'marks', { called: '15' }]) expect(isWorkerWriteMarks(value)).toBe(false);
  });
});

it('answers heartbeat before init and envelope generation validation', async () => {
  const listen = vi.fn<Worker['addEventListener']>();
  const postMessage = vi.fn<(message: unknown, transferOrOptions?: Transferable[] | StructuredSerializeOptions) => void>();
  const worker: Worker = {
    onerror: null, onmessage: null, onmessageerror: null,
    addEventListener: listen, postMessage,
    removeEventListener: vi.fn(), dispatchEvent: vi.fn(() => true), terminate: vi.fn(),
  };
  start(worker);
  const listener = listen.mock.calls.find(([type]) => type === 'message')?.[1];
  assert.ok(listener);
  assert.equal(typeof listener, 'function');
  if (typeof listener !== 'function') throw new Error('Expected the worker message callback');
  await listener.call(worker, new MessageEvent('message', { data: { op: '__worker_ping__', instanceId: 7 } }));
  expect(postMessage).toHaveBeenCalledExactlyOnceWith({
    op: '__worker_pong__', instanceId: 7, data: { ts: expect.any(Number) },
  });
  await listener.call(worker, new MessageEvent('message', { data: null }));
  await listener.call(worker, new MessageEvent('message', { data: { op: 'init', data: {} } }));
  expect(postMessage).toHaveBeenCalledOnce();
});
