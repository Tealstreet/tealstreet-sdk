import { Wallet } from 'ethers';
import { describe, expect, it } from 'vitest';
import vectors from '../../exchanges/rust/hyperliquid-signer-setup.golden.json';

describe('Hyperliquid scalar spelling before provider setup cutover', () => {
  for (const row of vectors) {
    it(row.name, () => {
      let address: string | undefined;
      try {
        address = new Wallet(row.scalar).address;
      } catch {
        address = undefined;
      }
      expect(address !== undefined).toBe(row.legacySdkValid);
    });
  }
});
