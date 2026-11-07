import type { HistoricalOrder, Market } from '../../../types';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../../types';

/**
 * OKX `/api/v5/trade/orders-history` row -> `HistoricalOrder`.
 *
 * Shape verified live (sheet row 13): `ordId, instId, side, ordType, px, avgPx, sz,
 * accFillSz, reduceOnly, posSide, state, cTime, uTime, fee, pnl, cancelSource`.
 *
 * Two things here are load-bearing:
 *
 * **`sz` / `accFillSz` are CONTRACTS, not base units.** A DOGE-USDT-SWAP row reading
 * `sz: "0.14"` at `avgPx: 0.07168` carries a `fee` of 0.005 USDT — 0.05% of $10, not of
 * $0.01 — because one contract is 1000 DOGE. Emitting the raw field would put this axis
 * four orders of magnitude away from bybit/binance/blofin, which report base units, and
 * nothing downstream would flag it. Hence the required `toBase` argument.
 *
 * **The status table is history-specific and must stay that way.** `ORDER_STATUS` in
 * `okx.types.ts` maps only `live` and `partially_filled`, because the live order feed
 * never sees a terminal state. Reusing it here would send `filled`, `canceled` and
 * `mmp_canceled` through the caller's default and report every cancellation as a fill —
 * the exact population this axis exists to preserve.
 */

const ORDER_TYPE: Record<string, OrderType> = {
  market: OrderType.Market,
  limit: OrderType.Limit,
  post_only: OrderType.Limit,
  fok: OrderType.Limit,
  ioc: OrderType.Limit,
  optimal_limit_ioc: OrderType.Market,
  mmp: OrderType.Limit,
  mmp_and_post_only: OrderType.Limit,
};

const ORDER_STATUS: Record<string, OrderStatus> = {
  live: OrderStatus.Open,
  partially_filled: OrderStatus.Open,
  filled: OrderStatus.Closed,
  canceled: OrderStatus.Canceled,
  mmp_canceled: OrderStatus.Canceled,
};

/** OKX sends `""` for an unset numeric field, and `parseFloat('')` is NaN. */
const toOptionalNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) && parsed !== 0 ? parsed : undefined;
};

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapOkxHistoricalOrder = (
  row: Record<string, any>,
  market: Market | undefined,
  toBase: (contractsNative: number, market: Market, referencePrice?: number) => number,
): HistoricalOrder | null => {
  const id = String(row.ordId ?? '');
  const instId = String(row.instId ?? '');
  const createdTime = parseInt(String(row.cTime ?? '0'), 10);
  // Without the market there is no contract size, and a contract count emitted as a base
  // amount is silently wrong rather than visibly missing. Drop the row instead.
  if (!id || !instId || !createdTime || !market) {
    return null;
  }

  const side = String(row.side ?? '').toLowerCase();
  if (side !== 'buy' && side !== 'sell') {
    return null;
  }

  const price = toOptionalNumber(row.px);
  const averagePrice = toOptionalNumber(row.avgPx);
  // Inverse contracts are quote-denominated, so the conversion needs a price; the fill
  // price is the truthful one and the limit price is the only fallback available.
  const referencePrice = averagePrice ?? price;

  const posSide = String(row.posSide ?? '').toLowerCase();

  return {
    id,
    symbol: market.symbol,
    side: side === 'buy' ? OrderSide.Buy : OrderSide.Sell,
    type: ORDER_TYPE[String(row.ordType ?? '').toLowerCase()] ?? OrderType.Limit,
    status: ORDER_STATUS[String(row.state ?? '').toLowerCase()] ?? OrderStatus.Closed,
    price,
    averagePrice,
    amount: toBase(toNumber(row.sz), market, referencePrice),
    filled: toBase(toNumber(row.accFillSz), market, referencePrice),
    reduceOnly: String(row.reduceOnly ?? '').toLowerCase() === 'true' || row.reduceOnly === true,
    positionSide: posSide === 'long' ? PositionSide.Long : posSide === 'short' ? PositionSide.Short : 'both',
    createdTime,
    updatedTime: parseInt(String(row.uTime ?? '0'), 10) || undefined,
  };
};
