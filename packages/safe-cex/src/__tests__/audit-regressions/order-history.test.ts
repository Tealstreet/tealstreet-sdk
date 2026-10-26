import type { Market } from '../../types';

import { describe, expect, it } from 'vitest';

import { mapBinanceHistoricalOrder } from '../reference/binance/binance.order-history';
import { mapBitunixHistoricalOrder } from '../reference/bitunix/bitunix.order-history';
import { mapBlofinHistoricalOrder } from '../reference/blofin/blofin.order-history';
import {
  collapseHyperliquidOrderHistory,
  mapHyperliquidHistoricalOrder,
} from '../reference/hyperliquid/hyperliquid.order-history';
import { mapOkxHistoricalOrder } from '../reference/okx/okx.order-history';
import { mapParadexHistoricalOrder } from '../reference/paradex/paradex.order-history';
import { mapWooHistoricalOrder } from '../reference/woo/woo.order-history';
import { normalizeSymbol } from '../reference/woo/woo.utils';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';

/** Shapes captured live: binance sheet row 2, blofin row 41. */
const binanceOrder = (over: Record<string, any> = {}) => ({
  orderId: 101205954323,
  symbol: 'DOGEUSDT',
  status: 'FILLED',
  price: '0.0717',
  avgPrice: '0.07169',
  origQty: '143',
  executedQty: '143',
  type: 'LIMIT',
  origType: 'LIMIT',
  side: 'BUY',
  positionSide: 'LONG',
  reduceOnly: false,
  time: 1786158465224,
  updateTime: 1786158466000,
  ...over,
});

const blofinOrder = (over: Record<string, any> = {}) => ({
  orderId: '1000164597347',
  instId: 'DOGE-USDT',
  side: 'buy',
  orderType: 'limit',
  price: '0.07169',
  size: '420',
  filledSize: '420',
  averagePrice: '0.07169',
  state: 'filled',
  positionSide: 'long',
  reduceOnly: 'false',
  createTime: '1786521873701',
  updateTime: '1786521923388',
  ...over,
});

const resolveBlofin = (instId: string) => (instId === 'DOGE-USDT' ? 'DOGEUSDT' : undefined);

describe('binance historical orders', () => {
  it('prefers origType, because a triggered stop rewrites type to MARKET', () => {
    const stop = mapBinanceHistoricalOrder(binanceOrder({ origType: 'STOP_MARKET', type: 'MARKET' }))!;
    expect(stop.type).toBe(OrderType.StopLoss);
  });

  it('drops "0" price and avgPrice rather than claiming a zero price', () => {
    const mapped = mapBinanceHistoricalOrder(binanceOrder({ price: '0', avgPrice: '0' }))!;
    expect(mapped.price).toBeUndefined();
    expect(mapped.averagePrice).toBeUndefined();
  });

  it('keeps orders that never filled — 81 of 329 on a live 7-day window', () => {
    const canceled = mapBinanceHistoricalOrder(binanceOrder({ status: 'CANCELED', executedQty: '0', avgPrice: '0' }))!;
    expect(canceled.status).toBe(OrderStatus.Canceled);
    expect(canceled.filled).toBe(0);
  });

  it('maps hedge position side', () => {
    expect(mapBinanceHistoricalOrder(binanceOrder({ positionSide: 'BOTH' }))!.positionSide).toBe('both');
    expect(mapBinanceHistoricalOrder(binanceOrder({ positionSide: 'SHORT' }))!.positionSide).toBe(PositionSide.Short);
  });

  it('drops rows with no id, symbol, time or valid side', () => {
    expect(mapBinanceHistoricalOrder(binanceOrder({ orderId: '' }))).toBeNull();
    expect(mapBinanceHistoricalOrder(binanceOrder({ time: 0 }))).toBeNull();
    expect(mapBinanceHistoricalOrder(binanceOrder({ side: 'HOLD' }))).toBeNull();
  });
});

describe('blofin historical orders', () => {
  it('reads LOWER-CASE state, which a shared capitalised table would miss', () => {
    // bybit's statuses are capitalised; running blofin through that table falls through to
    // the default and marks every order Closed, including live and cancelled ones.
    expect(mapBlofinHistoricalOrder(blofinOrder({ state: 'canceled' }), resolveBlofin)!.status).toBe(
      OrderStatus.Canceled,
    );
    expect(mapBlofinHistoricalOrder(blofinOrder({ state: 'live' }), resolveBlofin)!.status).toBe(OrderStatus.Open);
  });

  it('maps side and type from lower-case values', () => {
    const mapped = mapBlofinHistoricalOrder(blofinOrder({ side: 'sell', orderType: 'market' }), resolveBlofin)!;
    expect(mapped.side).toBe(OrderSide.Sell);
    expect(mapped.type).toBe(OrderType.Market);
  });

  it('treats positionSide "net" as both', () => {
    expect(mapBlofinHistoricalOrder(blofinOrder({ positionSide: 'net' }), resolveBlofin)!.positionSide).toBe('both');
  });

  it('handles reduceOnly arriving as the STRING "true"', () => {
    expect(mapBlofinHistoricalOrder(blofinOrder({ reduceOnly: 'true' }), resolveBlofin)!.reduceOnly).toBe(true);
    expect(mapBlofinHistoricalOrder(blofinOrder({ reduceOnly: 'false' }), resolveBlofin)!.reduceOnly).toBe(false);
  });

  it('falls back to the raw instId for an unknown market', () => {
    expect(mapBlofinHistoricalOrder(blofinOrder({ instId: 'NEW-USDT' }), resolveBlofin)!.symbol).toBe('NEW-USDT');
  });
});

/** OKX sheet row 13. One DOGE-USDT-SWAP contract is 1000 DOGE. */
const okxMarket = {
  symbol: 'DOGEUSDT',
  contractSize: 1000,
  isInverse: false,
  quote: 'USDT',
  info: { ctValCcy: 'DOGE' },
} as unknown as Market;

const okxToBase = (contracts: number, market: Market) => contracts * (market.contractSize || 1);

const okxOrder = (over: Record<string, any> = {}) => ({
  ordId: '3825929554199449600',
  instId: 'DOGE-USDT-SWAP',
  side: 'sell',
  ordType: 'market',
  px: '',
  avgPx: '0.07168',
  sz: '0.14',
  accFillSz: '0.14',
  reduceOnly: 'false',
  posSide: 'net',
  state: 'filled',
  cTime: '1786523986006',
  uTime: '1786523986007',
  ...over,
});

describe('okx historical orders', () => {
  it('converts CONTRACTS to base units', () => {
    // 0.14 contracts of DOGE-USDT-SWAP is 140 DOGE, not 0.14. The raw field at 0.07168
    // carries a 0.005 USDT fee — 0.05% of $10, which only reconciles after conversion.
    const mapped = mapOkxHistoricalOrder(okxOrder(), okxMarket, okxToBase)!;
    expect(mapped.amount).toBeCloseTo(140);
    expect(mapped.filled).toBeCloseTo(140);
  });

  it('maps terminal states, which the live-order table in okx.types.ts cannot', () => {
    // That table has only `live` and `partially_filled`; reusing it here sends every
    // cancellation through the default and reports it as a completed order.
    expect(mapOkxHistoricalOrder(okxOrder({ state: 'canceled' }), okxMarket, okxToBase)!.status).toBe(
      OrderStatus.Canceled,
    );
    expect(mapOkxHistoricalOrder(okxOrder({ state: 'mmp_canceled' }), okxMarket, okxToBase)!.status).toBe(
      OrderStatus.Canceled,
    );
    expect(mapOkxHistoricalOrder(okxOrder({ state: 'filled' }), okxMarket, okxToBase)!.status).toBe(OrderStatus.Closed);
    expect(mapOkxHistoricalOrder(okxOrder({ state: 'live' }), okxMarket, okxToBase)!.status).toBe(OrderStatus.Open);
  });

  it('keeps a cancelled order with zero fill', () => {
    const canceled = mapOkxHistoricalOrder(
      okxOrder({ state: 'canceled', accFillSz: '0', avgPx: '', px: '0.07' }),
      okxMarket,
      okxToBase,
    )!;
    expect(canceled.filled).toBe(0);
    expect(canceled.status).toBe(OrderStatus.Canceled);
    expect(canceled.price).toBeCloseTo(0.07);
  });

  it('drops "" price fields rather than reporting zero', () => {
    const mapped = mapOkxHistoricalOrder(okxOrder({ px: '', avgPx: '' }), okxMarket, okxToBase)!;
    expect(mapped.price).toBeUndefined();
    expect(mapped.averagePrice).toBeUndefined();
  });

  it('drops a row with no market, because the contract size is unknowable', () => {
    // Emitting the raw contract count as a base amount would be silently wrong by the
    // contract size — here a factor of 1000 — with nothing downstream able to notice.
    expect(mapOkxHistoricalOrder(okxOrder(), undefined, okxToBase)).toBeNull();
  });

  it('threads a reference price to toBase, preferring the fill price over the limit price', () => {
    // Inverse contracts are quote-denominated, so the conversion needs a price. Passing
    // none silently returns 0 there, which reads as a zero-size order.
    const seen: Array<number | undefined> = [];
    const spy = (contracts: number, market: Market, referencePrice?: number) => {
      seen.push(referencePrice);
      return contracts * (market.contractSize || 1);
    };

    mapOkxHistoricalOrder(okxOrder({ avgPx: '0.07168', px: '0.07' }), okxMarket, spy);
    expect(seen[0]).toBeCloseTo(0.07168);

    seen.length = 0;
    mapOkxHistoricalOrder(okxOrder({ avgPx: '', px: '0.07' }), okxMarket, spy);
    expect(seen[0]).toBeCloseTo(0.07);
  });

  it('maps hedge posSide and the string reduceOnly', () => {
    expect(mapOkxHistoricalOrder(okxOrder({ posSide: 'short' }), okxMarket, okxToBase)!.positionSide).toBe(
      PositionSide.Short,
    );
    expect(mapOkxHistoricalOrder(okxOrder({ posSide: 'net' }), okxMarket, okxToBase)!.positionSide).toBe('both');
    expect(mapOkxHistoricalOrder(okxOrder({ reduceOnly: 'true' }), okxMarket, okxToBase)!.reduceOnly).toBe(true);
  });
});

/** Paradex sheet row 81. */
const paradexOrder = (over: Record<string, any> = {}) => ({
  id: '1786522748690201709227960000',
  market: 'DOGE-USD-PERP',
  side: 'BUY',
  type: 'LIMIT',
  size: '165',
  remaining_size: '0',
  price: '0.0727',
  avg_fill_price: '0.0727',
  status: 'CLOSED',
  created_at: 1786522748701,
  last_updated_at: 1786522748776,
  cancel_reason: '',
  flags: [],
  ...over,
});

describe('paradex historical orders', () => {
  it('derives filled from size - remaining_size, because there is no filled field', () => {
    const partial = mapParadexHistoricalOrder(paradexOrder({ size: '165', remaining_size: '65' }))!;
    expect(partial.amount).toBe(165);
    expect(partial.filled).toBe(100);
  });

  it('does NOT trust status — every history row reads CLOSED', () => {
    // 100/100 rows on a live account came back CLOSED whether they filled or were
    // rejected. cancel_reason is the only thing separating the two.
    const rejected = mapParadexHistoricalOrder(
      paradexOrder({
        status: 'CLOSED',
        remaining_size: '165',
        avg_fill_price: '',
        cancel_reason: 'REDUCE_ONLY_WILL_INCREASE',
      }),
    )!;
    expect(rejected.status).toBe(OrderStatus.Canceled);
    expect(rejected.filled).toBe(0);

    const filled = mapParadexHistoricalOrder(paradexOrder())!;
    expect(filled.status).toBe(OrderStatus.Closed);
  });

  it('reads reduceOnly out of the flags array', () => {
    expect(mapParadexHistoricalOrder(paradexOrder({ flags: ['REDUCE_ONLY'] }))!.reduceOnly).toBe(true);
    expect(mapParadexHistoricalOrder(paradexOrder({ flags: [] }))!.reduceOnly).toBe(false);
  });

  it('classifies STOP_* by reduce-only, matching the live-order path', () => {
    // Paradex places OPENING trigger orders as non-reduce-only STOP_LIMIT. Going by type
    // alone reports every conditional Tealstreet placed as a protective stop-loss, and
    // disagrees with `reverseMapOrderType` on the same rows.
    expect(mapParadexHistoricalOrder(paradexOrder({ type: 'STOP_LIMIT', flags: [] }))!.type).toBe(
      OrderType.Conditional,
    );
    expect(mapParadexHistoricalOrder(paradexOrder({ type: 'STOP_MARKET', flags: [] }))!.type).toBe(
      OrderType.Conditional,
    );
    expect(mapParadexHistoricalOrder(paradexOrder({ type: 'STOP_MARKET', flags: ['REDUCE_ONLY'] }))!.type).toBe(
      OrderType.StopLoss,
    );
    // TAKE_PROFIT_* is protective either way.
    expect(mapParadexHistoricalOrder(paradexOrder({ type: 'TAKE_PROFIT_MARKET', flags: [] }))!.type).toBe(
      OrderType.TakeProfit,
    );
  });

  it('drops "" avg_fill_price and "0" price rather than reporting zero', () => {
    const mapped = mapParadexHistoricalOrder(paradexOrder({ price: '0', avg_fill_price: '' }))!;
    expect(mapped.price).toBeUndefined();
    expect(mapped.averagePrice).toBeUndefined();
  });

  it('drops rows with no id, market, time or valid side', () => {
    expect(mapParadexHistoricalOrder(paradexOrder({ id: '' }))).toBeNull();
    expect(mapParadexHistoricalOrder(paradexOrder({ created_at: 0 }))).toBeNull();
    expect(mapParadexHistoricalOrder(paradexOrder({ side: 'LONG' }))).toBeNull();
  });
});

/** Bitunix sheet row 56. */
const bitunixOrder = (over: Record<string, any> = {}) => ({
  orderId: '2087456606002233344',
  symbol: 'LINKUSDT',
  qty: '0.23',
  tradeQty: '0.23',
  price: 'MARKET',
  avgPrice: '8.767',
  side: 'BUY',
  orderType: 'MARKET',
  reduceOnly: true,
  status: 'FILLED',
  ctime: '1786523412601',
  mtime: '1786523412685',
  ...over,
});

describe('bitunix historical orders', () => {
  it('treats the literal string "MARKET" price as absent, not as NaN', () => {
    // `price` is the WORD on a market order, so parseFloat gives NaN and an unguarded
    // mapper publishes NaN into the axis.
    const mapped = mapBitunixHistoricalOrder(bitunixOrder())!;
    expect(mapped.price).toBeUndefined();
    expect(Number.isFinite(mapped.averagePrice!)).toBe(true);
  });

  it('reads filled from tradeQty, not qty', () => {
    const partial = mapBitunixHistoricalOrder(bitunixOrder({ qty: '10', tradeQty: '4' }))!;
    expect(partial.amount).toBe(10);
    expect(partial.filled).toBe(4);
  });

  it('maps the terminal statuses', () => {
    expect(mapBitunixHistoricalOrder(bitunixOrder({ status: 'CANCELED' }))!.status).toBe(OrderStatus.Canceled);
    expect(mapBitunixHistoricalOrder(bitunixOrder({ status: 'EXPIRED' }))!.status).toBe(OrderStatus.Canceled);
    expect(mapBitunixHistoricalOrder(bitunixOrder({ status: 'PART_FILLED' }))!.status).toBe(OrderStatus.Open);
    expect(mapBitunixHistoricalOrder(bitunixOrder({ status: 'FILLED' }))!.status).toBe(OrderStatus.Closed);
  });

  it('maps side and type', () => {
    const mapped = mapBitunixHistoricalOrder(bitunixOrder({ side: 'SELL', orderType: 'LIMIT', price: '8.5' }))!;
    expect(mapped.side).toBe(OrderSide.Sell);
    expect(mapped.type).toBe(OrderType.Limit);
    expect(mapped.price).toBe(8.5);
  });

  it('drops rows with no id, symbol, time or valid side', () => {
    expect(mapBitunixHistoricalOrder(bitunixOrder({ orderId: '' }))).toBeNull();
    expect(mapBitunixHistoricalOrder(bitunixOrder({ ctime: '0' }))).toBeNull();
    expect(mapBitunixHistoricalOrder(bitunixOrder({ side: 'OPEN' }))).toBeNull();
  });
});

/** WOO sheet row 28. Times are fractional SECONDS in a string. */
const wooOrder = (over: Record<string, any> = {}) => ({
  order_id: 82471774410,
  symbol: 'PERP_ETH_USDT',
  side: 'SELL',
  type: 'IOC',
  status: 'FILLED',
  price: 1900,
  quantity: 0.0052,
  executed: 0.0052,
  average_executed_price: 1900.1,
  reduce_only: true,
  position_side: 'LONG',
  created_time: '1786536593.515',
  updated_time: '1786536593.515',
  ...over,
});

/**
 * The REAL resolver shape, not a hand-rolled one.
 *
 * WOO's `Market.id` is `ETH/USDT:USDT` while this endpoint returns `PERP_ETH_USDT`, so the
 * obvious `store.marketsById[venueSymbol]` lookup never matches and every order silently
 * falls back to the raw venue string. An earlier version of this test hard-coded a
 * resolver that mapped `PERP_ETH_USDT -> ETHUSDT` and therefore asserted behaviour the
 * exchange did not have. Going through `normalizeSymbol` is what the exchange does.
 */
const wooMarketsBySymbol: Record<string, { symbol: string }> = { ETHUSDT: { symbol: 'ETHUSDT' } };
const resolveWoo = (venueSymbol: string) => wooMarketsBySymbol[normalizeSymbol(venueSymbol)]?.symbol;

describe('woo historical orders', () => {
  it('reads fractional SECONDS, not milliseconds', () => {
    // Every other venue in this axis reports integer ms. Passing WOO's value straight
    // through dates the whole account to January 1970, where a window filter then
    // discards all of it — an empty axis rather than a visibly wrong date.
    const mapped = mapWooHistoricalOrder(wooOrder(), resolveWoo)!;
    expect(mapped.createdTime).toBe(1786536593515);
    expect(mapped.updatedTime).toBe(1786536593515);
  });

  it('keeps orders that never filled — 169 of 617 on a live 30-day window', () => {
    const canceled = mapWooHistoricalOrder(
      wooOrder({ status: 'CANCELLED', executed: 0, average_executed_price: 0 }),
      resolveWoo,
    )!;
    expect(canceled.status).toBe(OrderStatus.Canceled);
    expect(canceled.filled).toBe(0);
    expect(canceled.averagePrice).toBeUndefined();
  });

  it('maps the TIF-shaped order types WOO reports as types', () => {
    // POST_ONLY and IOC are time-in-force everywhere else, but WOO puts them in `type`.
    expect(mapWooHistoricalOrder(wooOrder({ type: 'POST_ONLY' }), resolveWoo)!.type).toBe(OrderType.Limit);
    expect(mapWooHistoricalOrder(wooOrder({ type: 'IOC' }), resolveWoo)!.type).toBe(OrderType.Limit);
    expect(mapWooHistoricalOrder(wooOrder({ type: 'MARKET' }), resolveWoo)!.type).toBe(OrderType.Market);
  });

  it('normalises PERP_X_USDT to the safe-cex symbol, falling back to the raw venue string', () => {
    // A `marketsById[venueSymbol]` lookup would return undefined for EVERY row here and
    // emit `PERP_ETH_USDT` as the symbol — indistinguishable from a deliberate fallback.
    expect(mapWooHistoricalOrder(wooOrder(), resolveWoo)!.symbol).toBe('ETHUSDT');
    expect(mapWooHistoricalOrder(wooOrder({ symbol: 'PERP_NEW_USDT' }), resolveWoo)!.symbol).toBe('PERP_NEW_USDT');
  });

  it('maps hedge position side', () => {
    expect(mapWooHistoricalOrder(wooOrder({ position_side: 'SHORT' }), resolveWoo)!.positionSide).toBe(
      PositionSide.Short,
    );
    expect(mapWooHistoricalOrder(wooOrder({ position_side: 'BOTH' }), resolveWoo)!.positionSide).toBe('both');
  });

  it('drops rows with no id, symbol, time or valid side', () => {
    expect(mapWooHistoricalOrder(wooOrder({ order_id: '' }), resolveWoo)).toBeNull();
    expect(mapWooHistoricalOrder(wooOrder({ created_time: '0' }), resolveWoo)).toBeNull();
    expect(mapWooHistoricalOrder(wooOrder({ side: 'OPEN LONG' }), resolveWoo)).toBeNull();
  });
});

/** Hyperliquid sheet row 50. `historicalOrders` wraps the order under `order`. */
const hlRow = (over: Record<string, any> = {}, orderOver: Record<string, any> = {}) => ({
  order: {
    coin: 'DOGE',
    side: 'A',
    limitPx: '0.068087',
    sz: '170.0',
    oid: 515037792125,
    timestamp: 1786523095017,
    isTrigger: true,
    triggerPx: '0.069517',
    isPositionTpsl: false,
    reduceOnly: true,
    orderType: 'Stop Market',
    origSz: '170.0',
    tif: null,
    ...orderOver,
  },
  status: 'canceled',
  statusTimestamp: 1786523100177,
  ...over,
});

const resolveHl = (coin: string) => (coin === 'DOGE' ? 'DOGE-PERP' : undefined);

describe('hyperliquid historical orders', () => {
  it('collapses one row per status TRANSITION down to one row per order', () => {
    // A live account returned 2000 rows carrying only 1020 distinct oids — the surplus is
    // the `open` state of orders that later filled or cancelled.
    const rows = [
      hlRow({ status: 'open', statusTimestamp: 1786523095017 }),
      hlRow({ status: 'canceled', statusTimestamp: 1786523100177 }),
    ];
    const collapsed = collapseHyperliquidOrderHistory(rows);
    expect(collapsed).toHaveLength(1);
    expect(collapsed[0].status).toBe('canceled');
  });

  it('prefers the TERMINAL row when both rows share a statusTimestamp', () => {
    // An order that fills on placement carries the same statusTimestamp on both rows.
    // Breaking the tie by array position reported 179 filled orders as still open, and
    // the axis came back with ZERO filled out of 490 — while ids and counts all matched,
    // so the conformance check passed. Both orderings must resolve to `filled`.
    const open = hlRow({ status: 'open', statusTimestamp: 1786522506093 });
    const filled = hlRow({ status: 'filled', statusTimestamp: 1786522506093 }, { sz: '0.0' });

    expect(collapseHyperliquidOrderHistory([open, filled])[0].status).toBe('filled');
    expect(collapseHyperliquidOrderHistory([filled, open])[0].status).toBe('filled');
  });

  it('reads filled as origSz - sz, because sz is REMAINING', () => {
    const done = mapHyperliquidHistoricalOrder(hlRow({ status: 'filled' }, { sz: '0.0', origSz: '170.0' }), resolveHl)!;
    expect(done.amount).toBe(170);
    expect(done.filled).toBe(170);
    expect(done.status).toBe(OrderStatus.Closed);

    const untouched = mapHyperliquidHistoricalOrder(hlRow({}, { sz: '170.0', origSz: '170.0' }), resolveHl)!;
    expect(untouched.filled).toBe(0);
  });

  it('treats every unrecognised status as terminal-not-filled', () => {
    // The venue publishes a long and growing list of rejection statuses; this account
    // alone produced five. Only the two live states are named.
    for (const status of ['reduceOnlyCanceled', 'badAloPxRejected', 'minTradeNtlRejected', 'somethingNewRejected']) {
      expect(mapHyperliquidHistoricalOrder(hlRow({ status }), resolveHl)!.status).toBe(OrderStatus.Canceled);
    }
    expect(mapHyperliquidHistoricalOrder(hlRow({ status: 'open' }), resolveHl)!.status).toBe(OrderStatus.Open);
  });

  it('maps A/B to sell/buy', () => {
    expect(mapHyperliquidHistoricalOrder(hlRow({}, { side: 'A' }), resolveHl)!.side).toBe(OrderSide.Sell);
    expect(mapHyperliquidHistoricalOrder(hlRow({}, { side: 'B' }), resolveHl)!.side).toBe(OrderSide.Buy);
    expect(mapHyperliquidHistoricalOrder(hlRow({}, { side: 'X' }), resolveHl)).toBeNull();
  });

  it('resolves the coin to a symbol, falling back to the raw coin', () => {
    expect(mapHyperliquidHistoricalOrder(hlRow(), resolveHl)!.symbol).toBe('DOGE-PERP');
    // An unresolvable market — a delisted asset, or a HIP-3 dex absent from this store —
    // must not erase the order.
    expect(mapHyperliquidHistoricalOrder(hlRow({}, { coin: 'vntls:VKRAKEN' }), resolveHl)!.symbol).toBe(
      'vntls:VKRAKEN',
    );
  });

  it('reports the resting price, not the trigger price', () => {
    const mapped = mapHyperliquidHistoricalOrder(hlRow(), resolveHl)!;
    expect(mapped.price).toBeCloseTo(0.068087);
    expect(mapped.type).toBe(OrderType.StopLoss);
  });
});
