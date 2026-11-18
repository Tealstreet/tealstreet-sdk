import type { ExchangeMetadata } from './exchangeMetadata';
import type { Order } from './types';

import { OrderStatus, OrderType } from './types';

/** A UI eligibility check; native reconciliation and dispatch remain engine-owned. */
export const getOrderAmendmentError = (
  metadata: Pick<ExchangeMetadata, 'orderAmendment' | 'prettyName'> | undefined,
  order: Order,
  fields: { price?: unknown; amount?: unknown },
): string | undefined => {
  const capability = metadata?.orderAmendment;
  if (!capability) return undefined;
  const venue = metadata.prettyName;
  if (order.status !== OrderStatus.Open || (capability.unfilledOnly && order.filled !== 0)) {
    return `${venue} can amend only open, unfilled orders`;
  }
  const price = fields.price !== undefined;
  const amount = fields.amount !== undefined;
  if (price === amount) return `${venue} changes an order's price or size, not both at once`;
  if (capability.triggerPriceTypes.includes(order.type)) {
    return amount ? `${venue} supports only price changes for stop-loss and take-profit orders` : undefined;
  }
  if (
    order.type !== OrderType.Limit ||
    order.close === true ||
    !order.timeInForce ||
    !capability.limitTimeInForce.includes(order.timeInForce)
  ) {
    return `${venue} amends only an open GTC or PostOnly LIMIT order`;
  }
  if (
    price &&
    capability.preserveAttachedLimitProtection &&
    (order.stopLoss !== undefined || order.takeProfit !== undefined)
  ) {
    return `${venue} cannot move this limit order's price without dropping its TP/SL. Cancel it and place it again with its TP/SL, or change its size instead`;
  }
  return undefined;
};
