import { parsePrecision } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import {
  Balance,
  Candle,
  Execution,
  HistoricalTrade,
  IncomeRecord,
  MarginType,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  PublicTrade,
  Ticker,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { incomeCategoryFromType } from '../../../utils/incomeCategoryFromType';
import { omitUndefined } from '../../../utils/omitUndefined';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import {
  MARGIN_TYPE,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_MODE,
  POSITION_SIDE,
  TIME_IN_FORCE,
  WeexKlineRow,
} from './weex.types';

export const toNumber = (value: unknown, fallback = 0): number => {
  if (value === undefined || value === null || value === '') {
    return fallback;
  }
  const parsed = Number.parseFloat(String(value));
  return Number.isFinite(parsed) ? parsed : fallback;
};

export const toIntegerString = (value: unknown): string => String(value ?? '');

export const normalizeTimestampToMs = (value: unknown): number => {
  if (value === undefined || value === null || value === '') {
    return 0;
  }

  const numeric = Number(value);
  if (Number.isFinite(numeric) && numeric > 0) {
    if (numeric >= 1_000_000_000_000_000_000) return Math.floor(numeric / 1_000_000);
    if (numeric >= 1_000_000_000_000_000) return Math.floor(numeric / 1_000);
    if (numeric >= 1_000_000_000_000) return Math.floor(numeric);
    return Math.floor(numeric * 1000);
  }

  const parsedDate = Date.parse(String(value));
  return Number.isFinite(parsedDate) ? parsedDate : 0;
};

export const normalizeTimestampToSeconds = (value: unknown): number => {
  const milliseconds = normalizeTimestampToMs(value);
  return milliseconds ? Math.floor(milliseconds / 1000) : 0;
};

export const normalizeMarket = (raw: Record<string, any>): Market => {
  const pricePrecision = raw.pricePrecision ?? raw.quotePrecision ?? 8;
  const quantityPrecision = raw.quantityPrecision ?? raw.baseAssetPrecision ?? 8;
  const priceStep = toNumber(parsePrecision(`${pricePrecision}`), 0.00000001);
  const amountStep = toNumber(raw.minOrderSize, toNumber(parsePrecision(`${quantityPrecision}`), 0.00000001));
  const contractVal = toNumber(raw.contractVal, 1);

  return {
    id: raw.symbol,
    symbol: raw.symbol,
    base: raw.baseAsset,
    quote: raw.quoteAsset,
    active: raw.forwardContractFlag !== false,
    precision: {
      amount: amountStep,
      price: priceStep,
    },
    limits: {
      amount: {
        min: toNumber(raw.minOrderSize, amountStep),
        max: toNumber(raw.maxOrderSize, Number.MAX_SAFE_INTEGER),
      },
      leverage: {
        min: toNumber(raw.minLeverage, 1),
        max: toNumber(raw.maxLeverage, 1),
      },
    },
    contractSize: contractVal,
    makerFee: toNumber(raw.makerFeeRate),
    takerFee: toNumber(raw.takerFeeRate),
    info: raw,
  };
};

const normalizePercentage = (rawPercentage: unknown, last: number, open: number) => {
  const parsed = toNumber(rawPercentage, Number.NaN);
  if (Number.isFinite(parsed)) {
    return Math.abs(parsed) <= 1 ? parsed * 100 : parsed;
  }
  if (!open) {
    return 0;
  }
  return ((last - open) / open) * 100;
};

export const normalizeTicker = (
  rawTicker: Record<string, any> | undefined,
  market: Market,
  rawBook?: Record<string, any>,
): Ticker => {
  const raw = rawTicker || {};
  const last = toNumber(raw.lastPrice ?? raw.c ?? raw.last);
  const open = toNumber(raw.openPrice ?? raw.o);
  const mark = toNumber(raw.markPrice ?? raw.m, last);
  const index = toNumber(raw.indexPrice ?? raw.i, mark);

  return {
    id: market.id,
    symbol: market.symbol,
    bid: toNumber(rawBook?.bidPrice ?? raw.bidPrice ?? raw.b, last),
    ask: toNumber(rawBook?.askPrice ?? raw.askPrice ?? raw.a, last),
    last,
    mark,
    index,
    percentage: normalizePercentage(raw.priceChangePercent ?? raw.P, last, open),
    openInterest: toNumber(raw.openInterest),
    fundingRate: toNumber(raw.fundingRate),
    fundingTime: toNumber(raw.fundingTime),
    volume: toNumber(raw.volume ?? raw.v),
    quoteVolume: toNumber(raw.quoteVolume ?? raw.q),
    info: {
      ticker: rawTicker,
      bookTicker: rawBook,
    },
  };
};

export const normalizePositionMode = (value: unknown): PositionMode =>
  POSITION_MODE[String(value || '').toUpperCase()] || PositionMode.OneWay;

export const normalizeMarginType = (value: unknown): MarginType =>
  MARGIN_TYPE[String(value || '').toUpperCase()] || MarginType.Cross;

export const normalizePositionSide = (value: unknown): PositionSide => {
  const normalized = String(value || '').toUpperCase();
  return POSITION_SIDE[normalized] || PositionSide.Long;
};

export const normalizeOrderSide = (value: unknown): OrderSide | undefined => {
  const normalized = String(value || '').toUpperCase();
  return ORDER_SIDE[normalized];
};

export const normalizeOrderStatus = (value: unknown): OrderStatus => {
  const normalized = String(value || '').toUpperCase();
  return ORDER_STATUS[normalized] || OrderStatus.Open;
};

export const normalizeOrderType = (value: unknown): Order['type'] => {
  const normalized = String(value || '').toUpperCase();
  return ORDER_TYPE[normalized] || OrderType.Limit;
};

const normalizeTriggerType = (value: unknown): OrderStopTriggerType => {
  const normalized = String(value || '').toUpperCase();
  return ORDER_TRIGGER_TYPE[normalized] || OrderStopTriggerType.LastPrice;
};

export const normalizeOrder = (raw: Record<string, any>, market?: Market): Order | null => {
  const id = toIntegerString(raw.orderId ?? raw.id ?? raw.algoId);
  const symbol = String(raw.symbol ?? raw.s ?? market?.symbol ?? '');
  const side = normalizeOrderSide(raw.side ?? raw.orderSide);
  if (!id || !symbol || !side) {
    return null;
  }

  const type = normalizeOrderType(raw.type ?? raw.orderType ?? raw.planType);
  const amount = Math.abs(toNumber(raw.origQty ?? raw.quantity ?? raw.size ?? raw.qty));
  const filled = Math.abs(toNumber(raw.executedQty ?? raw.cumFillSize ?? raw.fillSize));
  const rawPrice = toNumber(raw.price ?? raw.actualPrice ?? raw.executePrice);
  const triggerPrice = toNumber(raw.stopPrice ?? raw.triggerPrice);
  const price = rawPrice || triggerPrice;

  return omitUndefined({
    id,
    status: normalizeOrderStatus(raw.status ?? raw.algoStatus),
    symbol,
    type,
    side,
    price,
    amount,
    filled,
    remaining: Math.max(amount - filled, 0),
    reduceOnly: Boolean(raw.reduceOnly),
    timeInForce: TIME_IN_FORCE[String(raw.timeInForce || '').toUpperCase()] || OrderTimeInForce.GoodTillCancel,
    lastUpdate: normalizeTimestampToMs(
      raw.updateTime ?? raw.updatedTime ?? raw.time ?? raw.createTime ?? raw.createdTime,
    ),
    info: raw,
    tpTriggerType:
      raw.tpOrderType || raw.TpWorkingType ? normalizeTriggerType(raw.tpOrderType ?? raw.TpWorkingType) : undefined,
    slTriggerType:
      raw.workingType || raw.SlWorkingType ? normalizeTriggerType(raw.workingType ?? raw.SlWorkingType) : undefined,
    takeProfit: toNumber(raw.tpTriggerPrice ?? raw.takeProfit, 0) || undefined,
    stopLoss: toNumber(raw.slTriggerPrice ?? raw.stopLoss, 0) || undefined,
  }) as Order;
};

export const normalizeAlgoOrder = (raw: Record<string, any>, market?: Market): Order[] => {
  const rawOrderType = String(raw.orderType ?? raw.type ?? '').toUpperCase();
  const planType = String(raw.planType ?? '').toUpperCase();
  const algoType = String(raw.algoType ?? '').toUpperCase();
  const isTriggerOrder = ['STOP', 'STOP_MARKET', 'TAKE_PROFIT', 'TAKE_PROFIT_MARKET'].includes(rawOrderType);
  const isProtective =
    raw.reduceOnly === true ||
    planType === 'STOP_LOSS' ||
    planType === 'TAKE_PROFIT' ||
    algoType === 'TPSL' ||
    algoType === 'TP_SL';
  const normalizedType =
    isProtective && (planType === 'STOP_LOSS' || planType === 'TAKE_PROFIT')
      ? planType
      : isTriggerOrder && !isProtective
        ? 'CONDITIONAL'
        : (raw.orderType ?? raw.planType);
  const baseOrder = normalizeOrder(
    {
      ...raw,
      orderId: raw.algoId ?? raw.orderId,
      type: normalizedType,
      status: raw.algoStatus ?? raw.status,
      origQty: raw.quantity,
      price: raw.price || raw.actualPrice,
      stopPrice: raw.triggerPrice,
      side: raw.side,
    },
    market,
  );

  const orders: Order[] = baseOrder ? [baseOrder] : [];
  const parentId = toIntegerString(raw.algoId ?? raw.orderId);
  const symbol = String(raw.symbol ?? market?.symbol ?? '');
  const side = normalizeOrderSide(raw.side ?? raw.orderSide);

  if (!parentId || !symbol || !side) {
    return orders;
  }

  const pushVirtual = (suffix: 'tp' | 'sl', triggerPrice: unknown, executionPrice: unknown, triggerType: unknown) => {
    const price = toNumber(triggerPrice, 0);
    if (!price) return;
    orders.push(
      omitUndefined({
        id: `${parentId}__${suffix}`,
        parentId,
        status: normalizeOrderStatus(raw.algoStatus ?? raw.status),
        symbol,
        type: suffix === 'tp' ? OrderType.TakeProfit : OrderType.StopLoss,
        side,
        price,
        amount: Math.abs(toNumber(raw.quantity ?? raw.size)),
        filled: 0,
        remaining: Math.abs(toNumber(raw.quantity ?? raw.size)),
        reduceOnly: true,
        timeInForce: TIME_IN_FORCE[String(raw.timeInForce || '').toUpperCase()] || OrderTimeInForce.GoodTillCancel,
        lastUpdate: normalizeTimestampToMs(raw.updateTime ?? raw.createTime),
        info: raw,
        tpTriggerType: suffix === 'tp' ? normalizeTriggerType(triggerType) : undefined,
        slTriggerType: suffix === 'sl' ? normalizeTriggerType(triggerType) : undefined,
        extra: {
          remainingNotional: toNumber(executionPrice, 0) || undefined,
        },
      }) as Order,
    );
  };

  pushVirtual('tp', raw.tpTriggerPrice, raw.tpPrice, raw.tpOrderType ?? raw.TpWorkingType);
  pushVirtual('sl', raw.slTriggerPrice, raw.slPrice, raw.workingType ?? raw.SlWorkingType);

  return orders;
};

export const normalizePosition = (
  raw: Record<string, any>,
  market?: Market,
  fallback?: Partial<Pick<Position, 'leverage' | 'marginType' | 'positionMode'>>,
): Position | null => {
  const symbol = String(raw.symbol ?? market?.symbol ?? '');
  if (!symbol) {
    return null;
  }

  const side = normalizePositionSide(raw.side ?? raw.positionSide);
  const rawPositionMode = POSITION_MODE[String(raw.separatedMode ?? raw.separatedType ?? '').toUpperCase()];
  const rawMarginType = MARGIN_TYPE[String(raw.marginType ?? '').toUpperCase()];
  const positionMode = rawPositionMode || fallback?.positionMode || PositionMode.OneWay;
  const marginType = rawMarginType || fallback?.marginType || MarginType.Cross;
  const contracts = Math.abs(toNumber(raw.size ?? raw.positionAmt ?? raw.positions));
  const openValue = Math.abs(toNumber(raw.openValue ?? raw.notional));
  const entryPrice = contracts ? openValue / contracts : toNumber(raw.entryPrice ?? raw.averagePrice);
  const leverage = toNumber(raw.leverage, fallback?.leverage || 1);
  const unrealizedPnl = toNumber(raw.unrealizePnl ?? raw.unRealizedProfit ?? raw.unrealizedPnl);

  return {
    id: createPositionId(symbol, side, positionMode),
    symbol,
    side,
    entryPrice,
    notional: openValue,
    leverage,
    unrealizedPnl,
    realizedPnl: toNumber(raw.realizedPnl ?? raw.realizePnl),
    contracts,
    liquidationPrice: toNumber(raw.liquidatePrice ?? raw.liquidationPrice),
    marginType,
    positionMode,
    marginSize: toNumber(raw.marginSize ?? raw.isolatedMargin),
    info: raw,
    lastUpdate: normalizeTimestampToMs(raw.updatedTime ?? raw.updateTime ?? raw.createdTime),
  };
};

export const normalizeBalance = (rows: Array<Record<string, any>>): Balance => {
  const usdt = rows.find((row) => String(row.asset || row.coin).toUpperCase() === 'USDT') || rows[0] || {};
  const total = toNumber(usdt.balance ?? usdt.amount);
  const free = toNumber(usdt.availableBalance ?? usdt.amount);
  const upnl = toNumber(usdt.unrealizePnl);
  const frozen = toNumber(usdt.frozen);

  return {
    total,
    free,
    used: frozen || Math.max(total - free, 0),
    upnl,
    info: rows,
  };
};

export const normalizeCandle = (row: WeexKlineRow | Record<string, any>): Candle | null => {
  const source = Array.isArray(row)
    ? {
        timestamp: row[0],
        open: row[1],
        high: row[2],
        low: row[3],
        close: row[4],
        volume: row[5],
      }
    : row;

  const candle = {
    timestamp: normalizeTimestampToSeconds(source.timestamp ?? source.t ?? source.time),
    open: toNumber(source.open ?? source.o),
    high: toNumber(source.high ?? source.h),
    low: toNumber(source.low ?? source.l),
    close: toNumber(source.close ?? source.c),
    volume: toNumber(source.volume ?? source.v),
  };

  return Object.values(candle).some((value) => !Number.isFinite(value)) ? null : candle;
};

export const normalizeExecution = (raw: Record<string, any>, market?: Market): Execution | null => {
  const symbol = String(raw.symbol ?? market?.symbol ?? '');
  const side = normalizeOrderSide(raw.side ?? raw.orderSide);
  const price = toNumber(raw.price);
  const qty = Math.abs(toNumber(raw.qty ?? raw.fillSize));
  const timestamp = normalizeTimestampToSeconds(raw.time ?? raw.createdTime ?? raw.updatedTime);
  if (!symbol || !side || price <= 0 || qty <= 0 || !timestamp) {
    return null;
  }

  return omitUndefined({
    // The venue's own fill id. Without it a fill has no idempotent key and
    // cannot be stored or de-duplicated — orderId cannot serve, since one order
    // fills across many rows.
    id: raw.tradeId ?? raw.fillId ?? raw.id ? toIntegerString(raw.tradeId ?? raw.fillId ?? raw.id) : undefined,
    timestamp,
    symbol,
    orderId: raw.orderId ? toIntegerString(raw.orderId) : undefined,
    price,
    qty,
    side,
    // Ported up from `normalizeHistoricalTrade`, which wrapped this function and
    // added them. The two had drifted, so anything reading the execution shape
    // saw a venue reporting no PnL and no hedge leg rather than a mapper that
    // simply never carried them.
    realizedPnl: toNumber(raw.realizedPnl ?? raw.realizePnl, undefined as any),
    positionSide: POSITION_SIDE[String(raw.positionSide || '').toUpperCase()] || 'both',
    maker: raw.maker,
    commission: Math.abs(toNumber(raw.commission ?? raw.fillFee, Number.NaN)) || undefined,
    commissionAsset: raw.commissionAsset ?? raw.coin ?? market?.quote,
  });
};

export const normalizeHistoricalTrade = (raw: Record<string, any>, market?: Market): HistoricalTrade | null => {
  const execution = normalizeExecution(raw, market);
  if (!execution?.orderId) {
    return null;
  }

  // Now purely the millisecond-shaped view of the same execution: everything
  // except `time`, `quoteQty` and `buyer` comes straight off it.
  return omitUndefined({
    id: execution.id,
    symbol: execution.symbol,
    orderId: execution.orderId,
    price: execution.price,
    qty: execution.qty,
    quoteQty: toNumber(raw.quoteQty, undefined as any),
    realizedPnl: execution.realizedPnl,
    positionSide: execution.positionSide ?? 'both',
    side: execution.side,
    buyer: raw.buyer,
    maker: execution.maker,
    commission: execution.commission,
    commissionAsset: execution.commissionAsset,
    time: normalizeTimestampToMs(raw.time ?? raw.createdTime ?? raw.updatedTime),
  }) as HistoricalTrade;
};

export const normalizeIncome = (raw: Record<string, any>): IncomeRecord => ({
  symbol: String(raw.symbol || ''),
  incomeType: String(raw.incomeType || ''),
  income: toNumber(raw.income),
  asset: String(raw.asset || raw.coin || 'USDT'),
  timestamp: normalizeTimestampToMs(raw.time ?? raw.createdTime),
  tranId: toIntegerString(raw.billId ?? raw.tranId ?? raw.id),
  tradeId: raw.tradeId ? toIntegerString(raw.tradeId) : undefined,
  category: incomeCategoryFromType(raw.incomeType),
});

export const normalizePublicTrade = (raw: Record<string, any>, symbol: string): PublicTrade | null => {
  const price = toNumber(raw.price ?? raw.p);
  const qty = Math.abs(toNumber(raw.qty ?? raw.q));
  const timestamp = normalizeTimestampToMs(raw.time ?? raw.T ?? raw.createdTime);
  if (price <= 0 || qty <= 0 || !timestamp) {
    return null;
  }

  return {
    id: toIntegerString(raw.id ?? raw.t ?? `${timestamp}:${price}:${qty}`),
    symbol,
    price,
    qty,
    quoteQty: toNumber(raw.quoteQty ?? raw.v, undefined as any),
    timestamp,
    side: raw.buyer === false || raw.m === true || raw.isBuyerMaker === true ? OrderSide.Sell : OrderSide.Buy,
  };
};

export const normalizeOrderBookSide = (
  rows: Array<[string, string]> | undefined,
): Array<{ price: number; amount: number; total: number }> =>
  (rows || []).reduce<Array<{ price: number; amount: number; total: number }>>((acc, [price, amount]) => {
    const parsedPrice = toNumber(price);
    const parsedAmount = toNumber(amount);
    if (parsedPrice > 0 && parsedAmount >= 0) {
      acc.push({ price: parsedPrice, amount: parsedAmount, total: 0 });
    }
    return acc;
  }, []);

export const normalizeOrderBook = (raw: Record<string, any>) => {
  const book = {
    bids: normalizeOrderBookSide(raw.bids ?? raw.b),
    asks: normalizeOrderBookSide(raw.asks ?? raw.a),
  };

  calcOrderBookTotal(book);
  return book;
};

export const fakePositionsForMarket = (
  market: Market,
  positionMode: PositionMode,
  marginType: MarginType,
  leverage: number,
): Position[] => {
  const createFake = (side: PositionSide): Writable<Position> => ({
    id: createPositionId(market.symbol, side, positionMode),
    symbol: market.symbol,
    side,
    entryPrice: 0,
    notional: 0,
    leverage,
    unrealizedPnl: 0,
    contracts: 0,
    liquidationPrice: 0,
    marginType,
    positionMode,
    info: {},
  });

  return [createFake(PositionSide.Long), createFake(PositionSide.Short)];
};
