import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Store } from '../../store/store.interface';
import type { StoreData } from '../../types';

import { vi } from 'vitest';

import { consumerFrame } from '../cutover/consumerFrame';

/**
 * Command-wrapper fakes still publish the native datasets that authorize startup.
 * After start, a frame is published whenever the native overrides (e.g. loaded flags) change.
 */
export function installNativeStartupFrames(
  engine: WasmEngineLike,
  store: Store,
  native: () => Partial<StoreData> = () => ({}),
): void {
  let started = false;
  let published: string | undefined;
  const start = engine.start.bind(engine);
  engine.start = vi.fn((now: number) => {
    started = true;
    return start(now);
  });
  engine.takeFrame = (_now, marketOnly) => {
    if (!started || marketOnly) return undefined;
    const overrides = structuredClone(native());
    const key = JSON.stringify({ loaded: store.loaded, ...overrides });
    if (key === published) return undefined;
    published = key;
    return new Uint8Array(consumerFrame({ ...store.serialize(), ...overrides }));
  };
  engine.snapshotFrame = () => new Uint8Array(consumerFrame({ ...store.serialize(), ...structuredClone(native()) }));
  // Current native account state, read without advancing freshness IDs.
  engine.accountSnapshotFrame = () =>
    new Uint8Array(
      consumerFrame({ ...store.serialize(), ...structuredClone(native()) }, undefined, false, undefined, 0, true),
    );
}

const readyMarket = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
} as StoreData['markets'][number];

/** A frame carrying every dataset native startup requires, public or private. */
export const nativeReadyFrame = (): Uint8Array =>
  new Uint8Array(
    consumerFrame({
      markets: [readyMarket],
      tickers: [
        {
          id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 99, ask: 101, last: 100, mark: 100, index: 100,
          percentage: 0, fundingRate: 0, volume: 0, quoteVolume: 0, openInterest: 0,
        } as StoreData['tickers'][number],
      ],
      balance: { used: 0, free: 0, total: 0, upnl: 0 },
      orders: [],
      positions: [],
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    }),
  );
