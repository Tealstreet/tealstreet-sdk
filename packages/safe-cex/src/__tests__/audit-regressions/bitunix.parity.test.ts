import type { ExchangeOptions, Market, Order, Position } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  testnet: false,
  tuning: { consoleLogging: false },
};

const market: Market = {
  id: 'ETH/USDT',
  symbol: 'ETHUSDT',
  base: 'ETH',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.01, price: 0.1 },
  limits: { amount: { min: 0.01, max: 1000 }, leverage: { min: 1, max: 100 } },
};

const exchanges: BitunixExchange[] = [];
const createExchange = () => {
  const exchange = new BitunixExchange('bitunix', options, new DefaultStore());
  exchange.store.update({ markets: [market] });
  exchanges.push(exchange);
  return exchange;
};

afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

const pendingOrder = (over: Record<string, any> = {}) => ({
  orderId: 'order-1',
  clientId: 'caller-1',
  symbol: 'ETHUSDT',
  side: 'BUY',
  orderType: 'LIMIT',
  price: '3000',
  qty: '1',
  tradeQty: '0',
  reduceOnly: false,
  effect: 'GTC',
  ctime: 1_790_000_000_000,
  mtime: 1_790_000_000_000,
  ...over,
});

const fetchRegularOrders = (exchange: BitunixExchange, rows: Array<Record<string, any>>) => {
  exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { data: { orderList: rows } } }) } as any;
  return exchange.fetchRegularOrders();
};

const wsOrderFrame = (event: string, over: Record<string, any> = {}) =>
  JSON.stringify({
    ch: 'order',
    ts: 1_790_000_000_000,
    data: {
      event,
      symbol: 'ETHUSDT',
      orderId: 'order-1',
      side: 'Buy',
      type: 'Limit',
      qty: '1',
      price: '3000',
      ctime: '2026-10-21T00:00:00.000Z',
      mtime: '2026-10-21T00:00:01.000Z',
      orderStatus: 'PART_FILLED',
      averagePrice: '0',
      dealAmount: '0',
      clientId: 'caller-1',
      ...over,
    },
  });

const storedOrder = (over: Partial<Order> = {}): Order => ({
  id: 'order-1',
  clientOrderId: 'caller-1',
  status: OrderStatus.Open,
  symbol: 'ETHUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 3000,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 1,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  info: { orderId: 'order-1', clientId: 'caller-1' },
  ...over,
});

const captureFills = (exchange: BitunixExchange) => {
  const fills: Array<Record<string, any>> = [];
  exchange.emitter.on('fill', (fill: Record<string, any>) => fills.push(fill));
  return fills;
};

const crossLeg = (side: PositionSide) =>
  ({
    id: `ETHUSDT:${side}`,
    symbol: 'ETHUSDT',
    side,
    contracts: 0,
    entryPrice: 0,
    notional: 0,
    leverage: 10,
    unrealizedPnl: 0,
    liquidationPrice: 0,
    marginType: MarginType.Cross,
    positionMode: PositionMode.Hedged,
  }) as Position;

describe('Bitunix TS/Rust parity regressions', () => {
  it('(1) REST orders carry clientOrderId from clientId; virtual TP/SL legs do not', async () => {
    const exchange = createExchange();
    const orders = await fetchRegularOrders(exchange, [pendingOrder({ tpPrice: '3300', slPrice: '2700' })]);

    expect(orders.find((o) => o.id === 'order-1')?.clientOrderId).toBe('caller-1');
    expect(orders.find((o) => o.id === 'order-1__tp')?.clientOrderId).toBeUndefined();
    expect(orders.find((o) => o.id === 'order-1__sl')?.clientOrderId).toBeUndefined();
  });

  it('(1) WS CREATE carries clientOrderId from clientId', () => {
    const exchange = createExchange();
    exchange.privateWebsocket.onMessage({ data: wsOrderFrame('CREATE', { orderStatus: 'NEW' }) } as MessageEvent);

    expect(exchange.store.orders.find((o) => o.id === 'order-1')?.clientOrderId).toBe('caller-1');
  });

  it('(2) WS UPDATE/CLOSE emit one fill per execution from cumulative dealAmount/averagePrice', () => {
    const exchange = createExchange();
    exchange.store.update({ orders: [storedOrder()] });
    const fills = captureFills(exchange);

    exchange.privateWebsocket.onMessage({
      data: wsOrderFrame('UPDATE', { dealAmount: '0.4', averagePrice: '2999.5' }),
    } as MessageEvent);
    exchange.privateWebsocket.onMessage({
      data: wsOrderFrame('CLOSE', { dealAmount: '1', averagePrice: '2999.2', orderStatus: 'FILLED' }),
    } as MessageEvent);

    expect(fills).toEqual([
      { id: 'order-1', side: OrderSide.Buy, symbol: 'ETHUSDT', price: 2999.5, amount: 0.4 },
      { id: 'order-1', side: OrderSide.Buy, symbol: 'ETHUSDT', price: 2999, amount: 0.6 },
    ]);
  });

  it('(2) does not re-emit the fill already known from the REST snapshot', () => {
    const exchange = createExchange();
    exchange.store.update({
      orders: [storedOrder({ filled: 0.4, remaining: 0.6, info: { orderId: 'order-1', averagePrice: '2999.5' } })],
    });
    const fills = captureFills(exchange);

    exchange.privateWebsocket.onMessage({
      data: wsOrderFrame('CLOSE', { dealAmount: '1', averagePrice: '2999.2', orderStatus: 'FILLED' }),
    } as MessageEvent);

    expect(fills).toEqual([{ id: 'order-1', side: OrderSide.Buy, symbol: 'ETHUSDT', price: 2999, amount: 0.6 }]);
  });

  it('(3) REST orders set tp/sl trigger types only when tpPrice/slPrice exist', async () => {
    const exchange = createExchange();
    const orders = await fetchRegularOrders(exchange, [
      pendingOrder(),
      pendingOrder({ orderId: 'order-2', clientId: 'caller-2', tpPrice: '3300' }),
    ]);

    const plain = orders.find((o) => o.id === 'order-1')!;
    expect(plain.tpTriggerType).toBeUndefined();
    expect(plain.slTriggerType).toBeUndefined();
    const withTp = orders.find((o) => o.id === 'order-2')!;
    expect(withTp.tpTriggerType).toBe(OrderStopTriggerType.MarkPrice);
    expect(withTp.slTriggerType).toBeUndefined();
  });

  it('(4) the flat placeholder leg inherits leverage/marginType from the real leg', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [
            {
              positionId: 'pos-1',
              symbol: 'ETHUSDT',
              side: 'LONG',
              qty: '1',
              avgOpenPrice: '3000',
              entryValue: '3000',
              leverage: '25',
              marginMode: 'ISOLATION',
              positionMode: 'HEDGE',
            },
          ],
        },
      }),
    } as any;

    const positions = await exchange.fetchPositions();
    const short = positions.find((p) => p.id === 'ETHUSDT:short')!;

    expect(short.contracts).toBe(0);
    expect(short.leverage).toBe(25);
    expect(short.marginType).toBe(MarginType.Isolated);
  });

  it('(5) changeMarginType updates both legs in the store on success', async () => {
    const exchange = createExchange();
    exchange.store.update({ positions: [crossLeg(PositionSide.Long), crossLeg(PositionSide.Short)] });
    exchange.xhr = { post: vi.fn().mockResolvedValue({ data: { code: 0 } }) } as any;

    await exchange.changeMarginType(true, 'ETHUSDT');

    expect(exchange.store.positions.map((p) => p.marginType)).toEqual([MarginType.Isolated, MarginType.Isolated]);
  });

  it('(2) values a REST-known fill without an average at the order price', async () => {
    const exchange = createExchange();
    const orders = await fetchRegularOrders(exchange, [pendingOrder({ price: '90000', tradeQty: '0.001' })]);
    exchange.store.update({ orders });
    const fills = captureFills(exchange);

    exchange.privateWebsocket.onMessage({
      data: wsOrderFrame('UPDATE', { price: '90000', dealAmount: '0.002', averagePrice: '90000' }),
    } as MessageEvent);

    expect(fills).toHaveLength(1);
    expect(fills[0].amount).toBeCloseTo(0.001);
    expect(fills[0].price).toBeCloseTo(90000);
  });

  it('(2) a REST refresh ahead of the stream does not swallow the next streamed delta', async () => {
    const exchange = createExchange();
    exchange.store.update({ orders: [storedOrder()] });
    const fills = captureFills(exchange);

    exchange.privateWebsocket.onMessage({
      data: wsOrderFrame('UPDATE', { dealAmount: '0.4', averagePrice: '3000' }),
    } as MessageEvent);
    const refreshed = await fetchRegularOrders(exchange, [pendingOrder({ tradeQty: '0.6', avgPrice: '3000' })]);
    exchange.store.update({ orders: refreshed });
    expect(exchange.store.orders.find((o) => o.id === 'order-1')?.filled).toBe(0.6);
    exchange.privateWebsocket.onMessage({
      data: wsOrderFrame('UPDATE', { dealAmount: '0.6', averagePrice: '3000' }),
    } as MessageEvent);

    expect(fills.map((f) => f.amount)).toEqual([0.4, expect.closeTo(0.2)]);
  });

  it('(5) changeMarginType rejected inside an HTTP 200 throws and changes nothing', async () => {
    const exchange = createExchange();
    exchange.store.update({ positions: [crossLeg(PositionSide.Long), crossLeg(PositionSide.Short)] });
    exchange.symbolSettings.set('ETHUSDT', { marginType: MarginType.Cross, leverage: 10 });
    exchange.xhr = { post: vi.fn().mockResolvedValue({ data: { code: 10001, msg: 'rejected' } }) } as any;

    await expect(exchange.changeMarginType(true, 'ETHUSDT')).rejects.toThrow('rejected');

    expect(exchange.store.positions.map((p) => p.marginType)).toEqual([MarginType.Cross, MarginType.Cross]);
    expect(exchange.symbolSettings.get('ETHUSDT')).toEqual({ marginType: MarginType.Cross, leverage: 10 });
  });

  it('(6) market limits.amount.max is a number', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          data: [
            {
              symbol: 'ETHUSDT',
              base: 'ETH',
              quote: 'USDT',
              basePrecision: 2,
              quotePrecision: 1,
              minTradeVolume: '0.01',
              maxLimitOrderVolume: '1000',
              minLeverage: 1,
              maxLeverage: 100,
              symbolStatus: 'OPEN',
              isApiSupported: true,
            },
          ],
        },
      }),
    } as any;

    const [fetched] = await exchange.fetchMarkets();
    expect(fetched.limits.amount.max).toBe(1000);
  });

  it.each([
    { name: 'ISO text', nft: '2026-10-21T16:00:00Z', expected: Date.parse('2026-10-21T16:00:00Z') },
    { name: 'epoch ms', nft: '1792598400000', expected: 1_792_598_400_000 },
  ])('(7) WS ticker fundingTime reads nft as $name', ({ nft, expected }) => {
    const exchange = createExchange();
    exchange.store.update({
      tickers: [
        {
          id: 'ETHUSDT',
          symbol: 'ETHUSDT',
          bid: 0,
          ask: 0,
          last: 3000,
          mark: 3000,
          index: 3000,
          percentage: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
          openInterest: 0,
        },
      ],
    });

    exchange.publicWebsocket.handlePriceStreamEvent({
      symbol: 'ETHUSDT',
      data: { mp: '3001', ip: '3000', fr: '0.0001', ft: '2026-10-21T08:00:00Z', nft },
    });

    expect(exchange.store.tickersBySymbol.ETHUSDT.fundingTime).toBe(expected);
  });

  it('(8) fetchHistoricalFills keeps equal partial fills with distinct tradeIds', async () => {
    const exchange = createExchange();
    const row = (tradeId: string) => ({
      tradeId,
      orderId: 'order-1',
      symbol: 'ETHUSDT',
      side: 'BUY',
      price: '3000',
      qty: '0.5',
      fee: '0.1',
      ctime: '1790000000000',
    });
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({ data: { data: { tradeList: [row('t-1'), row('t-2'), row('t-2')] } } }),
    } as any;

    const fills = await exchange.fetchHistoricalFills({ symbol: 'ETHUSDT' });

    expect(fills.map((f) => f.id).sort()).toEqual(['t-1', 't-2']);
  });
});
