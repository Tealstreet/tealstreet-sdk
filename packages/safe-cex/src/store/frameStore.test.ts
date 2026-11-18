import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { nativeFixture, nativeStoreFrame } from '../__tests__/cutover/nativeFixture';
import fixtures from '../../../../sdk-rs/engine/tests/fixtures/store-frames.json';
import { PositionSide } from '../generated/PositionSide';
import { FrameStore } from './frameStore';

const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

describe('native position notional', () => {
  const positionFrame = (notional: number | null) =>
    nativeFixture({
      positions: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 100,
          notional,
          leverage: 1,
          unrealizedPnl: 0,
          contracts: 1,
          liquidationPrice: null,
        },
      ],
    });

  it.each([null, 0, 1234.5])('decodes nullable native position notional %s exactly', (notional) => {
    const store = new FrameStore();
    store.apply(positionFrame(notional));
    expect(store.positions[0].notional).toBe(notional);
  });

  it.each([Infinity, -Infinity])('refuses nonfinite native position notional %s', (notional) => {
    const store = new FrameStore();
    expect(() => store.apply(positionFrame(notional))).toThrow(/invalid Position notional/);
    expect(store.positions).toEqual([]);
  });
});

describe('FrameStore native state', () => {
  it('retains unchanged section identities and versions across equal frames', () => {
    const store = new FrameStore();
    store.ingest(frame(fixtures.ready), { snapshot: true, seq: 10 });
    const before = store.data;
    const versions = store.versions;
    store.ingest(frame(fixtures.ready), { seq: 11 });
    expect(store.orders).toBe(before.orders);
    expect(store.positions).toBe(before.positions);
    expect(store.markets).toBe(before.markets);
    expect(store.tickers).toBe(before.tickers);
    expect(store.versions).toEqual(versions);
    expect(store.hasMarketIdentity).toBe(true);
    expect(store.hasAccountState).toBe(true);
    expect(store.latency).toBe(0);
  });

  it('publishes typed section changes with independent versions', () => {
    const store = new FrameStore();
    const values: Array<number | null> = [];
    const stop = store.subscribe('latency', (value, version) => {
      values.push(value);
      expect(version).toBe(store.version('latency'));
    });
    store.ingest(frame(fixtures.ready));
    const markets = store.markets;
    store.ingest(frame(fixtures.empty));
    expect(values).toEqual([0, null]);
    expect(store.markets).toBe(markets);
    stop();
    store.ingest(frame(fixtures.ready));
    expect(values).toEqual([0, null]);
  });

  it('rejects gaps until a fresh snapshot establishes a new sequence domain', () => {
    const store = new FrameStore();
    store.ingest(frame(fixtures.ready), { snapshot: true, seq: 1 });
    const before = store.data;
    expect(() => store.ingest(frame(fixtures.empty), { seq: 3 })).toThrow('sequence gap');
    expect(store.data).toBe(before);
    expect(store.snapshotRequired).toBe(true);
    expect(store.hasMarketIdentity).toBe(false);
    expect(() => store.ingest(frame(fixtures.empty), { seq: 2 })).toThrow('sequence gap');
    store.ingest(frame(fixtures.ready), { snapshot: true, seq: 20 });
    expect(store.snapshotRequired).toBe(false);
    expect(store.hasMarketIdentity).toBe(true);
    expect(store.seq).toBe(20);
  });

  it('keeps valid peers when SETTINGS fails and distinguishes subscriber failures', () => {
    const store = new FrameStore();
    store.ingest(frame(fixtures.ready));
    const leverage = store.fetchedLeverage;
    const malformed = frame(fixtures.ready);
    const settings = malformed.settings();
    if (!settings?.buyLeverage) throw new Error('Fixture missing rails');
    settings.buyLeverage[0] = NaN;
    expect(() => store.ingest(malformed)).toThrow('Invalid native Store leverage rails');
    expect(store.fetchedLeverage).toBe(leverage);
    expect(store.hasAccountState).toBe(true);
    const observed: boolean[] = [];
    store.subscribe(() => {
      throw new Error('subscriber');
    });
    store.subscribe(() => {
      observed.push(true);
    });
    expect(() => store.ingest(frame(fixtures.empty))).toThrow('subscriber failed');
    expect(observed).toEqual([true]);
    expect(store.snapshotRequired).toBe(false);
  });
});

it('delivers reentrant section notifications in version order with immutable snapshots', () => {
  const store = new FrameStore();
  const seen: Array<[number, number]> = [];
  store.subscribe('balance', (balance) => {
    if (balance.total === 1) store.ingest(nativeFixture({ balance: { ...balance, total: 2 } }));
  });
  store.subscribe('balance', (balance, version) => seen.push([balance.total, version]));
  store.ingest(nativeFixture({ balance: { used: 0, free: 0, total: 1, upnl: 0 } }));
  expect(seen).toEqual([
    [1, 1],
    [2, 2],
  ]);
});

it('applies independent derived sections when one native section is malformed', () => {
  const store = new FrameStore();
  const bytes = nativeStoreFrame({
    socketHealth: { publicHealthy: true, privateHealthy: false, publicInitialized: true, privateInitialized: false },
    ordersRisk: {},
    displayedMargin: { longExposure: null, shortExposure: 5, totalNotional: null },
  });
  const view = new DataView(bytes);
  for (let at = 16; at < bytes.byteLength; at += 8 + view.getUint32(at + 4, true)) {
    if (view.getUint16(at, true) === 29) view.setUint8(at + 12, 255);
  }
  expect(() => store.ingest(new Frame(bytes))).toThrow('invalid native store tag');
  expect(store.socketHealth?.publicHealthy).toBe(true);
  expect(store.displayedMargin).toEqual({ longExposure: null, shortExposure: 5, totalNotional: null });
});

it('retains only confirmed native settings after closing positions and clears them on reset', () => {
  const store = new FrameStore();
  store.ingest(
    nativeFixture({
      positions: [
        {
          id: 'native-id',
          symbol: 'BTC',
          side: PositionSide.Long,
          contracts: 2,
          entryPrice: 10,
          notional: 20,
          leverage: 3,
          unrealizedPnl: 1,
          liquidationPrice: null,
        },
      ],
    }),
  );
  store.ingest(nativeFixture({ positions: [] }));
  expect(store.positionSettings['native-id']).toEqual({
    id: 'native-id',
    symbol: 'BTC',
    side: PositionSide.Long,
    leverage: 3,
  });
  expect(store.positions).toEqual([]);
  store.reset();
  expect(store.positionSettings).toEqual({});
});

it('refuses readiness from the exact 40-byte STATUS-only snapshot, including after recovery', () => {
  const bytes = new ArrayBuffer(40);
  const view = new DataView(bytes);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setUint16(16, 10, true);
  view.setUint32(20, 16, true);
  view.setUint32(24, 31, true);
  const store = new FrameStore();
  for (const recovered of [false, true]) {
    if (recovered) store.ingest(frame(fixtures.ready), { snapshot: true });
    store.ingest(new Frame(bytes), { snapshot: true });
    expect(store.hasMarketIdentity).toBe(false);
    expect(store.hasTickerState).toBe(false);
    expect(store.hasAccountState).toBe(false);
    expect(store.loaded).toEqual({ markets: false, tickers: false, balance: false, orders: false, positions: false });
  }
});

it('keeps order and position update counters monotonic through snapshot recovery and reset', () => {
  const store = new FrameStore();
  store.ingest(frame(fixtures.ready), { snapshot: true });
  const first = store.updateIds;
  store.ingest(frame(fixtures.ready), { snapshot: true });
  expect(store.updateIds).toEqual(first);
  store.reset();
  const reset = store.updateIds;
  expect(reset.orders).toBeGreaterThanOrEqual(first.orders);
  expect(reset.positions).toBeGreaterThanOrEqual(first.positions);
  store.ingest(frame(fixtures.ready), { snapshot: true });
  expect(store.updateIds.orders).toBeGreaterThanOrEqual(reset.orders);
  expect(store.updateIds.positions).toBeGreaterThanOrEqual(reset.positions);
});

it('retains native closed-position settings through same-account snapshot recovery', () => {
  const store = new FrameStore();
  store.ingest(
    nativeFixture({
      positions: [
        {
          id: 'closed-native',
          symbol: 'BTC',
          side: PositionSide.Long,
          contracts: 2,
          entryPrice: 100,
          notional: 200,
          leverage: 10,
          unrealizedPnl: 7,
          liquidationPrice: null,
        },
      ],
    }),
  );
  store.ingest(nativeFixture({ positions: [] }), { snapshot: true });
  expect(store.positionSettings['closed-native']?.leverage).toBe(10);
  expect(store.positions).toEqual([]);
  store.ingest(nativeFixture({ fetchedLeverage: { BTC: { buyLeverage: 20, sellLeverage: 20 } } }, ['BTC']));
  expect(store.positionSettings['closed-native']?.leverage).toBe(20);
});

it('counts independent empty stream observations after equal account reads without replacing rows', () => {
  const store = new FrameStore();
  store.apply(nativeFixture({ orders: [] }));
  const orders = store.orders;
  const before = store.observationIds.orders;
  store.apply(new Frame(nativeStoreFrame({ orders: [] }, undefined, true)));
  expect(store.observationIds.orders).toBe(before);
  store.apply(nativeFixture({ orders: [] }));
  expect(store.observationIds.orders).toBe(before + 1);
  expect(store.updateIds.orders).toBe(0);
  expect(store.orders).toBe(orders);
});

it('keeps empty stream censuses out of UI update IDs and subscriptions', () => {
  const store = new FrameStore();
  let notifications = 0;
  let receipts = 0;
  store.subscribe(() => notifications++);
  store.subscribeAccountObservations(() => receipts++);
  const before = store.data;
  const versions = store.versions;
  store.apply(nativeFixture({ orders: [], positions: [] }));
  expect(store.updateIds).toEqual({ orders: 0, positions: 0 });
  expect(store.data).toBe(before);
  expect(store.versions).toEqual(versions);
  expect(notifications).toBe(0);
  expect(receipts).toBe(1);
  expect(store.observationIds).toEqual({ orders: 1, positions: 1 });
  store.apply(nativeFixture({ latency: 1 }));
  expect(receipts).toBe(1);
  expect(store.updateIds).toEqual({ orders: 0, positions: 0 });
});
