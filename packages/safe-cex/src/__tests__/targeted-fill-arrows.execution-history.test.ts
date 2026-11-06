import { describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from './reference/apexomni/apexomni.exchange';
import { ENDPOINTS as APEX_ENDPOINTS } from './reference/apexomni/apexomni.types';
import { BitunixExchange } from './reference/bitunix/bitunix.exchange';
import { ENDPOINTS as BITUNIX_ENDPOINTS } from './reference/bitunix/bitunix.types';
import { BlofinExchange } from './reference/blofin/blofin.exchange';
import { ENDPOINTS as BLOFIN_ENDPOINTS } from '../exchanges/blofin/blofin.types';
import { LighterExchange } from './reference/lighter/lighter.exchange';
import { ENDPOINTS as LIGHTER_ENDPOINTS } from '../exchanges/lighter/lighter.types';
import { OKXExchange } from './reference/okx/okx.exchange';
import { ENDPOINTS as OKX_ENDPOINTS } from '../exchanges/okx/okx.types';
import { ParadexExchange } from './reference/paradex/paradex.exchange';
import { ENDPOINTS as PARADEX_ENDPOINTS } from './reference/paradex/paradex.types';
import { PhemexExchange } from './reference/phemex/phemex.exchange';
import { ENDPOINTS as PHEMEX_ENDPOINTS } from '../exchanges/phemex/phemex.types';
import { OrderSide } from '../types';

const baseOptions = {
  key: 'key',
  secret: 'secret',
  passphrase: 'passphrase',
  testnet: false,
} as any;

const createStore = (markets: any[]) =>
  ({
    subscribe: vi.fn(),
    markets,
    marketsBySymbol: Object.fromEntries(markets.map((market) => [market.symbol, market])),
    marketsById: Object.fromEntries(markets.map((market) => [market.id, market])),
    tickersBySymbol: {},
    tickersById: {},
    orders: [],
    positions: [],
    update: vi.fn(),
  }) as any;

describe('Targeted exchange fetchHistoricalFills mapping', () => {
  it('maps OKX fills and falls back to fills-history', async () => {
    const market = {
      id: 'BTC-USDT-SWAP',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.01, price: 0.1 },
      contractSize: 0.01,
      isInverse: false,
      info: { ctValCcy: 'BTC' },
    };
    const exchange = new OKXExchange('okx' as any, baseOptions, createStore([market]));
    const get = vi
      .fn()
      .mockResolvedValueOnce({ data: { data: [] } })
      .mockResolvedValueOnce({
        data: {
          data: [
            {
              instId: 'BTC-USDT-SWAP',
              ordId: 'okx-order',
              side: 'buy',
              fillPx: '50000',
              fillSz: '2',
              fillTime: '1700000001000',
              fee: '-0.5',
              feeCcy: 'USDT',
            },
          ],
        },
      });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 20 });

    expect(get.mock.calls[0][0]).toBe(OKX_ENDPOINTS.FILLS);
    expect(get.mock.calls[1][0]).toBe(OKX_ENDPOINTS.FILLS_HISTORY);
    expect(executions).toEqual([
      {
        timestamp: 1700000001,
        symbol: 'BTCUSDT',
        orderId: 'okx-order',
        price: 50000,
        qty: 0.02,
        side: OrderSide.Buy,
        commission: 0.5,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('maps Phemex executions and filters non-trade rows', async () => {
    const market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
      info: { symbol: 'BTCUSDT' },
    };
    const exchange = new PhemexExchange('phemex' as any, baseOptions, createStore([market]));
    const get = vi
      .fn()
      .mockResolvedValueOnce({
        data: {
          data: {
            list: [
              {
                symbol: 'BTCUSDT',
                orderId: 'phemex-trade',
                side: 'Buy',
                execType: 'Trade',
                execPriceRp: '50100',
                execQty: '0.01',
                createTime: 1700000002000,
                execFeeRv: '-0.01',
                feeCurrency: 'USDT',
              },
              {
                symbol: 'BTCUSDT',
                orderId: 'phemex-funding',
                side: 'Sell',
                execType: 'Funding',
                execPriceRp: '50000',
                execQty: '0.02',
                createTime: 1700000003000,
              },
            ],
          },
        },
      });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 50 });

    expect(get.mock.calls[0][0]).toBe(PHEMEX_ENDPOINTS.USER_TRADES);
    expect(executions).toEqual([
      {
        timestamp: 1700000002,
        symbol: 'BTCUSDT',
        orderId: 'phemex-trade',
        price: 50100,
        qty: 0.01,
        side: OrderSide.Buy,
        commission: 0.01,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('returns empty for Bitunix when the documented history endpoint is empty', async () => {
    const market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new BitunixExchange('bitunix' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockResolvedValueOnce({ data: { data: { data: { tradeList: [] } } } });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 100 });

    expect(get.mock.calls[0][0]).toBe(BITUNIX_ENDPOINTS.HISTORY_TRADES);
    expect(get).toHaveBeenCalledTimes(1);
    expect(executions).toEqual([]);
  });

  it('uses Bitunix primary history rows without fallback for data.tradeList shape', async () => {
    const market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new BitunixExchange('bitunix' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockResolvedValueOnce({
      data: {
        data: {
          tradeList: [
            {
              symbol: 'BTCUSDT',
              orderId: 'bitunix-primary-order',
              side: 'BUY',
              price: '51500',
              qty: '0.01',
              fee: '-0.05',
              ctime: 1700000004500,
              feeCurrency: 'USDT',
            },
          ],
        },
      },
    });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 100 });

    expect(get).toHaveBeenCalledTimes(1);
    expect(get.mock.calls[0][0]).toBe(BITUNIX_ENDPOINTS.HISTORY_TRADES);
    expect(executions).toEqual([
      {
        timestamp: 1700000004,
        symbol: 'BTCUSDT',
        orderId: 'bitunix-primary-order',
        price: 51500,
        qty: 0.01,
        side: OrderSide.Buy,
        commission: 0.05,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('returns empty for Bitunix when the documented history endpoint errors', async () => {
    const market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new BitunixExchange('bitunix' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockRejectedValueOnce(new Error('Network Error'));
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 100 });

    expect(get).toHaveBeenCalledTimes(1);
    expect(get.mock.calls[0][0]).toBe(BITUNIX_ENDPOINTS.HISTORY_TRADES);
    expect(get.mock.calls[0][1]).toMatchObject({ __suppressExchangeError: true });
    expect(executions).toEqual([]);
  });

  it('handles Bitunix null open-orders payload without throwing', async () => {
    const market = {
      id: 'BTCUSDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new BitunixExchange('bitunix' as any, baseOptions, createStore([market]));
    exchange.xhr = {
      get: vi
        .fn()
        .mockResolvedValueOnce({ data: { data: null } })
        .mockResolvedValueOnce({ data: { data: null } }),
    } as any;

    const orders = await exchange.fetchOrders();

    expect(orders).toEqual([]);
    expect((exchange.xhr.get as any).mock.calls[0][0]).toBe(BITUNIX_ENDPOINTS.OPEN_ORDERS);
    expect((exchange.xhr.get as any).mock.calls[1][0]).toBe(BITUNIX_ENDPOINTS.OPEN_TPSL_ORDERS);
  });

  it('maps Blofin fills and falls back to orders-history rows', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
      contractSize: 0.001,
      info: { contractValue: '0.001' },
    };
    const exchange = new BlofinExchange('blofin' as any, baseOptions, createStore([market]));
    const get = vi
      .fn()
      .mockResolvedValueOnce({ data: { code: '0', data: [] } })
      .mockResolvedValueOnce({
        data: {
          code: '0',
          data: [
            {
              instId: 'BTC-USDT',
              orderId: 'blofin-order',
              side: 'buy',
              averagePrice: '52000',
              filledSize: '2',
              uTime: '1700000005000',
              fee: '-0.05',
              feeCurrency: 'USDT',
            },
          ],
        },
      });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(get.mock.calls[0][0]).toBe(BLOFIN_ENDPOINTS.FILLS);
    expect(get.mock.calls[1][0]).toBe(BLOFIN_ENDPOINTS.ORDERS_HISTORY);
    expect(executions).toEqual([
      {
        timestamp: 1700000005,
        symbol: 'BTCUSDT',
        orderId: 'blofin-order',
        price: 52000,
        qty: 0.002,
        side: OrderSide.Buy,
        commission: 0.05,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('maps Blofin fills-history rows without fallback', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
      contractSize: 0.001,
      info: { contractValue: '0.001' },
    };
    const exchange = new BlofinExchange('blofin' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockResolvedValueOnce({
      data: {
        code: '0',
        data: [
          {
            instId: 'BTC-USDT',
            orderId: 'blofin-fill-order',
            side: 'buy',
            fillPrice: '52100',
            fillSize: '3',
            ts: '1700000006000',
            fee: '-0.06',
          },
        ],
      },
    });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(get).toHaveBeenCalledTimes(1);
    expect(get.mock.calls[0][0]).toBe(BLOFIN_ENDPOINTS.FILLS);
    expect(get.mock.calls[0][1]).toMatchObject({
      params: {
        instId: 'BTC-USDT',
        limit: 100,
      },
    });
    expect(executions).toEqual([
      {
        timestamp: 1700000006,
        symbol: 'BTCUSDT',
        orderId: 'blofin-fill-order',
        price: 52100,
        qty: 0.003,
        side: OrderSide.Buy,
        commission: 0.06,
      },
    ]);
  });

  it('maps Blofin alternate fill payload fields for qty and fee', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
      contractSize: 0.001,
      info: { contractValue: '0.001' },
    };
    const exchange = new BlofinExchange('blofin' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockResolvedValueOnce({
      data: {
        code: '0',
        data: [
          {
            instId: 'BTC-USDT',
            algoId: 'blofin-algo-order',
            actualSide: 'sell',
            avgFillPrice: '52250',
            actualSize: '-4',
            updateTime: '1700000010000',
            actualFee: '-0.08',
            feeCcy: 'USDT',
          },
        ],
      },
    });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(executions).toEqual([
      {
        timestamp: 1700000010,
        symbol: 'BTCUSDT',
        orderId: 'blofin-algo-order',
        price: 52250,
        qty: 0.004,
        side: OrderSide.Sell,
        commission: 0.08,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('suppresses Blofin unsupported-operation fallback errors', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
      contractSize: 0.001,
      info: { contractValue: '0.001' },
    };
    const exchange = new BlofinExchange('blofin' as any, baseOptions, createStore([market]));
    (exchange as any).emitAndLogError = vi.fn();
    const get = vi
      .fn()
      .mockResolvedValueOnce({ data: { code: '0', data: [] } })
      .mockResolvedValueOnce({ data: { code: '51000', msg: 'This operation is not supported', data: [] } });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(executions).toEqual([]);
    expect(get.mock.calls[0][0]).toBe(BLOFIN_ENDPOINTS.FILLS);
    expect(get.mock.calls[1][0]).toBe(BLOFIN_ENDPOINTS.ORDERS_HISTORY);
    expect((exchange as any).emitAndLogError).not.toHaveBeenCalled();
  });

  it('maps Paradex private fills and falls back to order rows', async () => {
    const market = {
      id: 'BTC-USD-PERP',
      symbol: 'BTC-USD-PERP',
      base: 'BTC',
      quote: 'USD',
      precision: { amount: 0.001, price: 1 },
    };
    // Paradex uses key + paradexAddress + subkeyAddress (no secret) — its isPublicDataOnly
    // override requires both paradex creds, so the bare baseOptions fixture would short-circuit
    // fetchHistoricalFills to []. Provide test-only StarkNet/subkey addresses to exercise the REST path.
    const exchange = new ParadexExchange(
      'paradex' as any,
      { ...baseOptions, paradexAddress: '0xPARADEX', subkeyAddress: '0xSUBKEY' } as any,
      createStore([market]),
    );
    // The constructor kicks off real SDK init when paradexAddress is provided, which rejects
    // in the test environment. Replace the promise with an already-resolved no-op so
    // fetchHistoricalFills can proceed past the `await this.sdkInitPromise` guard, and stub the
    // jwtManager so the post-init guard also passes.
    (exchange as any).sdkInitPromise = undefined;
    (exchange as any).jwtManager = {} as any;
    const get = vi
      .fn()
      .mockResolvedValueOnce({ data: { results: [] } })
      .mockResolvedValueOnce({
        data: {
          results: [
            {
              market: 'BTC-USD-PERP',
              order_id: 'paradex-order',
              side: 'BUY',
              price: '53000',
              size: '0.01',
              fee: '-1.5',
              fee_currency: 'USD',
              created_at: 1700000006000,
            },
          ],
        },
      });
    exchange.xhr = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTC-USD-PERP', limit: 10 });

    expect(get.mock.calls[0][0]).toBe(PARADEX_ENDPOINTS.FILLS);
    expect(get.mock.calls[1][0]).toBe(PARADEX_ENDPOINTS.ORDERS_HISTORY);
    expect(executions).toEqual([
      {
        timestamp: 1700000006,
        symbol: 'BTC-USD-PERP',
        orderId: 'paradex-order',
        price: 53000,
        qty: 0.01,
        side: OrderSide.Buy,
        commission: 1.5,
        commissionAsset: 'USD',
      },
    ]);
  });

  it('maps ApexOmni fills and falls back to history-orders', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new ApexOmniExchange('apexomni' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === APEX_ENDPOINTS.FILLS) {
        return Promise.resolve({ data: { data: { fills: [] } } });
      }

      if (endpoint === APEX_ENDPOINTS.HISTORY_ORDERS) {
        return Promise.resolve({
          data: {
            data: {
              list: [
                {
                  symbol: 'BTC-USDT',
                  id: 'apex-order',
                  side: 'BUY',
                  latestMatchFillPrice: '54000',
                  cumSuccessFillSize: '0.03',
                  cumSuccessFillFee: '-0.01',
                  createdAt: 1700000007000,
                  token: 'USDT',
                },
              ],
            },
          },
        });
      }

      return Promise.resolve({ data: {} });
    });
    exchange.xhr = { get } as any;
    exchange.unlimitedXHR = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    const calledEndpoints = get.mock.calls.map((call) => call[0]);
    expect(calledEndpoints).toContain(APEX_ENDPOINTS.FILLS);
    expect(calledEndpoints).toContain(APEX_ENDPOINTS.HISTORY_ORDERS);
    expect(executions).toEqual([
      {
        timestamp: 1700000007,
        symbol: 'BTCUSDT',
        orderId: 'apex-order',
        price: 54000,
        qty: 0.03,
        side: OrderSide.Buy,
        commission: 0.01,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('falls back to ApexOmni history-orders when fills request fails without hard error event', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new ApexOmniExchange('apexomni' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockImplementation((endpoint: string) => {
      if (endpoint === APEX_ENDPOINTS.FILLS) {
        return Promise.reject(new Error('WSoverHTTP request timed out'));
      }

      if (endpoint === APEX_ENDPOINTS.HISTORY_ORDERS) {
        return Promise.resolve({
          data: {
            data: {
              list: [
                {
                  symbol: 'BTC-USDT',
                  id: 'apex-order-timeout-fallback',
                  side: 'SELL',
                  latestMatchFillPrice: '54100',
                  cumSuccessFillSize: '0.02',
                  cumSuccessFillFee: '-0.02',
                  createdAt: 1700000010000,
                  token: 'USDT',
                },
              ],
            },
          },
        });
      }

      return Promise.resolve({ data: {} });
    });
    exchange.xhr = { get } as any;
    exchange.unlimitedXHR = { get } as any;

    const emitAndLogErrorSpy = vi.spyOn(exchange, 'emitAndLogError');
    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    const calledEndpoints = get.mock.calls.map((call) => call[0]);
    expect(calledEndpoints).toContain(APEX_ENDPOINTS.FILLS);
    expect(calledEndpoints).toContain(APEX_ENDPOINTS.HISTORY_ORDERS);
    expect(emitAndLogErrorSpy).not.toHaveBeenCalled();
    expect(executions).toEqual([
      {
        timestamp: 1700000010,
        symbol: 'BTCUSDT',
        orderId: 'apex-order-timeout-fallback',
        price: 54100,
        qty: 0.02,
        side: OrderSide.Sell,
        commission: 0.02,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('maps ApexOmni compact fills rows from /v3/fills', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new ApexOmniExchange('apexomni' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockResolvedValueOnce({
      data: {
        data: {
          orders: [
            {
              s: 'BTCUSDT',
              S: 'Sell',
              p: '54200.5',
              v: '0.015',
              T: 1700000012345,
              i: 'apex-compact-trade-id',
              fee: '-0.005',
              feeCurrency: 'USDT',
            },
          ],
        },
      },
    });
    exchange.xhr = { get } as any;
    exchange.unlimitedXHR = { get } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(get).toHaveBeenCalledTimes(1);
    expect(get.mock.calls[0][0]).toBe(APEX_ENDPOINTS.FILLS);
    expect(executions).toEqual([
      {
        timestamp: 1700000012,
        symbol: 'BTCUSDT',
        orderId: 'apex-compact-trade-id',
        price: 54200.5,
        qty: 0.015,
        side: OrderSide.Sell,
        commission: 0.005,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('uses ApexOmni ws execution cache when REST history times out', async () => {
    const market = {
      id: 'BTC-USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new ApexOmniExchange('apexomni' as any, baseOptions, createStore([market]));
    const get = vi.fn().mockRejectedValue(new Error('WSoverHTTP request timed out'));
    exchange.xhr = { get } as any;
    exchange.unlimitedXHR = { get } as any;

    exchange.recordWsExecution({
      symbol: 'BTC-USDT',
      side: 'BUY',
      orderId: 'apex-ws-fill-order',
      price: '54300',
      size: '0.02',
      fee: '-0.01',
      feeCurrency: 'USDT',
      createdAt: 1700000019999,
    });

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(get).toHaveBeenCalled();
    expect(executions).toEqual([
      {
        timestamp: 1700000019,
        symbol: 'BTCUSDT',
        orderId: 'apex-ws-fill-order',
        price: 54300,
        qty: 0.02,
        side: OrderSide.Buy,
        commission: 0.01,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('maps Lighter inactive orders and falls back to trades endpoint', async () => {
    const market = {
      id: '255',
      symbol: 'BTC/USDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.1 },
    };
    const exchange = new LighterExchange('lighter' as any, baseOptions, createStore([market]));
    (exchange as any).ensureAccountIndex = vi.fn().mockResolvedValue(1n);
    (exchange as any).getPrivateWsAuthToken = vi.fn().mockResolvedValue('auth-token');
    (exchange as any).lighterApiClient = {
      get: vi
        .fn()
        .mockResolvedValueOnce({ data: { orders: [] } })
        .mockResolvedValueOnce({
          data: {
            trades: [
              {
                market_id: 255,
                bid_account_id: 1,
                bid_id: 'lighter-order',
                price: '55000',
                size: '0.05',
                maker_fee: '-0.2',
                timestamp: 1700000008000,
                feeCurrency: 'USDT',
              },
            ],
          },
        }),
    } as any;

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTC/USDT' });

    expect((exchange as any).lighterApiClient.get.mock.calls[0][0]).toBe(LIGHTER_ENDPOINTS.INACTIVE_ORDERS);
    expect((exchange as any).lighterApiClient.get.mock.calls[1][0]).toBe(LIGHTER_ENDPOINTS.TRADES);
    expect((exchange as any).lighterApiClient.get.mock.calls[1][1]).toMatchObject({
      params: {
        account_index: '1',
        market_id: 255,
        sort_by: 'timestamp',
        sort_dir: 'desc',
        limit: 100,
      },
    });
    expect((exchange as any).lighterApiClient.get.mock.calls[1][1].params.var_from).toBeUndefined();
    expect((exchange as any).lighterApiClient.get.mock.calls[1][1].params.var_to).toBeUndefined();
    expect(executions).toEqual([
      {
        timestamp: 1700000008,
        symbol: 'BTC/USDT',
        orderId: 'lighter-order',
        price: 55000,
        qty: 0.05,
        side: OrderSide.Buy,
        commission: 0.2,
        commissionAsset: 'USDT',
      },
    ]);
  });
});
