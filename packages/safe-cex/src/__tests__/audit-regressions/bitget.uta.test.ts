import { describe, expect, it, vi } from 'vitest';

import { createAPI } from '../reference/bitget/bitget.api';
import { BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE, BitgetExchange } from '../reference/bitget/bitget.exchange';
import { BitgetInverseExchange } from '../reference/bitget/bitgetinverse.exchange';
import { BitgetUSDCExchange } from '../reference/bitget/bitgetusdc.exchange';
import { BASE_WS_PRIVATE_UTA_URL, ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionMode, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 125 } },
};

const usdcMarket = {
  ...market,
  id: 'BTCUSDC',
  symbol: 'BTCUSDC',
  quote: 'USDC',
};

const createExchange = (opts: { isUta?: boolean; accountTypeDetected?: boolean } = {}) => {
  const store = new DefaultStore();
  store.update({ markets: [market] as any });
  const exchange = new BitgetExchange(
    'bitget',
    { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
    store,
  );
  (exchange as any).isUta = opts.isUta ?? true;
  (exchange as any)._accountTypeDetected = opts.accountTypeDetected ?? true;
  exchange.xhr = { get: vi.fn(), post: vi.fn() } as any;
  exchange.unlimitedXHR = { get: vi.fn(), post: vi.fn() } as any;
  return exchange;
};

const createUsdcExchange = (opts: { isUta?: boolean; accountTypeDetected?: boolean } = {}) => {
  const store = new DefaultStore();
  store.update({ markets: [usdcMarket] as any });
  const exchange = new BitgetUSDCExchange(
    'bitgetusdc',
    { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
    store,
  );
  (exchange as any).isUta = opts.isUta ?? true;
  (exchange as any)._accountTypeDetected = opts.accountTypeDetected ?? true;
  exchange.xhr = { get: vi.fn(), post: vi.fn() } as any;
  exchange.unlimitedXHR = { get: vi.fn(), post: vi.fn() } as any;
  return exchange;
};

const accountSettingsResponse = {
  data: {
    code: '00000',
    msg: 'success',
    data: {
      holdMode: 'hedge_mode',
      symbolConfigList: [
        {
          category: 'USDT-FUTURES',
          symbol: 'BTCUSDT',
          marginMode: 'crossed',
          longLeverage: '10',
          shortLeverage: '7',
        },
      ],
    },
  },
};

const usdcAccountSettingsResponse = {
  data: {
    code: '00000',
    msg: 'success',
    data: {
      holdMode: 'hedge_mode',
      symbolConfigList: [
        {
          category: 'USDC-FUTURES',
          symbol: 'BTCUSDC',
          marginMode: 'crossed',
          longLeverage: '10',
          shortLeverage: '7',
        },
      ],
    },
  },
};

const BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE =
  'Failed to stop the strategy because the corresponding strategy ID could not be found';

const createUtaStopLossOrder = (overrides: Record<string, any> = {}) => ({
  id: 'sl-1',
  symbol: 'BTCUSDT',
  status: OrderStatus.Open,
  type: OrderType.StopLoss,
  side: OrderSide.Sell,
  price: 49_000,
  amount: 0.5,
  filled: 0,
  remaining: 0.5,
  reduceOnly: true,
  lastUpdate: Date.now(),
  info: { orderId: 'sl-1', clientOid: 'client-1' },
  ...overrides,
});

describe('audit regression: Bitget UTA routing', () => {
  it('routes UTA fill history through v3 fills with cursor pagination', async () => {
    const exchange = createExchange();
    const from = 1_750_000_000_000;
    const to = from + 60_000;
    const firstFill = {
      execId: '131111111111111111',
      orderId: 'order-1',
      category: 'USDT-FUTURES',
      symbol: 'BTCUSDT',
      side: 'sell',
      execPrice: '106950.1',
      execQty: '0.01',
      tradeScope: 'taker',
      tradeSide: 'open',
      feeDetail: [{ feeCoin: 'USDT', fee: '0.6417006' }],
      createdTime: '1750141421721',
      execPnl: '-0.002',
    };
    const secondFill = {
      ...firstFill,
      execId: '131111111111111112',
      orderId: 'order-2',
      side: 'buy',
      tradeSide: 'close',
      execPrice: '106900',
      execPnl: '0.5',
    };

    (exchange.xhr.get as any).mockImplementation((endpoint: string, config?: { params?: Record<string, any> }) => {
      if (endpoint !== ENDPOINTS.UTA_ORDER_FILLS) {
        throw new Error(`Unexpected endpoint ${endpoint}`);
      }
      if (config?.params?.cursor === 'cursor-1') {
        return Promise.resolve({
          data: { code: '00000', msg: 'success', data: { list: [secondFill], cursor: undefined } },
        });
      }
      return Promise.resolve({
        data: { code: '00000', msg: 'success', data: { list: [firstFill], cursor: 'cursor-1' } },
      });
    });

    const fills = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', orderId: 'order-1', from, to, limit: 1000 });

    expect(fills).toEqual([
      expect.objectContaining({
        id: '131111111111111111',
        orderId: 'order-1',
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        positionSide: PositionSide.Short,
        realizedPnl: -0.002,
      }),
      expect.objectContaining({
        id: '131111111111111112',
        side: OrderSide.Buy,
        positionSide: PositionSide.Short,
        realizedPnl: 0.5,
      }),
    ]);
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(1, ENDPOINTS.UTA_ORDER_FILLS, {
      params: {
        category: 'USDT-FUTURES',
        orderId: 'order-1',
        startTime: from,
        endTime: to,
        limit: 100,
      },
    });
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(2, ENDPOINTS.UTA_ORDER_FILLS, {
      params: {
        category: 'USDT-FUTURES',
        orderId: 'order-1',
        startTime: from,
        endTime: to,
        limit: 100,
        cursor: 'cursor-1',
      },
    });
    expect(
      (exchange.xhr.get as any).mock.calls.some(([endpoint]: [string]) => endpoint === ENDPOINTS.ORDER_FILLS),
    ).toBe(false);
  });

  it('splits UTA fill history requests into sub-30-day windows', async () => {
    const exchange = createExchange();
    const from = 1_750_000_000_000;
    const thirtyDaysMinusOne = 30 * 24 * 60 * 60 * 1000 - 1;
    const to = from + 40 * 24 * 60 * 60 * 1000;
    (exchange.xhr.get as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { list: [], cursor: undefined } },
    });

    await exchange.fetchHistoricalFills({ from, to });

    expect(exchange.xhr.get).toHaveBeenNthCalledWith(1, ENDPOINTS.UTA_ORDER_FILLS, {
      params: {
        category: 'USDT-FUTURES',
        startTime: from,
        endTime: from + thirtyDaysMinusOne,
        limit: 100,
      },
    });
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(2, ENDPOINTS.UTA_ORDER_FILLS, {
      params: {
        category: 'USDT-FUTURES',
        startTime: from + thirtyDaysMinusOne + 1,
        endTime: to,
        limit: 100,
      },
    });
  });

  it('validates UTA accounts through v3 account assets after detection', async () => {
    const exchange = createExchange({ accountTypeDetected: false });
    (exchange.xhr.get as any)
      .mockResolvedValueOnce(accountSettingsResponse)
      .mockResolvedValueOnce({
        data: {
          code: '00000',
          msg: 'success',
          data: {
            accountEquity: '105',
            effEquity: '98',
            unrealisedPnl: '5',
            assets: [{ coin: 'USDT', available: '98', equity: '105', usdValue: '105' }],
          },
        },
      });

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: '',
      balance: expect.objectContaining({ total: 100, free: 98, used: 7, upnl: 5 }),
    });
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(1, ENDPOINTS.ACCOUNT_SETTINGS_V3);
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(2, ENDPOINTS.UTA_ACCOUNT_ASSETS);
  });

  it('validates UTA USDC accounts through v3 account assets', async () => {
    const exchange = createUsdcExchange({ isUta: false, accountTypeDetected: false });
    (exchange.xhr.get as any)
      .mockResolvedValueOnce(usdcAccountSettingsResponse)
      .mockResolvedValueOnce({
        data: {
          code: '00000',
          msg: 'success',
          data: {
            accountEquity: '205',
            effEquity: '198',
            unrealisedPnl: '5',
            assets: [{ coin: 'USDC', available: '198', equity: '205', usdValue: '205' }],
          },
        },
      });

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: '',
      balance: expect.objectContaining({ total: 200, free: 198, used: 7, upnl: 5 }),
    });
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(1, ENDPOINTS.ACCOUNT_SETTINGS_V3);
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(2, ENDPOINTS.UTA_ACCOUNT_ASSETS);
  });

  it('lazily detects UTA before routing private reads on an unstarted instance', async () => {
    const exchange = createExchange({ isUta: false, accountTypeDetected: false });
    (exchange.xhr.get as any)
      .mockResolvedValueOnce(accountSettingsResponse)
      .mockResolvedValueOnce({
        data: {
          code: '00000',
          msg: 'success',
          data: {
            accountEquity: '105',
            effEquity: '98',
            unrealisedPnl: '5',
            assets: [{ coin: 'USDT', available: '98', equity: '105', usdValue: '105' }],
          },
        },
      });

    await expect(exchange.fetchBalance()).resolves.toEqual(
      expect.objectContaining({ total: 100, free: 98, used: 7, upnl: 5 }),
    );
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(1, ENDPOINTS.ACCOUNT_SETTINGS_V3);
    expect(exchange.xhr.get).toHaveBeenNthCalledWith(2, ENDPOINTS.UTA_ACCOUNT_ASSETS);
  });

  it('shares one in-flight UTA detection across concurrent private reads', async () => {
    const exchange = createExchange({ isUta: false, accountTypeDetected: false });
    (exchange.xhr.get as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.ACCOUNT_SETTINGS_V3) return Promise.resolve(accountSettingsResponse);
      if (endpoint === ENDPOINTS.UTA_ACCOUNT_ASSETS) {
        return Promise.resolve({
          data: {
            code: '00000',
            msg: 'success',
            data: {
              accountEquity: '105',
              effEquity: '98',
              unrealisedPnl: '5',
              assets: [{ coin: 'USDT', available: '98', equity: '105', usdValue: '105' }],
            },
          },
        });
      }
      if (endpoint === ENDPOINTS.UTA_POSITIONS) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: { list: [] } } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });

    await Promise.all([exchange.fetchBalance(), exchange.fetchOpenPositions()]);

    const accountSettingsCalls = (exchange.xhr.get as any).mock.calls.filter(
      ([endpoint]: [string]) => endpoint === ENDPOINTS.ACCOUNT_SETTINGS_V3,
    );
    expect(accountSettingsCalls).toHaveLength(1);
  });

  it('maps UTA positions including hold mode, side, margin mode, and blank liquidation fallback', async () => {
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Short, PositionMode.Hedged),
          symbol: 'BTCUSDT',
          side: PositionSide.Short,
          entryPrice: 50000,
          notional: 50000,
          leverage: 7,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 1,
          liquidationPrice: 62000,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
          info: {},
        },
      ] as any,
    });
    (exchange.xhr.get as any).mockResolvedValue({
      data: {
        code: '00000',
        msg: 'success',
        data: {
          list: [
            {
              symbol: 'BTCUSDT',
              holdMode: 'hedge_mode',
              posSide: 'short',
              marginMode: 'crossed',
              total: '0.5',
              avgPrice: '50000',
              markPrice: '49900',
              leverage: '7',
              unrealisedPnl: '50',
              liquidationPrice: '',
            },
          ],
        },
      },
    });

    const positions = await exchange.fetchOpenPositions();

    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.UTA_POSITIONS, {
      params: { category: 'USDT-FUTURES' },
    });
    expect(positions[0]).toEqual(
      expect.objectContaining({
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        positionMode: PositionMode.Hedged,
        marginType: MarginType.Cross,
        contracts: 0.5,
        leverage: 7,
        liquidationPrice: 62000,
      }),
    );
  });

  it('maps empty UTA position snapshots and paged open orders without v2 fields', async () => {
    const exchange = createExchange();
    let utaOrderPages = 0;
    (exchange.xhr.get as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UTA_POSITIONS) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: { list: [] } } });
      }
      if (endpoint === ENDPOINTS.UTA_ORDERS) {
        utaOrderPages += 1;
        return Promise.resolve({
          data: {
            code: '00000',
            msg: 'success',
            data:
              utaOrderPages === 1
                ? {
                    cursor: 'next',
                    list: [
                      {
                        orderId: 'order-1',
                        category: 'USDT-FUTURES',
                        symbol: 'BTCUSDT',
                        orderStatus: 'new',
                        orderType: 'limit',
                        side: 'buy',
                        qty: '1',
                        cumExecQty: '0.25',
                        price: '50000',
                        timeInForce: 'gtc',
                      },
                    ],
                  }
                : { cursor: 'next', list: [] },
          },
        });
      }
      if (endpoint === ENDPOINTS.UTA_STRATEGY_ORDERS) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: [] } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });

    await expect(exchange.fetchOpenPositions()).resolves.toEqual([]);
    const orders = await exchange.fetchOrders();

    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.UTA_ORDERS, {
      params: { category: 'USDT-FUTURES', limit: '100' },
    });
    expect(orders[0]).toEqual(
      expect.objectContaining({
        id: 'order-1',
        status: OrderStatus.Open,
        amount: 1,
        filled: 0.25,
        remaining: 0.75,
      }),
    );
  });

  it('does not fall back to Classic v2 fill history for UTA accounts', async () => {
    const exchange = createExchange();
    (exchange.xhr.get as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { list: [], cursor: undefined } },
    });

    await expect(exchange.fetchHistoricalFills({ from: 1_700_000_000_000, to: 1_700_086_400_000 })).resolves.toEqual(
      [],
    );
    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.UTA_ORDER_FILLS, {
      params: {
        category: 'USDT-FUTURES',
        startTime: 1_700_000_000_000,
        endTime: 1_700_086_400_000,
        limit: 100,
      },
    });
    expect(
      (exchange.xhr.get as any).mock.calls.some(([endpoint]: [string]) => endpoint === ENDPOINTS.ORDER_FILLS),
    ).toBe(false);
  });

  it('lazily detects UTA before placing normal limit orders with v3 fields only', async () => {
    const exchange = createExchange({ isUta: false, accountTypeDetected: false });
    (exchange.xhr.get as any).mockResolvedValue(accountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: null, clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.5,
        price: 50000,
      }),
    ).resolves.toEqual(['client-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        qty: '0.5',
        side: 'buy',
        posSide: 'long',
        marginMode: 'crossed',
        clientOid: expect.any(String),
      }),
    );
    expect(exchange.xhr.get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT_SETTINGS_V3);
    const [, payload] = (exchange.unlimitedXHR.post as any).mock.calls[0];
    expect(payload).not.toHaveProperty('productType');
    expect(payload).not.toHaveProperty('marginCoin');
    expect(payload).not.toHaveProperty('size');
  });

  it('uses documented UTA cross-margin default when account settings omit a flat symbol config', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    (exchange.xhr.get as any).mockResolvedValue({
      data: {
        code: '00000',
        msg: 'success',
        data: {
          holdMode: 'hedge_mode',
          symbolConfigList: [],
        },
      },
    });
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'order-1', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.5,
        price: 50_000,
      }),
    ).resolves.toEqual(['order-1']);

    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        qty: '0.5',
        side: 'buy',
        posSide: 'long',
        marginMode: 'crossed',
        clientOid: expect.any(String),
      }),
    );
  });

  it('places UTA normal orders with attached TP/SL fields on the v3 place-order endpoint', async () => {
    const exchange = createExchange();
    (exchange.xhr.get as any).mockResolvedValue(accountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'order-1', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.5,
        price: 50_000,
        takeProfit: 55_000,
        stopLoss: 49_000,
        tpTriggerType: OrderStopTriggerType.LastPrice,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ).resolves.toEqual(['order-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        qty: '0.5',
        side: 'buy',
        posSide: 'long',
        orderType: 'limit',
        takeProfit: '55000',
        stopLoss: '49000',
        tpTriggerBy: 'market',
        slTriggerBy: 'mark',
        tpOrderType: 'market',
        slOrderType: 'market',
      }),
    );
    expect(exchange.unlimitedXHR.post).not.toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
      expect.anything(),
    );
  });

  it('places UTA USDC linear orders with the USDC-FUTURES category', async () => {
    const exchange = createUsdcExchange();
    (exchange.xhr.get as any).mockResolvedValue(usdcAccountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: null, clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.5,
        price: 50_000,
      }),
    ).resolves.toEqual(['client-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_ORDER,
      expect.objectContaining({
        category: 'USDC-FUTURES',
        symbol: 'BTCUSDC',
        qty: '0.5',
        side: 'buy',
        posSide: 'long',
        marginMode: 'crossed',
        clientOid: expect.any(String),
      }),
    );
    const [, payload] = (exchange.unlimitedXHR.post as any).mock.calls[0];
    expect(payload).not.toHaveProperty('productType');
    expect(payload).not.toHaveProperty('marginCoin');
    expect(payload).not.toHaveProperty('size');
  });

  it('places UTA USDC normal orders with attached stop-loss fields', async () => {
    const exchange = createUsdcExchange();
    (exchange.xhr.get as any).mockResolvedValue(usdcAccountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'order-1', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDC',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.5,
        price: 50_000,
        stopLoss: 49_000,
      }),
    ).resolves.toEqual(['order-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_ORDER,
      expect.objectContaining({
        category: 'USDC-FUTURES',
        symbol: 'BTCUSDC',
        qty: '0.5',
        side: 'buy',
        posSide: 'long',
        stopLoss: '49000',
        slTriggerBy: 'mark',
        slOrderType: 'market',
      }),
    );
  });

  it('places UTA USDC stop-loss orders through USDC-FUTURES strategy orders', async () => {
    const exchange = createUsdcExchange();
    (exchange.xhr.get as any).mockResolvedValue(usdcAccountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'sl-1', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDC',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0,
        price: 49_000,
      }),
    ).resolves.toEqual(['sl-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
      expect.objectContaining({
        category: 'USDC-FUTURES',
        symbol: 'BTCUSDC',
        type: 'tpsl',
        side: 'sell',
        posSide: 'long',
        tpslMode: 'full',
        stopLoss: '49000',
        slOrderType: 'market',
      }),
    );
  });

  it('places full-position UTA stop-loss orders through v3 strategy orders', async () => {
    const exchange = createExchange();
    (exchange.xhr.get as any).mockResolvedValue(accountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'sl-1', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0,
        price: 49_000,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ).resolves.toEqual(['sl-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        type: 'tpsl',
        side: 'sell',
        posSide: 'long',
        tpslMode: 'full',
        reduceOnly: 'yes',
        stopLoss: '49000',
        slTriggerBy: 'mark',
        slOrderType: 'market',
        clientOid: expect.any(String),
      }),
    );
    const [, payload] = (exchange.unlimitedXHR.post as any).mock.calls[0];
    expect(payload).not.toHaveProperty('qty');
    expect(payload).not.toHaveProperty('productType');
    expect(payload).not.toHaveProperty('marginCoin');
  });

  it('resolves live position size for infinity-marker UTA TP/SL and sends the partial+qty shape', async () => {
    const exchange = createExchange();
    const longPosition = {
      id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      entryPrice: 50_000,
      notional: 75_000,
      leverage: 10,
      unrealizedPnl: 0,
      realizedPnl: 0,
      contracts: 1.5,
      liquidationPrice: 40_000,
      positionMode: PositionMode.Hedged,
      marginType: MarginType.Cross,
      info: {},
    };
    exchange.store.update({ positions: [longPosition] as any });
    vi.spyOn(exchange, 'getOrFetchPositionsForSymbol').mockResolvedValue([longPosition] as any);
    (exchange.xhr.get as any).mockResolvedValue(accountSettingsResponse);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'sl-live', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0,
        price: 49_000,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ).resolves.toEqual(['sl-live']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        type: 'tpsl',
        side: 'sell',
        posSide: 'long',
        tpslMode: 'partial',
        qty: '1.5',
        stopLoss: '49000',
        slTriggerBy: 'mark',
        slOrderType: 'market',
      }),
    );
    const [, payload] = (exchange.unlimitedXHR.post as any).mock.calls[0];
    expect(payload).toHaveProperty('qty', '1.5');
  });

  it('places partial UTA take-profit orders with qty and market trigger mapping', async () => {
    const exchange = createExchange();
    const longPosition = {
      id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      entryPrice: 50_000,
      notional: 50_000,
      leverage: 10,
      unrealizedPnl: 0,
      realizedPnl: 0,
      contracts: 1,
      liquidationPrice: 40_000,
      positionMode: PositionMode.Hedged,
      marginType: MarginType.Cross,
      info: {},
    };
    exchange.store.update({ positions: [longPosition] as any });
    vi.spyOn(exchange, 'getOrFetchPositionsForSymbol').mockResolvedValue([longPosition] as any);
    (exchange.unlimitedXHR.post as any).mockResolvedValue({
      data: { code: '00000', msg: 'success', data: { orderId: 'tp-1', clientOid: 'client-1' } },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        amount: 0.5,
        price: 55_000,
        tpTriggerType: OrderStopTriggerType.LastPrice,
      }),
    ).resolves.toEqual(['tp-1']);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(
      ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        type: 'tpsl',
        side: 'sell',
        posSide: 'long',
        tpslMode: 'partial',
        qty: '0.5',
        takeProfit: '55000',
        tpTriggerBy: 'market',
        tpOrderType: 'market',
      }),
    );
  });

  it('fetches and cancels UTA strategy stop-loss orders through v3 strategy endpoints', async () => {
    const exchange = createExchange();
    (exchange.xhr.get as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UTA_ORDERS) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: { list: [] } } });
      }
      if (endpoint === ENDPOINTS.UTA_STRATEGY_ORDERS) {
        return Promise.resolve({
          data: {
            code: '00000',
            msg: 'success',
            data: [
              {
                orderId: 'sl-1',
                clientOid: 'client-1',
                category: 'USDT-FUTURES',
                symbol: 'BTCUSDT',
                qty: '0.5',
                posSide: 'long',
                status: 'pending',
                stopLoss: '49000',
                slTriggerBy: 'mark',
                slOrderType: 'market',
              },
            ],
          },
        });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });
    (exchange.unlimitedXHR.post as any).mockResolvedValue({ data: { code: '00000', msg: 'success', data: null } });

    const orders = await exchange.fetchOrders();
    expect(orders).toHaveLength(1);
    expect(orders[0]).toEqual(
      expect.objectContaining({
        id: 'sl-1',
        status: OrderStatus.Open,
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 49_000,
        amount: 0.5,
        reduceOnly: true,
        slTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    );

    await exchange.cancelOrders(orders);
    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER, {
      orderId: 'sl-1',
      clientOid: 'client-1',
    });
  });

  it('treats already-missing UTA strategy orders as idempotent cancel success', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined as any);

    (exchange.unlimitedXHR.post as any)
      .mockResolvedValueOnce({
        data: {
          code: '40830',
          msg: BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE,
          data: null,
        },
      })
      .mockRejectedValueOnce({
        response: {
          data: {
            code: '40830',
            msg: BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE,
          },
        },
      });

    await expect(exchange.cancelOrders([createUtaStopLossOrder({ id: 'sl-resolved' })])).resolves.toBeUndefined();
    await expect(exchange.cancelOrders([createUtaStopLossOrder({ id: 'sl-rejected' })])).resolves.toBeUndefined();

    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(log).toHaveBeenCalledTimes(2);
  });

  it('does not emit when the Bitget interceptor sees resolved strategy-cancel not-found responses', async () => {
    const exchange = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    const api = createAPI(
      { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
      exchange,
    );

    api.defaults.adapter = vi.fn(async (config: any) => ({
      data: {
        code: '40830',
        msg: BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE,
        data: null,
      },
      status: 400,
      statusText: 'Bad Request',
      headers: {},
      config,
    }));

    await expect(api.post(ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER, { orderId: 'sl-1' })).resolves.toMatchObject({
      data: {
        code: '40830',
        msg: BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE,
      },
    });
    expect(emitAndLogError).not.toHaveBeenCalled();
  });

  it('does not place a UTA strategy replacement when update cancel finds the original already gone', async () => {
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 1,
          liquidationPrice: 40_000,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
          info: {},
        },
      ] as any,
    });
    (exchange.unlimitedXHR.post as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER) {
        return Promise.resolve({
          data: {
            code: '40830',
            msg: BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE,
            data: null,
          },
        });
      }
      if (endpoint === ENDPOINTS.UTA_PLACE_STRATEGY_ORDER) {
        throw new Error('replacement should not be placed');
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.updateOrder({
        order: createUtaStopLossOrder({ amount: 0, remaining: 0 }),
        update: { price: 48_000 },
      }),
    ).rejects.toThrow(BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE);

    expect(exchange.unlimitedXHR.post).toHaveBeenCalledTimes(1);
    expect(exchange.unlimitedXHR.post).toHaveBeenCalledWith(ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER, {
      orderId: 'sl-1',
      clientOid: 'client-1',
    });
    expect(emitAndLogError).toHaveBeenCalledWith(BITGET_STRATEGY_ORDER_NOT_FOUND_MESSAGE);
  });

  it('updates UTA TP/SL orders by canceling and replacing through v3 strategy endpoints', async () => {
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 1,
          liquidationPrice: 40_000,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
          info: {},
        },
      ] as any,
    });
    (exchange.unlimitedXHR.post as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: null } });
      }
      if (endpoint === ENDPOINTS.UTA_PLACE_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: { orderId: 'sl-2' } } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });

    await expect(
      exchange.updateOrder({
        order: {
          id: 'sl-1',
          symbol: 'BTCUSDT',
          status: OrderStatus.Open,
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49_000,
          amount: 0,
          filled: 0,
          remaining: 0,
          reduceOnly: true,
          lastUpdate: Date.now(),
          info: { orderId: 'sl-1', clientOid: 'client-1' },
        },
        update: { price: 48_000 },
      }),
    ).resolves.toEqual(['sl-2']);

    expect(exchange.unlimitedXHR.post).toHaveBeenNthCalledWith(1, ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER, {
      orderId: 'sl-1',
      clientOid: 'client-1',
    });
    expect(exchange.unlimitedXHR.post).toHaveBeenNthCalledWith(
      2,
      ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
      expect.objectContaining({
        category: 'USDT-FUTURES',
        symbol: 'BTCUSDT',
        type: 'tpsl',
        side: 'sell',
        posSide: 'long',
        tpslMode: 'partial',
        qty: '1',
        reduceOnly: 'yes',
        stopLoss: '48000',
        slTriggerBy: 'mark',
        slOrderType: 'market',
      }),
    );
    expect(exchange.unlimitedXHR.post).not.toHaveBeenCalledWith(ENDPOINTS.UPDATE_ALGO_ORDER, expect.anything());
  });

  it('updates the qty on the replacement payload when the caller changes amount instead of price', async () => {
    // Mirror of the price-update path: the amount branch must reach the
    // replacement payload as `qty`, not silently regress to the position size.
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 1,
          liquidationPrice: 40_000,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
          info: {},
        },
      ] as any,
    });
    (exchange.unlimitedXHR.post as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: null } });
      }
      if (endpoint === ENDPOINTS.UTA_PLACE_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: { orderId: 'sl-2' } } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });

    await expect(
      exchange.updateOrder({
        order: {
          id: 'sl-1',
          symbol: 'BTCUSDT',
          status: OrderStatus.Open,
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49_000,
          amount: 1,
          filled: 0,
          remaining: 1,
          reduceOnly: true,
          lastUpdate: Date.now(),
          info: { orderId: 'sl-1', clientOid: 'client-1' },
        },
        update: { amount: 0.5 },
      }),
    ).resolves.toEqual(['sl-2']);

    const placeCall = (exchange.unlimitedXHR.post as any).mock.calls.find(
      ([endpoint]: [string]) => endpoint === ENDPOINTS.UTA_PLACE_STRATEGY_ORDER,
    );
    expect(placeCall?.[1]).toEqual(
      expect.objectContaining({
        symbol: 'BTCUSDT',
        type: 'tpsl',
        side: 'sell',
        posSide: 'long',
        tpslMode: 'partial',
        qty: '0.5',
        // Price was not part of the update; the original trigger must be preserved.
        stopLoss: '49000',
      }),
    );
  });

  it('rolls back to the original stop when the replacement place call fails', async () => {
    // Regression: cancel-and-replace has no in-place amend. If cancel succeeds
    // and place fails, updateUtaStrategyOrder must attempt to re-place the
    // original stop so the user is not left unprotected. The place error is
    // still re-thrown so the caller knows the update did not land.
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 1,
          liquidationPrice: 40_000,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
          info: {},
        },
      ] as any,
    });

    const placePayloads: Array<Record<string, any>> = [];
    (exchange.unlimitedXHR.post as any).mockImplementation((endpoint: string, payload: Record<string, any>) => {
      if (endpoint === ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: null } });
      }
      if (endpoint === ENDPOINTS.UTA_PLACE_STRATEGY_ORDER) {
        placePayloads.push(payload);
        // First place is the replacement (fails), second place is the rollback (succeeds).
        if (placePayloads.length === 1) {
          return Promise.resolve({ data: { code: '40808', msg: 'insufficient margin', data: null } });
        }
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: { orderId: 'sl-restored' } } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.updateOrder({
        order: {
          id: 'sl-1',
          symbol: 'BTCUSDT',
          status: OrderStatus.Open,
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49_000,
          amount: 0,
          filled: 0,
          remaining: 0,
          reduceOnly: true,
          lastUpdate: Date.now(),
          info: { orderId: 'sl-1', clientOid: 'client-1' },
        },
        update: { price: 48_000 },
      }),
    ).rejects.toThrow('insufficient margin');

    expect(placePayloads).toHaveLength(2);
    // Replacement attempted with the updated price
    expect(placePayloads[0]).toEqual(expect.objectContaining({ stopLoss: '48000' }));
    // Rollback attempted with the ORIGINAL price
    expect(placePayloads[1]).toEqual(expect.objectContaining({ stopLoss: '49000' }));
    // User is informed the update failed but the original stop is restored
    expect(emitAndLogError.mock.calls.some(([msg]) => String(msg).includes('Original stop was restored'))).toBe(true);
  });

  it('emits an actionable "please re-add" error when both the replacement and the rollback fail', async () => {
    // If cancel succeeds, replacement fails, AND rollback fails, the user
    // truly has no stop on the book. Silence is unacceptable here.
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: createPositionId('BTCUSDT', PositionSide.Long, PositionMode.Hedged),
          symbol: 'BTCUSDT',
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 50_000,
          leverage: 10,
          unrealizedPnl: 0,
          realizedPnl: 0,
          contracts: 1,
          liquidationPrice: 40_000,
          positionMode: PositionMode.Hedged,
          marginType: MarginType.Cross,
          info: {},
        },
      ] as any,
    });

    (exchange.unlimitedXHR.post as any).mockImplementation((endpoint: string) => {
      if (endpoint === ENDPOINTS.UTA_CANCEL_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '00000', msg: 'success', data: null } });
      }
      if (endpoint === ENDPOINTS.UTA_PLACE_STRATEGY_ORDER) {
        return Promise.resolve({ data: { code: '40808', msg: 'insufficient margin', data: null } });
      }
      throw new Error(`Unexpected endpoint ${endpoint}`);
    });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.updateOrder({
        order: {
          id: 'sl-1',
          symbol: 'BTCUSDT',
          status: OrderStatus.Open,
          type: OrderType.StopLoss,
          side: OrderSide.Sell,
          price: 49_000,
          amount: 0,
          filled: 0,
          remaining: 0,
          reduceOnly: true,
          lastUpdate: Date.now(),
          info: { orderId: 'sl-1', clientOid: 'client-1' },
        },
        update: { price: 48_000 },
      }),
    ).rejects.toThrow('insufficient margin');

    expect(
      emitAndLogError.mock.calls.some(([msg]) =>
        String(msg).includes('Your stop is not on the book — please re-add it.'),
      ),
    ).toBe(true);
  });

  it('subscribes and routes UTA private WS topics', async () => {
    const exchange = createExchange();
    const ws = exchange.privateWebsocket as any;
    const sendWithLog = vi.fn();
    ws.sendWithLog = sendWithLog;
    vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchBalance').mockResolvedValue({ total: 0, free: 0, used: 0, upnl: 0 });
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);

    ws.subscribe();
    expect(sendWithLog).toHaveBeenCalledWith(
      JSON.stringify({
        op: 'subscribe',
        args: [
          { instType: 'UTA', topic: 'account' },
          { instType: 'UTA', topic: 'position' },
          { instType: 'UTA', topic: 'order' },
        ],
      }),
    );

    const createWebsocket = vi.fn().mockReturnValue({ addEventListener: vi.fn() });
    ws.createWebsocket = createWebsocket;
    ws.connectAndSubscribe();
    expect(createWebsocket).toHaveBeenCalledWith(BASE_WS_PRIVATE_UTA_URL);

    ws.handleUtaPositionsTopic({
      action: 'snapshot',
      data: [{ symbol: 'BTCUSDT', holdMode: 'hedge_mode', posSide: 'long', total: '1', avgPrice: '50000' }],
    });
    expect(exchange.store.positions[0]).toEqual(expect.objectContaining({ symbol: 'BTCUSDT', contracts: 1 }));

    ws.handleUtaOrderTopic({
      data: [
        {
          orderId: 'uta-order',
          symbol: 'BTCUSDT',
          orderType: 'limit',
          side: 'buy',
          qty: '1',
          cumExecQty: '0',
          price: '50000',
          orderStatus: 'new',
        },
      ],
    });
    expect(exchange.store.orders[0]).toEqual(expect.objectContaining({ id: 'uta-order', amount: 1 }));
  });

  it('keeps inverse variants on the UTA unsupported guard', async () => {
    const inverse = new BitgetInverseExchange(
      'bitgetinverse',
      { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tuning: { consoleLogging: false } },
      new DefaultStore(),
    );
    (inverse as any).isUta = true;
    inverse.xhr = {
      get: vi.fn().mockResolvedValue({
        data: {
          code: '40085',
          msg: 'You are in Unified Account mode, and the Classic Account API is not supported at this time',
          data: null,
        },
      }),
    } as any;

    await expect(inverse.fetchBalance()).rejects.toThrow(BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE);
  });

  it('awaits Bitget account-type detection before private WS and tick startup', async () => {
    const exchange = createExchange({ accountTypeDetected: false });
    const events: string[] = [];

    (exchange as any).initExternalPublicData = vi.fn().mockResolvedValue(true);
    (exchange as any)._detectAccountType = vi.fn().mockImplementation(async () => {
      events.push('detect');
    });
    exchange.privateWebsocket.connectAndSubscribe = vi.fn().mockImplementation(() => {
      events.push('ws');
    });
    exchange.tick = vi.fn().mockImplementation(async () => {
      events.push('tick');
    });
    exchange.fetchOrders = vi.fn().mockImplementation(async () => {
      events.push('orders');
      return [];
    });

    await exchange.start();

    expect(events).toEqual(['detect', 'ws', 'tick', 'orders']);
  });
});
