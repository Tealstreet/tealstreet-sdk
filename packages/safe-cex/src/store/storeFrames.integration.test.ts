import type { FetchedLeverage } from '../generated/FetchedLeverage';
import type { StoreData } from '../generated/StoreData';

import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import fixtures from '../../../../sdk-rs/engine/tests/fixtures/store-frames.json';
import { initialStoreData } from '../generated/StoreData';
import { MarginType, PositionMode } from '../types';
import { FrameApplier } from './frameApplier';
import { defaultStore, DefaultStore } from './store.base';

const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

describe('native Store state through packed frames and serialization', () => {
  it('uses the Rust initial contract while keeping each Store mutable state independent', () => {
    expect(defaultStore).toBe(initialStoreData);
    const first = new DefaultStore();
    const second = new DefaultStore();
    first.update({ fetchedLeverage: { BTCUSDT: { buyLeverage: 0, sellLeverage: 0 } } });
    expect(second.fetchedLeverage).toEqual({});
    expect(initialStoreData.fetchedLeverage).toEqual({});
  });

  it('preserves null versus measured zero latency and authoritative worker update IDs', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(frame(fixtures.empty));
    expect(store.latency).toBeNull();
    expect(store.updateIds).toEqual({ orders: 1, positions: 1 });
    applier.apply(frame(fixtures.ready));
    const snapshot: StoreData = store.serialize();
    expect(snapshot.latency).toBe(0);
    expect(snapshot.options).toEqual({ isHedged: true });
    expect(snapshot.loaded).toMatchObject({ balance: true, orders: true, positions: true });
    expect(snapshot.updateIds).toEqual({ orders: 2, positions: 2 });
    const late = new DefaultStore();
    late.update(snapshot, true);
    late.update(snapshot, true);
    expect(late.serialize().updateIds).toEqual(snapshot.updateIds);
  });

  it('refuses malformed native rails without replacing confirmed leverage', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
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
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
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
    expect(store.serialize().fetchedLeverage).toEqual({});
  });
});
