import {
  Market,
  Order,
  OrderRiskProfile,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
} from '../types';
import { multiply } from './safe-math';

const formatRoundedNumber = (val: number | string, precision: number): string => {
  const floatVal = parseFloat(String(val));
  if (Number.isNaN(floatVal)) return '0.0';

  return floatVal.toLocaleString('en-US', {
    minimumFractionDigits: precision,
    maximumFractionDigits: precision,
    useGrouping: true,
  });
};

const isFullStop = (order: Order): boolean =>
  order.type !== OrderType.Limit && (order.amount === 0 || order.filled + order.remaining === 0);

export const formatPnl = (pnl: number, precision = 2, prefix?: string): string => {
  if (!prefix) {
    return formatRoundedNumber(pnl, precision);
  }
  const formattedAbsPnl = formatRoundedNumber(Math.abs(pnl), precision);
  if (pnl > 0) {
    return `${prefix}${formattedAbsPnl}`;
  } else {
    return `-${prefix}${formattedAbsPnl}`;
  }
};

export const filterAndSortOrdersInProfitDirection = (
  orders: Order[],
  entryPrice: Position['entryPrice'],
  positionSide: PositionSide,
): Order[] => {
  return orders
    .filter(
      (order) =>
        order.status === OrderStatus.Open &&
        // filter orders which are not important for us straight away (other side of a trade)
        order.side === (positionSide === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy) &&
        (positionSide === PositionSide.Long ? order.price >= entryPrice : order.price <= entryPrice),
    )
    .sort((a, b) => (positionSide === PositionSide.Long ? a.price - b.price : b.price - a.price));
};

export const filterAndSortOrdersInLossDirection = (
  orders: Order[],
  entryPrice: Position['entryPrice'],
  positionSide: PositionSide,
): Order[] => {
  return orders
    .filter(
      (order) =>
        order.status === OrderStatus.Open &&
        // capture opposite direction for stops and all limit orders
        (order.side === (positionSide === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy) ||
          order.type === OrderType.Limit) &&
        (positionSide === PositionSide.Long ? order.price < entryPrice : order.price > entryPrice),
    )
    .sort((a, b) => (positionSide === PositionSide.Short ? a.price - b.price : b.price - a.price));
};

export type OrdersRiskMaps = {
  tickerBySymbol: Record<string, Ticker>;
  marketBySymbol: Record<string, Market>;
};

/**
 * Build the symbol-keyed lookup maps used by `calculateOrdersRisk`.
 * Exposed so the postprocessor can cache them across passes via
 * source-array reference equality (the store mutates ticker/market
 * fields in place but only swaps the array reference on full
 * snapshots).
 */
export const buildOrdersRiskMaps = (markets: Market[], tickers: Ticker[]): OrdersRiskMaps => {
  const tickerBySymbol: Record<string, Ticker> = {};
  for (let i = 0; i < tickers.length; i++) {
    tickerBySymbol[tickers[i].symbol] = tickers[i];
  }
  const marketBySymbol: Record<string, Market> = {};
  for (let i = 0; i < markets.length; i++) {
    marketBySymbol[markets[i].symbol] = markets[i];
  }
  return { tickerBySymbol, marketBySymbol };
};

export const calculateOrdersRisk = (
  markets: Market[],
  tickers: Ticker[],
  positions: Position[],
  orders: Order[],
  precomputedMaps?: OrdersRiskMaps,
): { [orderId: string]: OrderRiskProfile[] } => {
  // Build per-symbol indexes once. The prior shape did `tickers.find(...)`,
  // `markets.find(...)`, `orders.some(...)`, `orders.filter(...)`, and
  // `positions.filter(...)` once per position iteration. For 50 positions
  // over 600 tickers / 600 markets / 50 orders / 50 positions that's
  // ~65K comparisons per call — and `OrdersRiskProfilePostprocessor` runs
  // it twice (primary + secondary) on every throttled store update (4 Hz).
  const { tickerBySymbol, marketBySymbol } = precomputedMaps ?? buildOrdersRiskMaps(markets, tickers);
  // Open orders bucketed by symbol (closed orders never used downstream).
  const openOrdersBySymbol: Record<string, Order[]> = {};
  const symbolsWithOpenOrders = new Set<string>();
  for (let i = 0; i < orders.length; i++) {
    const o = orders[i];
    if (o.status !== OrderStatus.Open) continue;
    symbolsWithOpenOrders.add(o.symbol);
    (openOrdersBySymbol[o.symbol] ||= []).push(o);
  }
  // Position counts by symbol for the `isLikelyHedged` check (was an
  // O(N²) `positions.filter` per iteration).
  const positionCountBySymbol: Record<string, number> = {};
  for (let i = 0; i < positions.length; i++) {
    const sym = positions[i].symbol;
    positionCountBySymbol[sym] = (positionCountBySymbol[sym] ?? 0) + 1;
  }

  const activePositions = positions
    // only care about positions with open orders...
    .filter((position) => symbolsWithOpenOrders.has(position.symbol))
    .flatMap((position) => {
      // if open position, return it
      if (position.contracts !== 0) {
        return [position];

        //   otherwise we create fake positions for each side of the trade. If hedge mode we assume that both positions will show up in this filter func on its own...
      } else {
        const lastPrice = tickerBySymbol[position.symbol]?.last;
        if (!lastPrice) return null;
        if (position.positionMode === PositionMode.Hedged) {
          return [
            {
              ...position,
              contracts: 0,
              entryPrice: lastPrice,
            },
          ];
        } else {
          return [
            {
              ...position,
              contracts: 0,
              entryPrice: lastPrice,
            },
            {
              ...position,
              contracts: 0,
              side: position.side === PositionSide.Long ? PositionSide.Short : PositionSide.Long,
              entryPrice: lastPrice,
            },
          ];
        }
      }
    })
    .filter((position) => position) as Position[];

  const [primaryPositionRisk, secondaryPositions] = processOrdersRiskForPositions(
    tickerBySymbol,
    marketBySymbol,
    openOrdersBySymbol,
    positionCountBySymbol,
    activePositions,
    false,
  );

  const [secondaryPositionRisk] = processOrdersRiskForPositions(
    tickerBySymbol,
    marketBySymbol,
    openOrdersBySymbol,
    positionCountBySymbol,
    secondaryPositions,
    true,
  );

  return { ...secondaryPositionRisk, ...primaryPositionRisk };
};

const processOrdersRiskForPositions = (
  tickerBySymbol: Record<string, Ticker>,
  marketBySymbol: Record<string, Market>,
  openOrdersBySymbol: Record<string, Order[]>,
  positionCountBySymbol: Record<string, number>,
  positions: Position[],
  areSecondaryPositions = false,
): [{ [orderId: string]: OrderRiskProfile[] }, Position[]] => {
  const ordersRisk: { [orderId: string]: OrderRiskProfile[] } = {};
  const possibleSecondaryPositions: Position[] = [];

  // Now find orders above and below position
  for (const position of positions) {
    const lastPrice = tickerBySymbol[position.symbol]?.last;

    const market = marketBySymbol[position.symbol];
    const takerFee = market?.takerFee || 0;
    const makerFee = market?.makerFee || 0;

    const isLikelyHedged =
      (positionCountBySymbol[position.symbol] ?? 0) > 1 || position.positionMode === PositionMode.Hedged;

    // if we have no markPrice, we can't calculate risk
    if (!lastPrice) {
      continue;
    }

    const entryPrice = position.entryPrice;

    // Pre-bucketed in `calculateOrdersRisk` — saves an O(N) filter per
    // position iteration (only open orders are bucketed).
    const symbolOrders = openOrdersBySymbol[position.symbol] || [];

    // Find orders in profit
    const ordersInProfitDirection = filterAndSortOrdersInProfitDirection(
      symbolOrders,
      areSecondaryPositions ? position.entryPrice : lastPrice,
      position.side,
    );

    // Find orders in loss
    const ordersInLossDirection = filterAndSortOrdersInLossDirection(
      symbolOrders,
      areSecondaryPositions ? position.entryPrice : lastPrice,
      position.side,
    );

    // Process orders in profit direction
    let currentPositionSize = Math.abs(position.contracts);
    let totalRealizedPnl = 0;
    for (const order of ordersInProfitDirection) {
      // position was empty from beginning so no need to process profit orders
      if (currentPositionSize <= 0) {
        break;
      }
      let distance = Math.abs(order.price - entryPrice);
      if (
        (position.side === PositionSide.Long && order.price < entryPrice) ||
        (position.side === PositionSide.Short && order.price > entryPrice)
      ) {
        distance = -1 * distance;
      }
      if (!ordersRisk[order.id]) {
        ordersRisk[order.id] = [];
      }
      // full stop / trailing stop encountered
      if (isFullStop(order)) {
        const orderPnl = multiply(currentPositionSize, distance);
        totalRealizedPnl += orderPnl;
        ordersRisk[order.id].push({
          isLookingLikeAFullStop: true,
          orderPnl,
          totalRealizedPnl,
          rollingUnrealizedPnl: multiply(currentPositionSize, distance),
          orderLinePosSide: position.side,
          orderLineText: `PnL: ${formatPnl(totalRealizedPnl)}`,
          fee: multiply(currentPositionSize * order.price, takerFee),
          positionSize: 0,
        });
        currentPositionSize -= order.remaining;
        // possible secondary position to consider past this take profit order
        possibleSecondaryPositions.push({
          ...position,
          side: position.side === PositionSide.Long ? PositionSide.Short : PositionSide.Long,
          entryPrice: order.price,
          contracts: 0,
        });
        break;
      } else {
        if (
          order.type === OrderType.Limit &&
          ((order.side === OrderSide.Sell && position.side === PositionSide.Long) ||
            (order.side === OrderSide.Buy && position.side === PositionSide.Short)) &&
          isLikelyHedged &&
          !order.reduceOnly
          // below not needed because in hedgemode any non reduce order on opposite side of position is not part of this position?
          // &&
          // ((position.side === PositionSide.Long && order.price < entryPrice) ||
          // (position.side === PositionSide.Short && order.price > entryPrice))
        ) {
          continue;
        }
        const orderPnl = multiply(Math.min(currentPositionSize, order.remaining), distance);
        const rollingUnrealizedPnl = multiply(currentPositionSize, distance);
        totalRealizedPnl += orderPnl;
        const nextPositionSize = currentPositionSize - order.remaining;
        ordersRisk[order.id].push({
          orderPnl,
          totalRealizedPnl,
          rollingUnrealizedPnl,
          orderLinePosSide: position.side,
          orderLineText: `${
            orderPnl === rollingUnrealizedPnl
              ? `PnL: ${formatPnl(orderPnl)}`
              : `RPnL: ${orderPnl > 0 ? `+${formatPnl(orderPnl)}` : formatPnl(orderPnl)} UPnL: ${formatPnl(rollingUnrealizedPnl)}`
          }${!isLikelyHedged && order.remaining > currentPositionSize && !order.reduceOnly ? `, Opens ${position.side === PositionSide.Long ? 'Short' : 'Long'}` : ''}`,
          fee: multiply(Math.min(currentPositionSize, order.remaining) * order.price, makerFee),
          positionSize: nextPositionSize,
        });
        currentPositionSize = nextPositionSize;
        if (currentPositionSize <= 0) {
          // possible secondary position to consider past this take profit order
          possibleSecondaryPositions.push({
            ...position,
            side: position.side === PositionSide.Long ? PositionSide.Short : PositionSide.Long,
            entryPrice: order.price,
            contracts: Math.abs(currentPositionSize),
          });
          break;
        }
      }
    }

    // Process orders in loss direction
    currentPositionSize = Math.abs(position.contracts);
    totalRealizedPnl = 0;
    let adjustedEntryPrice = entryPrice;
    for (const order of ordersInLossDirection) {
      const distance = Math.abs(order.price - adjustedEntryPrice);
      const mult =
        position.side === PositionSide.Long
          ? order.price < adjustedEntryPrice
            ? -1
            : 1
          : order.price > adjustedEntryPrice
            ? -1
            : 1;
      if (!ordersRisk[order.id]) {
        ordersRisk[order.id] = [];
      }
      // full stop / trailing stop encountered
      if (isFullStop(order) || order.type === OrderType.TrailingStopLoss) {
        const orderPnl = mult * multiply(currentPositionSize, distance);
        totalRealizedPnl += orderPnl;
        const rollingUnrealizedPnl = mult * multiply(currentPositionSize, distance);
        ordersRisk[order.id].push({
          isLookingLikeAFullStop: true,
          orderPnl,
          totalRealizedPnl,
          rollingUnrealizedPnl,
          orderLinePosSide: position.side,
          orderLineText: `PnL: ${formatPnl(totalRealizedPnl)}`,
          fee: multiply(currentPositionSize * order.price, takerFee),
          positionSize: 0,
        });
        currentPositionSize -= order.remaining;
        break;
      } else if (order.type === OrderType.Limit) {
        adjustedEntryPrice =
          (order.remaining * order.price + currentPositionSize * adjustedEntryPrice) /
          (currentPositionSize + order.remaining);
        const rollingUnrealizedPnl = mult * multiply(currentPositionSize, distance);
        const nextPositionSize = currentPositionSize + order.remaining;
        ordersRisk[order.id].push({
          orderPnl: 0,
          totalRealizedPnl,
          rollingUnrealizedPnl,
          orderLinePosSide: position.side,
          orderLineText:
            currentPositionSize === 0
              ? `Opens ${order.side === OrderSide.Buy ? 'Long' : 'Short'}`
              : `UPnL: ${formatPnl(rollingUnrealizedPnl)}`,
          positionSize: nextPositionSize,
          fee: multiply(order.remaining * order.price, makerFee),
        });
        currentPositionSize = nextPositionSize;
        if (currentPositionSize < 0) {
          break;
        }
      } else {
        // future proof non reduce only trigger orders
        let sizeOnTrigger = order.reduceOnly ? Math.min(currentPositionSize, order.remaining) : order.remaining;
        const orderPnl = mult * multiply(Math.min(currentPositionSize, order.remaining), distance);
        const rollingUnrealizedPnl = mult * multiply(currentPositionSize, distance);
        totalRealizedPnl += orderPnl;
        const nextPositionSize = currentPositionSize - sizeOnTrigger;
        ordersRisk[order.id].push({
          orderPnl,
          totalRealizedPnl,
          rollingUnrealizedPnl,
          orderLinePosSide: position.side,
          orderLineText:
            orderPnl === rollingUnrealizedPnl
              ? `PnL: ${formatPnl(orderPnl)}`
              : `RPnL: ${orderPnl > 0 ? `+${formatPnl(orderPnl)}` : formatPnl(orderPnl)} UPnL: ${formatPnl(rollingUnrealizedPnl)}`,
          fee: multiply(sizeOnTrigger * order.price, makerFee),
          positionSize: nextPositionSize,
        });
        currentPositionSize = nextPositionSize;
        if (currentPositionSize <= 0) {
          break;
        }
      }
    }
  }

  return [ordersRisk, possibleSecondaryPositions];
};

export const calculateRiskAtPrice = (
  price: number,
  symbol: string,
  ordersRisk: { [orderId: string]: OrderRiskProfile[] },
  positions: Position[],
  orders: Order[],
): {
  positionSize: number;
  positionSide: PositionSide;
  unrealizedPnl: number;
  realizedPnl: number;
} | null => {
  const symbolPositions = positions?.filter((p) => p.symbol === symbol) || [];
  const isHedgedMode = symbolPositions.some((p) => p.positionMode === PositionMode.Hedged);
  const openSymbolPositions = symbolPositions.filter((p) => Math.abs(p.contracts) > 0);

  let side = PositionSide.Long;
  let entryPrice = 0;
  let positionSize = 0;

  if (openSymbolPositions.length > 0) {
    let position: Position;
    if (isHedgedMode) {
      // Hedged mode can have two position rows per symbol (long + short).
      // Crosshair PnL is only unambiguous when exactly one side is open.
      if (openSymbolPositions.length !== 1) return null;
      position = openSymbolPositions[0];
    } else {
      if (symbolPositions.length !== 1) return null;
      position = symbolPositions[0];
    }

    side = position.side;
    entryPrice = position.entryPrice;
    positionSize = position.contracts;
  }

  // handle case where position is not open yet
  if (positionSize === 0) {
    const lowestSellOrder = orders
      ?.filter(
        (o) => o.status === OrderStatus.Open && o.symbol === symbol && o.side === OrderSide.Sell && !o.reduceOnly,
      )
      .sort((a, b) => a.price - b.price)[0];
    const highestBuyOrder = orders
      ?.filter((o) => o.status === OrderStatus.Open && o.symbol === symbol && o.side === OrderSide.Buy && !o.reduceOnly)
      .sort((a, b) => b.price - a.price)[0];

    const onlyHasBuyEntry = highestBuyOrder && !lowestSellOrder;
    const onlyHasSellEntry = lowestSellOrder && !highestBuyOrder;

    if (lowestSellOrder && (onlyHasSellEntry || price > lowestSellOrder.price)) {
      side = PositionSide.Short;
      entryPrice = lowestSellOrder.price;
      positionSize = lowestSellOrder.remaining;
    } else if (highestBuyOrder && (onlyHasBuyEntry || price < highestBuyOrder.price)) {
      side = PositionSide.Long;
      entryPrice = highestBuyOrder.price;
      positionSize = highestBuyOrder.remaining;
    } else {
      return null;
    }
  }

  let realizedPnl = 0;
  let referencePrice = entryPrice;
  let offsetPnl = 0;

  const closestOrder = orders
    ?.filter(
      (o) =>
        o.status === OrderStatus.Open &&
        o.symbol === symbol &&
        // if we are above position we are in a profit long or loss short and only want orders below position, else opposite
        (price > entryPrice ? o.price > entryPrice : o.price < entryPrice) &&
        // if we are above position we are in a profit long or loss short and only want orders below us, else opposite
        (price > entryPrice ? o.price <= price : o.price >= price) &&
        // order risk needs to exist to pivot off of, otherwise could be stacked stops etc that are already 0
        ordersRisk?.[o.id]?.[0],
    )
    // if above we sort from high to low to get highest order below us, else opposite
    .sort((a, b) => (price > entryPrice ? b.price - a.price : a.price - b.price))[0];

  if (closestOrder) {
    const orderRisk = ordersRisk?.[closestOrder.id]?.[0];
    if (orderRisk) {
      positionSize = orderRisk.positionSize;
      offsetPnl = orderRisk.rollingUnrealizedPnl - orderRisk.orderPnl;
      realizedPnl = orderRisk.totalRealizedPnl;
      referencePrice = closestOrder.price;
      side = orderRisk.orderLinePosSide;
    }
  }

  const pnlFromReferencePrice =
    (price - referencePrice) * positionSize * (side === PositionSide.Long ? 1 : -1) + offsetPnl;

  return {
    positionSize: positionSize,
    positionSide: side,
    unrealizedPnl: pnlFromReferencePrice,
    realizedPnl: realizedPnl,
  };
};
