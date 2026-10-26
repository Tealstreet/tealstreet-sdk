# Paradex Exchange TODO

## Pending Tasks

### 1. Fix cancelled order staying in UI until next fetchOrders
**Priority**: High

**Issue**: Canceled orders from WebSocket aren't immediately updating the UI, even though we're calling `addOrUpdateOrder()` with `Canceled` status. The orders only disappear after the next `fetchOrders()` poll.

**Current Implementation**:
- WebSocket handler in `paradex.ws-private.ts:157` calls `addOrUpdateOrder(update)` with `OrderStatus.Canceled`
- Status mapping is correct: `CANCELED` → `OrderStatus.Canceled`
- Info field is populated with raw order data

**Investigation Needed**:
- Check if WebSocket messages are arriving (verify logs show "Order X status updated to Canceled")
- Verify store's `addOrUpdateOrder()` triggers UI reactivity
- Consider if the 100ms throttle is causing issues
- Compare with Bybit implementation which uses `addOrUpdateOrders()` (plural)

### 2. Test short positions on Paradex
**Priority**: Medium

**What to Test**:
- Open a short position
- Verify position parsing from WebSocket and REST API
- Check that position side is correctly set to `PositionSide.Short`
- Verify PnL calculations for short positions
- Confirm position ID is created correctly without `:short` suffix (OneWay mode)

**Files to Monitor**:
- `paradex.ws-private.ts:190-244` - `handlePositionUpdate()`
- `paradex.exchange.ts:570-648` - `fetchPositions()`

### 3. Test TP/SL orders on Paradex
**Priority**: Medium

**What to Test**:
- Place orders with take profit (TAKE_PROFIT_LIMIT, TAKE_PROFIT_MARKET)
- Place orders with stop loss (STOP_LIMIT, STOP_MARKET)
- Verify order type mapping:
  - `TAKE_PROFIT_LIMIT` → `OrderType.TakeProfit`
  - `TAKE_PROFIT_MARKET` → `OrderType.TakeProfit`
  - `STOP_LIMIT` → `OrderType.StopLoss`
  - `STOP_MARKET` → `OrderType.StopLoss`
- Check trigger price handling
- Verify WebSocket updates for triggered orders

**Files to Check**:
- `paradex.types.ts:62-69` - Order type mappings
- `paradex.exchange.ts` - `reverseMapOrderType()` method
- Order placement with `trigger_price` field

## Completed Tasks

### ✅ Ping/Pong Keepalive
- Implemented legacy format ping/pong for private WebSocket
- 30-second ping interval
- Proper cleanup on disconnect

### ✅ Position Field Parsing
- Fixed field name mappings (`average_entry_price`, `realized_positional_pnl`, etc.)
- Added leverage and liquidation price parsing
- Populate `info` field with all raw data

### ✅ Position ID Duplicates
- Fixed OneWay mode to prevent `:long` suffix in position IDs
- Consistent position mode across WebSocket and REST API

### ✅ SetLeverage Implementation
- Corrected endpoint: `POST /account/margin/{market}`
- Added validation and margin type
- Leverage clamping to market limits

### ✅ UpdateOrder Cancel-and-Replace
- Implemented cancel-and-replace pattern
- Preserves order properties and applies updates

### ✅ Order Cancellation Endpoints
- Fixed to use DELETE method
- Corrected paths for all cancel operations

### ✅ Order Info Field
- Populate `info` field from WebSocket and REST API
- Store all raw order data for debugging

## Future Enhancements

### Low Priority
- [ ] Implement fills/trade history endpoint (`GET /fills`)
- [ ] Add funding payments history (`GET /funding-payments`)
- [ ] Transaction history (`GET /transactions`)
- [ ] Paraclear integration for on-chain operations
- [ ] Advanced order types (OCO, bracket orders)
