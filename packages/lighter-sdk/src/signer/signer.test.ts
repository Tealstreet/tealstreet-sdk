/**
 * Tests for transaction signer
 */

import { describe, expect, it, beforeAll } from 'vitest';

import { OrderSide, OrderType, TimeInForce, TxType } from '../types/constants';
import {
  L2CancelOrderTxInfo,
  L2CreateOrderTxInfo,
  L2TransferTxInfo,
  L2WithdrawTxInfo,
} from '../types/transactions';
import { generateKeyPair } from './key-manager';
import { createSigner, SignerError, TransactionSigner } from './signer';

describe('Transaction Signer', () => {
  let signer: TransactionSigner;
  let privateKey: string;
  let publicKey: string;
  const chainId = 1;

  const defaultOpts = {
    nonce: 1,
    expiry: 1700000000,
    accountIndex: 0,
    apiKeyIndex: 0,
  };

  beforeAll(async () => {
    signer = new TransactionSigner();
    await signer.initialize();

    const keys = await generateKeyPair();
    privateKey = keys.privateKey;
    publicKey = keys.publicKey;
  });

  describe('initialization', () => {
    it('should initialize successfully', async () => {
      const newSigner = new TransactionSigner();
      await expect(newSigner.initialize()).resolves.toBeUndefined();
    });

    it('should be safe to initialize multiple times', async () => {
      const newSigner = new TransactionSigner();
      await newSigner.initialize();
      await newSigner.initialize();
      await newSigner.initialize();
    });

    it('should throw error when signing without initialization', async () => {
      const uninitializedSigner = new TransactionSigner();
      const message = '0100000000000000';

      await expect(uninitializedSigner.signMessage(message, privateKey)).rejects.toThrow(
        SignerError,
      );
    });
  });

  describe('createSigner convenience function', () => {
    it('should create and initialize a signer', async () => {
      const newSigner = await createSigner();
      const message = '0100000000000000';
      const result = await newSigner.signMessage(message, privateKey);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
    });
  });

  describe('signMessage', () => {
    it('should sign a message correctly', async () => {
      const message = '0100000000000000';
      const result = await signer.signMessage(message, privateKey);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
      expect(result.txHash.length).toBe(64); // 32 bytes = 64 hex chars
      expect(result.signature.length).toBe(160); // 80 bytes = 160 hex chars
    });

    it('should handle message with 0x prefix', async () => {
      const message = '0x0100000000000000';
      const result = await signer.signMessage(message, privateKey);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
    });

    it('should throw error for message with invalid length', async () => {
      const invalidMessage = '01'; // Not a multiple of 16

      await expect(signer.signMessage(invalidMessage, privateKey)).rejects.toThrow(SignerError);
    });

    it('should produce consistent signatures for same message', async () => {
      const message = '0100000000000000';

      const result1 = await signer.signMessage(message, privateKey);
      const result2 = await signer.signMessage(message, privateKey);

      expect(result1.txHash).toBe(result2.txHash);
      // Note: Schnorr signatures may include randomness, so signatures might differ
      // but hashes should be identical
    });

    it('should produce different hashes for different messages', async () => {
      const message1 = '0100000000000000';
      const message2 = '0200000000000000';

      const result1 = await signer.signMessage(message1, privateKey);
      const result2 = await signer.signMessage(message2, privateKey);

      expect(result1.txHash).not.toBe(result2.txHash);
    });
  });

  describe('signCreateOrder', () => {
    it('should sign a create order transaction', async () => {
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

      const result = await signer.signCreateOrder(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
      expect(result.txHash.length).toBe(64);
      expect(result.signature.length).toBe(160);
    });

    it('should sign market order', async () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x456def',
        orderType: OrderType.Market,
        side: OrderSide.Sell,
        price: '0',
        quantity: '200000000',
        timeInForce: TimeInForce.ImmediateOrCancel,
        reduceOnly: false,
        postOnly: false,
        opts: defaultOpts,
      };

      const result = await signer.signCreateOrder(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
    });

    it('should sign order with stop price', async () => {
      const tx: L2CreateOrderTxInfo = {
        txType: TxType.L2CreateOrder,
        marketId: '0x789',
        orderType: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: '4000000000000',
        quantity: '300000000',
        timeInForce: TimeInForce.GoodTillTime,
        reduceOnly: false,
        postOnly: false,
        stopPrice: '3800000000000',
        opts: defaultOpts,
      };

      const result = await signer.signCreateOrder(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
    });

    it('should produce different signatures for different orders', async () => {
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
        price: '2000000000000',
      };

      const result1 = await signer.signCreateOrder(tx1, privateKey, chainId);
      const result2 = await signer.signCreateOrder(tx2, privateKey, chainId);

      expect(result1.txHash).not.toBe(result2.txHash);
    });
  });

  describe('signCancelOrder', () => {
    it('should sign a cancel order transaction', async () => {
      const tx: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123abc',
        orderId: '0x456def',
        opts: defaultOpts,
      };

      const result = await signer.signCancelOrder(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
      expect(result.txHash.length).toBe(64);
      expect(result.signature.length).toBe(160);
    });

    it('should produce different signatures for different order IDs', async () => {
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

      const result1 = await signer.signCancelOrder(tx1, privateKey, chainId);
      const result2 = await signer.signCancelOrder(tx2, privateKey, chainId);

      expect(result1.txHash).not.toBe(result2.txHash);
    });
  });

  describe('signTransfer', () => {
    it('should sign a transfer transaction', async () => {
      const tx: L2TransferTxInfo = {
        txType: TxType.L2Transfer,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        recipient: '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb',
        amount: '1000000000',
        opts: defaultOpts,
      };

      const result = await signer.signTransfer(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
      expect(result.txHash.length).toBe(64);
      expect(result.signature.length).toBe(160);
    });

    it('should produce different signatures for different amounts', async () => {
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

      const result1 = await signer.signTransfer(tx1, privateKey, chainId);
      const result2 = await signer.signTransfer(tx2, privateKey, chainId);

      expect(result1.txHash).not.toBe(result2.txHash);
    });
  });

  describe('signWithdraw', () => {
    it('should sign a withdraw transaction', async () => {
      const tx: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: false,
        opts: defaultOpts,
      };

      const result = await signer.signWithdraw(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
      expect(result.txHash.length).toBe(64);
      expect(result.signature.length).toBe(160);
    });

    it('should sign withdraw with fast withdraw flag', async () => {
      const tx: L2WithdrawTxInfo = {
        txType: TxType.L2Withdraw,
        tokenAddress: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
        amount: '1000000000',
        fastWithdraw: true,
        opts: defaultOpts,
      };

      const result = await signer.signWithdraw(tx, privateKey, chainId);

      expect(result).toBeDefined();
      expect(result.txHash).toBeDefined();
      expect(result.signature).toBeDefined();
    });

    it('should produce different signatures with different fast withdraw flags', async () => {
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

      const result1 = await signer.signWithdraw(tx1, privateKey, chainId);
      const result2 = await signer.signWithdraw(tx2, privateKey, chainId);

      expect(result1.txHash).not.toBe(result2.txHash);
    });
  });

  describe('createAuthToken', () => {
    it('should create an authentication token', async () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);

      const authToken = await signer.createAuthToken(
        accountIndex,
        apiKeyIndex,
        deadline,
        privateKey,
        chainId,
      );

      expect(authToken).toBeDefined();
      expect(authToken.token).toBeDefined();
      expect(authToken.message).toBeDefined();
      expect(authToken.messageHash).toBeDefined();
      expect(authToken.accountIndex).toBe(accountIndex);
      expect(authToken.apiKeyIndex).toBe(apiKeyIndex);
      expect(authToken.deadline).toBe(deadline);
      expect(authToken.token.length).toBe(160); // 80 bytes = 160 hex chars
      expect(authToken.messageHash.length).toBe(64); // 32 bytes = 64 hex chars
    });

    it('should create tokens with future deadlines', async () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline = BigInt(Math.floor(Date.now() / 1000) + 7200); // 2 hours from now

      const authToken = await signer.createAuthToken(
        accountIndex,
        apiKeyIndex,
        deadline,
        privateKey,
        chainId,
      );

      expect(authToken).toBeDefined();
      expect(authToken.deadline).toBe(deadline);
    });

    it('should throw error for invalid account index', async () => {
      const apiKeyIndex = 0;
      const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);

      await expect(
        signer.createAuthToken(-1, apiKeyIndex, deadline, privateKey, chainId),
      ).rejects.toThrow(SignerError);

      await expect(
        signer.createAuthToken(256, apiKeyIndex, deadline, privateKey, chainId),
      ).rejects.toThrow(SignerError);
    });

    it('should throw error for invalid API key index', async () => {
      const accountIndex = 0;
      const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);

      await expect(
        signer.createAuthToken(accountIndex, -1, deadline, privateKey, chainId),
      ).rejects.toThrow(SignerError);

      await expect(
        signer.createAuthToken(accountIndex, 256, deadline, privateKey, chainId),
      ).rejects.toThrow(SignerError);
    });

    it('should produce different tokens for different deadlines', async () => {
      const accountIndex = 0;
      const apiKeyIndex = 0;
      const deadline1 = BigInt(1700000000);
      const deadline2 = BigInt(1800000000);

      const token1 = await signer.createAuthToken(
        accountIndex,
        apiKeyIndex,
        deadline1,
        privateKey,
        chainId,
      );
      const token2 = await signer.createAuthToken(
        accountIndex,
        apiKeyIndex,
        deadline2,
        privateKey,
        chainId,
      );

      expect(token1.messageHash).not.toBe(token2.messageHash);
    });
  });

  describe('verifySignature', () => {
    it('should verify a valid signature', async () => {
      const message = '0100000000000000';
      const result = await signer.signMessage(message, privateKey);

      const isValid = await signer.verifySignature(message, publicKey, result.signature);

      expect(isValid).toBe(true);
    });

    it('should reject signature with wrong public key', async () => {
      const message = '0100000000000000';
      const result = await signer.signMessage(message, privateKey);

      // Generate a different key pair
      const wrongKeys = await generateKeyPair();

      const isValid = await signer.verifySignature(message, wrongKeys.publicKey, result.signature);

      expect(isValid).toBe(false);
    });

    it('should reject signature with wrong message', async () => {
      const message1 = '0100000000000000';
      const message2 = '0200000000000000';

      const result = await signer.signMessage(message1, privateKey);

      const isValid = await signer.verifySignature(message2, publicKey, result.signature);

      expect(isValid).toBe(false);
    });

    it('should handle message with 0x prefix', async () => {
      const message = '0x0100000000000000';
      const result = await signer.signMessage(message, privateKey);

      const isValid = await signer.verifySignature(message, publicKey, result.signature);

      expect(isValid).toBe(true);
    });

    it('should throw error for invalid message length', async () => {
      const invalidMessage = '01';
      const signature = 'a'.repeat(160);

      await expect(signer.verifySignature(invalidMessage, publicKey, signature)).rejects.toThrow(
        SignerError,
      );
    });
  });

  describe('end-to-end signature verification', () => {
    it('should verify create order signature', async () => {
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

      const result = await signer.signCreateOrder(tx, privateKey, chainId);

      // Re-hash the transaction to get the message
      const { hashCreateOrderTx } = await import('./transaction-hasher');
      const messageHex = hashCreateOrderTx(tx, chainId);

      const isValid = await signer.verifySignature(messageHex, publicKey, result.signature);

      expect(isValid).toBe(true);
    });

    it('should verify cancel order signature', async () => {
      const tx: L2CancelOrderTxInfo = {
        txType: TxType.L2CancelOrder,
        marketId: '0x123abc',
        orderId: '0x456def',
        opts: defaultOpts,
      };

      const result = await signer.signCancelOrder(tx, privateKey, chainId);

      const { hashCancelOrderTx } = await import('./transaction-hasher');
      const messageHex = hashCancelOrderTx(tx, chainId);

      const isValid = await signer.verifySignature(messageHex, publicKey, result.signature);

      expect(isValid).toBe(true);
    });
  });
});
