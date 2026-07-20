import { describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../../exchanges/woo/woo.exchange';
import { ENDPOINTS } from '../../exchanges/woo/woo.types';
import { DefaultStore } from '../../store/store.base';
import {
  OrderSide,
  OrderStatus,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const makeExchange = () => {
  const store = new DefaultStore();
  const exchange = new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      applicationId: 'application-id',
    } as any,
    store,
  );

  store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 } },
      } as any,
    ],
    tickers: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        last: 100,
      } as any,
    ],
  });
  vi.spyOn(exchange, 'getPositionMode').mockResolvedValue(PositionMode.OneWay);

  return { exchange, store };
};

describe('WooX remaining July audit regressions', () => {
  it('returns camelCase algo order IDs for partial TP/SL placement', async () => {
    const { exchange } = makeExchange();
    const post = vi.fn().mockResolvedValue({
      data: {
        success: true,
        data: { rows: [{ orderId: 101 }, { orderId: 102 }] },
        timestamp: 1,
      },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeStopMarket({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 1,
      price: 90,
    } as any);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({ algoType: 'STOP', triggerPrice: 90, quantity: 1 }),
    );
    expect(ids).toEqual(['101', '102']);
  });

  it('returns camelCase algo order IDs for trailing-stop placement', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      positions: [
        {
          id: 'BTCUSDT-Long',
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          contracts: 2,
        } as any,
      ],
    });
    const post = vi.fn().mockResolvedValue({
      data: {
        success: true,
        data: { rows: [{ orderId: 201 }] },
        timestamp: 1,
      },
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeTrailingStopLossOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.TrailingStopLoss,
      price: 95,
    } as any);

    expect(ids).toEqual(['201']);
  });

  it('does not recreate a stale sibling before replacing a positional bracket', async () => {
    const { exchange, store } = makeExchange();
    const takeProfit = {
      id: 'tp-child',
      parentId: 'bracket-parent',
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.TakeProfit,
      status: OrderStatus.Open,
      price: 120,
    } as any;
    const oldStopLoss = {
      ...takeProfit,
      id: 'sl-child',
      type: OrderType.StopLoss,
      price: 90,
    } as any;
    store.update({ orders: [takeProfit, oldStopLoss] });

    const remove = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { delete: remove } as any;
    const placePositional = vi.spyOn(exchange, 'placePositionalAlgoOrder').mockResolvedValue(['new-parent']);

    const ids = await exchange.placeAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      price: 92,
    } as any);

    expect(remove).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledWith(`${ENDPOINTS.ALGO_ORDER}/bracket-parent`);
    expect(placePositional).toHaveBeenCalledTimes(1);
    expect(placePositional).toHaveBeenCalledWith(
      expect.objectContaining({ stopLoss: 92, takeProfit: 120 }),
    );
    expect(ids).toEqual(['new-parent']);
  });

  it('selects the same closing side when replacing a hedged positional bracket', async () => {
    const { exchange, store } = makeExchange();
    const shortTakeProfit = {
      id: 'short-tp',
      parentId: 'short-parent',
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.TakeProfit,
      status: OrderStatus.Open,
      price: 80,
    } as any;
    const longTakeProfit = {
      ...shortTakeProfit,
      id: 'long-tp',
      parentId: 'long-parent',
      side: OrderSide.Sell,
      price: 120,
    } as any;
    store.update({ orders: [shortTakeProfit, longTakeProfit] });

    const remove = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { delete: remove } as any;
    const placePositional = vi.spyOn(exchange, 'placePositionalAlgoOrder').mockResolvedValue(['new-parent']);

    await exchange.placeAlgoOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      price: 92,
    } as any);

    expect(remove).toHaveBeenCalledWith(`${ENDPOINTS.ALGO_ORDER}/long-parent`);
    expect(placePositional).toHaveBeenCalledWith(
      expect.objectContaining({ stopLoss: 92, takeProfit: 120 }),
    );
  });

  it('rejects when replacement bracket placement fails after cancellation', async () => {
    const { exchange, store } = makeExchange();
    store.update({
      orders: [
        {
          id: 'tp-child',
          parentId: 'bracket-parent',
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type: OrderType.TakeProfit,
          status: OrderStatus.Open,
          price: 120,
        } as any,
      ],
    });
    const replacementError = {
      response: { data: { message: 'replacement rejected' } },
      message: 'Request failed',
    };
    const remove = vi.fn().mockResolvedValue({ data: { success: true } });
    const post = vi.fn().mockRejectedValue(replacementError);
    exchange.xhr = { delete: remove, post } as any;
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    await expect(
      exchange.placeAlgoOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.StopLoss,
        price: 92,
      } as any),
    ).rejects.toBe(replacementError);

    expect(remove).toHaveBeenCalledWith(`${ENDPOINTS.ALGO_ORDER}/bracket-parent`);
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({ algoType: 'POSITIONAL_TP_SL' }),
    );
    expect(emitError.mock.calls[0]?.[0]).toBe('replacement rejected');
  });

  it('surfaces private websocket authentication failures', () => {
    const { exchange } = makeExchange();
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({ event: 'auth', success: false, message: 'invalid signature' }),
    } as MessageEvent);

    expect(emitError.mock.calls[0]?.[0]).toBe('invalid signature');
  });

  it('returns the WooX response message when account validation fails', async () => {
    const { exchange } = makeExchange();
    exchange._primaryXhr = {
      get: vi.fn().mockRejectedValue({
        response: { data: { message: 'API key is invalid' } },
        message: 'Request failed',
      }),
    } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: 'API key is invalid',
    });
  });

  it('uses the current woox.io domain for legacy historical klines', () => {
    expect(ENDPOINTS.KLINE_HISTORY).toBe('https://api-pub.woox.io/v1/hist/kline');
  });
});
