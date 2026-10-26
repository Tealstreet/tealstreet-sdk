import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitDemoExchange } from '../reference/bybit/bybitdemo.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../reference/bybit/bybitspot.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { DefaultStore } from '../../store/store.base';

const createVariants = () => [
  new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new DefaultStore()),
  new BybitInverseExchange('bybitinverse', { key: 'key', secret: 'secret' }, new DefaultStore()),
  new BybitSpotExchange('bybitspot', { key: 'key', secret: 'secret' }, new DefaultStore()),
];

const unifiedBalance = {
  totalEquity: '100',
  totalMaintenanceMargin: '2',
  totalInitialMargin: '3',
  totalAvailableBalance: '95',
  totalMarginBalance: '96',
  coin: [{ coin: 'USDT', equity: '100', usdValue: '100' }],
};

describe('Bybit UTA-first account behavior', () => {
  it('queries the supported UNIFIED wallet on the first request for every variant', async () => {
    for (const exchange of createVariants()) {
      const get = vi.fn().mockResolvedValue({ data: { retCode: 0, retMsg: 'OK', result: { list: [unifiedBalance] } } });
      exchange.xhr = { get } as any;

      await exchange.fetchRawBalance();

      expect(exchange.accountType).toBe('UNIFIED');
      expect(get).toHaveBeenCalledWith(ENDPOINTS.BALANCE, { params: { accountType: 'UNIFIED' } });
    }
  });

  it('keeps the retired-account error fallback as a defensive retry', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new DefaultStore());
    (exchange as any).unifiedMarginStatus = 1;
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: { retCode: 10001, retMsg: 'accountType only support UNIFIED.' },
      })
      .mockResolvedValueOnce({
        data: { retCode: 0, retMsg: 'OK', result: { list: [unifiedBalance] } },
      });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchRawBalance()).resolves.toEqual([unifiedBalance]);
    expect(get).toHaveBeenNthCalledWith(1, ENDPOINTS.BALANCE, { params: { accountType: 'CONTRACT' } });
    expect(get).toHaveBeenNthCalledWith(2, ENDPOINTS.BALANCE, { params: { accountType: 'UNIFIED' } });
  });

  it('always formats UTA account totals and uses the UTA batch-cancel endpoint', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, new DefaultStore());
    (exchange as any).unifiedMarginStatus = 1;

    expect(exchange.formatBalance([unifiedBalance])).toMatchObject({
      total: 100,
      used: 5,
      free: 95,
      upnl: 0,
    });

    const post = vi.fn().mockResolvedValue({ data: { retCode: 0, retMsg: 'OK', retExtInfo: { list: [] } } });
    exchange.unlimitedXHR = { post } as any;
    await exchange.cancelOrdersHelper([{ id: 'order-1', symbol: 'BTCUSDT' } as any], 'linear');

    expect(post).toHaveBeenCalledWith(ENDPOINTS.CANCEL_ORDERS, {
      category: 'linear',
      request: [{ symbol: 'BTCUSDT', orderId: 'order-1' }],
    });
  });

  it('derives Bybit Demo available balance when UTA aggregate margin fields are blank', () => {
    const exchange = new BybitDemoExchange('bybitdemo', { key: 'key', secret: 'secret' }, new DefaultStore());
    const blankAggregateBalance = {
      accountType: 'UNIFIED',
      totalAvailableBalance: '',
      totalEquity: '195721.20725548',
      totalInitialMargin: '',
      totalMaintenanceMargin: '',
      totalMarginBalance: '',
      totalWalletBalance: '195721.20725548',
      coin: [
        {
          coin: 'USDT',
          equity: '195721.20725548',
          usdValue: '195721.20725548',
          walletBalance: '195721.20725548',
          totalPositionIM: '6886.51',
          totalOrderIM: '100',
          locked: '5',
          bonus: '0',
        },
      ],
    };

    const balance = exchange.formatBalance([blankAggregateBalance]);

    expect(balance).toMatchObject({
      total: 195721.20725548,
      used: 6986.51,
      free: 188729.69725548,
      upnl: 0,
    });
    expect(Number.isFinite(balance.total)).toBe(true);
    expect(Number.isFinite(balance.used)).toBe(true);
    expect(Number.isFinite(balance.free)).toBe(true);
  });
});
