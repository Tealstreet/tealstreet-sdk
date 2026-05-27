import { describe, test, expect } from 'vitest';
import { RingBufferTrackingSymbolUpdates } from './RingBufferTrackingSymbolUpdates';

describe('RingBufferTrackingSymbolUpdates', () => {
  test('Create ring buffer.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(10);
    expect(ringBuffer).toBeTruthy();
    expect(ringBuffer.getSize()).toBe(10);
  });

  test('Correctly adds symbols to buffer and clears newUpdates after getLimit.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(10);

    ringBuffer.add({ symbol: 'BTCUSDT', id: '1' });
    let limit = ringBuffer.getLimit('BTCUSDT');

    expect(limit).toBe(1);
    expect(ringBuffer.getLastN(limit)).toStrictEqual([{ symbol: 'BTCUSDT', id: '1' }]);

    ringBuffer.add({ symbol: 'BTCUSDT', id: '2' });
    limit = ringBuffer.getLimit('BTCUSDT');

    expect(limit).toBe(1);
    expect(ringBuffer.getLastN(limit)).toStrictEqual([{ symbol: 'BTCUSDT', id: '2' }]);

    ringBuffer.add({ symbol: 'BTCUSDT', id: '3' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '4' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '5' });
    limit = ringBuffer.getLimit('BTCUSDT');

    expect(limit).toBe(3);
    expect(ringBuffer.getLastN(limit)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '3' },
      { symbol: 'BTCUSDT', id: '4' },
      { symbol: 'BTCUSDT', id: '5' },
    ]);
  });

  test('Correctly returns max buffer size elements when went over capacity.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(3);

    ringBuffer.add({ symbol: 'BTCUSDT', id: '1' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '2' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '3' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '4' });
    const limit = ringBuffer.getLimit('BTCUSDT');

    expect(limit).toBe(4);
    expect(ringBuffer.getLastN(limit)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '2' },
      { symbol: 'BTCUSDT', id: '3' },
      { symbol: 'BTCUSDT', id: '4' },
    ]);
  });

  test('Correctly returns max buffer size elements when went over capacity if array is passed.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(3);

    ringBuffer.add(
      { symbol: 'BTCUSDT', id: '1' },
      { symbol: 'BTCUSDT', id: '2' },
      { symbol: 'BTCUSDT', id: '3' },
      { symbol: 'BTCUSDT', id: '4' },
    );
    const limit = ringBuffer.getLimit('BTCUSDT');

    expect(limit).toBe(4);
    expect(ringBuffer.getLastN(limit)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '2' },
      { symbol: 'BTCUSDT', id: '3' },
      { symbol: 'BTCUSDT', id: '4' },
    ]);
  });

  test('Correctly returns max buffer size elements when went over capacity using filterBySinceLimit', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(3);

    ringBuffer.add({ symbol: 'BTCUSDT', id: '1' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '2' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '3' });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '4' });
    const limit = ringBuffer.getLimit('BTCUSDT');

    expect(limit).toBe(4);
    expect(ringBuffer.filterBySinceLimit(undefined, 3)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '2' },
      { symbol: 'BTCUSDT', id: '3' },
      { symbol: 'BTCUSDT', id: '4' },
    ]);
  });

  test('Correctly returns items since given timestamp.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(3);
    ringBuffer.add({ symbol: 'BTCUSDT', id: '1', timestamp: 1000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '2', timestamp: 2000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '3', timestamp: 3000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '4', timestamp: 4000 });

    expect(ringBuffer.filterBySinceLimit(3000)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '3', timestamp: 3000 },
      { symbol: 'BTCUSDT', id: '4', timestamp: 4000 },
    ]);
  });

  test('Correctly returns items from tail using filterBySinceLimit.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(3);
    ringBuffer.add({ symbol: 'BTCUSDT', id: '1', timestamp: 1000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '2', timestamp: 2000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '3', timestamp: 3000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '4', timestamp: 4000 });

    expect(ringBuffer.filterBySinceLimit(undefined, 2, 'timestamp', true)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '3', timestamp: 3000 },
      { symbol: 'BTCUSDT', id: '4', timestamp: 4000 },
    ]);
  });

  test('Correctly returns items from head using filterBySinceLimit.', () => {
    const ringBuffer = new RingBufferTrackingSymbolUpdates(3);
    ringBuffer.add({ symbol: 'BTCUSDT', id: '1', timestamp: 1000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '2', timestamp: 2000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '3', timestamp: 3000 });
    ringBuffer.add({ symbol: 'BTCUSDT', id: '4', timestamp: 4000 });

    expect(ringBuffer.filterBySinceLimit(undefined, 2, 'timestamp', false)).toStrictEqual([
      { symbol: 'BTCUSDT', id: '2', timestamp: 2000 },
      { symbol: 'BTCUSDT', id: '3', timestamp: 3000 },
    ]);
  });
});
