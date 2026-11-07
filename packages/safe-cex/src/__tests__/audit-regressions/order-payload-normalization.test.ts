import type { ExchangeOptions, Market } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { OKXExchange } from '../reference/okx/okx.exchange';
import { BROKER_ID as OKX_BROKER_ID } from '../../exchanges/okx/okx.types';
import { DefaultStore } from '../../store/store.base';
import {
  OrderSide,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
} from '../../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  tuning: { consoleLogging: false },
};

const market = (id: string): Market => ({
  id,
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  contractSize: 1,
});

const withoutDynamicIds = (payload: Record<string, unknown>) => {
  const { newClientOrderId, clientAlgoId, clOrdId, algoClOrdId, clientOid, ...contract } = payload;
  expect(newClientOrderId ?? clientAlgoId ?? clOrdId ?? algoClOrdId ?? clientOid).toEqual(expect.any(String));
  return contract;
};

describe('audit regression: exact order payload normalization', () => {
  it('matches Binance USD-M market, post-only reduce limit, and conditional stop contracts', () => {
    const store = new DefaultStore();
    store.update({ markets: [market('BTCUSDT')] });
    const exchange = new BinanceExchange('binance', options, store);

    const marketOrder = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1.25,
    });
    expect(marketOrder.algo).toEqual([]);
    expect(withoutDynamicIds(marketOrder.regular[0])).toEqual({
      symbol: 'BTCUSDT',
      positionSide: 'BOTH',
      side: 'BUY',
      type: 'MARKET',
      quantity: '1.25',
    });

    const limitOrder = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 2,
      price: 50_000,
      timeInForce: OrderTimeInForce.PostOnly,
      reduceOnly: true,
    });
    expect(withoutDynamicIds(limitOrder.regular[0])).toEqual({
      symbol: 'BTCUSDT',
      positionSide: 'BOTH',
      side: 'SELL',
      type: 'LIMIT',
      quantity: '2',
      price: '50000',
      timeInForce: 'GTX',
      reduceOnly: 'true',
    });

    const stopOrder = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.5,
      price: 49_000,
      reduceOnly: true,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });
    expect(stopOrder.regular).toEqual([]);
    expect(withoutDynamicIds(stopOrder.algo[0])).toEqual({
      algoType: 'CONDITIONAL',
      symbol: 'BTCUSDT',
      positionSide: 'BOTH',
      side: 'SELL',
      type: 'STOP_MARKET',
      quantity: '0.5',
      triggerPrice: '49000',
      reduceOnly: 'true',
      workingType: 'CONTRACT_PRICE',
    });
  });

  it('matches OKX market, post-only reduce limit, and conditional stop contracts', () => {
    const store = new DefaultStore();
    store.update({ markets: [market('BTC-USDT-SWAP')] });
    const exchange = new OKXExchange('okx', options, store);
    exchange._accountPositionMode = PositionMode.OneWay;
    exchange._accountLevel = 2;

    const marketOrder = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1.25,
    });
    expect(marketOrder).toHaveLength(1);
    expect(withoutDynamicIds(marketOrder[0])).toEqual({
      instId: 'BTC-USDT-SWAP',
      tag: OKX_BROKER_ID,
      tdMode: 'cross',
      side: 'buy',
      ordType: 'market',
      sz: '1.25',
    });

    const limitOrder = (exchange as any).formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 2,
      price: 50_000,
      timeInForce: OrderTimeInForce.PostOnly,
      reduceOnly: true,
    });
    expect(withoutDynamicIds(limitOrder[0])).toEqual({
      instId: 'BTC-USDT-SWAP',
      tag: OKX_BROKER_ID,
      tdMode: 'cross',
      side: 'sell',
      ordType: 'post_only',
      px: '50000',
      reduceOnly: true,
      sz: '2',
    });

    const stopOrder = (exchange as any).formatAlgoOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.5,
      price: 49_000,
      reduceOnly: true,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });
    expect(withoutDynamicIds(stopOrder)).toEqual({
      instId: 'BTC-USDT-SWAP',
      tdMode: 'cross',
      tag: OKX_BROKER_ID,
      side: 'sell',
      ordType: 'conditional',
      sz: '0.5',
      reduceOnly: true,
      slTriggerPx: '49000',
      slOrdPx: '-1',
      slTriggerPxType: 'last',
    });
  });

  // Builds a full Bitget client; under CI load this outruns vitest's 5s default.
  it('matches Bitget V2 market, post-only reduce limit, and TPSL stop contracts', async () => {
    const store = new DefaultStore();
    store.update({ markets: [market('BTCUSDT')] });
    const exchange = new BitgetExchange('bitget', options, store);
    // This is the V2/Classic payload fixture, not the lazy account-type probe.
    vi.spyOn(exchange as any, 'ensureAccountTypeDetected').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
    vi.spyOn(exchange as any, 'getOrderPlanTypeAndAmount').mockReturnValue(['loss_plan', 0.5]);

    const [marketOrder] = await exchange.formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Market,
      side: OrderSide.Buy,
      amount: 1.25,
    });
    expect(withoutDynamicIds(marketOrder)).toEqual({
      symbol: 'BTCUSDT',
      size: '1.25',
      side: 'buy',
      orderType: 'market',
      reduceOnly: 'NO',
    });

    const [limitOrder] = await exchange.formatCreateOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Limit,
      side: OrderSide.Sell,
      amount: 2,
      price: 50_000,
      timeInForce: OrderTimeInForce.PostOnly,
      reduceOnly: true,
    });
    expect(withoutDynamicIds(limitOrder)).toEqual({
      symbol: 'BTCUSDT',
      size: '2',
      price: '50000',
      side: 'sell',
      orderType: 'limit',
      force: 'post_only',
      reduceOnly: 'YES',
    });

    const stopOrder = await exchange.formatAlgoOrder({
      symbol: 'BTCUSDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.5,
      price: 49_000,
      reduceOnly: true,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });
    expect(withoutDynamicIds(stopOrder)).toEqual({
      marginCoin: 'USDT',
      symbol: 'BTCUSDT',
      productType: 'usdt-futures',
      reduceOnly: 'YES',
      triggerPrice: '49000',
      holdSide: 'buy',
      planType: 'loss_plan',
      size: '0.5',
      triggerType: 'fill_price',
    });
  }, 30_000);
});
