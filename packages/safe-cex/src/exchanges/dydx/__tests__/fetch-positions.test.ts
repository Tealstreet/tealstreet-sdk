import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { DydxExchange } from '../dydx.exchange';
import { DefaultStore } from '../../../store/store.base';
import { PositionSide } from '../../../types';

/**
 * Test fetchPositions with real dYdX account that has BTC position
 */

interface ApiKeyConfig {
  exchange: string;
  testnet: boolean;
  apiKey: string;
  secret: string;
  dydxAddress?: string;
}

describe('DydxExchange - fetchPositions', () => {
  let exchange: DydxExchange;
  let apiConfig: ApiKeyConfig;

  beforeAll(async () => {
    // Load API credentials
    const configPath = join(__dirname, '..', '.apikey.json');
    const configData = readFileSync(configPath, 'utf-8');
    apiConfig = JSON.parse(configData);

    // Use DefaultStore for a complete store implementation
    const mockStore = new DefaultStore();

    // Create exchange instance
    exchange = new DydxExchange(
      'dydx',
      {
        key: apiConfig.apiKey,
        secret: apiConfig.secret,
        testnet: apiConfig.testnet,
        dydxAddress: apiConfig.dydxAddress, // Use the trading address
      },
      mockStore,
    );

    console.log('\n==============================');
    console.log('dYdX fetchPositions Test');
    console.log('==============================');
    console.log('API Wallet:', apiConfig.apiKey);
    console.log('Trading Wallet:', apiConfig.dydxAddress || apiConfig.apiKey);
    console.log('==============================\n');

    // Validate account to initialize SDK client
    // Note: We don't call start() because that starts WebSocket connections
    // which we don't need for fetchPositions REST API calls
    console.log('Calling validateAccount()...');
    const validationResult = await exchange.validateAccount();

    if (validationResult) {
      console.error('❌ Validation failed:', validationResult);
      throw new Error(`Account validation failed: ${validationResult}`);
    }

    console.log('✅ validateAccount() completed - SDK initialized\n');
  }, 60000);

  it('should fetch positions successfully', async () => {
    const positions = await exchange.fetchPositions();

    console.log(`\n✅ Fetched ${positions.length} position(s):\n`);

    for (const pos of positions) {
      console.log(`Symbol: ${pos.symbol}`);
      console.log(`  Side: ${pos.side}`);
      console.log(`  Contracts: ${pos.contracts}`);
      console.log(`  Entry Price: $${pos.entryPrice}`);
      console.log(`  Notional: $${pos.notional.toFixed(2)}`);
      console.log(`  Unrealized PnL: $${pos.unrealizedPnl.toFixed(2)}`);
      console.log(`  Realized PnL: $${pos.realizedPnl?.toFixed(2) || '0.00'}`);
      console.log(`  Margin Type: ${pos.marginType}`);
      console.log(`  Position Mode: ${pos.positionMode}`);
      console.log('');
    }

    // Verify we got positions array
    expect(Array.isArray(positions)).toBe(true);

    // If there are positions (we know there's a BTC position), verify structure
    if (positions.length > 0) {
      const firstPos = positions[0];

      // Check required fields exist
      expect(firstPos.id).toBeDefined();
      expect(firstPos.symbol).toBeDefined();
      expect(firstPos.side).toBeDefined();
      expect(typeof firstPos.contracts).toBe('number');
      expect(typeof firstPos.entryPrice).toBe('number');
      expect(typeof firstPos.notional).toBe('number');
      expect(typeof firstPos.unrealizedPnl).toBe('number');

      // Verify side is valid
      expect([PositionSide.Long, PositionSide.Short]).toContain(firstPos.side);

      // Verify positive values
      expect(firstPos.contracts).toBeGreaterThan(0);
      expect(firstPos.entryPrice).toBeGreaterThan(0);
      expect(firstPos.notional).toBeGreaterThan(0);

      console.log('✅ Position structure validated\n');
    }
  }, 30000);

  it('should filter positions by symbol', async () => {
    const allPositions = await exchange.fetchPositions();

    if (allPositions.length === 0) {
      console.log('⚠️  No positions to test filtering\n');
      return;
    }

    // Get first position's symbol
    const testSymbol = allPositions[0].symbol;

    // Fetch only that symbol
    const filteredPositions = await exchange.fetchPositions([testSymbol]);

    console.log(`\n✅ Filtered positions for ${testSymbol}:`);
    console.log(`  Total positions: ${allPositions.length}`);
    console.log(`  Filtered positions: ${filteredPositions.length}\n`);

    // All filtered positions should match the symbol
    for (const pos of filteredPositions) {
      expect(pos.symbol).toBe(testSymbol);
    }

    expect(filteredPositions.length).toBeGreaterThan(0);
    expect(filteredPositions.length).toBeLessThanOrEqual(allPositions.length);
  }, 30000);

  it('should handle empty positions gracefully', async () => {
    // Try to fetch positions for a symbol that doesn't exist
    const positions = await exchange.fetchPositions(['INVALID-SYMBOL']);

    console.log(`\n✅ Empty positions test:`);
    console.log(`  Positions for INVALID-SYMBOL: ${positions.length}\n`);

    expect(Array.isArray(positions)).toBe(true);
    expect(positions.length).toBe(0);
  }, 30000);
});
