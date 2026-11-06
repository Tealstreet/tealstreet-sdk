import { beforeEach, describe, expect, it, vi } from 'vitest';

import { BackpackExchange } from '../reference/backpack/backpack.exchange';
import { ENDPOINTS } from '../reference/backpack/backpack.types';
import { DefaultStore } from '../../store/store.base';
import { ExchangeOptions, OrderSide } from '../../types';

const symbol = 'BTC_USDC_PERP';

describe('Backpack execution history audit regression', () => {
  let exchange: BackpackExchange;
  let get: ReturnType<typeof vi.fn>;

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
    exchange = new BackpackExchange(
      'backpack',
      { key: 'public-key', secret: 'secret', testnet: false } as ExchangeOptions,
      store,
    );
    get = vi.fn().mockResolvedValue({
      data: [
        {
          clientId: 1,
          fee: '-0.25',
          feeSymbol: 'USDC',
          isMaker: false,
          orderId: 'order-1',
          price: '100.5',
          quantity: '2',
          side: 'Bid',
          symbol,
          timestamp: '2026-07-15T08:00:00.000Z',
          tradeId: 42,
        },
        {
          fee: '0.1',
          feeSymbol: 'USDC',
          isMaker: true,
          orderId: 'other-order',
          price: '101',
          quantity: '1',
          side: 'Ask',
          symbol: 'SOL_USDC_PERP',
          timestamp: '2026-07-15T08:00:01.000Z',
          tradeId: 43,
        },
      ],
    });
    exchange.xhr = { get } as any;
  });

  it('requests the official fill-history endpoint and maps documented fields', async () => {
    const executions = await exchange.fetchHistoricalFills({
      symbol,
      orderId: 'order-1',
      from: 1_721_000_000,
      to: 1_721_000_100_000,
      limit: 5_000,
    });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.FILLS_HISTORY, {
      params: {
        symbol,
        orderId: 'order-1',
        from: 1_721_000_000_000,
        to: 1_721_000_100_000,
        limit: 1000,
        sortDirection: 'Desc',
      },
    });
    expect(executions).toEqual([
      {
        id: '42',
        timestamp: 1_784_102_400,
        symbol,
        orderId: 'order-1',
        price: 100.5,
        qty: 2,
        side: OrderSide.Buy,
        commission: 0.25,
        commissionAsset: 'USDC',
      },
    ]);
  });
});
