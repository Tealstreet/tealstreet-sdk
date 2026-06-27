import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import { PositionSide } from '../../../types';

/**
 * Test validateAccount, fetchBalance, and fetchPositions for GRVT
 */

interface ApiKeyConfig {
  exchange: string;
  testnet: boolean;
  apiKey: string;
  secret: string;
  subaccountId?: string;
}

describe('GrvtExchange - Core Methods', () => {
  let exchange: GrvtExchange;
  let apiConfig: ApiKeyConfig;

  beforeAll(async () => {
    // Load API credentials
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    // Use DefaultStore for a complete store implementation
    const store = new DefaultStore();

    // Create exchange instance
    exchange = new GrvtExchange(
      'grvt',
      {
        key: apiConfig.apiKey,
        secret: apiConfig.secret,
        testnet: apiConfig.testnet,
      },
      store,
    );

    console.log('\n==============================');
    console.log('GRVT Core Methods Test');
    console.log('==============================');
    console.log('API Key:', apiConfig.apiKey.substring(0, 10) + '...');
    console.log('Testnet:', apiConfig.testnet);
    console.log('==============================\n');
  }, 60000);

  describe('validateAccount', () => {
    it('should validate account and authenticate successfully', async () => {
      console.log('Testing validateAccount with authentication...');
      const result = await exchange.validateAccount();

      console.log('✅ Validation result:', result.error === '' ? 'Success' : result.error);

      // Empty string means success
      expect(result.error).toBe('');

      // Check that session was established
      expect(exchange.apiExtras).toBeDefined();
      expect(exchange.apiExtras?.accountId).toBeDefined();
      console.log('Account ID:', exchange.apiExtras?.accountId);
      console.log('Sub-Account ID:', exchange.apiExtras?.subAccountId);
    }, 60000);

    it('should fail with invalid credentials', async () => {
      // Create exchange with invalid credentials
      const store = new DefaultStore();
      const invalidExchange = new GrvtExchange(
        'grvt',
        {
          key: 'invalid_key',
          secret: '0x' + '0'.repeat(64), // Invalid private key
          testnet: apiConfig.testnet,
        },
        store,
      );

      console.log('\nTesting with invalid credentials...');
      const result = await invalidExchange.validateAccount();

      console.log('✅ Expected failure:', result);

      // Should return error message
      expect(result.error).not.toBe('');
      expect(result.error.length).toBeGreaterThan(0);
    }, 60000);
  });

  describe('fetchBalance', () => {
    it('should fetch balance successfully', async () => {
      console.log('\nTesting fetchBalance...');

      // Ensure we're authenticated first
      if (!exchange.apiExtras?.sessionCookie) {
        await exchange.validateAccount();
      }

      const balance = await exchange.fetchBalance();

      console.log('Balance fetched:');
      console.log(`  Total Collateral: $${balance.total.toFixed(2)}`);
      console.log(`  Free: $${balance.free.toFixed(2)}`);
      console.log(`  Used: $${balance.used.toFixed(2)}`);
      console.log(`  UPNL: $${balance.upnl.toFixed(2)}`);

      if (balance.currencies) {
        const currencyCount = Object.keys(balance.currencies).length;
        console.log(`  Currencies: ${currencyCount}`);

        // Log all currencies
        for (const [currency, bal] of Object.entries(balance.currencies)) {
          const available = (bal as any).available;
          const locked = (bal as any).locked;
          if (available > 0 || locked > 0) {
            console.log(`    ${currency}: ${available} (available), ${locked} (locked)`);
          }
        }
      }

      // Verify balance structure
      expect(typeof balance.total).toBe('number');
      expect(typeof balance.free).toBe('number');
      expect(typeof balance.used).toBe('number');
      expect(typeof balance.upnl).toBe('number');

      // Values should be >= 0
      expect(balance.total).toBeGreaterThanOrEqual(0);
      expect(balance.free).toBeGreaterThanOrEqual(0);
      expect(balance.used).toBeGreaterThanOrEqual(0);

      // Currencies object should exist
      expect(balance.currencies).toBeDefined();
      expect(typeof balance.currencies).toBe('object');

      console.log('✅ Balance structure validated\n');
    }, 60000);

    it('should have USDC balance', async () => {
      const balance = await exchange.fetchBalance();

      // Check if USDC balance exists in currencies
      if (balance.currencies && 'USDC' in balance.currencies) {
        const usdcBalance = balance.currencies['USDC'];
        console.log('USDC Balance found:');
        console.log(`  Available: ${(usdcBalance as any).available}`);
        console.log(`  Locked: ${(usdcBalance as any).locked}`);

        expect((usdcBalance as any).available).toBeGreaterThanOrEqual(0);
        expect((usdcBalance as any).locked).toBeGreaterThanOrEqual(0);
      } else {
        console.log('ℹ️  No USDC balance (account may be empty)');
      }
    }, 60000);
  });

  describe('fetchPositions', () => {
    it('should fetch positions successfully', async () => {
      console.log('\nTesting fetchPositions...');

      // Ensure we're authenticated first
      if (!exchange.apiExtras?.sessionCookie) {
        await exchange.validateAccount();
      }

      const positions = await exchange.fetchPositions();

      console.log(`✅ Fetched ${positions.length} position(s)\n`);

      // Verify positions array
      expect(Array.isArray(positions)).toBe(true);

      // If there are positions, validate their structure
      if (positions.length > 0) {
        console.log('Position details:');

        for (const pos of positions) {
          console.log(`\nSymbol: ${pos.symbol}`);
          console.log(`  Side: ${pos.side}`);
          console.log(`  Contracts: ${pos.contracts}`);
          console.log(`  Entry Price: $${pos.entryPrice}`);
          console.log(`  Notional: $${pos.notional.toFixed(2)}`);
          console.log(`  Unrealized PnL: $${pos.unrealizedPnl.toFixed(2)}`);
          console.log(`  Realized PnL: $${pos.realizedPnl?.toFixed(2) || '0.00'}`);
          console.log(`  Leverage: ${pos.leverage}x`);
          console.log(`  Liquidation Price: $${pos.liquidationPrice}`);
          console.log(`  Margin Type: ${pos.marginType}`);
        }

        // Validate structure of first position
        const firstPos = positions[0];

        expect(firstPos.id).toBeDefined();
        expect(firstPos.symbol).toBeDefined();
        expect(firstPos.side).toBeDefined();
        expect(typeof firstPos.contracts).toBe('number');
        expect(typeof firstPos.entryPrice).toBe('number');
        expect(typeof firstPos.notional).toBe('number');
        expect(typeof firstPos.unrealizedPnl).toBe('number');
        expect(typeof firstPos.leverage).toBe('number');

        // Verify side is valid
        expect([PositionSide.Long, PositionSide.Short]).toContain(firstPos.side);

        // Verify positive values
        expect(firstPos.contracts).toBeGreaterThan(0);
        expect(firstPos.entryPrice).toBeGreaterThan(0);
        expect(firstPos.leverage).toBeGreaterThan(0);

        console.log('\n✅ Position structure validated');
      } else {
        console.log('ℹ️  No open positions (this is OK)');
      }
    }, 60000);

    it('should filter positions by symbol', async () => {
      const allPositions = await exchange.fetchPositions();

      if (allPositions.length === 0) {
        console.log('\n⚠️  No positions to test filtering');
        return;
      }

      // Get first position's symbol
      const testSymbol = allPositions[0].symbol;

      // Fetch only that symbol
      const filteredPositions = await exchange.fetchPositions([testSymbol]);

      console.log(`\n✅ Filtered positions for ${testSymbol}:`);
      console.log(`  Total positions: ${allPositions.length}`);
      console.log(`  Filtered positions: ${filteredPositions.length}`);

      // All filtered positions should match the symbol
      for (const pos of filteredPositions) {
        expect(pos.symbol).toBe(testSymbol);
      }

      expect(filteredPositions.length).toBeGreaterThan(0);
      expect(filteredPositions.length).toBeLessThanOrEqual(allPositions.length);
    }, 60000);

    it('should handle empty positions for invalid symbol', async () => {
      // Try to fetch positions for a symbol that doesn't exist
      const positions = await exchange.fetchPositions(['INVALID_SYMBOL_123']);

      console.log(`\n✅ Empty positions test:`);
      console.log(`  Positions for INVALID_SYMBOL_123: ${positions.length}`);

      expect(Array.isArray(positions)).toBe(true);
      expect(positions.length).toBe(0);
    }, 60000);
  });

  describe('position mode', () => {
    it('should use one-way position mode', async () => {
      const positionMode = await exchange.getPositionMode();

      console.log(`\nPosition mode: ${positionMode}`);

      // GRVT uses one-way position mode by default
      expect(positionMode).toBe('oneway');

      console.log('✅ Position mode verified\n');
    }, 30000);
  });
});
