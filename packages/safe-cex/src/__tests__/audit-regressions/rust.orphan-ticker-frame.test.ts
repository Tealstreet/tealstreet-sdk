import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { TICKER_FIELDS } from '@tealstreet/safe-cex-wasm/wire';
import { expect, it } from 'vitest';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

it('hydrates and updates a native orphan ticker without inventing a market', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  const table = {
    count: 1, market: Uint32Array.of(0xffffffff),
    identity: () => ({ symbol: 'DELISTEDUSDT', id: 'native-retired-id' }),
    ...Object.fromEntries(TICKER_FIELDS.map((field, index) => [field, Float64Array.of(index + 1)])),
  };
  const frame = {
    tickers: () => table, tickerDeltas: () => undefined,
    markets: () => undefined, orders: () => undefined, positions: () => undefined,
    balance: () => undefined, status: () => undefined, settings: () => undefined,
  } as unknown as Frame;
  applier.apply(frame);
  expect(store.markets).toHaveLength(0);
  expect(store.tickers).toHaveLength(1);
  expect(store.tickers[0]).toMatchObject({ symbol: 'DELISTEDUSDT', id: 'native-retired-id', last: 3, quoteVolume: 11 });
  expect(store.tickersBySymbol.DELISTEDUSDT).toBe(store.tickers[0]);
  frame.tickers = () => undefined;
  frame.tickerDeltas = () => Float64Array.of(0, 4, 500);
  applier.apply(frame);
  expect(store.tickers[0]).toMatchObject({ symbol: 'DELISTEDUSDT', id: 'native-retired-id', last: 500 });
  expect(store.markets).toHaveLength(0);
});
