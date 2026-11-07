import { MessageChannel } from 'node:worker_threads';

import { afterEach, expect, it, vi } from 'vitest';

import { RustPublicFeedProvider, rustPublicFeedIdentity } from '../../exchanges/rust/publicFeed';
import type { RustEngineExchange } from '../../exchanges/rust/rust.exchange';

// A chart's forced reset on a shared-feed account must repair the source engine's socket:
// the account's own engine holds no public topics, so a repair there only errors.
const cleanups: Array<() => void> = [];
afterEach(() => {
  while (cleanups.length) cleanups.pop()!();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 10));

it('the provider repairs only a candle stream the account holds', async () => {
  const market = { id: 'BTC', symbol: 'BTC-PERP', contractSize: 1, isSpot: false, isPerpetual: true, isInverse: false, isExpiring: false };
  const repairOHLCV = vi.fn();
  const source = {
    name: 'hyperliquid',
    options: {},
    hasStarted: true,
    isDisposed: false,
    isPublicDataOnly: true,
    publicFeedMode: 'direct',
    store: {
      loaded: { markets: true, tickers: true },
      markets: [market],
      marketsBySymbol: { 'BTC-PERP': market },
      tickers: [{ symbol: 'BTC-PERP', last: 100 }],
    },
    getActiveTickerSymbols: () => [],
    setActiveTickerSymbols: vi.fn(),
    observePublicFeed: () => () => {},
    listenOHLCVForFeed: vi.fn(() => () => {}),
    repairOHLCV,
  } as unknown as RustEngineExchange;
  const provider = new RustPublicFeedProvider(source);
  const channel = new MessageChannel();
  cleanups.push(() => {
    provider.dispose();
    channel.port1.close();
    channel.port2.close();
  });
  provider.attach(channel.port1 as unknown as MessagePort);
  const account = channel.port2;
  account.postMessage({ op: 'hello', identity: rustPublicFeedIdentity('hyperliquid', {} as never) });
  await settle();
  account.postMessage({ op: 'repair', symbol: 'BTC-PERP', interval: '1m' });
  await settle();
  expect(repairOHLCV).not.toHaveBeenCalled();
  account.postMessage({ op: 'candle', symbol: 'BTC-PERP', interval: '1m', subscribe: true });
  account.postMessage({ op: 'repair', symbol: 'BTC-PERP', interval: '1m' });
  account.postMessage({ op: 'repair', symbol: 'BTC-PERP', interval: '5m' });
  await settle();
  expect(repairOHLCV.mock.calls).toEqual([[{ symbol: 'BTC-PERP', interval: '1m' }]]);
});
