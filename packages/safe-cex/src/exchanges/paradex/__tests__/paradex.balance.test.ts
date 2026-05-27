import { describe, it, expect } from 'vitest';
import { createExchangeFromAccount } from '../../../createExchangeFromAccount';
import type { Balance } from '../../../types';

/**
 * Manual test to investigate Paradex balance display issue
 *
 * This test should be run with actual API credentials to verify
 * that balance is calculated and displayed correctly.
 *
 * To run: npx tsx packages/safe-cex/src/exchanges/paradex/__tests__/paradex.balance.test.ts
 */

describe.skip('Paradex Balance Investigation', () => {
  it('should fetch and display balance correctly', async () => {
    // This test requires actual credentials from environment
    const apiKey = process.env.PARADEX_API_KEY;
    const apiSecret = process.env.PARADEX_API_SECRET;

    if (!apiKey || !apiSecret) {
      console.log('⚠️  Skipping test - PARADEX_API_KEY and PARADEX_API_SECRET required');
      return;
    }

    const exchange = await createExchangeFromAccount({
      exchange: 'paradex',
      name: 'paradex-test',
      apiKey: apiKey,
      apiSecret: apiSecret,
      testnet: false,
    });

    try {
      console.log('🔍 Fetching balance from Paradex API...');
      const balance: Balance = await (exchange as any).fetchBalance();

      console.log('\n📊 Balance Information:');
      console.log('  Total (equity):    ', balance.total);
      console.log('  Free:              ', balance.free);
      console.log('  Used:              ', balance.used);
      console.log('  UPNL:              ', balance.upnl);

      // Check if balance values make sense
      console.log('\n✅ Balance Validation:');

      // Total should be >= 0
      expect(balance.total).toBeGreaterThanOrEqual(0);
      console.log('  ✓ Total is non-negative');

      // Free should be >= 0
      expect(balance.free).toBeGreaterThanOrEqual(0);
      console.log('  ✓ Free is non-negative');

      // Used should be >= 0
      expect(balance.used).toBeGreaterThanOrEqual(0);
      console.log('  ✓ Used is non-negative');

      // Total should equal free + used (approximately, accounting for upnl)
      const calculatedTotal = balance.free + balance.used;
      console.log(`\n🧮 Calculation Check:`);
      console.log(`  Free + Used = ${calculatedTotal}`);
      console.log(`  Total (equity) = ${balance.total}`);
      console.log(`  Difference = ${Math.abs(balance.total - calculatedTotal)}`);

      // Fetch positions to check if UPNL is calculated correctly
      console.log('\n🔍 Fetching positions to verify UPNL...');
      const positions = await exchange.fetchPositions();
      console.log(`  Found ${positions.length} open position(s)`);

      if (positions.length > 0) {
        let totalUpnl = 0;
        positions.forEach((pos: any, idx: number) => {
          console.log(`  Position ${idx + 1}: ${pos.symbol} ${pos.side} ${pos.contracts} @ ${pos.entryPrice}`);
          console.log(`    Unrealized PnL: ${pos.unrealizedPnl}`);
          totalUpnl += pos.unrealizedPnl;
        });
        console.log(`  Total UPNL from positions: ${totalUpnl}`);
        console.log(`  Balance UPNL: ${balance.upnl}`);

        // UPNL in balance should match sum of position UPNLs
        const upnlDiff = Math.abs(balance.upnl - totalUpnl);
        if (upnlDiff > 0.01) {
          console.log(`  ⚠️  UPNL mismatch: ${upnlDiff}`);
        } else {
          console.log(`  ✓ UPNL matches`);
        }
      }

      // Check if the issue is with how collateral vs equity is displayed
      console.log('\n💡 Possible Issue Analysis:');
      console.log('  The Paradex API provides:');
      console.log('    - equity: Total account value (collateral + upnl)');
      console.log('    - collateral: Deposited funds');
      console.log('    - free_collateral: Available for new positions');
      console.log('    - position_margin: Used in positions');

      console.log('\n  Current mapping:');
      console.log('    - total → equity');
      console.log('    - free → free_collateral');
      console.log('    - used → position_margin');

      console.log('\n  Possible issue: Should we show collateral instead of equity?');
      console.log('  Or maybe: total should be collateral, not equity?');

    } catch (error) {
      console.error('❌ Error testing balance:', error);
      throw error;
    } finally {
      await exchange.dispose();
    }
  });

  it('should show raw API response for balance', async () => {
    const apiKey = process.env.PARADEX_API_KEY;
    const apiSecret = process.env.PARADEX_API_SECRET;

    if (!apiKey || !apiSecret) {
      console.log('⚠️  Skipping test - PARADEX_API_KEY and PARADEX_API_SECRET required');
      return;
    }

    const exchange = await createExchangeFromAccount({
      exchange: 'paradex',
      name: 'paradex-test',
      apiKey: apiKey,
      apiSecret: apiSecret,
      testnet: false,
    });

    try {
      console.log('🔍 Fetching raw account data from Paradex API...');

      // Access the internal xhr to get raw response
      const paradexExchange = exchange as any;
      await paradexExchange.start();

      // Wait for initialization
      if (paradexExchange.sdkInitPromise) {
        await paradexExchange.sdkInitPromise;
      }

      const response = await paradexExchange.xhr.get('/account');

      console.log('\n📋 Raw API Response:');
      console.log(JSON.stringify(response.data, null, 2));

      console.log('\n🔑 Key Fields:');
      console.log('  collateral:       ', response.data.collateral);
      console.log('  equity:           ', response.data.equity);
      console.log('  free_collateral:  ', response.data.free_collateral);
      console.log('  position_margin:  ', response.data.position_margin);
      console.log('  buying_power:     ', response.data.buying_power);
      console.log('  max_withdrawable: ', response.data.max_withdrawable);

    } catch (error) {
      console.error('❌ Error fetching raw data:', error);
      throw error;
    } finally {
      await exchange.dispose();
    }
  });
});
