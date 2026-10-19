import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType } from '../../types';

it('rejects a private BloFin engine without a passphrase before startup', async () => {
  const { WasmEngine } = await loadNodeEngine();
  const credentials = { key: 'replay-key', secret: 'replay-secret' };
  expect(() => new WasmEngine('blofin', JSON.stringify({ ...credentials, passphrase: 'replay-passphrase' })))
    .not.toThrow();
  for (const passphrase of [undefined, '']) {
    expect(() => new WasmEngine('blofin', JSON.stringify({ ...credentials, passphrase })))
      .toThrow('BloFin requires a passphrase');
  }
  expect(() => new WasmEngine('blofin', JSON.stringify({ publicDataOnly: true }))).not.toThrow();
});

it('preserves BloFin SDK range units, flat leverage, global margin and caller IDs through wasm', async () => {
  let hedged = false;
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  const ok = (data: unknown) => ({ status: 200, body: new TextEncoder().encode(JSON.stringify({ code: '0', data })) });
  const transport: Transport = {
    now: Date.now,
    connect: (_url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return { send() {}, close() {} };
    },
    request: async (request) => {
      requests.push(request);
      const path = new URL(request.url).pathname;
      if (path.endsWith('/instruments'))
        return ok([
          {
            instId: 'BTC-USDT',
            instType: 'SWAP',
            contractType: 'linear',
            baseCurrency: 'BTC',
            quoteCurrency: 'USDT',
            state: 'live',
            contractValue: '0.001',
            minSize: '0.1',
            lotSize: '0.1',
            tickSize: '0.1',
            maxLimitSize: '1000',
            maxMarketSize: '1000',
            maxLeverage: '100',
          },
        ]);
      if (path.endsWith('/tickers')) return ok([{ instId: 'BTC-USDT', last: '60000' }]);
      if (path.endsWith('/margin-mode')) return ok({ marginMode: 'cross' });
      if (path.endsWith('/position-mode')) return ok({ positionMode: 'net_mode' });
      if (path.endsWith('/batch-leverage-info'))
        return ok(
          hedged
            ? [
                { instId: 'BTC-USDT', marginMode: 'cross', positionSide: 'long', leverage: '13' },
                { instId: 'BTC-USDT', marginMode: 'cross', positionSide: 'short', leverage: '7' },
              ]
            : [{ instId: 'BTC-USDT', marginMode: 'cross', positionSide: 'net', leverage: '20' }],
        );
      if (path.endsWith('/set-position-mode')) {
        hedged = true;
        return ok({ code: '0' });
      }
      if (path.endsWith('/fills-history'))
        return ok([
          {
            instId: 'BTC-USDT',
            orderId: '123',
            tradeId: '456',
            side: 'buy',
            fillPrice: '60000',
            fillSize: '1',
            ts: '1700000050000',
            fee: '0',
            feeCurrency: 'USDT',
          },
        ]);
      if (path.endsWith('/batch-orders')) return ok([{ orderId: '300', code: '0' }]);
      if (path.endsWith('/set-margin-mode')) return ok({ code: '0' });
      if (path.endsWith('/positions') || path.endsWith('/balances') || path.endsWith('-pending')) return ok([]);
      throw new Error(`Unexpected request ${request.method} ${request.url}`);
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    'blofin',
    { key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase' },
    store,
  );
  const main = new BaseExchange('blofin', { key: '', secret: '' }, new DefaultStore());
  exchange.onFrame = (frame) => main.applyWorkerFrame(frame.slice(0));
  try {
    await exchange.start();
    const leverage = await exchange.fetchLeverageForSymbol('BTCUSDT');
    expect(leverage).toMatchObject({ buyLeverage: 20, sellLeverage: 20, marginType: MarginType.Cross });
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 20, sellLeverage: 20 });
    const fills = await exchange.fetchHistoricalFills({
      symbol: 'BTCUSDT',
      from: 1700000000,
      to: 1700000100,
      limit: 1,
    });
    const millis = await exchange.fetchHistoricalFills({
      symbol: 'BTCUSDT',
      from: 1700000000000,
      to: 1700000100000,
      limit: 1,
    });
    expect(fills).toEqual(millis);
    expect(fills).toHaveLength(1);
    for (const request of requests.filter((r) => r.url.includes('/fills-history'))) {
      expect(new URL(request.url).searchParams.get('begin')).toBe('1700000000000');
      expect(new URL(request.url).searchParams.get('end')).toBe('1700000100000');
    }
    const ref = await exchange.placeOrderWithClientOrderId({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 60000,
      clientOrderId: 'b3cdedc0a20880baCaller123',
    });
    expect(ref.orderIds).toEqual(['300']);
    expect(JSON.parse(requests.find((r) => r.url.includes('/batch-orders'))!.body!)[0].clientOrderId).toBe(
      'b3cdedc0a20880baCaller123',
    );
    await exchange.changePositionMode(true);
    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toMatchObject({ buyLeverage: 13, sellLeverage: 7 });
    await exchange.changeMarginType(true);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 13, sellLeverage: 7 });
    expect(main.store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 13, sellLeverage: 7 });
    expect(
      requests.some((r) => r.url.includes('/set-margin-mode') && JSON.parse(r.body!).marginMode === 'isolated'),
    ).toBe(true);
  } finally {
    exchange.dispose();
    main.dispose();
  }
}, 30000);
