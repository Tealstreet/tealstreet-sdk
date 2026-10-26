import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStopTriggerType, OrderType, PositionMode } from '../../types';

describe('audit regression: Bitget trailing-stop placement', () => {
  it('builds the documented place-plan-order contract', async () => {
    const exchange = new BitgetExchange(
      'bitget',
      { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase' },
      new DefaultStore(),
    );
    exchange.store.update({
      markets: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
        },
      ],
      tickers: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 100,
          index: 100,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });
    vi.spyOn(exchange as any, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.Hedged);
    vi.spyOn(exchange, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    const post = vi.fn().mockResolvedValue({ data: { code: '00000', data: { orderId: 'trail-1' } } });
    exchange.unlimitedXHR = { post } as any;

    const payload = await exchange.formatAlgoOrder({
      symbol: 'BTCUSDT',
      type: OrderType.TrailingStopLoss,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 95,
      reduceOnly: true,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });

    expect(payload).toEqual(
      expect.objectContaining({
        symbol: 'BTCUSDT',
        productType: 'usdt-futures',
        marginCoin: 'USDT',
        marginMode: 'crossed',
        planType: 'track_plan',
        callbackRatio: '5',
        triggerPrice: '95',
        triggerType: 'fill_price',
        orderType: 'market',
        side: 'sell',
        tradeSide: 'close',
        size: '0.25',
      }),
    );
    expect(payload).not.toHaveProperty('rangeRate');

    await expect(exchange.placeAlgoOrders([payload])).resolves.toEqual(['trail-1']);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.PLACE_TRAILING_STOP_ORDER, payload);
    expect(ENDPOINTS.PLACE_TRAILING_STOP_ORDER).toBe('/api/v2/mix/order/place-plan-order');
  });
});
