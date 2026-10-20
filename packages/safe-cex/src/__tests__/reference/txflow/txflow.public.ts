import type { Candle, Market, Ticker } from '../../../types';
import type { TxFlowCandle, TxFlowContext, TxFlowInstrument, TxFlowTicker } from './txflow.types';

export const txflowNumber = (value: unknown): number => {
  const n = value === '' || value == null ? 0 : Number(value);
  if (!Number.isFinite(n)) throw new Error('Invalid TxFlow numeric value');
  return n;
};

export const mapTxFlowMarket = (row: TxFlowInstrument): Market | undefined => {
  if (row.currencyType !== 'usdc' || row.delisted) return;
  const amount = txflowNumber(row.basePrecision);
  const price = txflowNumber(row.priceTick);
  if (amount <= 0 || price <= 0 || row.maxLeverage < 1) return;
  return {
    id: String(row.index), symbol: row.name, base: row.baseCurrency, quote: row.quoteCurrency,
    active: !row.haltTrading, precision: { amount, price },
    limits: { amount: { min: amount, max: row.maxLimitOrderSize ?? Number.MAX_SAFE_INTEGER }, leverage: { min: 1, max: row.maxLeverage } },
    contractSize: 1, isInverse: false, isSpot: false, isPerpetual: true, info: row,
  };
};

export const emptyTxFlowTicker = (market: Market): Ticker => ({
  id: market.id, symbol: market.symbol, bid: 0, ask: 0, last: 0, mark: 0, index: 0,
  percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0,
});

export const mapTxFlowContext = (ticker: Ticker, ctx: TxFlowContext): Ticker => {
  const mark = txflowNumber(ctx.markPx);
  const mid = txflowNumber(ctx.midPx);
  const last = txflowNumber(ctx.lastPrice) || ticker.last;
  const previous = txflowNumber(ctx.prevDayPx);
  return {
    ...ticker, mark, index: txflowNumber(ctx.oraclePx), last,
    ...(mid > 0 ? { midPrice: mid } : {}),
    openInterest: txflowNumber(ctx.openInterest) * mark,
    fundingRate: txflowNumber(ctx.funding) / 100,
    fundingTime: ctx.nextFundingTime ?? 0,
    percentage: previous > 0 ? ((last - previous) / previous) * 100 : ticker.percentage,
    volume: txflowNumber(ctx.dayBaseVlm), quoteVolume: txflowNumber(ctx.dayNtlVlm),
  };
};

export const mapTxFlowTicker = (ticker: Ticker, row: TxFlowTicker, seed = false): Ticker => {
  if (String(row.instrumentId) !== ticker.id) throw new Error('Mismatched TxFlow ticker instrument');
  const last = txflowNumber(row.lastPrice);
  const previous = txflowNumber(row.prevDayPx);
  const mapped = {
    ...ticker, last, percentage: previous > 0 ? ((last - previous) / previous) * 100 : 0,
    volume: txflowNumber(row.coinVlm), quoteVolume: txflowNumber(row.dayNtlVlm),
    ...(seed ? { mark: txflowNumber(row.markPx), index: txflowNumber(row.oraclePx), openInterest: txflowNumber(row.openInterest) * txflowNumber(row.markPx) } : {}),
  };
  return row.nodeCtx ? mapTxFlowContext(mapped, row.nodeCtx) : mapped;
};

export const mapTxFlowCandle = (row: TxFlowCandle): Candle => ({
  timestamp: row.t / 1000, open: txflowNumber(row.o), high: txflowNumber(row.h),
  low: txflowNumber(row.l), close: txflowNumber(row.c), volume: txflowNumber(row.v),
});
