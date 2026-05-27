/**
 * Tests for transaction hashing utilities
 */

import { describe, expect, it } from 'vitest';

import { OrderSide, OrderType, TimeInForce, TxType } from '../types/constants';
import {
  L2CancelOrderTxInfo,
  L2CreateOrderTxInfo,
  L2TransferTxInfo,
  L2WithdrawTxInfo,
} from '../types/transactions';
import {
  hashAuthToken,
  hashCancelOrderTx,
  hashCreateOrderTx,
  hashTransferTx,
  hashWithdrawTx,
} from './transaction-hasher';

describe('Transaction Hasher', () => {
  const chainId = 1;
  const defaultOpts = {
    nonce: 1,
    expiry: 1700000000,
    accountIndex: 0,
    apiKeyIndex: 0,
  };

  describe('hashCreateOrderTx', () => {
    it('should hash a basic limit order correctly', () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x123abc',
        orderType: OrderType.Limit,
        side: OrderSide.Buy,
        price: '1000000000000',
        quantity: '100000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: false,
        postOnly: false,
        opts: defaultOpts,
      };

      const hash = hashCreateOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
      expect(hash.length % 16).toBe(0); // Must be multiple of 16 (8-byte field elements)
    });

    it('should hash order with reduce only flag', () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x123',
        orderType: OrderType.Limit,
        side: OrderSide.Sell,
        price: '2000000000000',
        quantity: '200000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: true,
        postOnly: false,
        opts: defaultOpts,
      };

      const hash = hashCreateOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });

    it('should hash order with post only flag', () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x456',
        orderType: OrderType.Limit,
        side: OrderSide.Buy,
        price: '3000000000000',
        quantity: '300000000',
        timeInForce: TimeInForce.PostOnly,
        reduceOnly: false,
        postOnly: true,
        opts: defaultOpts,
      };

      const hash = hashCreateOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });

    it('should hash order with stop price', () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x789',
        orderType: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: '4000000000000',
        quantity: '400000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: false,
        postOnly: false,
        stopPrice: '3800000000000',
        opts: defaultOpts,
      };

      const hash = hashCreateOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });

    it('should produce different hashes for different orders', () => {
      const tx1: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x111',
        orderType: OrderType.Limit,
        side: OrderSide.Buy,
        price: '1000000000000',
        quantity: '100000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: false,
        postOnly: false,
        opts: defaultOpts,
      };

      const tx2: L2CreateOrderTxInfo = {
        ...tx1,
        price: '2000000000000', // Different price
      };

      const hash1 = hashCreateOrderTx(tx1, chainId);
      const hash2 = hashCreateOrderTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should handle market ID without 0x prefix', () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '123abc',
        orderType: OrderType.Market,
        side: OrderSide.Buy,
        price: '0',
        quantity: '100000000',
        timeInForce: TimeInForce.ImmediateOrCancel,
        reduceOnly: false,
        postOnly: false,
        opts: defaultOpts,
      };

      const hash = hashCreateOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });
  });

  describe('hashCancelOrderTx', () => {
    it('should hash a cancel order transaction correctly', () => {
      const tx: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123abc',
        orderId: '0x456def',
        opts: defaultOpts,
      };

      const hash = hashCancelOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
      expect(hash.length % 16).toBe(0);
    });

    it('should produce different hashes for different order IDs', () => {
      const tx1: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123',
        orderId: '0x111',
        opts: defaultOpts,
      };

      const tx2: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123',
        orderId: '0x222',
        opts: defaultOpts,
      };

      const hash1 = hashCancelOrderTx(tx1, chainId);
      const hash2 = hashCancelOrderTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should handle IDs without 0x prefix', () => {
      const tx: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '123abc',
        orderId: '456def',
        opts: defaultOpts,
      };

      const hash = hashCancelOrderTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });

    it('should produce different hashes for different nonces', () => {
      const tx1: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123',
        orderId: '0x456',
        opts: { ...defaultOpts, nonce: 1 },
      };

      const tx2: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123',
        orderId: '0x456',
        opts: { ...defaultOpts, nonce: 2 },
      };

      const hash1 = hashCancelOrderTx(tx1, chainId);
      const hash2 = hashCancelOrderTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });
  });

  describe('hashTransferTx', () => {
    it('should hash a transfer transaction correctly', () => {
      const tx: L2TransferTxInfo = {
        txType: TxType.L2Transfer,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb',
        amount: '1000000000',
        opts: defaultOpts,
      };

      const hash = hashTransferTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
      expect(hash.length % 16).toBe(0);
    });

    it('should produce different hashes for different amounts', () => {
      const tx1: L2TransferTxInfo = {
        txType: TxType.L2Transfer,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb',
        amount: '1000000000',
        opts: defaultOpts,
      };

      const tx2: L2TransferTxInfo = {
        ...tx1,
        amount: '2000000000',
      };

      const hash1 = hashTransferTx(tx1, chainId);
      const hash2 = hashTransferTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should produce different hashes for different recipients', () => {
      const tx1: L2TransferTxInfo = {
        txType: TxType.L2Transfer,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb',
        amount: '1000000000',
        opts: defaultOpts,
      };

      const tx2: L2TransferTxInfo = {
        ...tx1,
        recipient: '0x1234567890123456789012345678901234567890',
      };

      const hash1 = hashTransferTx(tx1, chainId);
      const hash2 = hashTransferTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should handle addresses without 0x prefix', () => {
      const tx: L2TransferTxInfo = {
        txType: TxType.L2Transfer,
        tokenAddress: 'A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        recipient: '742d35Cc6634C0532925a3b844Bc9e7595f0bEb',
        amount: '1000000000',
        opts: defaultOpts,
      };

      const hash = hashTransferTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });
  });

  describe('hashWithdrawTx', () => {
    it('should hash a withdraw transaction correctly', () => {
      const tx: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: false,
        opts: defaultOpts,
      };

      const hash = hashWithdrawTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
      expect(hash.length % 16).toBe(0);
    });

    it('should hash withdraw with fast withdraw flag', () => {
      const tx: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: true,
        opts: defaultOpts,
      };

      const hash = hashWithdrawTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });

    it('should hash withdraw with L1 recipient', () => {
      const tx: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        l1Recipient: '0x1234567890123456789012345678901234567890',
        fastWithdraw: false,
        opts: defaultOpts,
      };

      const hash = hashWithdrawTx(tx, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });

    it('should produce different hashes with and without fast withdraw', () => {
      const tx1: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: false,
        opts: defaultOpts,
      };

      const tx2: L2WithdrawTxInfo = {
        ...tx1,
        fastWithdraw: true,
      };

      const hash1 = hashWithdrawTx(tx1, chainId);
      const hash2 = hashWithdrawTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should produce different hashes for different amounts', () => {
      const tx1: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: false,
        opts: defaultOpts,
      };

      const tx2: L2WithdrawTxInfo = {
        ...tx1,
        amount: '2000000000',
      };

      const hash1 = hashWithdrawTx(tx1, chainId);
      const hash2 = hashWithdrawTx(tx2, chainId);

      expect(hash1).not.toBe(hash2);
    });
  });

  describe('hashAuthToken', () => {
    it('should hash authentication token correctly', () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);

      const hash = hashAuthToken(accountIndex, apiKeyIndex, deadline, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
      expect(hash.length % 16).toBe(0);
    });

    it('should produce different hashes for different deadlines', () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline1 = BigInt(1700000000);
      const deadline2 = BigInt(1800000000);

      const hash1 = hashAuthToken(accountIndex, apiKeyIndex, deadline1, chainId);
      const hash2 = hashAuthToken(accountIndex, apiKeyIndex, deadline2, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should produce different hashes for different account indices', () => {
      const deadline = BigInt(1700000000);

      const hash1 = hashAuthToken(0, 0, deadline, chainId);
      const hash2 = hashAuthToken(1, 0, deadline, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should produce different hashes for different API key indices', () => {
      const deadline = BigInt(1700000000);

      const hash1 = hashAuthToken(0, 0, deadline, chainId);
      const hash2 = hashAuthToken(0, 1, deadline, chainId);

      expect(hash1).not.toBe(hash2);
    });

    it('should produce different hashes for different chain IDs', () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline = BigInt(1700000000);

      const hash1 = hashAuthToken(accountIndex, apiKeyIndex, deadline, 1);
      const hash2 = hashAuthToken(accountIndex, apiKeyIndex, deadline, 2);

      expect(hash1).not.toBe(hash2);
    });

    it('should handle large deadline values', () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline = BigInt(2 ** 32) - 1n; // Max uint32

      const hash = hashAuthToken(accountIndex, apiKeyIndex, deadline, chainId);

      expect(hash).toBeDefined();
      expect(typeof hash).toBe('string');
    });
  });

  describe('cross-transaction consistency', () => {
    it('should produce consistent hash length across all transaction types', () => {
      const createOrderTx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x123',
        orderType: OrderType.Limit,
        side: OrderSide.Buy,
        price: '1000000000000',
        quantity: '100000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: false,
        postOnly: false,
        opts: defaultOpts,
      };

      const cancelOrderTx: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123',
        orderId: '0x456',
        opts: defaultOpts,
      };

      const transferTx: L2TransferTxInfo = {
        txType: TxType.L2Transfer,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb',
        amount: '1000000000',
        opts: defaultOpts,
      };

      const withdrawTx: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: false,
        opts: defaultOpts,
      };

      const hash1 = hashCreateOrderTx(createOrderTx, chainId);
      const hash2 = hashCancelOrderTx(cancelOrderTx, chainId);
      const hash3 = hashTransferTx(transferTx, chainId);
      const hash4 = hashWithdrawTx(withdrawTx, chainId);

      // All hashes should be multiples of 16 (8-byte field elements)
      expect(hash1.length % 16).toBe(0);
      expect(hash2.length % 16).toBe(0);
      expect(hash3.length % 16).toBe(0);
      expect(hash4.length % 16).toBe(0);
    });

    it('should produce consistent hashes for same transaction data', () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x123',
        orderType: OrderType.Limit,
        side: OrderSide.Buy,
        price: '1000000000000',
        quantity: '100000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: false,
        postOnly: false,
        opts: defaultOpts,
      };

      const hash1 = hashCreateOrderTx(tx, chainId);
      const hash2 = hashCreateOrderTx(tx, chainId);

      expect(hash1).toBe(hash2);
    });
  });
});
