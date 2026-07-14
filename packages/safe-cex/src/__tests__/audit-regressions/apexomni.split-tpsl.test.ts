import { beforeEach, describe, expect, it, vi } from 'vitest';

import qs from 'qs';

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

  it('attaches one lot-sized, signed bracket to a split order', async () => {
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
    ).resolves.toEqual(['order-1', 'order-2', 'order-3', 'order-4']);

    const [firstRequest, ...remainingRequests] = post.mock.calls.map(([, body]) => qs.parse(body));
    const signedSizes = signContractMock.mock.calls.map(([, payload]) => payload.size);

    expect(initSdkMock).toHaveBeenCalledOnce();
    expect(signer.initZklinkSigner).toHaveBeenCalledWith('seed');
    expect(post).toHaveBeenCalledTimes(4);
    expect(signedSizes).toEqual([
      '6000000000000000000',
      '6000000000000000000',
      '6000000000000000000',
      '6000000000000000000',
      '6000000000000000000',
      '2000000000000000000',
    ]);

    expect(firstRequest).toMatchObject({
      size: '6',
      isOpenTpslOrder: 'true',
      isSetOpenTp: 'true',
      tpSize: '6',
      tpSignature: 'signature-6000000000000000000',
      isSetOpenSl: 'true',
      slSize: '6',
      slSignature: 'signature-6000000000000000000',
    });
    expect(remainingRequests.map((request) => request.size)).toEqual(['6', '6', '2']);
    for (const request of remainingRequests) {
      expect(request).not.toHaveProperty('isOpenTpslOrder');
      expect(request).not.toHaveProperty('isSetOpenTp');
      expect(request).not.toHaveProperty('tpSize');
      expect(request).not.toHaveProperty('tpSignature');
      expect(request).not.toHaveProperty('isSetOpenSl');
      expect(request).not.toHaveProperty('slSize');
      expect(request).not.toHaveProperty('slSignature');
    }
    expect(exchange.privateWebsocket.throttleFetchOrders).toHaveBeenCalledOnce();
  });
});
