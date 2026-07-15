import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from '../../exchanges/apexomni/apexomni.exchange';
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

vi.mock('../../exchanges/apexomni/zklink/signer/utils', () => ({
  initSdk: initSdkMock,
}));

vi.mock('../../exchanges/apexomni/zklink/signer/transactions/contract', () => ({
  signContract: signContractMock,
}));

vi.mock('../../exchanges/apexomni/apexomni.api', () => ({
  createAPI: (...args: unknown[]) => createApiMock(...args),
}));

vi.mock('../../exchanges/apexomni/apexomni.ws-public', () => ({
  ApexOmniPublicWebsocket: class {
    dispose = vi.fn();
  },
}));

vi.mock('../../exchanges/apexomni/apexomni.ws-private', () => ({
  ApexOmniPrivateWebsocket: class {
    throttleFetchOrders = vi.fn();
    dispose = vi.fn();
  },
}));

describe('Apex Omni split-order TP/SL regression', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000);
    vi.spyOn(Math, 'random').mockReturnValue(0.1);
  });

  it('rejects a split order with attached TP/SL before any write', async () => {
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

    const emitError = vi.spyOn(exchange, 'emitAndLogError').mockReturnValue(undefined as any);

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 20,
        price: 100,
        takeProfit: 110,
        stopLoss: 90,
      }),
    ).rejects.toThrow('ApeX Omni does not support attached TP/SL on split orders');

    expect(emitError).toHaveBeenCalledWith('ApeX Omni does not support attached TP/SL on split orders');
    expect(initSdkMock).not.toHaveBeenCalled();
    expect(signer.initZklinkSigner).not.toHaveBeenCalled();
    expect(signContractMock).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
    expect(exchange.privateWebsocket.throttleFetchOrders).not.toHaveBeenCalled();
  });
});
