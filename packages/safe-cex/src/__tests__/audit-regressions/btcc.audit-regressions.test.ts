import { describe, expect, it, vi } from 'vitest';

import { BtccExchange } from '../../exchanges/btcc/btcc.exchange';
import { ENDPOINTS, BTCCOrderStatus, BTCCOrderType, BTCCPositionStatus } from '../../exchanges/btcc/btcc.types';
import { getExchangeMetadata } from '../../exchangeMetadata';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';

const credentials = {
  key: 'api-key',
  secret: 'api-secret',
  applicationId: 'username',
  passphrase: 'password',
};

const createExchange = (options: Record<string, unknown> = {}) =>
  new BtccExchange('btcc', { ...credentials, ...options } as any, new DefaultStore());

describe('BTCC audit regressions', () => {
  it('keeps unsupported market-data and testnet paths gated', () => {
    const metadata = getExchangeMetadata('btcc');

    expect(metadata).toMatchObject({
      enabled: false,
      enabledInDev: false,
      enableInPreview: false,
      supportsPublicOnly: false,
      supportsTestnet: false,
      features: { kline: false, orderbook: false },
    });
  });

  it('fails remote signing closed before building an invalid local MD5 signature', () => {
    const exchange = createExchange({ remoteSigner: { sign: vi.fn() } });
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    expect(() => (exchange as any).ensureCredentials()).toThrow('BTCC does not support remote signing');
    expect(emitAndLogError).toHaveBeenCalledWith('BTCC does not support remote signing');
  });

  it('emits local preflight failures immediately before throwing', async () => {
    const missingKey = createExchange({ key: '' });
    const missingKeyError = vi.spyOn(missingKey, 'emitAndLogError').mockImplementation(() => undefined);
    expect(() => (missingKey as any).ensureCredentials()).toThrow('BTCC requires an API key');
    expect(missingKeyError).toHaveBeenCalledWith('BTCC requires an API key');

    const exchange = createExchange();
    (exchange as any).token = 'token';
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.Market,
        amount: 1,
        reduceOnly: true,
      }),
    ).rejects.toThrow('No BTCC position available to close');
    expect(emitAndLogError).toHaveBeenCalledWith('No BTCC position available to close');
  });

  it('collects every private snapshot page before mapping orders and positions', async () => {
    const exchange = createExchange();
    (exchange as any).token = 'token';
    (exchange as any).account = { id: 7, account_no: 'A-7' };

    const position = (id: number) => ({
      id,
      accountid: 7,
      symbol: 'BTCUSDT',
      direction: 1,
      status: BTCCPositionStatus.Open,
      volume: 1,
      exec_volume: 1,
      open_price: 50_000,
      close_price: 0,
      exec_time: 0,
      open_time: 0,
      close_time: 0,
      profit: 0,
      swap: 0,
    });
    const order = (id: number) => ({
      id,
      accountid: 7,
      symbol: 'BTCUSDT',
      direction: 1,
      type: BTCCOrderType.LimitOpen,
      status: BTCCOrderStatus.Pending,
      request_volume: 1,
      request_price: 50_000,
      execute_volume: 0,
      execute_price: 0,
      execute_time: 0,
    });
    const firstPositions = Array.from({ length: 200 }, (_, index) => position(index + 1));
    const firstOrders = Array.from({ length: 200 }, (_, index) => order(index + 1));
    const request = vi.fn().mockImplementation(({ url, params }: any) => {
      if (url === ENDPOINTS.POSITION_LIST) {
        return Promise.resolve({
          data: { code: 0, total: 201, positions: params.pageNo === '1' ? firstPositions : [position(201)] },
        });
      }
      if (url === ENDPOINTS.ORDER_LIST) {
        return Promise.resolve({
          data: { code: 0, total: 201, orders: params.pageNo === '1' ? firstOrders : [order(201)] },
        });
      }
      throw new Error(`Unexpected BTCC endpoint ${url}`);
    });
    exchange.xhr = { request } as any;

    await expect(exchange.fetchPositions()).resolves.toHaveLength(201);
    await expect(exchange.fetchOrders()).resolves.toHaveLength(201);
    expect(request.mock.calls.map(([config]) => [config.url, config.params.pageNo])).toEqual([
      [ENDPOINTS.POSITION_LIST, '1'],
      [ENDPOINTS.POSITION_LIST, '2'],
      [ENDPOINTS.ORDER_LIST, '1'],
      [ENDPOINTS.ORDER_LIST, '2'],
    ]);
  });

  it('maps BTCC position and pending-order snapshots into the shared contracts', () => {
    const exchange = createExchange();
    const position = (exchange as any).mapPosition({
      id: 44,
      symbol: 'BTCUSDT',
      direction: 2,
      status: BTCCPositionStatus.Open,
      volume: '3',
      exec_volume: '3',
      open_price: '50000',
      close_price: '40000',
      profit: '-12',
      multiple: '20',
    });
    const order = (exchange as any).mapOrder({
      id: 55,
      symbol: 'BTCUSDT',
      direction: 1,
      type: BTCCOrderType.MarketTakeProfit,
      status: BTCCOrderStatus.PartiallyFilled,
      request_volume: '4',
      request_price: '51000',
      execute_volume: '1.5',
    });

    expect(position).toMatchObject({
      symbol: 'BTCUSDT',
      side: PositionSide.Short,
      contracts: 3,
      entryPrice: 50_000,
      leverage: 20,
      unrealizedPnl: -12,
      liquidationPrice: 40_000,
    });
    expect(order).toMatchObject({
      id: '55',
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.TakeProfit,
      status: OrderStatus.Open,
      price: 51_000,
      amount: 4,
      filled: 1.5,
      remaining: 2.5,
      reduceOnly: true,
    });
  });

  it('sends market and pending orders through their distinct BTCC write contracts', async () => {
    const exchange = createExchange();
    (exchange as any).token = 'token';
    (exchange as any).account = { id: 7, account_no: 'A-7' };
    const request = vi.fn().mockImplementation(({ url }: any) => {
      if (url === ENDPOINTS.OPEN_POSITION) {
        return Promise.resolve({ data: { code: 0, position: { id: 101 } } });
      }
      if (url === ENDPOINTS.OPEN_PENDING) {
        return Promise.resolve({ data: { code: 0, order: { id: 202 } } });
      }
      throw new Error(`Unexpected BTCC endpoint ${url}`);
    });
    exchange.xhr = { request } as any;

    await expect(
      exchange.placeOrder({ symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Market, amount: 2 }),
    ).resolves.toEqual(['101']);
    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Sell,
        type: OrderType.Limit,
        amount: 3,
        price: 51_000,
      }),
    ).resolves.toEqual(['202']);

    expect(request.mock.calls.map(([config]) => [config.url, config.params])).toEqual([
      [
        ENDPOINTS.OPEN_POSITION,
        expect.objectContaining({
          symbol: 'BTCUSDT',
          direction: '1',
          request_volume: '2',
          request_price: '0',
          multiple: '20',
          token: 'token',
          accountid: '7',
          sign: expect.any(String),
        }),
      ],
      [
        ENDPOINTS.OPEN_PENDING,
        expect.objectContaining({
          symbol: 'BTCUSDT',
          direction: '2',
          type: String(BTCCOrderType.LimitOpen),
          request_volume: '3',
          request_price: '51000',
          token: 'token',
          accountid: '7',
          sign: expect.any(String),
        }),
      ],
    ]);
  });

  it('returns the login response balance from account validation without refetching it', async () => {
    const exchange = createExchange();
    const account = {
      id: 7,
      userid: 8,
      account_groupid: 9,
      account_no: 'A-7',
      balance: 100,
      equity: 112,
      margin: 30,
      free_margin: 82,
      positions: 1,
      status: 1,
    };
    const request = vi.fn().mockResolvedValue({
      data: { code: 0, msg: '', token: 'session-token', server_time: 1, account },
    });
    exchange.xhr = { request } as any;

    await expect(exchange.validateAccount()).resolves.toEqual({
      error: '',
      balance: { used: 30, free: 82, total: 112, upnl: 12, info: account },
    });
    expect(request).toHaveBeenCalledOnce();
    expect(request.mock.calls[0][0].url).toBe(ENDPOINTS.LOGIN);
  });

  it('serializes concurrent login callers onto one session request', async () => {
    const exchange = createExchange();
    let resolveLogin!: (response: unknown) => void;
    const request = vi.fn().mockReturnValue(
      new Promise((resolve) => {
        resolveLogin = resolve;
      }),
    );
    exchange.xhr = { request } as any;

    const logins = [
      (exchange as any).ensureLoggedIn(),
      (exchange as any).ensureLoggedIn(),
      (exchange as any).ensureLoggedIn(true),
    ];

    expect(request).toHaveBeenCalledOnce();
    resolveLogin({
      data: {
        code: 0,
        msg: '',
        token: 'session-token',
        server_time: 1,
        account: { id: 7, account_no: 'A-7' },
      },
    });
    await Promise.all(logins);
    await (exchange as any).ensureLoggedIn();
    expect(request).toHaveBeenCalledOnce();
    exchange.dispose();
  });
});
