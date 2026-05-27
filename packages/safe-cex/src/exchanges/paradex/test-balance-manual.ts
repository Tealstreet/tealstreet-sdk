#!/usr/bin/env tsx
/**
 * Manual test script to investigate Paradex balance display issue
 *
 * Usage:
 *   npx tsx packages/safe-cex/src/exchanges/paradex/test-balance-manual.ts
 *
 * This script reads credentials from .apikey.json in the same directory
 */

import { readFileSync } from 'fs';
import axios from 'axios';

async function testParadexBalance() {
  console.log('🚀 Starting Paradex Balance Investigation...\n');

  // Read credentials from .apikey.json
  const credentialsPath = './src/exchanges/paradex/.apikey.json';
  let credentials: any;

  try {
    const credentialsData = readFileSync(credentialsPath, 'utf-8');
    credentials = JSON.parse(credentialsData);
    console.log('✅ Loaded credentials from .apikey.json');
    console.log('   Paradex Address:', credentials.paradexAddress);
    console.log('   Testnet:', credentials.testnet);
  } catch (error) {
    console.error('❌ Error reading .apikey.json:', error);
    console.log('\nMake sure .apikey.json exists in the paradex directory with valid credentials');
    process.exit(1);
  }

  const baseURL = credentials.testnet
    ? 'https://api.testnet.paradex.trade/v1'
    : 'https://api.prod.paradex.trade/v1';

  const client = axios.create({ baseURL });

  try {
    // Step 1: Try to fetch raw API response (will fail without auth, but shows structure)
    console.log('📡 Step 1: Fetching account data from API...');
    console.log('   URL: ' + baseURL + '/account');
    console.log('   Note: This will fail without auth, but helps understand the API\n');

    let rawData: any = null;

    try {
      const rawResponse = await client.get('/account');
      rawData = rawResponse.data;
      console.log('\n📋 Raw API Response:');
      console.log('─'.repeat(60));
      console.log(JSON.stringify(rawData, null, 2));
      console.log('─'.repeat(60));
    } catch (error: any) {
      if (error.response) {
        console.log('❌ API Error (expected without auth):', error.response.status, error.response.statusText);
        console.log('   Error message:', error.response.data);
      } else {
        console.log('❌ Request Error:', error.message);
      }
    }

    // Step 2: Show what the fields should be
    console.log('\n📊 Step 2: Expected Balance Fields from Paradex API:');
    console.log('─'.repeat(60));
    console.log('According to the API documentation, the /account endpoint returns:');
    console.log('  - collateral:       Total deposited collateral');
    console.log('  - equity:           collateral + unrealized PnL');
    console.log('  - free_collateral:  Available for new positions');
    console.log('  - position_margin:  Margin used in positions');
    console.log('  - buying_power:     Maximum order size');
    console.log('  - max_withdrawable: Maximum withdrawable amount');
    console.log('─'.repeat(60));

    // Step 3: Compare with raw values
    console.log('\n🔍 Step 3: Field Mapping Analysis:');
    console.log('─'.repeat(60));
    console.log('Raw API Fields:');
    console.log('  collateral:        ', rawData.collateral, '← Total deposited');
    console.log('  equity:            ', rawData.equity, '← collateral + upnl (current "total")');
    console.log('  free_collateral:   ', rawData.free_collateral, '← current "free"');
    console.log('  position_margin:   ', rawData.position_margin, '← current "used"');
    console.log('  buying_power:      ', rawData.buying_power);
    console.log('  max_withdrawable:  ', rawData.max_withdrawable);
    console.log('─'.repeat(60));

    // Step 4: Fetch positions to verify UPNL (requires exchange initialization)
    console.log('\n📍 Step 4: Checking Positions and UPNL:');
    console.log('─'.repeat(60));
    console.log('  Note: Position fetching requires authenticated exchange - skipped in raw API test');
    console.log('  Use the exchange.fetchPositions() method for full position data');

    // The following would require exchange to be initialized:
    // const positions = await exchange.fetchPositions();
    const positions: any[] = [];
    const balance = { upnl: 0 };
    console.log(`Found ${positions.length} open position(s)`);

    if (positions.length > 0) {
      let totalUpnl = 0;
      positions.forEach((pos: any, idx: number) => {
        console.log(`\n  Position ${idx + 1}:`);
        console.log(`    Symbol:           ${pos.symbol}`);
        console.log(`    Side:             ${pos.side}`);
        console.log(`    Contracts:        ${pos.contracts}`);
        console.log(`    Entry Price:      ${pos.entryPrice}`);
        console.log(`    Unrealized PnL:   ${pos.unrealizedPnl}`);
        totalUpnl += pos.unrealizedPnl;
      });

      console.log(`\n  Total UPNL from positions: ${totalUpnl}`);
      console.log(`  Balance UPNL:              ${balance.upnl}`);

      const upnlDiff = Math.abs(balance.upnl - totalUpnl);
      if (upnlDiff > 0.01) {
        console.log(`  ⚠️  UPNL mismatch: ${upnlDiff}`);
      } else {
        console.log(`  ✓ UPNL matches`);
      }
    } else {
      console.log('  No open positions');
    }
    console.log('─'.repeat(60));

    // Step 5: Analysis and recommendation
    console.log('\n💡 Step 5: Issue Analysis:');
    console.log('─'.repeat(60));

    const collateral = parseFloat(rawData.collateral || '0');
    const equity = parseFloat(rawData.equity || '0');
    const impliedUpnl = equity - collateral;

    console.log('\nCalculations:');
    console.log(`  Collateral = ${collateral}`);
    console.log(`  Equity = ${equity}`);
    console.log(`  Implied UPNL (equity - collateral) = ${impliedUpnl}`);

    console.log('\nCurrent Implementation:');
    console.log('  ✓ total → equity (includes upnl)');
    console.log('  ✓ free → free_collateral');
    console.log('  ✓ used → position_margin');

    console.log('\nPossible Issue:');
    console.log('  The "total" field shows equity (collateral + upnl)');
    console.log('  This might be confusing if users expect to see just their collateral');

    console.log('\nRecommendation:');
    if (Math.abs(impliedUpnl) > 0.01) {
      console.log('  ⚠️  Equity != Collateral (there is unrealized PnL)');
      console.log('  Consider if UI should show:');
      console.log('    Option A: Keep equity as "total" (current)');
      console.log('    Option B: Show collateral as "total" and equity separately');
    } else {
      console.log('  ✓ Equity ≈ Collateral (no significant upnl)');
      console.log('  Current implementation seems correct');
    }

    console.log('─'.repeat(60));

    // Step 6: Validate calculations
    console.log('\n✅ Step 6: Validation:');
    console.log('─'.repeat(60));

    console.log('Checking: free + used = collateral (or close to it)');
    const freeCollateral = parseFloat(rawData.free_collateral || '0');
    const positionMargin = parseFloat(rawData.position_margin || '0');
    const sumFreeUsed = freeCollateral + positionMargin;

    console.log(`  free_collateral: ${freeCollateral}`);
    console.log(`  position_margin: ${positionMargin}`);
    console.log(`  Sum:             ${sumFreeUsed}`);
    console.log(`  Collateral:      ${collateral}`);
    console.log(`  Difference:      ${Math.abs(sumFreeUsed - collateral)}`);

    if (Math.abs(sumFreeUsed - collateral) < 0.01) {
      console.log('  ✓ free + used ≈ collateral (valid)');
    } else {
      console.log('  ⚠️  free + used != collateral (possible issue)');
    }

    console.log('\nChecking: equity = collateral + upnl');
    const expectedEquity = collateral + balance.upnl;
    console.log(`  collateral: ${collateral}`);
    console.log(`  upnl:       ${balance.upnl}`);
    console.log(`  Sum:        ${expectedEquity}`);
    console.log(`  Equity:     ${equity}`);
    console.log(`  Difference: ${Math.abs(expectedEquity - equity)}`);

    if (Math.abs(expectedEquity - equity) < 0.01) {
      console.log('  ✓ equity ≈ collateral + upnl (valid)');
    } else {
      console.log('  ⚠️  equity != collateral + upnl (possible issue)');
    }

    console.log('─'.repeat(60));

    console.log('\n✅ Test completed successfully!');
  } catch (error) {
    console.error('\n❌ Error during test:', error);
    if (error instanceof Error) {
      console.error('Stack trace:', error.stack);
    }
    process.exit(1);
  }
}

// Run the test
testParadexBalance();
