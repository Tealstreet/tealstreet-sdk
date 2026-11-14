import { afterEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { RustSocketHandle } from '../exchanges/rust/rustSockets';
import { FrameApplier } from '../store/frameApplier';
import { DefaultStore } from '../store/store.base';

type Row = { group: number; topic: string; flags: number; age: number };
const align = (n: number) => Math.ceil(n / 8) * 8;
function statsFrame(rows: Row[], replace = true): Frame {
  const labels = rows.map((row) => new TextEncoder().encode(row.topic));
  const stringsSize = align(4 + (labels.length + 1) * 4 + labels.reduce((sum, bytes) => sum + bytes.length, 0));
  const statsSize = 8 + rows.length * 24;
  const buffer = new ArrayBuffer(16 + 8 + statsSize + 8 + stringsSize);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 2, true);
  view.setFloat64(8, 1234, true);
  view.setUint16(16, 25, true);
  view.setUint32(20, statsSize, true);
  view.setUint32(24, rows.length, true);
  view.setUint32(28, Number(replace), true);
  rows.forEach((row, i) => {
    const at = 32 + i * 24;
    view.setUint32(at, row.group, true);
    view.setUint32(at + 4, i, true);
    view.setUint32(at + 8, row.flags, true);
    view.setFloat64(at + 16, row.age, true);
  });
  const strings = 24 + statsSize;
  view.setUint16(strings, 1, true);
  view.setUint32(strings + 4, stringsSize, true);
  const payload = strings + 8;
  view.setUint32(payload, labels.length, true);
  let offset = 0;
  labels.forEach((label, i) => {
    view.setUint32(payload + 4 + i * 4, offset, true);
    new Uint8Array(buffer, payload + 4 + (labels.length + 1) * 4 + offset, label.length).set(label);
    offset += label.length;
  });
  view.setUint32(payload + 4 + labels.length * 4, offset, true);
  return new Frame(buffer);
}

afterEach(() => vi.useRealTimers());
describe('Rust subscription diagnostics', () => {
  it('decodes all four buckets into exactly the TS handler shape and socket handles', () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_700_000_000_000);
    const frame = statsFrame([
      { group: 0, topic: '0.ticker.BTC', flags: 3, age: 2500 },
      { group: 1, topic: '1.candle.BTC', flags: 1, age: NaN },
      { group: 2, topic: '2.book.BTC', flags: 3, age: 10 },
      { group: 3, topic: '3.orders', flags: 0, age: NaN },
    ]);
    const store = new DefaultStore();
    new FrameApplier(store).apply(frame);
    expect(store.messageHandlerStats).toEqual({
      public: { '0.ticker.BTC': { lastMessageTimestamp: Date.now() - 2500, lastCallbacksCount: 1, hadHandlers: true } },
      business: { '1.candle.BTC': { lastMessageTimestamp: 0, lastCallbacksCount: 0, hadHandlers: true } },
      futures: { '2.book.BTC': { lastMessageTimestamp: Date.now() - 10, lastCallbacksCount: 1, hadHandlers: true } },
      private: { '3.orders': { lastMessageTimestamp: 0, lastCallbacksCount: 0, hadHandlers: true } },
    });
    const socket = new RustSocketHandle('public', { store, socketHost: undefined, isDisposed: false });
    expect(socket.messageHandlersStats).toBe(store.messageHandlerStats?.public);
    expect(frame.messageHandlerStats()?.active(0)).toBe(true);
    expect(frame.messageHandlerStats()?.active(1)).toBe(false);
  });

  it('replaces retired topics and clears the final unsubscribe, including the frame-fed seam', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(statsFrame([{ group: 0, topic: '0.book', flags: 3, age: 500 }]));
    applier.applyMessageHandlerStats(statsFrame([{ group: 0, topic: '0.book', flags: 0, age: NaN }]));
    expect(store.messageHandlerStats?.public?.['0.book'].lastCallbacksCount).toBe(0);
    applier.applyMessageHandlerStats(statsFrame([]));
    expect(store.messageHandlerStats).toEqual({ public: {}, business: {}, futures: {}, private: {} });
  });

  it('merges changed-topic flushes without refreshing an adjacent topic', () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_700_000_000_000);
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(
      statsFrame([
        { group: 0, topic: '0.one', flags: 3, age: 500 },
        { group: 0, topic: '0.two', flags: 3, age: 500 },
      ]),
    );
    const old = store.messageHandlerStats!.public!['0.two'];
    vi.advanceTimersByTime(1000);
    applier.apply(statsFrame([{ group: 0, topic: '0.one', flags: 3, age: 10 }], false));
    expect(store.messageHandlerStats!.public!['0.two']).toBe(old);
    expect(Date.now() - old.lastMessageTimestamp).toBe(1500);
    expect(Date.now() - store.messageHandlerStats!.public!['0.one'].lastMessageTimestamp).toBe(10);
  });

  it('keeps a previously live resubscription visible when it has not delivered again', () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_700_000_000_000);
    const store = new DefaultStore();
    new FrameApplier(store).apply(statsFrame([{ group: 0, topic: '0.book', flags: 1, age: 65000 }]));
    expect(store.messageHandlerStats!.public!['0.book']).toEqual({
      lastMessageTimestamp: Date.now() - 65000,
      lastCallbacksCount: 1,
      hadHandlers: true,
    });
  });

  it('does not erase shared public-provider diagnostics when the account owns only private topics', () => {
    const store = new DefaultStore();
    const shared = { sharedTicker: { lastMessageTimestamp: Date.now(), lastCallbacksCount: 1, hadHandlers: true } };
    store.update({ messageHandlerStats: { public: shared } });
    const applier = new FrameApplier(store);
    applier.apply(statsFrame([{ group: 3, topic: '0.orders', flags: 3, age: 10 }]));
    expect(store.messageHandlerStats!.public).toBe(shared);
    applier.apply(statsFrame([]));
    expect(store.messageHandlerStats!.private).toEqual({});
    expect(store.messageHandlerStats!.public).toBe(shared);
  });

  it('keeps source monotonic ages meaningful across a different page clock and wall-clock correction', () => {
    vi.useFakeTimers();
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    for (const wall of [1_700_000_000_000, 1_600_000_000_000]) {
      vi.setSystemTime(wall);
      applier.apply(statsFrame([{ group: 0, topic: '0.ticker', flags: 3, age: 65000 }]));
      expect(Date.now() - store.messageHandlerStats!.public!['0.ticker'].lastMessageTimestamp).toBe(65000);
    }
  });

  it.each([
    { group: 4, flags: 3, age: 0 },
    { group: 0, flags: 4, age: 0 },
    { group: 0, flags: 2, age: 0 },
    { group: 0, flags: 3, age: NaN },
    { group: 0, flags: 3, age: -1 },
    { group: 0, flags: 3, age: Infinity },
  ])('refuses invalid diagnostic rows without publishing them: %j', (row) => {
    const store = new DefaultStore();
    expect(() => new FrameApplier(store).apply(statsFrame([{ ...row, topic: 'bad' }]))).toThrow();
    expect(store.messageHandlerStats).toEqual({});
  });
});

it.each([0xffffffff, 99])('refuses a missing or unresolved topic reference %i', (reference) => {
  const frame = statsFrame([{ group: 0, topic: 'book', flags: 3, age: 1 }]);
  new DataView(frame.buffer).setUint32(36, reference, true);
  const store = new DefaultStore();
  expect(() => new FrameApplier(store).apply(frame)).toThrow(/missing required string|invalid string reference/);
  expect(store.messageHandlerStats).toEqual({});
});
