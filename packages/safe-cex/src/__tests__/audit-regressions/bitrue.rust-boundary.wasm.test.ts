import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { OrderFillEvent } from '../../types';

import { createHmac } from 'node:crypto';

import { expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

it('preserves Bitrue private IDs, fill units, leverage and exact signed writes across the SDK wasm boundary', async () => {
  const symbol = 'E-BTC-USDT';
  const orderId = '2334151155877155281';
  const placedId = '2334151155877155282';
  let now = 1_700_000_000_000;
  let cumulative = 0;
  let privateSocket: Parameters<Transport['connect']>[1] | undefined;
  const requests: Array<Parameters<Transport['request']>[0]> = [];
  const fills: OrderFillEvent[] = [];
  const errors: unknown[] = [];
  const raw = (body: string) => ({ status: 200, body: new TextEncoder().encode(body) });
  const ok = (data: unknown) => raw(JSON.stringify({ code: '0', data }));
  let placements = 0;
  const transport: Transport = {
    now: () => ++now,
    prepareHttp: prepareRecordedHttp,
    connect: (url, handlers) => {
      if (url.includes('fapiws.bitrue.com')) privateSocket = handlers;
      queueMicrotask(() => handlers.onOpen());
      return { send() {}, close() {} };
    },
    request: async (request) => {
      requests.push(request);
      const path = new URL(request.url).pathname;
      if (path.endsWith('/contracts'))
        return raw(
          JSON.stringify([
            {
              symbol,
              pricePrecision: 1,
              multiplier: 10,
              minOrderVolume: 1,
              maxMarketVolume: 10000,
              maxLimitVolume: 10000,
              minOrderMoney: 5,
              status: 1,
              side: 1,
              type: 'E',
              multiplierCoin: 'BTC',
            },
          ]),
        );
      if (path.endsWith('/listenKey')) return ok({ listenKey: 'futures-key' });
      if (path.endsWith('/leverageBracket')) return ok({ contractName: symbol, brackets: [{ initialLeverage: 125 }] });
      if (path.endsWith('/account'))
        return ok({
          account: [
            {
              marginCoin: 'USDT',
              totalEquity: 100,
              accountNormal: 80,
              accountLock: 5,
              totalPositionNormal: 7,
              sumOpenRealizedAmount: 2,
              positionVos: [
                {
                  contractName: symbol,
                  positions: [
                    {
                      side: 'BUY',
                      volume: 100,
                      openPrice: 100,
                      leverageLevel: 25,
                      positionType: 2,
                      positionBalance: 200,
                      forceLiquidationPrice: 50,
                    },
                  ],
                },
              ],
            },
          ],
        });
      if (path.endsWith('/openOrders'))
        return raw(
          `{"code":"0","data":[{"orderId":${orderId},"clientOrderId":"seed-caller","contractName":"${symbol}","type":"LIMIT","side":"BUY","status":"NEW","action":"OPEN","price":100,"origQty":100,"executedQty":${cumulative},"transactTime":1}]}`,
        );
      if (path.endsWith('/order') && request.method === 'POST') {
        placements += 1;
        if (placements === 1) return raw('{"code":"-1022","msg":"signature expired"}');
        return raw(`{"code":"0","data":{"orderId":${placedId}}}`);
      }
      throw new Error(`Unexpected request ${request.method} ${request.url}`);
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    'bitrue',
    { key: 'replay-key', secret: 'replay-secret', extra: { recvWindow: 12345 } },
    store,
  );
  const main = new BaseExchange('bitrue', { key: '', secret: '' }, new DefaultStore());
  exchange.onFrame = (frame) => main.applyWorkerFrame(frame.slice(0));
  exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));
  exchange.on('error', (error: unknown) => errors.push(error));
  const bounded = <T>(operation: Promise<T>, stage: string) =>
    new Promise<T>((resolve, reject) => {
      const timer = setTimeout(
        () =>
          reject(
            new Error(
              `${stage}: ${JSON.stringify({ loaded: store.loaded, errors, requests: requests.map((r) => r.url) })}`,
            ),
          ),
        3000,
      );
      operation.then(
        (result) => {
          clearTimeout(timer);
          resolve(result);
        },
        (error) => {
          clearTimeout(timer);
          reject(error);
        },
      );
    });
  try {
    await bounded(exchange.start(), 'startup');
    expect(store.markets[0]).toMatchObject({ symbol, precision: { amount: 10 } });
    expect(store.orders[0]).toMatchObject({ id: orderId, amount: 100, filled: 0, remaining: 100 });
    expect(main.store.orders[0].id).toBe(orderId);
    expect(await bounded(exchange.fetchSymbolMaxLeverage(symbol), 'native bracket')).toBe(125);
    expect(store.marketsBySymbol[symbol].limits.leverage.max).toBe(125);
    expect(main.store.marketsBySymbol[symbol].limits.leverage.max).toBe(125);
    expect(store.fetchedLeverage[symbol]?.buyLeverage).toBe(25);
    const bracketRequest = requests.find((request) => new URL(request.url).pathname.endsWith('/leverageBracket'))!;
    expect(bracketRequest.method).toBe('GET');
    expect(bracketRequest.url).toBe(`https://fapi.bitrue.com/fapi/v2/leverageBracket?contractName=${symbol}`);
    // Prove this goes through the engine cache, not BaseExchange's JS store.
    const cachedOrders = store.orders;
    store.update({ orders: [] });
    const lookupRequestsBefore = requests.filter((request) =>
      new URL(request.url).pathname.endsWith('/openOrders'),
    ).length;
    expect(await bounded(exchange.findOrderByClientOrderId(symbol, 'seed-caller'), 'cached lookup')).toMatchObject({
      found: true,
      order: { id: orderId, clientOrderId: 'seed-caller', symbol },
    });
    expect(requests.filter((request) => new URL(request.url).pathname.endsWith('/openOrders'))).toHaveLength(
      lookupRequestsBefore,
    );
    store.update({ orders: cachedOrders });
    expect(await bounded(exchange.findOrderByClientOrderId(symbol, 'missing-caller'), 'open lookup')).toEqual({
      found: false,
      definitive: false,
    });
    const lookupRequest = requests.filter((request) => new URL(request.url).pathname.endsWith('/openOrders')).at(-1)!;
    expect(lookupRequest.url).toBe(`https://fapi.bitrue.com/fapi/v2/openOrders?contractName=${symbol}`);
    expect(lookupRequest.method).toBe('GET');
    expect(lookupRequest.headers['X-CH-APIKEY']).toBe('replay-key');
    expect(await bounded(exchange.fetchLeverageForSymbol(symbol), 'leverage')).toMatchObject({
      buyLeverage: 25,
      sellLeverage: 25,
      marginType: MarginType.Isolated,
    });
    const ref = await bounded(
      exchange.placeOrderWithClientOrderId({
        symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 20,
        price: 100,
        clientOrderId: 'caller / punctuation',
      }),
      'placement',
    );
    expect(ref.orderIds).toEqual([placedId]);
    const writes = requests.filter(
      (request) => request.method === 'POST' && new URL(request.url).pathname === '/fapi/v2/order',
    );
    expect(writes).toHaveLength(2);
    expect(writes[0].body).toBe(writes[1].body);
    expect(JSON.parse(writes[0].body!)).toMatchObject({
      clientOrderId: 'caller / punctuation',
      volume: '20',
      leverage: 25,
      positionType: 2,
    });
    expect(writes[0].headers['X-CH-SIGN']).not.toBe(writes[1].headers['X-CH-SIGN']);
    for (const request of writes) {
      const path = new URL(request.url).pathname;
      expect(request.headers.recvWindow).toBe('12345');
      expect(request.headers['X-CH-SIGN']).toBe(
        createHmac('sha256', 'replay-secret')
          .update(`${request.headers['X-CH-TS']}${request.method}${path}${request.body}`)
          .digest('hex'),
      );
    }
    await expect(
      exchange.placeOrderWithClientOrderId({
        symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 20,
        price: 100,
        clientOrderId: 'invalid\ncaller',
      }),
    ).rejects.toThrow();
    expect(placements).toBe(2);
    // l is this event's native fill quantity and cannot exceed cumulative z.
    // The replay and late event overlap execution ranges already delivered.
    const update = (nativeFilled: number, lastNativeFilled: number, executionPrice: number) => {
      cumulative = Math.max(cumulative, nativeFilled * 10);
      privateSocket!.onMessage(
        `{"e":"ORDER_TRADE_UPDATE","E":${++now},"o":{"s":"E_BTCUSDT","i":${orderId},"S":"BUY","o":"LIMIT","x":"TRADE","X":"PARTIALLY_FILLED","q":"100","z":"${nativeFilled}","l":"${lastNativeFilled}","p":"100","ap":"102","L":"${executionPrice}"}}`,
      );
    };
    expect(privateSocket).toBeDefined();
    update(1, 1, 101);
    update(3, 2, 103);
    update(3, 2, 103);
    update(2, 1, 102);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(fills.map(({ id, amount, price }) => ({ id, amount, price }))).toEqual([
      { id: orderId, amount: 10, price: 101 },
      { id: orderId, amount: 20, price: 103 },
    ]);
    expect(store.orders.find((order) => order.id === orderId)).toMatchObject({
      amount: 100,
      filled: 30,
      remaining: 70,
    });
    expect(main.store.orders.find((order) => order.id === orderId)).toMatchObject({
      amount: 100,
      filled: 30,
      remaining: 70,
    });
  } finally {
    exchange.dispose();
    main.dispose();
  }
}, 30000);
