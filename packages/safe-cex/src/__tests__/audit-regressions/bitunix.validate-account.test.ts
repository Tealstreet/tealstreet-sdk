import type { ExchangeOptions } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { ENDPOINTS } from '../reference/bitunix/bitunix.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  testnet: false,
  tuning: { consoleLogging: false },
};

const createExchange = () => new BitunixExchange('bitunix', options, new DefaultStore());

describe('Bitunix account validation regression', () => {
  it.each([
    { code: 10003, msg: 'Token invalid' },
    { code: 10007, msg: 'Sign signature error' },
  ])('rejects HTTP-200 exchange error code $code with its upstream message', async ({ code, msg }) => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({ data: { code, data: null, msg } });
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({ error: msg });
    expect(get).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { marginCoin: 'USDT' } });
  });

  it.each([
    { code: 10003, response: { code: 10003, data: null }, caseName: 'is missing' },
    { code: 10007, response: { code: 10007, data: null, msg: '' }, caseName: 'is empty' },
  ])('rejects HTTP-200 exchange error code $code when its message $caseName', async ({ response }) => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({ data: response });
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({ error: 'Bitunix account validation failed' });
    expect(get).toHaveBeenCalledOnce();
  });

  it('preserves successful account balance and position-mode extraction', async () => {
    const exchange = createExchange();
    const response = {
      data: {
        code: 0,
        data: [
          {
            marginCoin: 'USDT',
            available: '41.5',
            frozen: '1.25',
            margin: '3.25',
            crossUnrealizedPNL: '1.5',
            isolationUnrealizedPNL: '0',
            positionMode: 'ONE_WAY',
          },
        ],
        msg: 'Success',
      },
    };
    const get = vi.fn().mockResolvedValue(response);
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: '',
      balance: { total: 46, free: 41.5, used: 3.25, upnl: 1.5 },
    });
    expect(get).toHaveBeenCalledTimes(2);
    expect(exchange.accountPositionMode).toBe(PositionMode.OneWay);
  });

  it('reads position mode from the documented account-array response', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: 0,
        data: [
          { marginCoin: 'BTC', positionMode: 'HEDGE' },
          { marginCoin: 'USDT', positionMode: 'ONE_WAY' },
        ],
        msg: 'Success',
      },
    });
    exchange.xhr = { get } as any;

    await expect(exchange.fetchPositionMode()).resolves.toBe(false);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { marginCoin: 'USDT' } });
  });

  it('keeps malformed account numeric fields finite instead of publishing NaN', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: 0,
        data: [
          {
            marginCoin: 'USDT',
            availableBalance: 0,
            frozen: 'not-a-number',
            margin: '',
            crossUnrealizedPNL: 'not-a-number',
            positionMode: 'HEDGE',
          },
        ],
        msg: 'Success',
      },
    });
    exchange.xhr = { get } as any;

    const result = await exchange.fetchBalance();

    expect(result).toEqual({
      balance: { total: 0, free: 0, used: 0, upnl: 0 },
      positions: [],
    });
    expect(Object.values(result.balance).every(Number.isFinite)).toBe(true);
    expect(exchange.accountPositionMode).toBe(PositionMode.Hedged);
  });

  it('parses nested position wrappers and alias fields without known markets', async () => {
    const exchange = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: 0,
        data: {
          result: {
            positionList: [
              {
                positionId: 'position-1',
                instId: 'DOGEUSDT',
                positionAmt: '1234.5',
                positionSide: 'SHORT',
                posMode: 'ONE_WAY',
                marginType: 'ISOLATED',
                avgPrice: '0.1725',
                positionValue: '212.953125',
                lever: '5',
                unRealizedProfit: '6.25',
                realizedPnl: '1.5',
                liqPx: '0.08',
              },
            ],
          },
        },
        msg: 'Success',
      },
    });
    exchange.xhr = { get } as any;

    const positions = await exchange.fetchPositions();

    expect(positions).toEqual([
      expect.objectContaining({
        symbol: 'DOGEUSDT',
        side: PositionSide.Short,
        positionMode: PositionMode.OneWay,
        marginType: MarginType.Isolated,
        contracts: 1234.5,
        entryPrice: 0.1725,
        notional: 212.953125,
        leverage: 5,
        unrealizedPnl: 6.25,
        realizedPnl: 1.5,
        liquidationPrice: 0.08,
        info: expect.objectContaining({ positionId: 'position-1' }),
      }),
    ]);
    expect(exchange.positionIdCache.size).toBe(0);
  });

  it('preserves thrown transport-error handling', async () => {
    const exchange = createExchange();
    const error = new Error('transport unavailable');
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');
    const get = vi.fn().mockRejectedValue(error);
    exchange.xhr = { get } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({ error: 'transport unavailable' });
    expect(emitAndLogError).toHaveBeenCalledWith('transport unavailable');
    expect(get).toHaveBeenCalledOnce();
  });
});
