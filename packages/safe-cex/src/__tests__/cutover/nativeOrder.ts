import type { Order } from '../../generated/Order';

import { OrderSide } from '../../generated/OrderSide';
import { OrderStatus } from '../../generated/OrderStatus';
import { OrderType } from '../../generated/OrderType';

/** Complete generated order facts for tests focused on one amendment field. */
export function nativeOrder(row: Pick<Order, 'id' | 'symbol'> & Partial<Order>): Order {
  return {
    status: OrderStatus.Open,
    type: OrderType.Limit,
    side: OrderSide.Buy,
    price: 0,
    amount: 0,
    filled: 0,
    remaining: 0,
    reduceOnly: false,
    lastUpdate: 0,
    ...row,
  };
}
