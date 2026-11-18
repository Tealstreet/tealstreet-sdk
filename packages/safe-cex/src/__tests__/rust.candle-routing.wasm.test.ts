import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { Market } from '../types';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { consumerFrame } from './cutover/consumerFrame';
import { prepareRecordedHttp } from './support/managedHttp';

const accounts: RustEngineExchange[] = [];
afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  setRustEngineTransport();
  setWasmEngineLoader(loadNodeEngine);
});

it('keyed Bybit candles use a real public WASM source while its frame-fed account has no worker bridge', async () => {
  const requests: Array<{ path: string; headers: Record<string, string> }> = [];
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect: (_url, { onOpen }) => {
      queueMicrotask(onOpen);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ url, headers }) => {
      const parsed = new URL(url);
      requests.push({ path: parsed.pathname, headers });
      let list: unknown[];
      switch (parsed.pathname) {
        case '/v5/market/instruments-info':
          list = parsed.searchParams.has('status')
            ? []
            : [
                {
                  symbol: 'BTCUSDT',
                  status: 'Trading',
                  baseCoin: 'BTC',
                  quoteCoin: 'USDT',
                  priceFilter: { tickSize: '0.1' },
                  lotSizeFilter: { qtyStep: '0.001', minOrderQty: '0.001', maxOrderQty: '120' },
                },
              ];
          break;
        case '/v5/market/tickers':
          list = [{ symbol: 'BTCUSDT', lastPrice: '2', bid1Price: '1', ask1Price: '2' }];
          break;
        case '/v5/market/kline':
          list = [['1700000000000', '1', '2', '1', '2', '3', '6']];
          break;
        default:
          throw new Error(`Unexpected public request: ${parsed.pathname}`);
      }
      return {
        status: 200,
        body: new TextEncoder().encode(JSON.stringify({ retCode: 0, result: { list, nextPageCursor: '' } })),
      };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(loadNodeEngine);
  const exchange = new RustEngineExchange(
    'bybit',
    {
      key: 'fixture-key',
      secret: 'fixture-secret',
      workerMirror: true,
      publicDataOnly: false,
    },
    new DefaultStore(),
  );
  accounts.push(exchange);
  expect(await exchange.frameFedReady).toBe(true);
  const market = {
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    isPerpetual: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 120 }, leverage: { min: 1, max: 100 } },
  } satisfies Market;
  exchange.ingestSourceFrame(
    consumerFrame({
      markets: [market],
      loaded: { markets: true, tickers: false, balance: false, positions: false, orders: false },
    }),
    true,
  );
  const bridge = vi.fn();
  exchange.on('callMethod', bridge);
  exchange.off('callMethod', bridge);
  await expect(
    exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '15m', from: 1_700_000_000_000, to: 1_700_000_900_000 }),
  ).resolves.toEqual([{ timestamp: 1_700_000_000, open: 1, high: 2, low: 1, close: 2, volume: 3 }]);
  expect(requests.filter(({ path }) => path === '/v5/market/kline')).toHaveLength(1);
  expect(requests.every(({ path }) => path.startsWith('/v5/market/'))).toBe(true);
  expect(requests.every(({ headers }) => Object.keys(headers).every((name) => !/^X-BAPI-/i.test(name)))).toBe(true);
  expect(bridge).not.toHaveBeenCalled();
  expect(exchange.isFrameFed).toBe(true);
  expect(exchange.store.loaded.balance).toBe(false);
}, 15_000);
