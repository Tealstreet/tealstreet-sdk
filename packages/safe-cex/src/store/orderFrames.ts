import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Order } from '../generated/Order';
import type { Market } from '../types';
import type { Store } from './store.interface';

import { orderEnums } from '@tealstreet/safe-cex-wasm/wire';
import { OrderRejectReason } from '../generated/OrderRejectReason';
import { OrderSide } from '../generated/OrderSide';
import { OrderStatus } from '../generated/OrderStatus';
import { OrderStopTriggerType } from '../generated/OrderStopTriggerType';
import { OrderTimeInForce } from '../generated/OrderTimeInForce';
import { OrderType } from '../generated/OrderType';

const statuses = {
  open: OrderStatus.Open,
  closed: OrderStatus.Closed,
  canceled: OrderStatus.Canceled,
  creating: OrderStatus.Creating,
  updating: OrderStatus.Updating,
  canceling: OrderStatus.Canceling,
};
const types = {
  market: OrderType.Market,
  limit: OrderType.Limit,
  conditional: OrderType.Conditional,
  stop_market: OrderType.StopLoss,
  take_profit_market: OrderType.TakeProfit,
  trailing_stop_market: OrderType.TrailingStopLoss,
};
const sides = { buy: OrderSide.Buy, sell: OrderSide.Sell };
const times = {
  GoodTillCancel: OrderTimeInForce.GoodTillCancel,
  ImmediateOrCancel: OrderTimeInForce.ImmediateOrCancel,
  FillOrKill: OrderTimeInForce.FillOrKill,
  PostOnly: OrderTimeInForce.PostOnly,
};
const triggers = {
  mark_price: OrderStopTriggerType.MarkPrice,
  last_price: OrderStopTriggerType.LastPrice,
  index_price: OrderStopTriggerType.IndexPrice,
};
const rejects = {
  PostOnly: OrderRejectReason.PostOnly,
  InsufficientBalance: OrderRejectReason.InsufficientBalance,
  ReduceOnly: OrderRejectReason.ReduceOnly,
};

/** NaN is the packed optional-number sentinel; zero remains a reported value. */
const optional = (value: number | undefined): number | undefined => {
  if (value === undefined || Number.isNaN(value)) return undefined;
  if (!Number.isFinite(value)) throw new Error('invalid Order optional number');
  return value;
};

/** Decode native Order output without deriving quantities or trigger aliases. */
export function readOrderFrame(frame: Frame, store: Store, markets?: readonly Market[]): Order[] | undefined {
  const table = frame.orders();
  if (!table) return undefined;
  const rows: Order[] = [];
  for (let i = 0; i < table.count; i++) {
    const enums = orderEnums(table.enums[i]);
    if (enums.status === undefined || enums.type === undefined || enums.side === undefined)
      throw new Error('missing Order enum');
    const status = statuses[enums.status];
    const type = types[enums.type];
    const side = sides[enums.side];
    if (status === undefined || type === undefined || side === undefined) throw new Error('missing Order enum');
    const symbol = table.symbol?.(i) ?? (markets ?? store.markets)[table.market[i]]?.symbol;
    if (symbol === undefined) throw new Error('missing Order symbol');
    const remainingNative = optional(table.remainingNative?.[i]);
    const filledInPlacementUnits = optional(table.filledInPlacementUnits?.[i]);
    const remainingNotional = optional(table.remainingNotional?.[i]);
    const conditionalOrderPrice = optional(table.conditionalOrderPrice?.[i]);
    const rejectReason = enums.rejectReason === undefined ? undefined : rejects[enums.rejectReason];
    const extra = {
      ...(remainingNative !== undefined && { remainingNative }),
      ...(filledInPlacementUnits !== undefined && { filledInPlacementUnits }),
      ...(remainingNotional !== undefined && { remainingNotional }),
      ...(conditionalOrderPrice !== undefined && { conditionalOrderPrice }),
      ...(rejectReason !== undefined && { rejectReason }),
    };
    const clientOrderId = table.clientOrderId(i);
    const parentId = table.parentId(i);
    const trigger = enums.triggerType === undefined ? undefined : triggers[enums.triggerType];
    const tp = table.tpTriggerType?.(i);
    const sl = table.slTriggerType?.(i);
    const takeProfit = optional(table.takeProfit[i]);
    const stopLoss = optional(table.stopLoss[i]);
    rows.push({
      id: table.id(i),
      status,
      symbol,
      type,
      side,
      price: table.price[i],
      amount: table.amount[i],
      filled: table.filled[i],
      remaining: table.remaining[i],
      reduceOnly: enums.reduceOnly,
      lastUpdate: table.lastUpdate[i],
      ...(enums.close !== undefined && { close: enums.close }),
      ...(clientOrderId !== undefined && { clientOrderId }),
      ...(parentId !== undefined && { parentId }),
      ...(enums.timeInForce !== undefined && { timeInForce: times[enums.timeInForce] }),
      ...(trigger !== undefined && { triggerType: trigger }),
      ...(tp !== undefined && { tpTriggerType: triggers[tp] }),
      ...(sl !== undefined && { slTriggerType: triggers[sl] }),
      ...(takeProfit !== undefined && { takeProfit }),
      ...(stopLoss !== undefined && { stopLoss }),
      ...(Object.keys(extra).length > 0 && { extra }),
    });
  }
  return rows;
}
