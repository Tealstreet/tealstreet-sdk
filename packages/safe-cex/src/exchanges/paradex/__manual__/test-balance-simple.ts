#!/usr/bin/env tsx
/**
 * Simple test to show Paradex balance field mapping issue
 */

console.log('🔍 Paradex Balance Display Issue Analysis\n');
console.log('='.repeat(70));

console.log('\n📋 Current Implementation (paradex.exchange.ts lines 296-301):');
console.log('─'.repeat(70));
console.log(`
  return {
    total: parseFloat(data.equity || '0'),           // ← Issue here!
    free: parseFloat(data.free_collateral || '0'),
    used: parseFloat(data.position_margin || '0'),
    upnl: 0, // Will be calculated from positions
  };
`);

console.log('\n📊 Paradex API /account Endpoint Fields:');
console.log('─'.repeat(70));
console.log(`
  From paradex.types.ts (ACCOUNT_RESPONSE):

  - collateral:       string;  ← Total deposited collateral (wallet balance)
  - equity:           string;  ← collateral + unrealized PnL
  - free_collateral:  string;  ← Available for new positions
  - position_margin:  string;  ← Margin used in open positions
  - buying_power:     string;  ← Maximum order size
  - max_withdrawable: string;  ← Maximum withdrawable amount
`);

console.log('\n❌ The Problem:');
console.log('─'.repeat(70));
console.log(`
  Current mapping uses "equity" for "total":

  total = equity = collateral + upnl

  This means:
  - If you have $1000 deposited and +$50 upnl → shows $1050 total
  - If you have $1000 deposited and -$50 upnl → shows $950 total

  Users might expect to see their actual deposited collateral ($1000)
  as the "total" balance, not the fluctuating equity value.
`);

console.log('\n💡 The Solution:');
console.log('─'.repeat(70));
console.log(`
  Option 1: Show collateral as "total" (recommended)
  ═══════════════════════════════════════════════════
  return {
    total: parseFloat(data.collateral || '0'),       // ← Use collateral
    free: parseFloat(data.free_collateral || '0'),
    used: parseFloat(data.position_margin || '0'),
    upnl: 0, // Will be calculated from positions
  };

  This shows the actual deposited funds, not affected by unrealized PnL.
  The UI can then display: Total = Collateral, Equity = Collateral + UPNL


  Option 2: Keep equity but clarify in UI
  ═══════════════════════════════════════
  Keep current implementation but make sure the UI clearly indicates
  that "total" includes unrealized PnL.
`);

console.log('\n✅ Recommendation:');
console.log('─'.repeat(70));
console.log(`
  Use Option 1: Change "total" to use "collateral" field

  Reasoning:
  - More intuitive for users (shows actual funds deposited)
  - Consistent with how other exchanges show wallet balance
  - UPNL is shown separately, so no information is lost
  - Equity can be calculated as: total + upnl
`);

console.log('\n📝 Implementation:');
console.log('─'.repeat(70));
console.log(`
  File: packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts
  Line: 297

  Change from:
    total: parseFloat(data.equity || '0'),

  To:
    total: parseFloat(data.collateral || '0'),
`);

console.log('\n' + '='.repeat(70));
console.log('✅ Analysis complete!\n');
