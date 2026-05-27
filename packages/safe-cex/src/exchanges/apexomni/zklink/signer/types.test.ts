import { describe, it, expect } from 'vitest';
import type { TxEthSignature, TxSignature } from './types';

describe('zklink signer types', () => {
  describe('TxEthSignature', () => {
    it('should allow EthereumSignature type', () => {
      const sig: TxEthSignature = {
        type: 'EthereumSignature',
        signature: '0xabcd1234...',
      };

      expect(sig.type).toBe('EthereumSignature');
      expect(sig.signature).toBeDefined();
    });

    it('should allow EIP1271Signature type', () => {
      const sig: TxEthSignature = {
        type: 'EIP1271Signature',
        signature: '0xef567890...',
      };

      expect(sig.type).toBe('EIP1271Signature');
    });
  });

  describe('TxSignature', () => {
    it('should allow valid signature structure', () => {
      const sig: TxSignature = {
        pubKey: '0x1234567890abcdef',
        signature: 'aabbccdd1122334455667788', // without 0x prefix
      };

      expect(sig.pubKey).toMatch(/^0x/);
      expect(sig.signature).not.toMatch(/^0x/);
    });

    it('should accept any hex string format', () => {
      const sig: TxSignature = {
        pubKey: '0xabc123',
        signature: 'deadbeef',
      };

      expect(sig.pubKey).toBe('0xabc123');
      expect(sig.signature).toBe('deadbeef');
    });
  });
});
