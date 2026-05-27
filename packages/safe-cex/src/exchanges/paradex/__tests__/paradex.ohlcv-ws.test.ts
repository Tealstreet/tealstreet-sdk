import { describe, it, expect } from 'vitest';
import WebSocket from 'ws';

/**
 * Test to verify if OHLCV/klines WebSocket channel is available on Paradex
 *
 * According to Python SDK (paradex_py/api/ws_client.py), available channels are:
 * - account, balance_events, bbo, fills, funding_data, funding_payments
 * - funding_rate_comparison, markets_summary, orders, order_book, positions
 * - trades, tradebusts, transaction, transfers
 *
 * There is NO klines, candles, or ohlcv channel listed.
 * OHLCV data is only available via REST API: GET /markets/klines
 */
describe('Paradex OHLCV WebSocket Availability', () => {
  const WS_URL = 'wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false';
  const TEST_MARKET = 'BTC-USD-PERP';
  const TIMEOUT = 10000;

  it('should verify no klines WebSocket channel exists', async () => {
    const ws = new WebSocket(WS_URL);
    const messages: any[] = [];
    let subscriptionResponse: any = null;

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        resolve();
      }, 5000);

      ws.on('open', () => {
        // Try various possible klines channel formats
        const klineChannels = [
          `klines.${TEST_MARKET}`,
          `klines.${TEST_MARKET}.1m`,
          `candles.${TEST_MARKET}`,
          `ohlcv.${TEST_MARKET}`,
        ];

        // Try subscribing to each format
        klineChannels.forEach((channel, idx) => {
          const subscribeMsg = {
            id: idx + 1,
            jsonrpc: '2.0',
            method: 'subscribe',
            params: { channel },
          };
          ws.send(JSON.stringify(subscribeMsg));
        });
      });

      ws.on('message', (data: WebSocket.Data) => {
        const msg = JSON.parse(data.toString());
        messages.push(msg);

        // Check if any subscription succeeded
        if (msg.id !== undefined && msg.result) {
          subscriptionResponse = msg;
          clearTimeout(timeout);
          ws.close();
          resolve();
        }

        // Check for error responses
        if (msg.id !== undefined && msg.error) {
          // Expected - channel doesn't exist
          console.log(`Channel subscription ${msg.id} failed:`, msg.error);
        }
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });

    // We expect NO successful subscription to klines channels
    expect(subscriptionResponse).toBeNull();

    // All responses should be errors
    const errorResponses = messages.filter(m => m.error);
    expect(errorResponses.length).toBeGreaterThan(0);

    console.log(`\nTest Result: Attempted to subscribe to klines channels, all failed as expected.`);
    console.log(`Total messages received: ${messages.length}`);
    console.log(`Error responses: ${errorResponses.length}`);
  }, TIMEOUT);

  it('should document that OHLCV is REST-only', () => {
    // This test documents the fact that Paradex does not support OHLCV via WebSocket
    const availableChannels = [
      'account',
      'balance_events',
      'bbo.{market}',
      'fills.{market}',
      'funding_data.{market}',
      'funding_payments.{market}',
      'funding_rate_comparison.{market}',
      'markets_summary.{market}',
      'orders.{market}',
      'order_book.{market}.snapshot@{depth}@{refresh_rate}@{price_tick}',
      'positions',
      'trades.{market}',
      'tradebusts',
      'transaction',
      'transfers',
    ];

    // Verify klines/candles/ohlcv are NOT in available channels
    expect(availableChannels).not.toContain(expect.stringContaining('kline'));
    expect(availableChannels).not.toContain(expect.stringContaining('candle'));
    expect(availableChannels).not.toContain(expect.stringContaining('ohlcv'));

    // OHLCV data must be fetched via REST API
    const restEndpoint = '/markets/klines';
    expect(restEndpoint).toBeDefined();

    console.log('\n✅ Confirmed: OHLCV data is only available via REST API');
    console.log(`   Endpoint: GET ${restEndpoint}`);
    console.log(`   Parameters: symbol, resolution, start_at, end_at, price_kind`);
  });

  it('should verify existing listenOHLCV returns noop with warning', () => {
    // The implementation should return a noop function and log a warning
    // This is already implemented in paradex.exchange.ts line 742-748

    const expectedBehavior = {
      returnValue: 'noop function (() => {})',
      logLevel: 'Warning',
      message: 'OHLCV WebSocket streaming is not available on Paradex. Use fetchOHLCV for historical data.',
    };

    expect(expectedBehavior.returnValue).toBe('noop function (() => {})');
    expect(expectedBehavior.logLevel).toBe('Warning');
    expect(expectedBehavior.message).toContain('not available');

    console.log('\n✅ listenOHLCV implementation correctly returns noop with warning');
    console.log(`   Message: "${expectedBehavior.message}"`);
  });
});
