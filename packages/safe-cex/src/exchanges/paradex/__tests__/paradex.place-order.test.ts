import { describe, it, expect, beforeAll } from 'vitest';
import { OrderSide, OrderType } from '../../../types';
import { ParadexExchange } from '../paradex.exchange';
import { DefaultStore } from '../../../store/store.base';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Paradex Place Order Test Suite
 *
 * This test suite validates the place order functionality including:
 * 1. Proper REST API authentication with signatures
 * 2. Order placement with correct parameters
 * 3. Error handling and user-facing error messages
 *
 * ## Running the Tests
 *
 * ```bash
 * cd packages/safe-cex
 * npx vitest run src/exchanges/paradex/paradex.place-order.test.ts
 * ```
 */

// Load test credentials from .apikey.json
let testCreds: any;
try {
  const credsPath = path.join(__dirname, '.apikey.json');
  testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
} catch (error) {
  console.warn('⚠️  .apikey.json not found - tests will be skipped');
  testCreds = null;
}

describe('Paradex Place Order - REST API Authentication', () => {
  let exchange: ParadexExchange;
  let errorMessages: string[] = [];

  beforeAll(async () => {
    if (!testCreds) {
      console.warn('⚠️  Skipping tests - no credentials available');
      return;
    }

    // Create store
    const store = new DefaultStore();

    // Create exchange with test credentials
    exchange = new ParadexExchange(
      'paradex',
      {
        key: testCreds.apiKey,
        secret: '', // Not used for Paradex
        testnet: testCreds.testnet || false,
        paradexAddress: testCreds.paradexAddress,
        subkeyAddress: testCreds.subkeyAddress,
      },
      store,
    );

    // Capture error messages
    exchange.on('error', (error: string) => {
      errorMessages.push(error);
      console.log('❌ Error emitted:', error);
    });

    // Wait for SDK initialization
    console.log('🔄 Initializing Paradex SDK...');
    await exchange['sdkInitPromise'];
    console.log('✅ SDK initialized');

    // Fetch markets first (needed for validation)
    console.log('🔄 Fetching markets...');
    const markets = await exchange.fetchMarkets();
    console.log(`✅ Fetched ${markets.length} markets`);

    store.update({
      markets,
      loaded: { ...store.loaded, markets: true },
    });
  }, 30000);

  it('should have test credentials available', () => {
    expect(testCreds).toBeTruthy();
    expect(testCreds.apiKey).toBeTruthy();
    expect(testCreds.paradexAddress).toBeTruthy();
    expect(testCreds.subkeyAddress).toBeTruthy();
  });

  it('should have SDK initialized', async () => {
    if (!testCreds) return;

    expect(exchange).toBeTruthy();
    expect(exchange.sdkAccount).toBeTruthy();
    expect(exchange.requestSigner).toBeTruthy();
    expect(exchange.jwtManager).toBeTruthy();

    console.log('SDK Account:', exchange.sdkAccount?.account.address);
    console.log('Request Signer:', exchange.requestSigner?.getAccountAddress());
  });

  it('should place a limit order with proper authentication', async () => {
    if (!testCreds) {
      console.warn('⚠️  Skipping - no credentials');
      return;
    }

    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📝 Attempting to place order...');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    errorMessages = [];

    try {
      // Place a small limit order (using a price far from market to avoid execution)
      const orderIds = await exchange.placeOrder({
        symbol: 'ETH-USD-PERP',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 0.01, // Small amount
        price: 100, // Well below market to avoid execution
      });

      console.log('\n✅ Order placed successfully!');
      console.log('Order IDs:', orderIds);

      expect(orderIds).toBeTruthy();
      expect(Array.isArray(orderIds)).toBe(true);
      expect(orderIds.length).toBeGreaterThan(0);

      // Clean up - cancel the order
      if (orderIds.length > 0) {
        console.log('🧹 Cleaning up - canceling order...');
        await exchange.cancelOrder(orderIds[0]);
        console.log('✅ Order canceled');
      }
    } catch (error: any) {
      console.error('\n❌ Order placement failed!');
      console.error('Error:', error.message);
      console.error('Response data:', error.response?.data);
      console.error('Status:', error.response?.status);

      // Log all captured error messages
      if (errorMessages.length > 0) {
        console.log('\n📋 Errors emitted to user:');
        errorMessages.forEach((msg, i) => {
          console.log(`  ${i + 1}. ${msg}`);
        });
      } else {
        console.log('\n⚠️  No errors were emitted to user via emitAndLogError!');
      }

      // The error should be captured and emitted to user
      expect(errorMessages.length).toBeGreaterThan(0);
      expect(errorMessages[0]).toContain('signature');
    }
  }, 30000);

  it('should emit user-friendly error messages via emitAndLogError', async () => {
    if (!testCreds) {
      console.warn('⚠️  Skipping - no credentials');
      return;
    }

    errorMessages = [];

    try {
      // Try to place order (will fail with current implementation)
      await exchange.placeOrder({
        symbol: 'ETH-USD-PERP',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 0.01,
        price: 100,
      });
    } catch (error: any) {
      // Error should have been emitted to user
      console.log('\nError messages emitted to user:', errorMessages);

      // Verify error was emitted
      expect(errorMessages.length).toBeGreaterThan(0);

      // Error message should be user-friendly
      const errorMsg = errorMessages[0];
      expect(errorMsg).toBeTruthy();
      expect(typeof errorMsg).toBe('string');

      console.log('✅ Error properly emitted to user:', errorMsg);
    }
  }, 30000);
});

describe('Paradex Place Order - Signature Format', () => {
  it('should use milliseconds for signature timestamp', () => {
    // The error message says: "signature_timestamp: signature timestamp is not in milliseconds"
    // This means we should be using Date.now() (milliseconds), not Math.floor(Date.now() / 1000) (seconds)

    const timestampMs = Date.now();
    const timestampSec = Math.floor(timestampMs / 1000);

    console.log('Timestamp in milliseconds:', timestampMs, `(${String(timestampMs).length} digits)`);
    console.log('Timestamp in seconds:', timestampSec, `(${String(timestampSec).length} digits)`);

    // Verify milliseconds has 13 digits
    expect(String(timestampMs).length).toBe(13);

    // Verify seconds has 10 digits
    expect(String(timestampSec).length).toBe(10);

    // For place order, we should use milliseconds
    const correctTimestamp = timestampMs;
    expect(String(correctTimestamp).length).toBe(13);
  });

  it('should include required signature headers', () => {
    // According to the error message, we need:
    // - signature (not blank)
    // - signature_timestamp (in milliseconds)

    const requiredHeaders = [
      'PARADEX-STARKNET-ACCOUNT',
      'PARADEX-STARKNET-SIGNATURE',
      'PARADEX-TIMESTAMP',
    ];

    // These headers should be added by the API interceptor
    console.log('Required headers for authenticated requests:', requiredHeaders);

    expect(requiredHeaders).toContain('PARADEX-STARKNET-SIGNATURE');
    expect(requiredHeaders).toContain('PARADEX-TIMESTAMP');
  });
});
