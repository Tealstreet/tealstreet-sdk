import type { ExchangeOptions } from '../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createLighterSignerAdapter } from './reference/lighter/lighter.signer';

const getCreateOrderTransactionMock = vi.fn();
const getCreateOrderTransactionsMock = vi.fn();
const getCreateGroupedOrdersTransactionMock = vi.fn();
const getCancelOrderTransactionMock = vi.fn();
const getUpdateLeverageTransactionMock = vi.fn();
const getApproveIntegratorTransactionMock = vi.fn();
const getAuthTokenMock = vi.fn();
const sendRawTxMock = vi.fn();
const transactionClientConstructorMock = vi.fn();

vi.mock('@tealstreet/poseidon-crypto', () => ({
  isWasmReady: vi.fn(() => true),
  initWasm: vi.fn(async () => undefined),
  getCryptoApi: vi.fn(() => ({})),
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  TransactionClient: class {
    constructor(params: unknown) {
      transactionClientConstructorMock(params);
    }

    getCreateOrderTransaction = getCreateOrderTransactionMock;
    getCreateOrderTransactions = getCreateOrderTransactionsMock;
    getCreateGroupedOrdersTransaction = getCreateGroupedOrdersTransactionMock;
    getCancelOrderTransaction = getCancelOrderTransactionMock;
    getUpdateLeverageTransaction = getUpdateLeverageTransactionMock;
    getApproveIntegratorTransaction = getApproveIntegratorTransactionMock;
    getAuthToken = getAuthTokenMock;
    resetNonce = vi.fn();
  },
  LighterApiClient: class {},
}));

describe('createLighterSignerAdapter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getCreateOrderTransactionMock.mockResolvedValue({ type: 'create_order' });
    getCreateOrderTransactionsMock.mockResolvedValue([{ type: 'create_order' }, { type: 'create_order' }]);
    getCreateGroupedOrdersTransactionMock.mockResolvedValue({ type: 'create_grouped_orders' });
    getCancelOrderTransactionMock.mockResolvedValue({ type: 'cancel_order' });
    getUpdateLeverageTransactionMock.mockResolvedValue({ type: 'update_leverage' });
    getApproveIntegratorTransactionMock.mockResolvedValue({ type: 'approve_integrator' });
    getAuthTokenMock.mockResolvedValue('auth-token');
    sendRawTxMock.mockResolvedValue('tx-hash');
    transactionClientConstructorMock.mockClear();
  });

  it('signs and submits create/cancel/leverage transactions through the internal provider', async () => {
    const options = {
      key: '',
      secret: '0xabc',
      apiKeyIndex: '0',
      testnet: false,
      extra: {
        lighterSignerProvider: 'tealstreet-sdk',
      },
    } as unknown as ExchangeOptions;

    const signer = await createLighterSignerAdapter({
      exchangeName: 'lighter',
      options,
      apiClient: { sendRawTx: sendRawTxMock } as any,
      accountIndex: 123n,
      apiKeyIndex: 0,
    });

    const createHash = await signer.createOrder({
      orderId: 11n,
      marketId: 1,
      isBuy: true,
      price: 1000n,
      amount: 200n,
      postOnly: false,
      reduceOnly: false,
      orderType: 'limit',
      timeInForce: 'GoodTillCancel',
      integratorAccountIndex: 1234n,
      integratorMakerFee: 300,
      integratorTakerFee: 300,
    });
    const cancelHash = await signer.cancelOrder({ orderId: 11n, marketId: 1 });
    await signer.updateLeverage({
      marketId: 1,
      leverage: 5,
      marginMode: 0,
    });
    const approveHash = await signer.approveIntegrator({
      integratorAccountIndex: 158385n,
      maxPerpsTakerFee: 300,
      maxPerpsMakerFee: 300,
      maxSpotTakerFee: 300,
      maxSpotMakerFee: 300,
      approvalExpiry: 1777585206059n,
    });
    const token = await signer.getAuthToken(60);

    expect(createHash).toBe('tx-hash');
    expect(cancelHash).toBe('tx-hash');
    expect(approveHash).toBe('tx-hash');
    expect(token).toBe('auth-token');
    expect(sendRawTxMock).toHaveBeenCalledTimes(4);
    expect(getCreateOrderTransactionMock).toHaveBeenCalledTimes(1);
    expect(getCreateOrderTransactionMock).toHaveBeenCalledWith({
      orderId: 11n,
      marketId: 1,
      isBuy: true,
      price: 1000n,
      amount: 200n,
      postOnly: false,
      reduceOnly: false,
      orderType: 'limit',
      timeInForce: 'GoodTillCancel',
      integratorAccountIndex: 1234n,
      integratorMakerFee: 300,
      integratorTakerFee: 300,
    });
    expect(getCancelOrderTransactionMock).toHaveBeenCalledTimes(1);
    expect(getUpdateLeverageTransactionMock).toHaveBeenCalledWith({
      marketId: 1,
      leverage: 5,
      marginMode: 0,
    });
    expect(getApproveIntegratorTransactionMock).toHaveBeenCalledWith({
      integratorAccountIndex: 158385n,
      maxPerpsTakerFee: 300,
      maxPerpsMakerFee: 300,
      maxSpotTakerFee: 300,
      maxSpotMakerFee: 300,
      approvalExpiry: 1777585206059n,
    });
    expect(transactionClientConstructorMock).toHaveBeenCalledWith(
      expect.objectContaining({
        chainId: 304,
      }),
    );
  });

  it('uses the Robinhood L2 signer chain id for the Robinhood exchange variant', async () => {
    const options = {
      key: '',
      secret: '0xabc',
      apiKeyIndex: '0',
      testnet: false,
    } as unknown as ExchangeOptions;

    await createLighterSignerAdapter({
      exchangeName: 'lighter-robinhood',
      options,
      apiClient: { sendRawTx: sendRawTxMock } as any,
      accountIndex: 123n,
      apiKeyIndex: 0,
    });

    expect(transactionClientConstructorMock).toHaveBeenCalledWith(
      expect.objectContaining({
        chainId: 466324,
      }),
    );
  });

  it('signs and submits create-order batches through sendTxBatch', async () => {
    const options = {
      key: '',
      secret: '0xabc',
      apiKeyIndex: '0',
      testnet: false,
    } as unknown as ExchangeOptions;
    const sendRawTxBatchMock = vi.fn().mockResolvedValue({ txHashes: ['tx-1', 'tx-2'] });

    const signer = await createLighterSignerAdapter({
      exchangeName: 'lighter',
      options,
      apiClient: {
        sendRawTx: sendRawTxMock,
        sendRawTxBatch: sendRawTxBatchMock,
      } as any,
      accountIndex: 123n,
      apiKeyIndex: 0,
    });

    const requests = [
      {
        orderId: 21n,
        marketId: 1,
        isBuy: true,
        price: 1000n,
        amount: 200n,
        postOnly: false,
        reduceOnly: false,
        orderType: 'limit' as const,
      },
      {
        orderId: 22n,
        marketId: 1,
        isBuy: true,
        price: 990n,
        amount: 200n,
        postOnly: false,
        reduceOnly: false,
        orderType: 'limit' as const,
      },
    ];

    await expect(signer.createOrders(requests)).resolves.toEqual(['tx-1', 'tx-2']);
    expect(getCreateOrderTransactionsMock).toHaveBeenCalledWith(requests);
    expect(sendRawTxBatchMock).toHaveBeenCalledWith([{ type: 'create_order' }, { type: 'create_order' }]);
  });

  it('signs and submits grouped orders through sendTx', async () => {
    const options = {
      key: '',
      secret: '0xabc',
      apiKeyIndex: '0',
      testnet: false,
    } as unknown as ExchangeOptions;

    const signer = await createLighterSignerAdapter({
      exchangeName: 'lighter',
      options,
      apiClient: { sendRawTx: sendRawTxMock } as any,
      accountIndex: 123n,
      apiKeyIndex: 0,
    });

    const request = {
      groupingType: 2 as const,
      orders: [
        {
          orderId: 31n,
          marketId: 1,
          isBuy: false,
          price: 900n,
          amount: 200n,
          postOnly: false,
          reduceOnly: true,
          orderType: 'stop_loss' as const,
        },
        {
          orderId: 32n,
          marketId: 1,
          isBuy: false,
          price: 1100n,
          amount: 200n,
          postOnly: false,
          reduceOnly: true,
          orderType: 'take_profit' as const,
        },
      ],
      integratorAccountIndex: 158385n,
      integratorMakerFee: 300,
      integratorTakerFee: 300,
    };

    await expect(signer.createGroupedOrders(request)).resolves.toBe('tx-hash');
    expect(getCreateGroupedOrdersTransactionMock).toHaveBeenCalledWith(request);
    expect(sendRawTxMock).toHaveBeenCalledWith({ type: 'create_grouped_orders' });
  });

  it('rejects unsupported legacy signer provider configuration', async () => {
    const options = {
      secret: '0xabc',
      apiKeyIndex: '0',
      testnet: false,
      extra: {
        lighterSignerProvider: 'legacy-sdk',
      },
    } as unknown as ExchangeOptions;

    await expect(
      createLighterSignerAdapter({
        exchangeName: 'lighter',
        options,
        apiClient: { sendRawTx: sendRawTxMock } as any,
        accountIndex: 123n,
        apiKeyIndex: 0,
      }),
    ).rejects.toThrow('Unsupported Lighter signer provider');
  });
});
