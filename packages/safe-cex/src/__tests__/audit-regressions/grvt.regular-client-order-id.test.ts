import { beforeEach, describe, expect, it, vi } from 'vitest';

import { GrvtExchange } from '../../exchanges/grvt/grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const { grvtDecodeMock, grvtSignMock } = vi.hoisted(() => ({
  grvtDecodeMock: vi.fn(() => ({ r: '1'.repeat(64), s: '2'.repeat(64), v: 27 })),
  grvtSignMock: vi.fn(() => 'raw-signature'),
}));

vi.mock('@tealstreet/grvt-ts-sdk', () => ({
  domain: { name: 'Grvt', version: '0' },
  EGrvtEnvironment: { PRODUCTION: 'production', TESTNET: 'testnet' },
  GrvtClient: class {},
  OrderWithBuilderFee: { primaryType: 'OrderWithBuilderFee', types: {} },
  Signer: { decode: grvtDecodeMock, sign: grvtSignMock },
}));

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC_USDT_Perp',
        symbol: 'BTC_USDT_PERP',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 } },
        info: { ih: '0x030501', bd: 3, qd: 6 },
      } as any,
    ],
  });
  const exchange = new GrvtExchange('grvt' as any, { key: 'key', secret: `0x${'1'.repeat(64)}` } as any, store);
  exchange.grvtTradingAccountId = '7';
  (exchange as any).getBuilderInfo = vi.fn(async () => ({
    builder: '0x79a52a6658c23cf7e7fe4fe43ff91c5195eb86ca',
    builderFee: '0.03',
    builderFeeInt: 300,
  }));
  const createOrder = vi.fn(async () => ({ r: { oi: '0x00' } }));
  exchange.grvtClient = {
    authenticatedEndpoint: vi.fn(async () => 'authenticated-endpoint'),
    tdgClient: { createOrder },
  } as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { createOrder, exchange };
};

describe('GRVT regular order clientOrderId', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000);
    vi.spyOn(Math, 'random').mockReturnValue(0.123);
  });

  it('uses a caller-provided client_order_id for regular orders', async () => {
    const { createOrder, exchange } = createExchange();

    await expect(
      exchange.placeOrder({
        symbol: 'BTC_USDT_PERP',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 50_000,
        clientOrderId: 'caller-client-id',
      }),
    ).resolves.toEqual(['caller-client-id']);

    const request = createOrder.mock.calls[0][0];
    expect(request.order.metadata.client_order_id).toBe('caller-client-id');
    expect(exchange._nonceCreationTimes.has('caller-client-id')).toBe(true);
  });

  it('preserves generated client_order_id values for attached TP/SL paths', async () => {
    const { createOrder, exchange } = createExchange();

    await expect(
      exchange.placeOrder({
        symbol: 'BTC_USDT_PERP',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 50_000,
        takeProfit: 55_000,
        clientOrderId: 'ignored-client-id',
      }),
    ).resolves.toEqual(['123000000']);

    const request = createOrder.mock.calls[0][0];
    expect(request.order.metadata.client_order_id).toBe('123000000');
    expect(request.order.metadata.trigger).toBeDefined();
  });

  it('rejects an empty caller clientOrderId on regular orders', async () => {
    const { createOrder, exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder({
        symbol: 'BTC_USDT_PERP',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 50_000,
        clientOrderId: '',
      }),
    ).rejects.toThrow('GRVT clientOrderId must be a non-empty string');

    expect(emitAndLogError).toHaveBeenCalledWith('GRVT clientOrderId must be a non-empty string');
    expect(createOrder).not.toHaveBeenCalled();
  });
});
