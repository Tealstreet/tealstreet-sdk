import { describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { ENDPOINTS } from '../reference/bitrue/bitrue.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../types';

const createExchange = () => {
  const store = new DefaultStore();
  return { exchange: new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store), store };
};

const storedOrder = (id: string, symbol: string) => ({
  id,
  symbol,
  status: OrderStatus.Open,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 1,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  lastUpdate: 1_700_000_000_000,
});

describe('audit regression: Bitrue remaining reconciliation and market limits', () => {
  it('keeps a failed per-symbol order read scoped to that symbol and uses the helper-wired read client', async () => {
    const { exchange, store } = createExchange();
    store.update({
      orders: [storedOrder('btc-order', 'E-BTC-USDT'), storedOrder('eth-order', 'E-ETH-USDT')],
    });

    const get = vi.fn().mockRejectedValue(new Error('temporary order read failure'));
    exchange.xhr = { get } as any;
    exchange.unlimitedXHR = { get: vi.fn() } as any;
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(exchange.fetchOrdersForSymbol('E-BTC-USDT')).resolves.toEqual([
      expect.objectContaining({ id: 'btc-order', symbol: 'E-BTC-USDT' }),
    ]);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ORDERS, { params: { contractName: 'E-BTC-USDT' } });
    expect(exchange.unlimitedXHR.get).not.toHaveBeenCalled();
  });

  it('starts markets at a conservative 100x until the per-contract 125x ceiling is discovered', async () => {
    const { exchange } = createExchange();
    exchange.unlimitedXHR = {
      get: vi.fn().mockResolvedValue({
        data: [
          {
            symbol: 'E-BTC-USDT',
            status: 1,
            multiplier: 0.001,
            pricePrecision: 1,
            minOrderVolume: 1,
            maxMarketVolume: 1_000,
          },
        ],
      }),
    } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets[0].limits.leverage).toEqual({ min: 1, max: 100 });
    expect(exchange.unlimitedXHR.get).toHaveBeenCalledWith(ENDPOINTS.MARKETS);
  });
});
