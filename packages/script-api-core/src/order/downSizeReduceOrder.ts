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
import { assertNativeReduceAmount, nativeAmountRules, sameNativeAmount } from './nativeAmountRules';
import { venueUsesBaseAmountRails } from './orderPricing';
import { ScriptValidationError } from './ScriptValidationError';

// MEXC's named policy converts every contract size, including DOGE's 100 base units.
// Other venues retain their existing fractional-lot conversion policy.
const needsFractionalLotBaseConversion = (market: Market): boolean => {
  const contractSize = market.contractSize || 1;
  return !market.isInverse && contractSize > 0 && contractSize < 1;
};

/** A reduce-only order cut by the position size its venue holds for resting close orders, in base units. */
export type ReduceOrderLock = {
  symbol: string;
  requested: number;
  sent: number;
  total: number;
  locked: number;
  /** How many resting close orders hold it. */
  orders: number;
};

/** "X of your <symbol> position is reserved by N open close orders", in base units at the market's precision. */
export const reservedText = (
  lock: Pick<ReduceOrderLock, 'symbol' | 'locked' | 'total' | 'orders'>,
  precision: number,
): string =>
  // Close orders can outsize the position; never say more is reserved than is held.
  `${formatAmount(Math.min(lock.locked, lock.total), precision)} of your ${lock.symbol} position is reserved by ` +
  `${lock.orders} open close order${lock.orders === 1 ? '' : 's'}`;

/** Every unit of the position is held by resting close orders: nothing can be closed until they are cancelled. */
export class ReduceOrderLockedError extends Error {
  constructor(
    readonly lock: Omit<ReduceOrderLock, 'sent'>,
    precision: number,
  ) {
    super(`${reservedText(lock, precision)}; cancel them first`);
    this.name = 'ReduceOrderLockedError';
  }
}

export const formatAmount = (amount: number, precision: number): string => {
  const decimals = Math.min(12, Math.max(0, Math.ceil(-Math.log10(precision || DEFAULT_AMOUNT_PRECISION))));
  return String(Number(amount.toFixed(decimals)));
};

/** What a close order still holds: its unfilled part. */
const unfilled = (order: Order): number => Math.max(0, order.amount - (order.filled || 0));

export const downSizeReduceOrder = (
  market: Market,
  order: PlaceOrderOpts,
  positions: Position[],
  orders: readonly Order[],
  price: number,
  {
    exchangeName,
    locksPositionSize = false,
    onLocked,
  }: {
    exchangeName?: string;
    /** `reduceOrdersLockPositionSize`: resting close orders hold position size at the venue. */
    locksPositionSize?: boolean;
    /** Told when held size, not the position, cut the order. */
    onLocked?: (lock: ReduceOrderLock) => void;
  } = {},
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

  const rules = nativeAmountRules(market, exchangeName);
  if (order.amount !== 0) assertNativeReduceAmount(order.amount, market, exchangeName);
  const fractionalLots = rules.basePerContract !== undefined || needsFractionalLotBaseConversion(market);
  const contractSize = rules.basePerContract ?? (market.contractSize || 1);
  const nativeToBase = (nativeAmount: number): number => (fractionalLots ? nativeAmount * contractSize : nativeAmount);
  const baseToNative = (baseAmount: number): number =>
    fractionalLots && contractSize > 0 ? baseAmount / contractSize : baseAmount;

  const orderAmountBase = nativeToBase(order.amount);
  const precision = market?.precision?.amount || DEFAULT_AMOUNT_PRECISION;
  const resting = (o: Order) =>
    o.symbol === order.symbol &&
    o.side === order.side &&
    (o.status === OrderStatus.Open || o.status === OrderStatus.Creating);

  // Close orders the venue holds size for, at any price: reduce-only limits and sized plans.
  const holding = locksPositionSize ? orders.filter((o) => resting(o) && o.reduceOnly && unfilled(o) > 0) : [];
  const locked = holding.reduce((sum, o) => sum + unfilled(o), 0);
  const free = position.contracts - locked;
  if (holding.length > 0 && roundToNearestMultiple(Math.max(0, free), precision) <= 0) {
    throw new ReduceOrderLockedError(
      { symbol: order.symbol, requested: orderAmountBase, total: position.contracts, locked, orders: holding.length },
      precision,
    );
  }

  let reducedSize = Math.min(orderAmountBase, free);
  const ordersAhead = orders.filter(
    (o) =>
      resting(o) &&
      !holding.includes(o) &&
      o.type === OrderType.Limit &&
      (positionMode !== PositionMode.Hedged || o.reduceOnly) &&
      ((order.side === OrderSide.Buy && o.price >= price) || (order.side === OrderSide.Sell && o.price <= price)),
  );
  for (const o of ordersAhead) {
    reducedSize -= o.amount;
  }

  if (reducedSize <= 0) {
    return order;
  }

  const rounded = roundToNearestMultiple(reducedSize, precision);
  // Orders ahead that cover the position leave float residue (0.05 - 0.02 - 0.03 = 3.5e-18), which
  // rounds to 0. A zero amount is never an order: keep the order as placed, as when nothing is left.
  if (!(rounded > 0)) return order;
  const nativeAmount = baseToNative(rounded);
  if (venueUsesBaseAmountRails(exchangeName) && !sameNativeAmount(nativeAmount, order.amount)) {
    throw new ScriptValidationError(
      `MEXC reduce requested ${order.amount} native contracts but downsizing would send ${nativeAmount}; cancel conflicting orders or request the available quantity explicitly`,
    );
  }
  if (locked > 0 && free < orderAmountBase && rounded < orderAmountBase) {
    onLocked?.({
      symbol: order.symbol,
      requested: orderAmountBase,
      sent: rounded,
      total: position.contracts,
      locked,
      orders: holding.length,
    });
  }
  return {
    ...order,
    amount: nativeAmount,
  };
};
