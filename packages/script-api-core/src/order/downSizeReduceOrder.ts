import { roundToNearestMultiple } from '@tealstreet/math/src/roundToNearestMultiple';
import {
  Market,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  type Order,
  type PlaceOrderOpts,
} from '@tealstreet/safe-cex/src/types';
import { DEFAULT_AMOUNT_PRECISION } from '@tealstreet/utils/src/currency/currencyUtils';

// The incoming `order.amount` is in the exchange-native unit that `createMarketOrder` /
// `createLimitOrder` will hand to `placeOrder`. For MEXC-style fractional-lot linear futures
// (contractSize < 1, e.g. BTC_USDT with contractSize=0.0001) `position.contracts` and
// the stored `order.amount` are base-asset (0.006 BTC), while the incoming native amount
// is 60 contracts. The pre-existing `Math.min(order.amount, position.contracts)` clamped
// 60 → 0.006 which MEXC's `placeOrder` then rounded down to `vol: 1` — the "close only
// closes 1 lot" bug. Do the clamp in base units and convert back at the end so both
// sides of the comparison are apples-to-apples. Non-fractional venues (contractSize >= 1)
// keep the previous behaviour because their `order.amount` and stored amounts already
// share a unit.
const needsFractionalLotBaseConversion = (market: Market): boolean => {
  const contractSize = market.contractSize || 1;
  return !market.isInverse && contractSize > 0 && contractSize < 1;
};

export const downSizeReduceOrder = (
  market: Market,
  order: PlaceOrderOpts,
  positions: Position[],
  orders: readonly Order[],
  price: number,
): PlaceOrderOpts => {
  if (!order.reduceOnly) return order;
  if (!price) return order;

  const positionSide = order.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
  const foundPositions = positions.filter(
    (p) => p.symbol === order.symbol && p.contracts > 0 && p.side === positionSide,
  );
  if (foundPositions.length === 0) return order;
  if (foundPositions.length > 1) {
    console.log('autoDownSizeReduceOrders: found multiple positions, using first');
  }
  const position = foundPositions[0];
  const positionMode = position.positionMode;

  const fractionalLots = needsFractionalLotBaseConversion(market);
  const contractSize = market.contractSize || 1;
  const nativeToBase = (nativeAmount: number): number =>
    fractionalLots ? nativeAmount * contractSize : nativeAmount;
  const baseToNative = (baseAmount: number): number =>
    fractionalLots && contractSize > 0 ? baseAmount / contractSize : baseAmount;

  const orderAmountBase = nativeToBase(order.amount);

  let reducedSize = Math.min(orderAmountBase, position.contracts);
  const ordersAhead = orders.filter(
    (o) =>
      o.symbol === order.symbol &&
      o.side === order.side &&
      o.type === OrderType.Limit &&
      (o.status === OrderStatus.Open || o.status === OrderStatus.Creating) &&
      (positionMode !== PositionMode.Hedged || o.reduceOnly) &&
      ((order.side === OrderSide.Buy && o.price >= price) || (order.side === OrderSide.Sell && o.price <= price)),
  );
  for (const o of ordersAhead) {
    reducedSize -= o.amount;
  }

  if (reducedSize <= 0) {
    return order;
  }

  const rounded = roundToNearestMultiple(reducedSize, market?.precision?.amount || DEFAULT_AMOUNT_PRECISION);
  return {
    ...order,
    amount: baseToNative(rounded),
  };
};
