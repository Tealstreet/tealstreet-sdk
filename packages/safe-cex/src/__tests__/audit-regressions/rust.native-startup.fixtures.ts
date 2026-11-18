import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { StoreData } from '../../types';

import { vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { consumerFrame } from '../cutover/consumerFrame';
import { nativeStoreFrame } from '../cutover/nativeFixture';

/**
 * Command-wrapper fakes still publish the native datasets that authorize startup.
 * After start, a frame is published whenever the native overrides (e.g. loaded flags) change.
 */
export function installNativeStartupFrames(
  engine: WasmEngineLike,
  store: FrameStore,
  native: () => Partial<StoreData> = () => ({}),
): void {
  // Command fixtures do not sign; the real clock/signature controls live separately.
  engine.syncHostTime ??= () => true;
  let started = false;
  let published: string | undefined;
  // Rule 5 / #3640: the fake producer emits all five decoded datasets. Its
  // default completion flags must not inherit the consumer's clamped flags.
  const state = (): Partial<StoreData> => ({
    ...store.data,
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    ...structuredClone(native()),
  });
  const start = engine.start.bind(engine);
  engine.start = vi.fn((now: number) => {
    started = true;
    return start(now);
  });
  engine.takeFrame = (_now, marketOnly) => {
    if (!started || marketOnly) return undefined;
    const data = state();
    const key = JSON.stringify(data);
    if (key === published) return undefined;
    published = key;
    return new Uint8Array(nativeStoreFrame(data));
  };
  engine.snapshotFrame = () => {
    const data = state();
    return new Uint8Array(nativeStoreFrame(data));
  };
  // Current native account state, read without advancing freshness IDs.
  engine.accountSnapshotFrame = () => {
    const data = state();
    return new Uint8Array(nativeStoreFrame(data, undefined, true));
  };
}

const readyMarket: StoreData['markets'][number] = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

/** A frame carrying every dataset native startup requires, public or private. */
export const nativeReadyFrame = (): Uint8Array =>
  new Uint8Array(
    consumerFrame({
      markets: [readyMarket],
      tickers: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 100,
          index: 100,
          percentage: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
          openInterest: 0,
        },
      ],
      balance: { used: 0, free: 0, total: 0, upnl: 0 },
      orders: [],
      positions: [],
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    }),
  );
