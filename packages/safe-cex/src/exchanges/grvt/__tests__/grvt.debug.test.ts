/**
 * @vitest-environment node
 */

import { describe, it } from 'vitest';
import { resolve } from 'path';
import { existsSync, readFileSync, writeFileSync } from 'fs';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import { OrderType, OrderSide, type ExchangeOptions } from '../../../types';

// Helper to log to file
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

describe('GRVT Debug Test', () => {
  it('should debug authentication and order placement', async () => {
    const credPath = resolve(__dirname, '../.apikey.json');
    if (!existsSync(credPath)) {
      log('⚠️ No credentials file found');
      return;
    }

    const creds = JSON.parse(readFileSync(credPath, 'utf-8'));

    log('\n=== Testing GRVT Exchange ===');
    log('Testnet: ' + creds.testnet);
    log('API Key: ' + creds.apiKey);
    log('Has Secret: ' + !!creds.secret);
    log('Sub Account ID: ' + creds.subAccountId);

    const store = new DefaultStore();
    const options: ExchangeOptions = {
      key: creds.apiKey,
      secret: creds.secret,
      testnet: creds.testnet !== false,
      subaccount: creds.subAccountId,
    };

    const exchange = new GrvtExchange('grvt', options, store);

    log('\n=== Starting Exchange ===');
    await exchange.start();

    log('\n=== After Start ===');
    log('Session Cookie: ' + ((exchange as any).apiExtras?.sessionCookie ? 'SET (length: ' + (exchange as any).apiExtras?.sessionCookie.length + ')' : 'NOT SET'));
    log('Account ID: ' + ((exchange as any).apiExtras?.accountId || 'NOT SET'));
    log('Sub Account ID: ' + ((exchange as any).apiExtras?.subAccountId || 'NOT SET'));
    log('Markets loaded: ' + store.markets.length);

    // Find BTC market
    const market = store.markets.find(m => m.id === 'BTC_USDT_Perp');
    if (market) {
      log('\n=== BTC Market ===');
      log('Symbol: ' + market.symbol);
      log('Amount Precision: ' + market.precision.amount);
      log('Price Precision: ' + market.precision.price);
      log('Min Amount: ' + market.limits.amount.min);
    }

    log('\n=== Attempting Order Placement ===');
    log('Order params: 0.001 BTC @ $10,000');

    try {
      const orderIds = await exchange.placeOrder({
        symbol: 'BTC_USDT_Perp',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.001,
        price: 10000,
      });
      log('✅ Order placed successfully!');
      log('Order IDs: ' + JSON.stringify(orderIds));
    } catch (error: any) {
      log('❌ Order placement failed!');
      log('Error message: ' + error.message);
      if (error.response) {
        log('Status: ' + error.response.status);
        log('Status Text: ' + error.response.statusText);
        log('Response Data: ' + JSON.stringify(error.response.data, null, 2));

        // Log request details
        if (error.config) {
          log('\n=== Request Details ===');
          log('URL: ' + error.config.url);
          log('Method: ' + error.config.method);
          log('Headers: ' + JSON.stringify(error.config.headers, null, 2));
          if (error.config.data) {
            log('Request Body: ' + (typeof error.config.data === 'string' ? error.config.data : JSON.stringify(JSON.parse(error.config.data), null, 2)));
          }
        }
      }
    }

    exchange.dispose();
  }, 60000);
});
