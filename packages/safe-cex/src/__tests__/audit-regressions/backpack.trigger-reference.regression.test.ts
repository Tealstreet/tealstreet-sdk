import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType } from '../../types';

const symbol = 'BTC_USDC_PERP';

describe('Backpack trigger reference audit regression', () => {
  let exchange: BackpackExchange;
  let post: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: symbol,
          symbol,
          base: 'BTC',
          quote: 'USDC',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
          contractSize: 1,
          takerFee: 0,
          makerFee: 0,
        },
      ],
    });

    exchange = new BackpackExchange('backpack', { key: '', secret: '', testnet: false } as ExchangeOptions, store);
    post = vi.fn().mockImplementation(() => Promise.resolve({ data: { id: 'order-id' } }));
    exchange.xhr = { post } as any;
  });

  it('honors explicit trigger references for primary TP and SL orders', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      amount: 1,
      price: 101,
      tpTriggerType: OrderStopTriggerType.LastPrice,
    });
    await exchange.placeOrder({
      symbol,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 1,
      price: 99,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });

    expect(post.mock.calls[0][1].triggerBy).toBe('LastPrice');
    expect(post.mock.calls[1][1].triggerBy).toBe('LastPrice');
  });

  it('uses Backpack metadata defaults when the primary trigger reference is not explicit', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      amount: 1,
      price: 101,
    });

    expect(post.mock.calls[0][1].triggerBy).toBe('MarkPrice');
  });

  it('honors explicit references for separate limit-order TP and SL children', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 100,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      takeProfit: 110,
      stopLoss: 90,
      tpTriggerType: OrderStopTriggerType.LastPrice,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });

    expect(post).toHaveBeenCalledTimes(3);
    expect(post.mock.calls[1][1].triggerBy).toBe('LastPrice');
    expect(post.mock.calls[2][1].triggerBy).toBe('LastPrice');
  });

  it('sets attached market-order TP and SL references from the advertised defaults', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1,
      takeProfit: 110,
      stopLoss: 90,
    });

    expect(post.mock.calls[0][1]).toMatchObject({
      takeProfitTriggerBy: 'MarkPrice',
      stopLossTriggerBy: 'MarkPrice',
    });
  });

  it('honors explicit references for attached market-order TP and SL orders', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1,
      takeProfit: 110,
      stopLoss: 90,
      tpTriggerType: OrderStopTriggerType.LastPrice,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });

    expect(post.mock.calls[0][1]).toMatchObject({
      takeProfitTriggerBy: 'LastPrice',
      stopLossTriggerBy: 'LastPrice',
    });
  });

  it('passes the officially supported IndexPrice reference through every placement path', async () => {
    await exchange.placeOrder({
      symbol,
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      amount: 1,
      price: 101,
      tpTriggerType: OrderStopTriggerType.IndexPrice,
    });
    await exchange.placeOrder({
      symbol,
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 1,
      price: 99,
      slTriggerType: OrderStopTriggerType.IndexPrice,
    });
    await exchange.placeOrder({
      symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 100,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      takeProfit: 110,
      stopLoss: 90,
      tpTriggerType: OrderStopTriggerType.IndexPrice,
      slTriggerType: OrderStopTriggerType.IndexPrice,
    });
    await exchange.placeOrder({
      symbol,
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1,
      takeProfit: 110,
      stopLoss: 90,
      tpTriggerType: OrderStopTriggerType.IndexPrice,
      slTriggerType: OrderStopTriggerType.IndexPrice,
    });

    expect(post.mock.calls[0][1].triggerBy).toBe('IndexPrice');
    expect(post.mock.calls[1][1].triggerBy).toBe('IndexPrice');
    expect(post.mock.calls[3][1].triggerBy).toBe('IndexPrice');
    expect(post.mock.calls[4][1].triggerBy).toBe('IndexPrice');
    expect(post.mock.calls[5][1]).toMatchObject({
      takeProfitTriggerBy: 'IndexPrice',
      stopLossTriggerBy: 'IndexPrice',
    });
  });
});
