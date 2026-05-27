/**
 * @vitest-environment node
 *
 * Integration test for dYdX order flow:
 * 1. Place a ~$100 BTC order
 * 2. Fetch orders to verify it was placed
 * 3. Cancel the order
 *
 * IMPORTANT: This test requires a funded dYdX account to complete successfully.
 * The account must have deposited USDC to the dYdX exchange.
 *
 * To fund your account:
 * 1. Visit https://trade.dydx.exchange/
 * 2. Connect your wallet (same address as in .apikey.json)
 * 3. Deposit USDC from your wallet to the dYdX exchange
 *
 * If the account is unfunded, the test will pass initialization but fail on order placement
 * with error: "account dydx1... not found: key not found"
 */

import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync, writeFileSync } from 'fs';
import { DydxExchange } from '../dydx.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderType, OrderSide, OrderTimeInForce, type ExchangeOptions } from '../../../types';

// Restore real fetch for dYdX SDK (it was mocked globally in setupVitest.ts)
// dYdX SDK needs real network access to connect to the blockchain
vi.unstubAllGlobals();
// Restore native fetch (Node 18+ has built-in fetch)
delete (global as any).fetch;

const logFile = resolve(__dirname, '../debug.log');
function log(message: string) {
  const timestamp = new Date().toISOString();
  const line = `[${timestamp}] ${message}\n`;
  try {
    writeFileSync(logFile, line, { flag: 'a' });
  } catch (e) {
    // Ignore
  }
  console.log(message);
}

/**
 * Integration test for dYdX order flow:
 * 1. Place a ~$100 BTC order
 * 2. Fetch orders to verify it was placed
 * 3. Cancel the order
 */
describe('dYdX Order Flow Integration Test', () => {
  let exchange: DydxExchange | null = null;
  let store: DefaultStore;
  let credentials: {
    apiKey?: string;
    secret?: string;
    testnet?: boolean;
  } = {};
  let placedOrderId: string | null = null;

  beforeAll(async () => {
    // Try to load credentials from .apikey.json
    const credPath = resolve(__dirname, '../.apikey.json');
    if (existsSync(credPath)) {
      try {
        credentials = JSON.parse(readFileSync(credPath, 'utf-8'));
        log('✅ Loaded API credentials from .apikey.json');
      } catch (error) {
        log('⚠️ Error loading credentials: ' + error);
      }
    } else {
      log('⚠️ No .apikey.json file found');
      log('💡 Create a .apikey.json file with your dYdX credentials to run this test');
      return;
    }

    // Create store
    store = new DefaultStore();

    // Initialize exchange
    const options: ExchangeOptions = {
      key: credentials.apiKey || '',
      secret: credentials.secret || '',
      testnet: credentials.testnet ?? true, // Default to testnet for safety if not specified
    };

    try {
      exchange = new DydxExchange('dydx', options, store);
      log('\n🔌 Initializing dYdX exchange...');

      // Validate account
      const validationResult = await exchange.validateAccount();
      if (validationResult !== '') {
        log('❌ Account validation failed: ' + validationResult);
        exchange = null;
        return;
      }
      log('✅ Account validated');

      // Fetch markets
      const markets = await exchange.fetchMarkets();
      store.update({ markets });
      log(`✅ Markets loaded: ${markets.length}`);

    } catch (error: any) {
      log('❌ Failed to initialize exchange:');
      log('   Error: ' + error.message);
      exchange = null;
    }
  }, 60000);

  afterAll(async () => {
    if (exchange) {
      exchange.dispose();
      log('\n🔌 Exchange disposed');
    }
  });

  it('should have valid credentials', () => {
    if (!credentials.apiKey || !credentials.secret) {
      log('⚠️ Skipping - no credentials found');
      return;
    }

    expect(credentials.apiKey).toBeDefined();
    expect(credentials.secret).toBeDefined();
    expect(typeof credentials.apiKey).toBe('string');
    expect(typeof credentials.secret).toBe('string');
  });

  it('should initialize exchange successfully', () => {
    if (!credentials.apiKey || !credentials.secret) {
      log('⚠️ Skipping - no credentials found');
      return;
    }

    expect(exchange).toBeDefined();
    expect(exchange).not.toBeNull();
  });

  it('should place a ~$100 BTC order', async () => {
    if (!exchange || !credentials.apiKey || !credentials.secret) {
      log('⚠️ Skipping - exchange not initialized or no credentials');
      return;
    }

    log('\n📊 Placing ~$100 BTC order...');

    // Get BTC-USD market
    const market = store.markets.find((m) => m.symbol === 'BTC-USD');
    expect(market).toBeDefined();

    if (!market) {
      throw new Error('BTC-USD market not found');
    }

    log(`   Market: ${market.symbol}`);
    log(`   Price precision: ${market.precision.price}`);
    log(`   Amount precision: ${market.precision.amount}`);
    log(`   Min amount: ${market.limits.amount.min}`);

    // Use a reasonable fixed price for BTC (around $95,000)
    const currentPrice = 95000;
    log(`   Using fixed BTC price: $${currentPrice.toLocaleString()}`);

    // Calculate order size for ~$100 notional value
    const orderSize = Math.max(100 / currentPrice, market.limits.amount.min);
    const notionalValue = orderSize * currentPrice;

    log(`   Notional: $${notionalValue.toFixed(2)}`);
    log(`   Order size: ${orderSize.toFixed(6)} BTC`);

    // Place order at current price - 20% (to ensure it's not immediately filled)
    const orderPrice = Math.floor(currentPrice * 0.8);
    log(`   Order price: $${orderPrice.toLocaleString()} (20% below market)`);

    try {
      // Place limit order
      const orderIds = await exchange.placeOrder({
        symbol: market.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: orderSize,
        price: orderPrice,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      });

      expect(orderIds).toBeDefined();
      expect(Array.isArray(orderIds)).toBe(true);
      expect(orderIds.length).toBeGreaterThan(0);

      placedOrderId = orderIds[0];

      log(`\n✅ Order placed successfully!`);
      log(`   Order ID: ${placedOrderId}`);
    } catch (error: any) {
      log('❌ Order placement failed!');
      log('   Error: ' + error.message);
      if (error.response?.data) {
        log('   Response data: ' + JSON.stringify(error.response.data, null, 2));
      }
      throw error;
    }
  }, 30000);

  it('should fetch orders and find the placed order', async () => {
    if (!exchange || !placedOrderId) {
      log('⚠️ Skipping - no order was placed');
      return;
    }

    log(`\n🔍 Fetching orders...`);

    // Wait for order to propagate
    await new Promise((resolve) => setTimeout(resolve, 3000));

    // Fetch open orders for BTC-USD
    const orders = await exchange.fetchOrders(['BTC-USD']);

    expect(orders).toBeDefined();
    expect(Array.isArray(orders)).toBe(true);

    log(`   Found ${orders.length} open order(s)`);

    // Find our placed order (check both order ID and client ID)
    const ourOrder = orders.find((o) => {
      const clientId = (o.info as any)?.clientId;
      return o.id === placedOrderId || String(clientId) === placedOrderId;
    });

    expect(ourOrder).toBeDefined();
    log(`\n✅ Order found in fetchOrders!`);
    log(`   Order ID: ${ourOrder?.id}`);
    log(`   Client ID: ${(ourOrder?.info as any)?.clientId}`);
    log(`   Symbol: ${ourOrder?.symbol}`);
    log(`   Side: ${ourOrder?.side}`);
    log(`   Amount: ${ourOrder?.amount} BTC`);
    log(`   Price: $${ourOrder?.price?.toLocaleString()}`);
    log(`   Status: ${ourOrder?.status}`);
  }, 30000);

  it('should cancel the placed order', async () => {
    if (!exchange || !placedOrderId) {
      log('⚠️ Skipping - no order to cancel');
      return;
    }

    log(`\n🗑️ Cancelling order ${placedOrderId}...`);

    // Fetch the order first
    const ordersBeforeCancel = await exchange.fetchOrders(['BTC-USD']);
    const orderToCancel = ordersBeforeCancel.find((o) => {
      const clientId = (o.info as any)?.clientId;
      return o.id === placedOrderId || String(clientId) === placedOrderId;
    });

    if (!orderToCancel) {
      throw new Error('Order not found for cancellation');
    }

    // Cancel the order
    await exchange.cancelOrder(orderToCancel);

    log(`✅ Cancel request sent`);

    // Wait for cancellation to propagate
    await new Promise((resolve) => setTimeout(resolve, 3000));

    // Verify order is no longer in open orders
    const orders = await exchange.fetchOrders(['BTC-USD']);
    const cancelledOrder = orders.find((o) => {
      const clientId = (o.info as any)?.clientId;
      return o.id === placedOrderId || String(clientId) === placedOrderId;
    });

    expect(cancelledOrder).toBeUndefined();

    log(`✅ Order successfully cancelled and removed from open orders`);
  }, 30000);
});
