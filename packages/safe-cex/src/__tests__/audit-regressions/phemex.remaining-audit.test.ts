import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../reference/phemex/phemex.exchange';
import { DefaultStore } from '../../store/store.base';
import {
  LogSeverity,
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
        info: { symbol: 'BTCUSDT' },
      },
    ] as any,
    tickers: [{ id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', last: 60_000 } as any],
    positions: [
      {
        id: 'BTCUSDT-long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        contracts: 0.25,
        entryPrice: 59_000,
        notional: 14_750,
        leverage: 10,
        marginType: MarginType.Cross,
        positionMode: PositionMode.Hedged,
        unrealizedPnl: 250,
        liquidationPrice: 50_000,
      },
    ],
  });
  return new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
};

describe('audit regression: Phemex dormant orders and public subscription cap', () => {
  it('keeps the dormant trailing-stop payload on the documented quantity field', async () => {
    const exchange = createExchange();
    const [payload] = await (exchange as any).formatCreateTrailingStopLossOrder({
      symbol: 'BTCUSDT',
      type: OrderType.TrailingStopLoss,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 59_500,
      reduceOnly: true,
    });

    expect(payload).toEqual(expect.objectContaining({ orderQtyRq: '0.25', ordType: 'Stop' }));
    expect(payload).not.toHaveProperty('quantity');
    expect(payload).not.toHaveProperty('type');
  });

  it('fails opening trigger orders closed until their stop contract is implemented', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      (exchange as any).formatCreateOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 61_000,
      }),
    ).rejects.toThrow('opening trigger orders are disabled');
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('stopPxRp and triggerType'));
  });

  it('hydrates a missing symbol market before formatting a market order', async () => {
    const store = new DefaultStore();
    const exchange = new PhemexExchange('phemex' as any, { key: 'test', secret: 'test' } as any, store);
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([
      {
        id: 'ADA/USDT:USDT',
        symbol: 'ADAUSDT',
        base: 'ADA',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.01, price: 0.0001 },
        limits: { amount: { min: 0.01, max: 10_000 }, leverage: { min: 1, max: 100 } },
        info: { symbol: 'ADAUSDT' },
      } as any,
    ]);
    vi.spyOn(exchange as any, 'getOrderPositionSide').mockResolvedValue('Merged');
    vi.spyOn(exchange as any, 'getOrFetchPositionsForSymbol').mockResolvedValue([]);

    const [payload] = await (exchange as any).formatCreateOrder({
      symbol: 'ADAUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 52.85,
    });

    expect(exchange.fetchMarkets).toHaveBeenCalledTimes(1);
    expect(store.marketsBySymbol.ADAUSDT?.symbol).toBe('ADAUSDT');
    expect(payload).toEqual(expect.objectContaining({ symbol: 'ADAUSDT', orderQtyRq: '52.85', ordType: 'Market' }));
  });

  it('upserts successful REST-created stop orders while waiting for websocket echo', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.unlimitedXHR, 'put').mockResolvedValue({
      data: {
        code: 0,
        msg: '',
        data: {
          orderID: 'rest-stop-1',
          symbol: 'BTCUSDT',
          side: 'Sell',
          ordStatus: 'Untriggered',
          execStatus: 'Registered',
          orderType: 'Stop',
          stopDirection: 'Falling',
          priceRp: '0',
          stopPxRp: '58000',
          orderQtyRq: '0.25',
          execQty: '0',
          execInst: 'CloseOnTrigger',
          trigger: 'ByMarkPrice',
          timeInForce: 'ImmediateOrCancel',
        },
      },
    } as any);

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 58_000,
      reduceOnly: true,
    });

    expect(ids).toEqual(['rest-stop-1']);
    expect(exchange.store.orders).toEqual([
      expect.objectContaining({
        id: 'rest-stop-1',
        status: OrderStatus.Open,
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 58_000,
        amount: 0.25,
        remaining: 0.25,
        filled: 0,
        reduceOnly: true,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ]);
  });

  it('warns once as active public subscriptions approach the 20-topic cap', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    vi.spyOn(websocket, 'send').mockImplementation(() => undefined);
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    const cleanups = Array.from({ length: 17 }, (_, index) =>
      websocket.listenOHLCV({ symbol: `SYMBOL${index}`, interval: '1m' } as any, vi.fn()),
    );

    expect(log).toHaveBeenCalledTimes(1);
    expect(log).toHaveBeenCalledWith(
      '[Phemex] Public WebSocket has 18/20 active subscriptions; additional charts or orderbooks may be rejected.',
      LogSeverity.Warning,
    );

    cleanups.forEach((cleanup) => cleanup());
    expect(Object.keys(websocket.topics)).toHaveLength(0);
    websocket.dispose();
  });
});
