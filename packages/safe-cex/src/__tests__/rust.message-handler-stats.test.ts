import { afterEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { RustSocketHandle } from '../exchanges/rust/rustSockets';
import { FrameStore } from '../store/frameStore';
import { nativeFixture } from './cutover/nativeFixture';
import { statsFrame } from './cutover/nativeStatsFixture';



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
    const store = new FrameStore();
    store.apply(frame);
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
    const store = new FrameStore();
    const applier = store;
    applier.apply(statsFrame([{ group: 0, topic: '0.book', flags: 3, age: 500 }]));
    applier.applyMessageHandlerStats(statsFrame([{ group: 0, topic: '0.book', flags: 0, age: NaN }]));
    expect(store.messageHandlerStats?.public?.['0.book'].lastCallbacksCount).toBe(0);
    applier.applyMessageHandlerStats(statsFrame([]));
    expect(store.messageHandlerStats).toEqual({ public: {}, business: {}, futures: {}, private: {} });
  });

  it('merges changed-topic flushes without refreshing an adjacent topic', () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_700_000_000_000);
    const store = new FrameStore();
    const applier = store;
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
    const store = new FrameStore();
    store.apply(statsFrame([{ group: 0, topic: '0.book', flags: 1, age: 65000 }]));
    expect(store.messageHandlerStats!.public!['0.book']).toEqual({
      lastMessageTimestamp: Date.now() - 65000,
      lastCallbacksCount: 1,
      hadHandlers: true,
    });
  });

  it('does not erase shared public-provider diagnostics when the account owns only private topics', () => {
    const store = new FrameStore();
    store.applyMessageHandlerStats(statsFrame([{ group: 0, topic: 'sharedTicker', flags: 3, age: 0 }]), 'public');
    const shared = store.messageHandlerStats!.public;
    const applier = store;
    applier.apply(statsFrame([{ group: 3, topic: '0.orders', flags: 3, age: 10 }]));
    expect(store.messageHandlerStats!.public).toBe(shared);
    applier.apply(statsFrame([]));
    expect(store.messageHandlerStats!.private).toEqual({});
    expect(store.messageHandlerStats!.public).toBe(shared);
  });

  it('keeps source monotonic ages meaningful across a different page clock and wall-clock correction', () => {
    vi.useFakeTimers();
    const store = new FrameStore();
    const applier = store;
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
    const store = new FrameStore();
    expect(() => store.apply(statsFrame([{ ...row, topic: 'bad' }]))).toThrow();
    expect(store.messageHandlerStats).toEqual({});
  });
});

it.each([0xffffffff, 99])('refuses a missing or unresolved topic reference %i', (reference) => {
  const frame = statsFrame([{ group: 0, topic: 'book', flags: 3, age: 1 }]);
  new DataView(frame.buffer).setUint32(36, reference, true);
  const store = new FrameStore();
  expect(() => store.apply(frame)).toThrow(/missing required string|invalid string reference/);
  expect(store.messageHandlerStats).toEqual({});
});

it('reads shared public diagnostics republished in native account snapshots', () => {
  const store = new FrameStore();
  store.apply(statsFrame([{ group: 3, topic: 'private-orders', flags: 3, age: 1 }]));
  store.apply(statsFrame([{ group: 0, topic: 'provider-ticker', flags: 3, age: 5 }], true, 34));
  expect(store.messageHandlerStats?.public?.['provider-ticker']?.lastCallbacksCount).toBe(1);
  expect(store.messageHandlerStats?.private?.['private-orders']?.lastCallbacksCount).toBe(1);
  store.apply(statsFrame([], true, 34));
  expect(store.messageHandlerStats?.public).toEqual({});
  expect(store.messageHandlerStats?.private?.['private-orders']?.lastCallbacksCount).toBe(1);
});
