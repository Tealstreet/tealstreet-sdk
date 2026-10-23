import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { Frame, LOADED_FLAG } from '@tealstreet/safe-cex-wasm/wire';
import { expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';
import { MarginType, OrderSide, OrderType } from '../../types';
import { directTransport } from '../../transport/direct';

it.each([false, true])('preserves BloFin broker gate, range units, flat leverage, margin and caller IDs through wasm with proxy=%s', async (tealstreetProxy) => {
  let hedged = false;
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  const ok = (data: unknown) => ({ status: 200, body: new TextEncoder().encode(JSON.stringify({ code: '0', data })) });
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
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
      if (path.endsWith('/orders-pending'))
        return ok(new URL(request.url).searchParams.has('after') ? [] : [{
          instId: 'BTC-USDT', orderId: 'native-order', orderType: 'limit', orderCategory: 'normal', side: 'buy',
          state: 'partially_filled', size: '4', filledSize: '3', price: '60000',
          positionSide: 'net', marginMode: 'cross',
        }]);
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
    {
      key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase', tealstreetProxy,
      // Accept the proxy intent while keeping every request on the scripted transport.
      transport: { ...directTransport, rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }) },
    },
    store,
  );
  const main = new BaseExchange('blofin', { key: '', secret: '' }, new DefaultStore());
  const nativeLoaded: number[] = [];
  const nativeBalances: Array<{ used: number; free: number; total: number; upnl: number }> = [];
  exchange.onFrame = (buffer) => {
    const frame = new Frame(buffer);
    const status = frame.status();
    if (status) nativeLoaded.push(status.loaded);
    const balance = frame.balance();
    if (balance) nativeBalances.push({ used: balance.used, free: balance.free, total: balance.total, upnl: balance.upnl });
    main.applyWorkerFrame(buffer.slice(0));
  };
  try {
    const startup = exchange.start();
    // Keep rejection observed if a readiness assertion fails before awaiting it.
    void startup.catch(() => {});
    const privateFlags = LOADED_FLAG.MARKETS | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS;
    await expect.poll(() => nativeLoaded.some((loaded) => (loaded & privateFlags) === privateFlags)).toBe(true);
    // This fixture is intentionally a real zero wallet. A STATUS bit cannot
    // stand in for the first explicitly decoded all-zero BALANCE section.
    expect(nativeBalances).toContainEqual({ used: 0, free: 0, total: 0, upnl: 0 });
    await startup;
    for (const current of [store, main.store]) {
      expect(current.loaded.balance).toBe(true);
      expect(current.balance).toEqual({ used: 0, free: 0, total: 0, upnl: 0 });
      expect(current.orders.find((order) => order.id === 'native-order')).toMatchObject({
        amount: 0.004, filled: 0.003, remaining: 0.001,
        extra: { remainingNative: 1, filledInPlacementUnits: 3 },
      });
    }
    const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 60000 };
    const originalIds = store.orders.map((current) => current.id);
    for (const controls of [
      { bybitSlippageToleranceTicks: NaN },
      { bybitSlippageToleranceTicks: Infinity },
      { bybitSlippageToleranceTicks: -Infinity },
      { bybitSlippageToleranceTicks: null },
      { bybitBbo: null },
    ]) {
      await expect(exchange.placeOrders([order, { ...order, ...controls }])).rejects.toThrow(
        'blofin does not support Bybit BBO or TickSize controls',
      );
      expect(requests.filter((request) => request.method === 'POST')).toEqual([]);
      expect(store.orders.map((current) => current.id)).toEqual(originalIds);
      expect(main.store.orders.map((current) => current.id)).toEqual(originalIds);
    }
    const bracket = { ...order, stopLoss: 58000, takeProfit: 62000 };
    for (const field of ['stopLoss', 'takeProfit', 'amount', 'price', 'triggerPrice', 'maxSlippagePct'] as const) {
      for (const value of [NaN, Infinity, -Infinity]) {
        await expect(exchange.placeOrders([order, { ...bracket, [field]: value }])).rejects.toThrow(
          `blofin ${field} must be finite`,
        );
        expect(requests.filter((request) => request.method === 'POST')).toEqual([]);
        expect(store.orders.map((current) => current.id)).toEqual(originalIds);
        expect(main.store.orders.map((current) => current.id)).toEqual(originalIds);
      }
    }
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
      clientOrderId: 'Caller123',
    });
    expect(ref.orderIds).toEqual(['300']);
    expect(JSON.parse(requests.find((r) => r.url.includes('/batch-orders'))!.body!)[0].clientOrderId).toBe(
      'Caller123',
    );
    const placed = JSON.parse(requests.find((r) => r.url.includes('/batch-orders'))!.body!)[0];
    expect(placed.brokerId).toBe(tealstreetProxy ? 'b3cdedc0a20880ba' : undefined);
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
