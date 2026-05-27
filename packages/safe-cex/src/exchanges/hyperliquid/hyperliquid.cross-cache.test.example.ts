/**
 * Example usage of HyperliquidCrossCache
 * This file demonstrates the API and can be used as a reference for testing
 */
/* eslint-disable @typescript-eslint/no-unused-vars -- example/reference functions are intentionally unused */

import { OrderSide, OrderType } from '../../types';
import { getCrossCache } from './hyperliquid.cross-cache';

// ============================================================================
// EXAMPLE 1: Pre-placement Cache (Order Placement Flow)
// ============================================================================

function examplePrePlacementCache() {
  const cache = getCrossCache();

  // Before placing order via REST API, cache the order details
  // This allows WS handler to identify the order type when it arrives
  cache.addPrePlacement(
    'VNTLS:VKRAKEN-PERP', // coin (HIP-3 format)
    OrderSide.Buy, // side
    OrderType.Limit, // type
    100.5, // price
    10, // amount
  );

  // ... REST API call happens ...

  // Later, when WS receives order update without type info:
  // Pass WS data (limitPx, origSz) for disambiguation when multiple orders exist
  const wsLimitPx = 100.5; // from orderData.limitPx
  const wsAmount = 10; // from orderData.origSz
  const match = cache.matchPrePlacement('VNTLS:VKRAKEN-PERP', OrderSide.Buy, wsLimitPx, wsAmount);

  if (match) {
    console.log('Matched order type:', match.type); // OrderType.Limit
    console.log('Matched order price:', match.price); // 100.5
    // Note: matchPrePlacement() removes entry from cache after match
  }
}

// ============================================================================
// EXAMPLE 2: Pending Orders Cache (After REST Returns)
// ============================================================================

function examplePendingOrdersCache() {
  const cache = getCrossCache();

  // After REST API returns order ID, cache it with type/price
  // This is a definitive cache keyed by order ID
  cache.addPendingOrder(
    '12345', // orderId from REST response
    OrderType.TakeProfit,
    105.0, // trigger price
  );

  // Later, when WS confirms order (may not include type for HIP-3):
  const pending = cache.getPendingOrder('12345');

  if (pending) {
    console.log('Order type:', pending.type); // OrderType.TakeProfit
    console.log('Order price:', pending.price); // 105.0
  }

  // After order is confirmed and added to store, remove from cache
  cache.removePendingOrder('12345');
}

// ============================================================================
// EXAMPLE 3: Fill Deduplication (Cross-Tab Sync)
// ============================================================================

function exampleFillDeduplication() {
  const cache = getCrossCache();

  // When receiving fill notification from WS
  const fillId = 'tx_abc123';

  // Check if already seen (prevents duplicate toast notifications)
  if (cache.hasSeenFill(fillId)) {
    console.log('Fill already processed, skipping');
    return;
  }

  // Mark as seen (broadcasts to other tabs/workers)
  cache.markFillSeen(fillId);

  // Now process the fill...
  console.log('Processing new fill:', fillId);

  // Other browser tabs will automatically receive this via BroadcastChannel
  // and won't show duplicate notifications
}

// ============================================================================
// EXAMPLE 4: Cross-Tab Synchronization
// ============================================================================

function exampleCrossTabSync() {
  // Tab 1: Place order
  const cache1 = getCrossCache(); // Same singleton instance
  cache1.addPrePlacement('BTC-PERP', OrderSide.Buy, OrderType.Limit, 50000, 1);

  // Tab 2: Order arrives via WS (different browser tab, same session)
  const cache2 = getCrossCache(); // Same singleton instance
  const match = cache2.matchPrePlacement('BTC-PERP', OrderSide.Buy);

  // match will be found! BroadcastChannel synced the data between tabs
  console.log('Found in tab 2:', match); // { type: OrderType.Limit, price: 50000 }
}

// ============================================================================
// EXAMPLE 5: Worker Context (Fallback to In-Memory)
// ============================================================================

function exampleWorkerContext() {
  // In web worker: sessionStorage is not available
  // Cache automatically uses in-memory fallback
  const cache = getCrossCache();

  // This works, but uses in-memory Map instead of sessionStorage
  cache.addPendingOrder('67890', OrderType.StopLoss, 95.0);

  // Worker will receive broadcasts from main thread via BroadcastChannel
  // and populate its in-memory cache
}

// ============================================================================
// EXAMPLE 6: Cleanup (Application Shutdown)
// ============================================================================

function exampleCleanup() {
  const cache = getCrossCache();

  // When application is shutting down, close BroadcastChannel
  cache.dispose();

  // Note: sessionStorage persists until browser tab is closed
  // On next page load, stale entries are automatically cleared
}

// ============================================================================
// USAGE PATTERNS IN HYPERLIQUID EXCHANGE
// ============================================================================

/*
// In hyperliquid.exchange.ts:

class HyperliquidExchange {
  // Before placing HIP-3 order
  async placeOrder(order: PlaceOrderOpts) {
    // Cache before REST call
    this.addToPrePlacementCache(coin, side, type, price, amount);
    
    // REST API call
    const response = await this.xhr.post('/exchange', { ... });
    
    // After REST returns order ID
    this.addPendingHip3Order(response.oid, type, price);
  }
  
  // Delegates to cross-cache
  addToPrePlacementCache(coin, side, type, price, amount) {
    getCrossCache().addPrePlacement(coin, side, type, price, amount);
  }
  
  addPendingHip3Order(orderId, type, price) {
    getCrossCache().addPendingOrder(orderId, type, price);
  }
}
*/

/*
// In hyperliquid.ws-private.ts:

handleFills(fills: Fill[]) {
  for (const fill of fills) {
    const fillId = `${fill.tid}`;
    
    // Deduplicate fills across all instances
    if (hasSeenFill(fillId)) continue;
    markFillSeen(fillId);
    
    // Emit fill event
    this.parent.emitter.emit('fill', { id: fillId, ... });
  }
}
*/
