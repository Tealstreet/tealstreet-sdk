import { describe, expect, it, vi } from 'vitest';
import { TransactionClient } from './transaction-client';

const createTransactionClient = () => {
  const httpClient = {
    getNextNonce: vi.fn().mockResolvedValue(1n),
  } as any;

  const crypto = {
    isReady: vi.fn().mockReturnValue(true),
    poseidon2Hash: vi.fn().mockReturnValue('0xhash'),
    poseidon2HashFp5: vi.fn().mockReturnValue('0xfp5hash'),
    poseidon2HashTwoToOne: vi.fn().mockReturnValue('0xcombinedhash'),
    schnorrSign: vi.fn().mockReturnValue('0xsig'),
  } as any;

  const client = new TransactionClient({
    httpClient,
    crypto,
    chainId: 304,
    privateKey: '1'.repeat(64),
    accountIndex: 1n,
    apiKeyIndex: 0,
  });

  return { client, crypto };
};

describe('TransactionClient order expiry behavior', () => {
  it('uses non-zero expiry for IOC stop-loss orders', async () => {
    const { client } = createTransactionClient();

    const tx = await client.getCreateOrderTransaction({
      orderId: 1n,
      marketId: 1,
      isBuy: false,
      price: 100n,
      amount: 10n,
      orderType: 'stop_loss',
      timeInForce: 'ImmediateOrCancel',
      triggerPrice: 99n,
      reduceOnly: true,
    });

    expect(tx.orderDetails.orderTypeValue).toBe(2);
    expect(tx.orderDetails.timeInForceValue).toBe(0);
    expect(tx.orderDetails.orderExpiry).toBeDefined();
    expect((tx.orderDetails.orderExpiry ?? 0n) > 0n).toBe(true);
  });

  it('keeps zero expiry for IOC market orders', async () => {
    const { client } = createTransactionClient();

    const tx = await client.getCreateOrderTransaction({
      orderId: 2n,
      marketId: 1,
      isBuy: true,
      price: 100n,
      amount: 10n,
      orderType: 'market',
      timeInForce: 'ImmediateOrCancel',
    });

    expect(tx.orderDetails.orderTypeValue).toBe(1);
    expect(tx.orderDetails.timeInForceValue).toBe(0);
    expect(tx.orderDetails.orderExpiry).toBe(0n);
  });

  it('propagates integrator fields when provided', async () => {
    const { client } = createTransactionClient();

    const tx = await client.getCreateOrderTransaction({
      orderId: 3n,
      marketId: 1,
      isBuy: true,
      price: 100n,
      amount: 10n,
      orderType: 'limit',
      timeInForce: 'GoodTillCancel',
      integratorAccountIndex: 1234n,
      integratorMakerFee: 300,
      integratorTakerFee: 300,
    });

    expect(tx.orderDetails.integratorAccountIndex).toBe(1234n);
    expect(tx.orderDetails.integratorMakerFee).toBe(300);
    expect(tx.orderDetails.integratorTakerFee).toBe(300);
  });

  it('signs create-order with aggregated L2 attributes preimage when integrator fields are provided', async () => {
    const { client, crypto } = createTransactionClient();
    crypto.poseidon2HashFp5
      .mockReturnValueOnce('0xbasehash')
      .mockReturnValueOnce('0xattrhash')
      .mockReturnValueOnce('0xaggregatedhash');

    await client.getCreateOrderTransaction({
      orderId: 33n,
      marketId: 1,
      isBuy: true,
      price: 100n,
      amount: 10n,
      orderType: 'limit',
      timeInForce: 'GoodTillCancel',
      integratorAccountIndex: 1234n,
      integratorMakerFee: 300,
      integratorTakerFee: 300,
    });

    // First two fp5 hashes are base tx + attributes, third is aggregated hash.
    expect(crypto.poseidon2HashFp5).toHaveBeenNthCalledWith(3, 'basehashattrhash');
    expect(crypto.schnorrSign).toHaveBeenCalledWith('basehashattrhash', expect.any(String));
  });

  it('uses standard signing path when integrator fields are omitted', async () => {
    const { client, crypto } = createTransactionClient();

    await client.getCreateOrderTransaction({
      orderId: 34n,
      marketId: 1,
      isBuy: false,
      price: 100n,
      amount: 10n,
      orderType: 'limit',
      timeInForce: 'GoodTillCancel',
    });

    expect(crypto.poseidon2HashFp5).not.toHaveBeenCalled();
    expect(crypto.poseidon2Hash).toHaveBeenCalledTimes(1);
    expect(crypto.schnorrSign).toHaveBeenCalledWith(expect.any(String), expect.any(String));
  });

  it('creates approve integrator transaction with expected fields', async () => {
    const { client } = createTransactionClient();

    const tx = await client.getApproveIntegratorTransaction({
      integratorAccountIndex: 158385n,
      maxPerpsTakerFee: 300,
      maxPerpsMakerFee: 300,
      maxSpotTakerFee: 300,
      maxSpotMakerFee: 300,
      approvalExpiry: 1777585206059n,
      l1Signature: '0xabc123',
    });

    expect(tx.type).toBe('approve_integrator');
    expect(tx.approveIntegratorDetails.integratorAccountIndex).toBe(158385n);
    expect(tx.approveIntegratorDetails.maxPerpsTakerFee).toBe(300);
    expect(tx.approveIntegratorDetails.maxPerpsMakerFee).toBe(300);
    expect(tx.approveIntegratorDetails.maxSpotTakerFee).toBe(300);
    expect(tx.approveIntegratorDetails.maxSpotMakerFee).toBe(300);
    expect(tx.approveIntegratorDetails.approvalExpiry).toBe(1777585206059n);
    expect(tx.approveIntegratorDetails.l1Signature).toBe('0xabc123');
  });

  it('creates multiple create-order transactions with increasing cached nonces', async () => {
    const { client } = createTransactionClient();

    const txs = await client.getCreateOrderTransactions([
      {
        orderId: 41n,
        marketId: 1,
        isBuy: true,
        price: 100n,
        amount: 10n,
        orderType: 'limit',
      },
      {
        orderId: 42n,
        marketId: 1,
        isBuy: true,
        price: 99n,
        amount: 11n,
        orderType: 'limit',
      },
    ]);

    expect(txs.map((tx) => tx.nonce)).toEqual([1n, 2n]);
    expect(txs.map((tx) => tx.orderDetails.orderId)).toEqual([41n, 42n]);
  });

  it('creates OCO grouped-order transactions with integrator attributes', async () => {
    const { client, crypto } = createTransactionClient();
    crypto.poseidon2Hash.mockReturnValueOnce('0xorderhash1').mockReturnValueOnce('0xorderhash2');
    crypto.poseidon2HashTwoToOne.mockReturnValueOnce('0xcombinedhash');
    crypto.poseidon2HashFp5
      .mockReturnValueOnce('0xbasehash')
      .mockReturnValueOnce('0xattrhash')
      .mockReturnValueOnce('0xaggregatedhash');

    const tx = await client.getCreateGroupedOrdersTransaction({
      groupingType: 2,
      integratorAccountIndex: 1234n,
      integratorMakerFee: 300,
      integratorTakerFee: 300,
      orders: [
        {
          orderId: 51n,
          marketId: 1,
          isBuy: false,
          price: 90n,
          amount: 10n,
          orderType: 'stop_loss',
          timeInForce: 'ImmediateOrCancel',
          triggerPrice: 90n,
          reduceOnly: true,
        },
        {
          orderId: 52n,
          marketId: 1,
          isBuy: false,
          price: 110n,
          amount: 10n,
          orderType: 'take_profit',
          timeInForce: 'ImmediateOrCancel',
          triggerPrice: 110n,
          reduceOnly: true,
        },
      ],
    });

    expect(tx.type).toBe('create_grouped_orders');
    expect(tx.groupedOrderDetails.groupingType).toBe(2);
    expect(tx.groupedOrderDetails.orders).toHaveLength(2);
    expect(tx.groupedOrderDetails.orders[0]).toEqual(
      expect.objectContaining({
        orderId: 51n,
        orderTypeValue: 2,
      }),
    );
    expect(tx.groupedOrderDetails.orders[1]).toEqual(
      expect.objectContaining({
        orderId: 52n,
        orderTypeValue: 4,
      }),
    );
    expect(tx.groupedOrderDetails.integratorAccountIndex).toBe(1234n);
    expect(crypto.poseidon2HashTwoToOne).toHaveBeenCalledWith('0xorderhash1', '0xorderhash2');
    expect(crypto.schnorrSign).toHaveBeenCalledWith('basehashattrhash', expect.any(String));
  });

  it('rejects grouped orders that do not match OCO constraints', async () => {
    const { client } = createTransactionClient();

    await expect(
      client.getCreateGroupedOrdersTransaction({
        groupingType: 2,
        orders: [
          {
            orderId: 61n,
            marketId: 1,
            isBuy: false,
            price: 90n,
            amount: 10n,
            orderType: 'stop_loss',
            timeInForce: 'ImmediateOrCancel',
            triggerPrice: 90n,
            reduceOnly: true,
          },
          {
            orderId: 62n,
            marketId: 2,
            isBuy: false,
            price: 110n,
            amount: 10n,
            orderType: 'take_profit',
            timeInForce: 'ImmediateOrCancel',
            triggerPrice: 110n,
            reduceOnly: true,
          },
        ],
      }),
    ).rejects.toThrow('same market id');
  });

  it('rejects invalid revocation semantics for approve integrator', async () => {
    const { client } = createTransactionClient();

    await expect(
      client.getApproveIntegratorTransaction({
        integratorAccountIndex: 158385n,
        maxPerpsTakerFee: 0,
        maxPerpsMakerFee: 0,
        maxSpotTakerFee: 0,
        maxSpotMakerFee: 0,
        approvalExpiry: 1n,
      }),
    ).rejects.toThrow('approvalExpiry must be 0 iff all max fee fields are 0');
  });
});
