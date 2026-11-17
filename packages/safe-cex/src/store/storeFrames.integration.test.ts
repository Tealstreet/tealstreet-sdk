import type { FetchedLeverage } from '../generated/FetchedLeverage';
import type { StoreData } from '../generated/StoreData';

import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { nativeFixture } from '../__tests__/cutover/nativeFixture';
import fixtures from '../../../../sdk-rs/engine/tests/fixtures/store-frames.json';
import { initialStoreData } from '../generated/StoreData';
import { MarginType, PositionMode } from '../types';
import { FrameStore } from './frameStore';

const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

describe('native FrameStore state through packed frames and serialization', () => {
  it('uses the Rust initial contract while keeping each FrameStore state independent', () => {
    expect(new FrameStore().data).toEqual(initialStoreData);
    const first = new FrameStore();
    const second = new FrameStore();
    first.apply(
      nativeFixture(
        { fetchedLeverage: { BTCUSDT: { buyLeverage: 0, sellLeverage: 0 } } },
        first.markets.map((row) => row.symbol),
      ),
    );
    expect(second.fetchedLeverage).toEqual({});
    expect(initialStoreData.fetchedLeverage).toEqual({});
  });

  it('preserves null versus measured zero latency and unchanged empty account section versions', () => {
    const store = new FrameStore();
    const applier = store;
    applier.apply(frame(fixtures.empty));
    expect(store.latency).toBeNull();
    expect(store.updateIds).toEqual({ orders: 0, positions: 0 });
    applier.apply(frame(fixtures.ready));
    const snapshot: StoreData = store.data;
    expect(snapshot.latency).toBe(0);
    expect(snapshot.options).toEqual({ isHedged: true });
    expect(snapshot.loaded).toMatchObject({ balance: true, orders: true, positions: true });
    expect(snapshot.updateIds).toEqual({ orders: 0, positions: 0 });
    const late = new FrameStore();
    late.apply(
      nativeFixture(
        snapshot,
        late.markets.map((row) => row.symbol),
      ),
    );
    late.apply(
      nativeFixture(
        snapshot,
        late.markets.map((row) => row.symbol),
      ),
    );
    expect(late.data.updateIds).toEqual(snapshot.updateIds);
  });

  it('refuses malformed native rails without replacing confirmed leverage', () => {
    const store = new FrameStore();
    const applier = store;
    applier.apply(frame(fixtures.ready));
    const previous = store.fetchedLeverage;
    const malformed = frame(fixtures.ready);
    const settings = malformed.settings();
    if (!settings?.buyLeverage) throw new Error('native rails fixture required');
    settings.buyLeverage[0] = NaN;
    expect(() => applier.apply(malformed)).toThrow('Invalid native Store leverage rails');
    expect(store.fetchedLeverage).toBe(previous);
  });

  it('keeps independent rails and zero while omitting unknown settings, then replaces the map', () => {
    const store = new FrameStore();
    const applier = store;
    expect(frame(fixtures.ready).settings()?.count).toBe(3);
    applier.apply(frame(fixtures.ready));
    const expected: FetchedLeverage = {
      buyLeverage: 15,
      sellLeverage: 30,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
    };
    expect(store.fetchedLeverage.BTCUSDT).toEqual(expected);
    expect(store.fetchedLeverage.ZERO).toMatchObject({ buyLeverage: 0, sellLeverage: 0 });
    expect(store.fetchedLeverage.FALLBACK).toMatchObject({ buyLeverage: 7, sellLeverage: 7 });
    expect(store.fetchedLeverage).not.toHaveProperty('UNKNOWN');
    applier.apply(frame(fixtures.cleared));
    expect(store.data.fetchedLeverage).toEqual({});
  });
});
