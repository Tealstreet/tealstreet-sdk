import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LighterApiClient } from './api-client';

const { axiosPostMock } = vi.hoisted(() => ({
  axiosPostMock: vi.fn(),
}));

vi.mock('axios', () => ({
  default: {
    create: vi.fn(() => ({
      get: vi.fn(),
      post: axiosPostMock,
      put: vi.fn(),
      delete: vi.fn(),
      interceptors: {
        request: { use: vi.fn() },
        response: { use: vi.fn() },
      },
    })),
  },
}));

describe('LighterApiClient sendRawTxBatch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    axiosPostMock.mockResolvedValue({
      data: {
        code: 200,
        tx_hash: ['tx-1', 'tx-2'],
        predicted_execution_time_ms: 12,
        volume_quota_remaining: '10780',
      },
    });
  });

  it('serializes signed transactions for sendTxBatch', async () => {
    const client = new LighterApiClient({
      baseUrl: 'https://lighter.example',
    });

    const response = await client.sendRawTxBatch([
      {
        type: 'create_order',
        data: '0x01',
        signature: `0x${'11'.repeat(64)}`,
        accountIndex: 123n,
        apiKeyIndex: 1,
        nonce: 10n,
        expiredAt: 1000n,
        orderDetails: {
          orderId: 55n,
          marketId: 1,
          isBuy: true,
          price: 1000n,
          amount: 100n,
          reduceOnly: false,
          orderTypeValue: 0,
          timeInForceValue: 1,
          orderExpiry: 2000n,
          integratorAccountIndex: 158385n,
          integratorMakerFee: 300,
          integratorTakerFee: 300,
        },
      },
      {
        type: 'create_order',
        data: '0x02',
        signature: `0x${'22'.repeat(64)}`,
        accountIndex: 123n,
        apiKeyIndex: 1,
        nonce: 11n,
        expiredAt: 1000n,
        orderDetails: {
          orderId: 56n,
          marketId: 1,
          isBuy: true,
          price: 990n,
          amount: 200n,
          reduceOnly: false,
          orderTypeValue: 0,
          timeInForceValue: 2,
          orderExpiry: 2000n,
        },
      },
    ]);

    expect(response).toEqual({
      txHashes: ['tx-1', 'tx-2'],
      predictedExecutionTimeMs: 12,
      volumeQuotaRemaining: '10780',
    });
    expect(axiosPostMock).toHaveBeenCalledTimes(1);
    const [path, body, config] = axiosPostMock.mock.calls[0];
    expect(path).toBe('/api/v1/sendTxBatch');
    expect(config.headers['Content-Type']).toBe('application/x-www-form-urlencoded');

    const params = new URLSearchParams(body);
    expect(JSON.parse(params.get('tx_types') || '')).toEqual([14, 14]);
    const txInfos = JSON.parse(params.get('tx_infos') || '');
    expect(txInfos).toHaveLength(2);
    const firstTxInfo = JSON.parse(txInfos[0]);
    expect(firstTxInfo).toEqual(
      expect.objectContaining({
        AccountIndex: 123,
        ApiKeyIndex: 1,
        MarketIndex: 1,
        ClientOrderIndex: 55,
        BaseAmount: 100,
        Price: 1000,
        IsAsk: 0,
        Type: 0,
        TimeInForce: 1,
        ReduceOnly: 0,
        TriggerPrice: 0,
        OrderExpiry: 2000,
        ExpiredAt: 1000,
        Nonce: 10,
        L2TxAttributes: {
          1: 158385,
          2: 300,
          3: 300,
        },
      }),
    );
  });
});
