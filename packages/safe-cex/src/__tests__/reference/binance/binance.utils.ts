import type { Store } from '../../../store/store.interface';
import type { HistoricalTrade, HistoricalTradeOptions, Order } from '../../../types';

import { OrderStatus } from '../../../types';

export const USER_TRADES_MAX_LIMIT = 1000;

/**
 * `/fapi/v1/userTrades` REJECTS a wider window outright rather than clamping it, and every
 * caller wraps the request in a catch that returns `[]` — so an unchunked ask reads as
 * "no history" instead of "request failed". Split with `splitTimeRange` before sending.
 */
export const USER_TRADES_MAX_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * Query params for `GET /fapi/v1/userTrades`.
 *
 * Every `HistoricalTradeOptions` field used to be discarded — the request was
 * hardcoded to `{ symbol, limit: 1000 }` — so a caller asking for one order,
 * one hour, or five rows silently received the account's most recent 1000
 * fills for that symbol instead.
 *
 * Binance rejects `fromId` combined with `startTime`/`endTime`, so a time
 * window wins and `fromId` is only sent when neither bound is present.
 */
export const buildUserTradesParams = (opts: HistoricalTradeOptions): Record<string, string | number> => {
  const params: Record<string, string | number> = {
    symbol: opts.symbol,
    limit: Math.min(Math.max(opts.limit ?? USER_TRADES_MAX_LIMIT, 1), USER_TRADES_MAX_LIMIT),
  };

  if (opts.orderId !== undefined && opts.orderId !== null && `${opts.orderId}` !== '') {
    params.orderId = `${opts.orderId}`;
  }

  const hasWindow = opts.from !== undefined || opts.to !== undefined;
  if (opts.from !== undefined) params.startTime = opts.from;
  if (opts.to !== undefined) params.endTime = opts.to;
  if (!hasWindow && opts.fromId !== undefined) params.fromId = opts.fromId;

  return params;
};

/**
 * `quoteQty` was passed through untouched while every sibling numeric field
 * was coerced with `+`. Binance returns it as a decimal string, so the value
 * violated `HistoricalTrade.quoteQty: number` on every row.
 */
export const mapBinanceUserTrade = (entry: Record<string, any>): HistoricalTrade => ({
  id: entry.id?.toString(),
  symbol: entry.symbol,
  orderId: entry.orderId?.toString(),
  price: +entry.price,
  qty: +entry.qty,
  quoteQty: entry.quoteQty === undefined || entry.quoteQty === null ? undefined : +entry.quoteQty,
  realizedPnl: +entry.realizedPnl,
  positionSide: `${entry.positionSide ?? 'BOTH'}`.toLocaleLowerCase() as HistoricalTrade['positionSide'],
  side: `${entry.side ?? ''}`.toLocaleLowerCase() as HistoricalTrade['side'],
  buyer: entry.buyer,
  maker: entry.maker,
  commission: +entry.commission,
  commissionAsset: entry.commissionAsset,
  time: +entry.time,
});

/** A fill arriving before the cancellation acknowledgement keeps its terminal status. */
export const markBinanceOrdersCanceled = (store: Store, requested: Array<Pick<Order, 'id'>>) => {
  const ids = new Set(requested.map((order) => order.id));
  store.updateOrdersStatus(
    store.orders.filter(
      (order) => ids.has(order.id) && order.status !== OrderStatus.Closed && order.status !== OrderStatus.Canceled,
    ),
    OrderStatus.Canceled,
  );
};
