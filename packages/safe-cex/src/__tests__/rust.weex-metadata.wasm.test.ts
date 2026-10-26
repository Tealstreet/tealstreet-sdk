vi.mock('../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('./reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../exchanges/rust/betaVenues')>());
});

import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { prepareRecordedHttp } from './support/managedHttp';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

// Documented WEEX shapes, delivered through the real native engine and host.
// No synthetic SDK frames, store writes or live requests supply the answer.
const setup = async (publicDataOnly = false) => {
  const requests: string[] = [];
  const state = { scopedReads: 0, malformed: false };
  const config = (leverage: number) =>
    JSON.stringify([
      {
        symbol: 'BTCUSDT',
        marginType: 'CROSSED',
        separatedType: 'COMBINED',
        crossLeverage: String(leverage),
        isolatedLongLeverage: '10',
        isolatedShortLeverage: '15',
      },
    ]);
  const transport: Transport = {
    // Identity and leverage reads are managed; route preparation precedes signing.
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect: (_url, handlers) => {
      setTimeout(handlers.onOpen, 0);
      return { send: () => {}, close: () => {} };
    },
    request: async ({ url }) => {
      requests.push(url);
      const parsed = new URL(url);
      const name = parsed.pathname.split('/').pop();
      let body: string;
      switch (name) {
        case 'exchangeInfo':
          body = JSON.stringify({
            symbols: [
              {
                symbol: 'BTCUSDT',
                baseAsset: 'BTC',
                quoteAsset: 'USDT',
                marginAsset: 'USDT',
                pricePrecision: 1,
                quantityPrecision: 6,
                contractVal: 0.000001,
                minOrderSize: 0.0001,
                maxOrderSize: 10000,
                maxLeverage: 408,
              },
            ],
          });
          break;
        case '24hr':
          body = '[{"symbol":"BTCUSDT","lastPrice":"90000"}]';
          break;
        case 'bookTicker':
          body = '[{"symbol":"BTCUSDT","bidPrice":"89999","askPrice":"90001"}]';
          break;
        case 'accountConfig':
          body = '{"dualSidePosition":false,"canTrade":true}';
          break;
        case 'symbolConfig':
          if (parsed.searchParams.has('symbol')) {
            expect(parsed.searchParams.get('symbol')).toBe('BTCUSDT');
            state.scopedReads++;
            body = state.malformed ? '{malformed' : config(40 + state.scopedReads);
          } else body = config(20);
          break;
        case 'balance':
        case 'allPosition':
        case 'openOrders':
        case 'openAlgoOrders':
          body = '[]';
          break;
        default:
          throw new Error(`Unexpected WEEX metadata fixture request: ${url}`);
      }
      return { status: 200, headers: '', body: new TextEncoder().encode(body) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const exchange = new RustEngineExchange(
    'weex',
    {
      key: publicDataOnly ? '' : 'fixture-key',
      secret: publicDataOnly ? '' : 'fixture-secret',
      passphrase: publicDataOnly ? '' : 'fixture-passphrase',
      publicDataOnly,
    },
    new DefaultStore(),
  );
  running.push(exchange);
  await exchange.start();
  return { exchange, requests, state };
};

it('refreshes cached WEEX rails for a flat symbol on every metadata hook and explicit read', async () => {
  const { exchange, state } = await setup();
  expect(exchange.store.positions).toEqual([]);
  expect(exchange.store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 20, sellLeverage: 20 });
  for (const leverage of [41, 42]) {
    await exchange.updateSymbolMetadata('BTCUSDT');
    expect(exchange.store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: leverage, sellLeverage: leverage });
  }
  await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toMatchObject({
    buyLeverage: 43,
    sellLeverage: 43,
  });
  expect(state.scopedReads).toBe(3);
});

it('rejects malformed WEEX metadata without replacing the last good rails', async () => {
  const { exchange, state } = await setup();
  state.malformed = true;
  await expect(exchange.updateSymbolMetadata('BTCUSDT')).rejects.toThrow(/invalid WEEX symbol configuration/);
  expect(exchange.store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 20, sellLeverage: 20 });
  state.malformed = false;
  await exchange.updateSymbolMetadata('BTCUSDT');
  expect(exchange.store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 42, sellLeverage: 42 });
});

it('keeps public-only WEEX metadata free of private requests', async () => {
  const { exchange, requests, state } = await setup(true);
  await exchange.updateSymbolMetadata('BTCUSDT');
  expect(state.scopedReads).toBe(0);
  expect(requests.some((url) => url.includes('/account/'))).toBe(false);
});
