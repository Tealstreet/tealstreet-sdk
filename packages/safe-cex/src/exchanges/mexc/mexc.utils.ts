import type { Balance, Candle, Execution, Market, Order, OrderBook, Position, Ticker } from '../../types';

import { parsePrecision } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import { createPositionId } from '../../utils/createPositionId';
import { omitUndefined } from '../../utils/omitUndefined';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { parseBinanceSymbol } from '../../utils/symbolConverter';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';
import {
  MARGIN_TYPE,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TIME_IN_FORCE,
  ORDER_TYPE,
  POSITION_MODE,
  POSITION_SIDE,
} from './mexc.types';

export const toNumber = (value: unknown, fallback = 0): number => {
  if (value === undefined || value === null || value === '') return fallback;
  const parsed = Number.parseFloat(String(value));
  return Number.isFinite(parsed) ? parsed : fallback;
};

export const toIntegerString = (value: unknown): string => String(value ?? '');

export const toMexcSymbol = (symbol: string): string => {
  if (symbol.includes('_')) return symbol;
  const { base, quote } = parseBinanceSymbol(symbol);
  return `${base}_${quote}`;
};

export const fromMexcSymbol = (symbol: unknown): string => String(symbol ?? '').replace(/_/g, '');

export const normalizeTimestampToMs = (value: unknown): number => {
  if (value === undefined || value === null || value === '') return 0;

  const numeric = Number(value);
  if (Number.isFinite(numeric) && numeric > 0) {
    if (numeric >= 1_000_000_000_000) return Math.floor(numeric);
    return Math.floor(numeric * 1000);
  }

  const parsedDate = Date.parse(String(value));
  return Number.isFinite(parsedDate) ? parsedDate : 0;
};

export const normalizeTimestampToSeconds = (value: unknown): number => Math.floor(normalizeTimestampToMs(value) / 1000);

const stepFromScale = (scale: unknown, fallback: number): number => {
  const numeric = Number(scale);
  if (Number.isInteger(numeric) && numeric >= 0) {
    return toNumber(parsePrecision(`${numeric}`), fallback);
  }
  return fallback;
};

export const getContractSize = (market?: Market): number => {
  const contractSize = toNumber(market?.contractSize, 1);
  return contractSize > 0 ? contractSize : 1;
};

export const nativeVolumeToAmount = (value: unknown, market?: Market): number => toNumber(value) * getContractSize(market);

export const amountToNativeVolume = (amount: number, market?: Market): number => amount / getContractSize(market);

export const normalizeMarket = (raw: Record<string, any>): Market => {
  const id = String(raw.symbol);
  const base = String(raw.baseCoin ?? id.split('_')[0] ?? '');
  const quote = String(raw.quoteCoin ?? raw.settleCoin ?? id.split('_')[1] ?? 'USDT');
  const contractSize = toNumber(raw.contractSize, 1);
  const volUnit = toNumber(raw.volUnit, 1);
  const amountStep = Math.max(contractSize * volUnit, contractSize);
  const priceStep = toNumber(raw.priceUnit, stepFromScale(raw.priceScale, 0.00000001));

  return {
    id,
    symbol: fromMexcSymbol(id),
    prettySymbol: id,
    base,
    quote,
    active: raw.state === undefined ? true : toNumber(raw.state) === 0,
    precision: {
      amount: amountStep,
      price: priceStep,
    },
    limits: {
      amount: {
        min: Math.max(nativeVolumeToAmount(raw.minVol, { contractSize } as Market), amountStep),
        max: nativeVolumeToAmount(raw.maxVol ?? raw.limitMaxVol ?? Number.MAX_SAFE_INTEGER, { contractSize } as Market),
      },
      leverage: {
        min: toNumber(raw.minLeverage, 1),
        max: toNumber(raw.maxLeverage ?? raw.regularMaxLeverage, 1),
      },
    },
    contractSize,
    makerFee: toNumber(raw.makerFeeRate),
    takerFee: toNumber(raw.takerFeeRate),
    info: raw,
  };
};

export const isTradableMexcMarket = (raw: Record<string, any>): boolean =>
  String(raw.quoteCoin ?? raw.settleCoin ?? '').toUpperCase() === 'USDT' &&
  raw.apiAllowed !== false &&
  raw.isHidden !== true &&
  (raw.state === undefined || toNumber(raw.state) === 0);

const normalizePercentage = (value: unknown): number => {
  const parsed = toNumber(value);
  return Math.abs(parsed) <= 1 ? parsed * 100 : parsed;
};

export const normalizeTicker = (raw: Record<string, any>, market: Market): Ticker => {
  const last = toNumber(raw.lastPrice ?? raw.last, 0);
  return {
    id: market.id,
    symbol: market.symbol,
    bid: toNumber(raw.bid1 ?? raw.bidPrice, last),
    ask: toNumber(raw.ask1 ?? raw.askPrice, last),
    last,
    mark: toNumber(raw.fairPrice ?? raw.markPrice, last),
    index: toNumber(raw.indexPrice, last),
    percentage: normalizePercentage(raw.riseFallRate ?? raw.priceChangePercent),
    openInterest: nativeVolumeToAmount(raw.holdVol ?? raw.openInterest, market),
    fundingRate: toNumber(raw.fundingRate),
    fundingTime: normalizeTimestampToMs(raw.fundingTime ?? raw.nextSettleTime),
    volume: nativeVolumeToAmount(raw.volume24 ?? raw.volume, market),
    quoteVolume: toNumber(raw.amount24 ?? raw.quoteVolume),
    info: raw,
  };
};

const normalizeOrderbookSide = (rows: unknown, market: Market): OrderBook['bids'] => {
  if (!Array.isArray(rows)) return [];

  return rows.reduce<OrderBook['bids']>((acc, row) => {
    const price = Array.isArray(row) ? row[0] : row?.p ?? row?.price;
    const amount = Array.isArray(row) ? row[1] : row?.v ?? row?.vol ?? row?.amount;
    const parsedPrice = toNumber(price);
    const parsedAmount = nativeVolumeToAmount(amount, market);
    if (parsedPrice > 0 && parsedAmount >= 0) {
      acc.push({ price: parsedPrice, amount: parsedAmount, total: 0 });
    }
    return acc;
  }, []);
};

export const normalizeOrderBook = (raw: Record<string, any>, market: Market): OrderBook => {
  const data = raw.data || raw;
  const orderbook = {
    bids: normalizeOrderbookSide(data.bids ?? data.bid, market).sort((a, b) => b.price - a.price),
    asks: normalizeOrderbookSide(data.asks ?? data.ask, market).sort((a, b) => a.price - b.price),
  };
  calcOrderBookTotal(orderbook);
  return orderbook;
};

export const normalizeCandle = (raw: Record<string, any>): Candle | null => {
  const timestamp = normalizeTimestampToSeconds(raw.time ?? raw.t ?? raw.timestamp);
  if (!timestamp) return null;

  return {
    timestamp,
    open: toNumber(raw.open ?? raw.o),
    high: toNumber(raw.high ?? raw.h),
    low: toNumber(raw.low ?? raw.l),
    close: toNumber(raw.close ?? raw.c),
    volume: toNumber(raw.vol ?? raw.volume ?? raw.q ?? raw.amount),
  };
};

export const normalizeKlineResponse = (payload: any): Candle[] => {
  const data = payload?.data ?? payload;
  if (Array.isArray(data)) {
    return data.map(normalizeCandle).filter((candle): candle is Candle => !!candle);
  }

  const times = Array.isArray(data?.time) ? data.time : [];
  return times
    .map((time: unknown, index: number) =>
      normalizeCandle({
        time,
        open: data.open?.[index],
        high: data.high?.[index],
        low: data.low?.[index],
        close: data.close?.[index],
        vol: data.vol?.[index] ?? data.amount?.[index],
      }),
    )
    .filter((candle: Candle | null): candle is Candle => !!candle);
};

export const normalizeBalance = (rawRows: Record<string, any>[]): Balance => {
  const rows = Array.isArray(rawRows) ? rawRows : [];
  const primary = rows.find((row) => String(row.currency ?? row.asset ?? '').toUpperCase() === 'USDT') || rows[0] || {};
  const free = toNumber(primary.availableBalance ?? primary.available ?? primary.free);
  const total = toNumber(primary.equity ?? primary.cashBalance ?? primary.balance ?? primary.total, free);
  const upnl = toNumber(primary.unrealized ?? primary.unrealizedPnl ?? primary.unRealizedProfit);
  const frozen = toNumber(primary.frozenBalance ?? primary.frozen);
  const used = Math.max(total - free, frozen, 0);

  return {
    free,
    used,
    total,
    upnl,
    info: rows,
    currencies: rows.reduce<Record<string, Balance>>((acc, row) => {
      const currency = String(row.currency ?? row.asset ?? '');
      if (!currency) return acc;
      const rowFree = toNumber(row.availableBalance ?? row.available ?? row.free);
      const rowTotal = toNumber(row.equity ?? row.cashBalance ?? row.balance ?? row.total, rowFree);
      const rowUpnl = toNumber(row.unrealized ?? row.unrealizedPnl ?? row.unRealizedProfit);
      acc[currency] = {
        free: rowFree,
        used: Math.max(rowTotal - rowFree, toNumber(row.frozenBalance ?? row.frozen), 0),
        total: rowTotal,
        upnl: rowUpnl,
        info: row,
      };
      return acc;
    }, {}),
  };
};

export const normalizePositionSide = (value: unknown): PositionSide => {
  const parsed = toNumber(value, Number.NaN);
  if (Number.isFinite(parsed)) return POSITION_SIDE[parsed] || PositionSide.Long;

  const normalized = String(value || '').toLowerCase();
  return normalized.includes('short') || normalized === 'sell' ? PositionSide.Short : PositionSide.Long;
};

export const normalizePositionMode = (value: unknown): PositionMode => {
  const parsed = toNumber(value, Number.NaN);
  if (Number.isFinite(parsed)) return POSITION_MODE[parsed] || PositionMode.Hedged;

  const normalized = String(value || '').toLowerCase();
  return normalized.includes('one') ? PositionMode.OneWay : PositionMode.Hedged;
};

export const normalizeMarginType = (value: unknown): MarginType => {
  const parsed = toNumber(value, Number.NaN);
  if (Number.isFinite(parsed)) return MARGIN_TYPE[parsed] || MarginType.Cross;

  const normalized = String(value || '').toLowerCase();
  return normalized.includes('isolated') || normalized.includes('isolate') ? MarginType.Isolated : MarginType.Cross;
};

export const normalizePosition = (
  raw: Record<string, any>,
  market?: Market,
  accountPositionMode = PositionMode.Hedged,
): Position | null => {
  const symbol = fromMexcSymbol(raw.symbol ?? raw.contract);
  if (!symbol) return null;

  const side = normalizePositionSide(raw.positionType ?? raw.positionSide ?? raw.side);
  const mode = raw.positionMode ? normalizePositionMode(raw.positionMode) : accountPositionMode;
  const contracts = nativeVolumeToAmount(raw.holdVol ?? raw.positionVol ?? raw.vol ?? raw.size, market);
  const entryPrice = toNumber(raw.holdAvgPrice ?? raw.openAvgPrice ?? raw.entryPrice ?? raw.avgPrice);
  const unrealizedPnl = toNumber(raw.unrealized ?? raw.unrealizedPnl ?? raw.unRealizedProfit);
  const notional = Math.abs(contracts) * entryPrice;
  const leverage = toNumber(raw.leverage, market?.limits.leverage.min || 1);

  return {
    id: createPositionId(symbol, side, mode),
    symbol,
    side,
    entryPrice,
    notional,
    leverage,
    marginType: normalizeMarginType(raw.openType ?? raw.marginType),
    positionMode: mode,
    unrealizedPnl,
    realizedPnl: toNumber(raw.realised ?? raw.realizedPnl, 0),
    contracts,
    liquidationPrice: toNumber(raw.liquidatePrice ?? raw.liquidationPrice),
    marginSize: toNumber(raw.im ?? raw.margin ?? raw.positionMargin, notional && leverage ? notional / leverage : 0),
    info: raw,
    extra: {
      contractsNative: toNumber(raw.holdVol ?? raw.positionVol ?? raw.vol ?? raw.size),
      unrealizedPnlNative: unrealizedPnl,
    },
    lastUpdate: normalizeTimestampToMs(raw.updateTime ?? raw.modifyTime ?? raw.createTime),
  };
};

export const fakePositionsForMarket = (
  market: Market,
  positionMode = PositionMode.Hedged,
  marginType = MarginType.Cross,
  leverage = 20,
): Position[] => {
  const sides = positionMode === PositionMode.OneWay ? [PositionSide.Long] : [PositionSide.Long, PositionSide.Short];
  return sides.map((side) => ({
    id: createPositionId(market.symbol, side, positionMode),
    symbol: market.symbol,
    side,
    entryPrice: 0,
    notional: 0,
    leverage,
    marginType,
    positionMode,
    unrealizedPnl: 0,
    realizedPnl: 0,
    contracts: 0,
    liquidationPrice: 0,
    marginSize: 0,
    info: {},
    extra: {
      contractsNative: 0,
      unrealizedPnlNative: 0,
    },
    lastUpdate: 0,
  }));
};

export const normalizeOrder = (raw: Record<string, any>, market?: Market): Order | null => {
  const id = toIntegerString(raw.orderId ?? raw.id);
  const symbol = fromMexcSymbol(raw.symbol ?? raw.contract);
  const sideInfo = ORDER_SIDE[toNumber(raw.side, Number.NaN)];
  if (!id || !symbol || !sideInfo) return null;

  const typeCode = toNumber(raw.type ?? raw.orderType, 1);
  const amount = nativeVolumeToAmount(raw.vol ?? raw.quantity ?? raw.origQty, market);
  const filled = nativeVolumeToAmount(raw.dealVol ?? raw.filledQty ?? raw.executedQty, market);
  const status = ORDER_STATUS[toNumber(raw.state ?? raw.status, Number.NaN)] || OrderStatus.Open;
  const type = ORDER_TYPE[typeCode] || OrderType.Limit;
  const price = toNumber(raw.price ?? raw.orderPrice, 0);
  const triggerPrice = toNumber(raw.triggerPrice ?? raw.stopPrice, 0);

  return omitUndefined({
    id,
    status,
    symbol,
    type,
    side: sideInfo.side,
    price: price || triggerPrice,
    amount,
    filled,
    remaining: Math.max(amount - filled, 0),
    reduceOnly: Boolean(raw.reduceOnly) || sideInfo.reduceOnly,
    timeInForce: ORDER_TIME_IN_FORCE[typeCode] || OrderTimeInForce.GoodTillCancel,
    lastUpdate: normalizeTimestampToMs(raw.updateTime ?? raw.modifyTime ?? raw.createTime),
    info: raw,
    extra: {
      remainingNative: Math.max(toNumber(raw.vol) - toNumber(raw.dealVol), 0),
    },
  }) as Order;
};

const normalizePlanOrderType = (raw: Record<string, any>, side: OrderSide): OrderType => {
  const triggerType = toNumber(raw.triggerType, 0);
  const closesLong = side === OrderSide.Sell;

  if (closesLong) {
    return triggerType === 1 ? OrderType.TakeProfit : OrderType.StopLoss;
  }
  return triggerType === 2 ? OrderType.TakeProfit : OrderType.StopLoss;
};

export const normalizePlanOrder = (raw: Record<string, any>, market?: Market): Order | null => {
  const id = toIntegerString(raw.id ?? raw.orderId);
  const symbol = fromMexcSymbol(raw.symbol ?? raw.contract);
  const sideInfo = ORDER_SIDE[toNumber(raw.side, Number.NaN)];
  if (!id || !symbol || !sideInfo) return null;

  const amount = nativeVolumeToAmount(raw.vol ?? raw.quantity ?? raw.origQty, market);
  const status = ORDER_STATUS[toNumber(raw.state ?? raw.status, Number.NaN)] || OrderStatus.Open;
  const triggerPrice = toNumber(raw.triggerPrice ?? raw.stopPrice ?? raw.price, 0);

  return omitUndefined({
    id,
    status,
    symbol,
    type: normalizePlanOrderType(raw, sideInfo.side),
    side: sideInfo.side,
    price: triggerPrice,
    amount,
    filled: 0,
    remaining: amount,
    reduceOnly: Boolean(raw.reduceOnly) || sideInfo.reduceOnly,
    timeInForce: OrderTimeInForce.GoodTillCancel,
    lastUpdate: normalizeTimestampToMs(raw.updateTime ?? raw.createTime),
    info: {
      ...raw,
      __mexcPlanOrder: true,
    },
    extra: {
      remainingNative: toNumber(raw.vol ?? raw.quantity ?? raw.origQty),
    },
  }) as Order;
};

export const normalizeExecution = (raw: Record<string, any>, market?: Market): Execution | null => {
  const symbol = fromMexcSymbol(raw.symbol ?? raw.contract);
  if (!symbol) return null;

  const sideInfo = ORDER_SIDE[toNumber(raw.side, Number.NaN)];
  const qty = nativeVolumeToAmount(raw.vol ?? raw.quantity ?? raw.qty, market);
  if (qty <= 0) return null;

  return omitUndefined({
    id: toIntegerString(raw.tradeId ?? raw.id),
    timestamp: normalizeTimestampToSeconds(raw.createTime ?? raw.time ?? raw.tradeTime),
    symbol,
    orderId: toIntegerString(raw.orderId),
    price: toNumber(raw.price),
    qty,
    side: sideInfo?.side || (String(raw.taker || '').toLowerCase() === 'sell' ? OrderSide.Sell : OrderSide.Buy),
    commission: Math.abs(toNumber(raw.fee)),
    commissionAsset: raw.feeCurrency ?? raw.feeAsset,
    positionSide: sideInfo?.positionSide,
    realizedPnl: toNumber(raw.profit ?? raw.realizedPnl, 0),
    maker: raw.role !== undefined ? toNumber(raw.role) === 1 : undefined,
  }) as Execution;
};
