// @vitest-environment node
import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { BackpackExchange } from '../backpack.exchange';
import { DefaultStore } from '../../../store/store.base';
import { PositionSide } from '../../../types';

/**
 * Test validateAccount, fetchBalance, and fetchPositions for Backpack
 */

interface ApiKeyConfig {
  exchange: string;
  testnet: boolean;
  apiKey: string;
  secret: string;
}

describe('BackpackExchange - Core Methods', () => {
  let exchange: BackpackExchange;
  let apiConfig: ApiKeyConfig;

  beforeAll(async () => {
    // Load API credentials
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    // Use DefaultStore for a complete store implementation
    const store = new DefaultStore();

    // Create exchange instance - direct connection for tests (no proxy)
    exchange = new BackpackExchange(
      'backpack',
      {
        key: apiConfig.apiKey,
        secret: apiConfig.secret,
        testnet: apiConfig.testnet,
      },
      store,
    );

    console.log('\n==============================');
    console.log('Backpack Core Methods Test');
    console.log('==============================');
    console.log('API Key:', apiConfig.apiKey.substring(0, 10) + '...');
    console.log('Testnet:', apiConfig.testnet);
    console.log('==============================\n');
  }, 30000);

  describe('validateAccount', () => {
    it('should validate account successfully', async () => {
      console.log('Testing validateAccount...');
      const result = await exchange.validateAccount();

      console.log('✅ Validation result:', result === '' ? 'Success' : result);

      // Empty string means success
      expect(result).toBe('');
    }, 30000);

    it('should fail with invalid credentials', async () => {
      // Create exchange with invalid credentials
      const store = new DefaultStore();
      const invalidExchange = new BackpackExchange(
        'backpack',
        {
          key: 'invalid_key',
          secret: 'invalid_secret',
          testnet: apiConfig.testnet,
        },
        store,
      );

      console.log('Testing with invalid credentials...');
      const result = await invalidExchange.validateAccount();

      console.log('✅ Expected failure:', result);

      // Should return error message
      expect(result).not.toBe('');
      expect(result.length).toBeGreaterThan(0);
    }, 30000);
  });

  describe('fetchBalance', () => {
    it('should fetch balance successfully', async () => {
      console.log('\nTesting fetchBalance...');
      const balance = await exchange.fetchBalance();

      console.log('Balance fetched:');
      console.log(`  Total: $${balance.total.toFixed(2)}`);
      console.log(`  Free: $${balance.free.toFixed(2)}`);
      console.log(`  Used: $${balance.used.toFixed(2)}`);
      console.log(`  UPNL: $${balance.upnl.toFixed(2)}`);

      if (balance.currencies) {
        const currencyCount = Object.keys(balance.currencies).length;
        console.log(`  Currencies: ${currencyCount}`);

        // Log first few currencies
        const currencies = Object.entries(balance.currencies).slice(0, 5);
        for (const [currency, bal] of currencies) {
          console.log(`    ${currency}: ${(bal as any).available} (available), ${(bal as any).locked} (locked)`);
        }
      }

      // Verify balance structure
      expect(typeof balance.total).toBe('number');
      expect(typeof balance.free).toBe('number');
      expect(typeof balance.used).toBe('number');
      expect(typeof balance.upnl).toBe('number');

      // Total should be >= 0
      expect(balance.total).toBeGreaterThanOrEqual(0);
      expect(balance.free).toBeGreaterThanOrEqual(0);
      expect(balance.used).toBeGreaterThanOrEqual(0);

      // Free + Used should equal Total (approximately, allowing for rounding)
      expect(Math.abs((balance.free + balance.used) - balance.total)).toBeLessThan(0.01);

      console.log('✅ Balance structure validated\n');
    }, 30000);

    it('should have currencies or coins object', async () => {
      const balance = await exchange.fetchBalance();

      // Backpack may use either currencies or coins structure
      const hasCurrencies = balance.currencies && Object.keys(balance.currencies).length > 0;
      const hasCoins = balance.coins && Object.keys(balance.coins).length > 0;

      if (hasCurrencies) {
        console.log(`  Currencies: ${Object.keys(balance.currencies!).length}`);
      }
      if (hasCoins) {
        console.log(`  Coins: ${Object.keys(balance.coins!).length}`);
        // Check structure of first coin balance
        const firstCoin = Object.values(balance.coins!)[0];
        expect(firstCoin).toHaveProperty('base');
        expect(firstCoin).toHaveProperty('amount');
      }
    }, 30000);
  });

  describe('fetchPositions', () => {
    it('should fetch positions successfully', async () => {
      console.log('\nTesting fetchPositions...');
      const positions = await exchange.fetchPositions();

      console.log(`✅ Fetched ${positions.length} position(s)\n`);

      // Verify positions array
      expect(Array.isArray(positions)).toBe(true);

      // If there are positions, validate their structure
      if (positions.length > 0) {
        const realPositions = positions.filter((p) => p.contracts > 0);
        const fakePositions = positions.filter((p) => p.contracts === 0);

        console.log(`Real positions (contracts > 0): ${realPositions.length}`);
        console.log(`Fake positions (contracts = 0): ${fakePositions.length}\n`);

        for (const pos of realPositions) {
          console.log(`Symbol: ${pos.symbol}`);
          console.log(`  Side: ${pos.side}`);
          console.log(`  Contracts: ${pos.contracts}`);
          console.log(`  Entry Price: $${pos.entryPrice}`);
          console.log(`  Notional: $${pos.notional.toFixed(2)}`);
          console.log(`  Unrealized PnL: $${pos.unrealizedPnl.toFixed(2)}`);
          console.log(`  Leverage: ${pos.leverage}x`);
          console.log(`  Liquidation Price: $${pos.liquidationPrice}`);
          console.log('');
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

        // Verify non-negative values
        expect(firstPos.contracts).toBeGreaterThanOrEqual(0);
        expect(firstPos.leverage).toBeGreaterThan(0);

        console.log('✅ Position structure validated\n');
      } else {
        console.log('ℹ️  No open positions (this is OK)\n');
      }
    }, 30000);

    it('should handle hedged position mode', async () => {
      const positions = await exchange.fetchPositions();
      const positionMode = await exchange.getPositionMode();

      console.log(`Position mode: ${positionMode}`);

      // Backpack uses hedged mode (lowercase)
      expect(positionMode).toBe('hedged');

      // In hedged mode, we can have both long and short positions for the same symbol
      const symbolsWithBothSides = new Map<string, Set<string>>();

      for (const pos of positions) {
        if (!symbolsWithBothSides.has(pos.symbol)) {
          symbolsWithBothSides.set(pos.symbol, new Set());
        }
        symbolsWithBothSides.get(pos.symbol)!.add(pos.side);
      }

      // Check if any symbol has both sides (this would only be true with actual positions)
      for (const [symbol, sides] of symbolsWithBothSides.entries()) {
        if (sides.size === 2) {
          console.log(`✅ Symbol ${symbol} has both Long and Short positions (hedged mode)`);
        }
      }

      console.log('✅ Hedged position mode verified\n');
    }, 30000);
  });
});
