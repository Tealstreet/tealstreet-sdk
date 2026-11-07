import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from '../reference/apexomni/apexomni.exchange';
import { BROKER_ID, ENDPOINTS } from '../reference/apexomni/apexomni.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const { createApiMock, initSdkMock, signContractMock, signer } = vi.hoisted(() => {
  const signer = {
    initZklinkSigner: vi.fn(),
  };
  const signContractMock = vi.fn(async (_signer: unknown, payload: { size: string }) => ({
    tx: { signature: { signature: `signature-${payload.size}` } },
  }));

  return {
    createApiMock: vi.fn(),
    initSdkMock: vi.fn(),
    signContractMock,
    signer,
  };
});

vi.mock('axios-rate-limit', () => ({
  default: (xhr: unknown) => xhr,
}));

vi.mock('zklink-web-sdk-alpha', () => ({
  newRpcSignerWithProvider: vi.fn(() => signer),
}));

vi.mock('../reference/apexomni/zklink/signer/utils', () => ({
  initSdk: initSdkMock,
}));

vi.mock('../reference/apexomni/zklink/signer/transactions/contract', () => ({
  signContract: signContractMock,
}));

vi.mock('../reference/apexomni/apexomni.api', () => ({
  createAPI: (...args: unknown[]) => createApiMock(...args),
}));

vi.mock('../reference/apexomni/apexomni.ws-public', () => ({
  ApexOmniPublicWebsocket: class {
    dispose = vi.fn();
  },
}));

vi.mock('../reference/apexomni/apexomni.ws-private', () => ({
  ApexOmniPrivateWebsocket: class {
    throttleFetchOrders = vi.fn();
    dispose = vi.fn();
  },
}));

const createBracketExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USDT',
        symbol: 'BTC-USDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 1, price: 1 },
        limits: {
          amount: { min: 1, max: 9 },
          leverage: { min: 1, max: 50 },
        },
        isExpiring: false,
        info: {},
      },
    ] as any,
    tickers: [
      {
        id: 'BTC-USDT',
        symbol: 'BTC-USDT',
        bid: 100,
        ask: 100,
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
  store.updateZkLinkConfig({
    id: '1',
    contractAccount: { makerFeeRate: '0.001', takerFeeRate: '0.001' },
    contractConfig: { perpetualContract: [{ symbol: 'BTC-USDT', l2PairId: '1' }] },
  } as any);

  let postCount = 0;
  const post = vi.fn(async (_path: string, _body: string) => ({ data: { data: { id: `order-${++postCount}` } } }));
  createApiMock.mockReturnValue({ post });
  const exchange = new ApexOmniExchange('apexomni' as any, { omniKeySeed: 'seed' } as any, store);
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

  return { exchange, post };
};

describe('Apex Omni order placement regressions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000);
    vi.spyOn(Math, 'random').mockReturnValue(0.1);
  });

  it.each([
    ['take profit', { takeProfit: 110 }],
    ['stop loss', { stopLoss: 90 }],
    ['take profit and stop loss', { takeProfit: 110, stopLoss: 90 }],
  ])('rejects a split order with attached %s before any write', async (_label, brackets) => {
    const { exchange, post } = createBracketExchange();
    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 20,
        price: 100,
        ...brackets,
      }),
    ).rejects.toThrow('ApeX Omni cannot atomically attach TP/SL to split orders');

    expect(emitError).toHaveBeenCalledWith('ApeX Omni cannot atomically attach TP/SL to split orders');
    expect(initSdkMock).not.toHaveBeenCalled();
    expect(signer.initZklinkSigner).not.toHaveBeenCalled();
    expect(signContractMock).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
    expect(exchange.privateWebsocket.throttleFetchOrders).not.toHaveBeenCalled();
  });

  it('keeps the documented one-request bracket path at the exact split boundary', async () => {
    const { exchange, post } = createBracketExchange();

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 9,
        price: 100,
        takeProfit: 110,
        stopLoss: 90,
      }),
    ).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledOnce();
    const body = post.mock.calls[0][1];
    const params = Object.fromEntries(new URLSearchParams(body));
    expect(params).toMatchObject({
      size: '9',
      isOpenTpslOrder: 'true',
      isSetOpenTp: 'true',
      tpSize: '9',
      isSetOpenSl: 'true',
      slSize: '9',
    });
    expect(signContractMock).toHaveBeenCalledTimes(3);
    expect(exchange.privateWebsocket.throttleFetchOrders).toHaveBeenCalledOnce();
  });

  it('emits the canonical numeric broker attribution in the exact order payload', async () => {
    const { exchange, post } = createBracketExchange();

    expect(BROKER_ID).toBe('6587');
    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
      }),
    ).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.CREATE_ORDER,
      'brokerId=6587&clientId=apexomni-1-1700000000000-111111&clientOrderId=apexomni-1-1700000000000-111111&expiration=1703199600000&limitFee=0.100000&price=100&side=BUY&signature=signature-1000000000000000000&size=1&symbol=BTC-USDT&timeInForce=GOOD_TIL_CANCEL&type=LIMIT',
      { headers: { 'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8' } },
    );
  });

  it('signs and sends a caller clientOrderId verbatim as clientId and clientOrderId', async () => {
    const { exchange, post } = createBracketExchange();

    await expect(
      exchange.placeOrderWithClientOrderId({
        symbol: 'BTC-USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
        clientOrderId: 'caller-apex-1',
      }),
    ).resolves.toEqual({ orderIds: ['order-1'], clientOrderId: 'caller-apex-1' });

    const params = Object.fromEntries(new URLSearchParams(post.mock.calls[0][1]));
    expect(params).toMatchObject({ clientId: 'caller-apex-1', clientOrderId: 'caller-apex-1' });
  });
});
