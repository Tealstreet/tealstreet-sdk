import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Candle, FetchedLeverage, Market, Order, OrderBook, OrderFillEvent, Position, Ticker, Writable } from '../types';
import type { PackedTickerDelta, Store, StoreUpdate } from './store.interface';

import { LOADED_FLAG, MARKET_FLAG, TICKER_FIELDS, orderEnums, positionEnums } from '@tealstreet/safe-cex-wasm/wire';
import { MarginType, OrderSide, PositionMode } from '../types';

/** What a frame carries besides store state, for the exchange to emit. */
export type FrameEvents = {
  books: Array<{ symbol: string; orderbook: OrderBook }>;
  candles: Array<{ symbol: string; interval: string; candle: Candle }>;
  fills: OrderFillEvent[];
};

const finite = (value: number) => (Number.isNaN(value) ? undefined : value);

/**
 * Applies the Rust engine's flush frames to a `DefaultStore`, so every reader
 * of the store keeps working: one `store.update` per frame, tickers patched in
 * place from deltas, everything else replaced by section. Frame row indices
 * refer to the latest `MARKETS` section, so the applier keeps that table.
 */
export class FrameApplier {
  private marketSymbols: string[] = [];

  constructor(private readonly store: Store) {}

  apply(frame: Frame): FrameEvents {
    const update: Writable<StoreUpdate> = {};

    const markets = frame.markets();
    if (markets) {
      const rows: Market[] = [];
      this.marketSymbols = new Array(markets.count);
      for (let i = 0; i < markets.count; i++) {
        const flags = markets.flags[i];
        const flag = (bit: number) => ((flags & bit) !== 0 ? true : undefined);
        const minPrice = finite(markets.minPrice[i]);
        rows.push({
          id: markets.id(i),
          symbol: markets.symbol(i),
          base: markets.base(i),
          quote: markets.quote(i),
          active: (flags & MARKET_FLAG.ACTIVE) !== 0,
          precision: { amount: markets.precisionAmount[i], price: markets.precisionPrice[i] },
          limits: {
            amount: { min: markets.minAmount[i], max: markets.maxAmount[i] },
            leverage: { min: markets.minLeverage[i], max: markets.maxLeverage[i] },
            ...(minPrice !== undefined && { price: { min: minPrice } }),
          },
          contractSize: finite(markets.contractSize[i]),
          isInverse: flag(MARKET_FLAG.INVERSE),
          isSpot: flag(MARKET_FLAG.SPOT),
          isPerpetual: flag(MARKET_FLAG.PERPETUAL),
          isExpiring: flag(MARKET_FLAG.EXPIRING),
          isReadOnly: flag(MARKET_FLAG.READ_ONLY),
          takerFee: finite(markets.takerFee[i]),
          makerFee: finite(markets.makerFee[i]),
        });
        this.marketSymbols[i] = rows[i].symbol;
      }
      update.markets = rows;
    }

    const tickers = frame.tickers();
    if (tickers) {
      const rows: Ticker[] = [];
      for (let i = 0; i < tickers.count; i++) {
        const market = update.markets?.[tickers.market[i]] ?? this.store.marketsBySymbol[this.symbol(tickers.market[i])];
        const ticker: Record<string, number | string> = { id: market?.id ?? '', symbol: market?.symbol ?? '' };
        for (const field of TICKER_FIELDS) ticker[field] = tickers[field][i];
        rows.push(ticker as unknown as Ticker);
      }
      update.tickers = rows;
    }

    const deltas = frame.tickerDeltas();
    if (deltas && !tickers) {
      // The engine's ticker field order is the store's packed delta order.
      const packed: PackedTickerDelta[] = [];
      for (let i = 0; i < deltas.length; ) {
        const mask = deltas[i + 1];
        let width = 0;
        for (let field = 0; field < TICKER_FIELDS.length; field++) if (mask & (1 << field)) width++;
        packed.push(Array.from(deltas.subarray(i, i + 2 + width)) as PackedTickerDelta);
        i += 2 + width;
      }
      update.tickerDeltas = packed;
    }

    const orders = frame.orders();
    if (orders) {
      const rows: Order[] = [];
      for (let i = 0; i < orders.count; i++) {
        const enums = orderEnums(orders.enums[i]);
        const trigger = enums.triggerType as Order['tpTriggerType'];
        rows.push({
          id: orders.id(i),
          status: enums.status as Order['status'],
          symbol: this.symbol(orders.market[i]),
          type: enums.type as Order['type'],
          side: enums.side as Order['side'],
          price: orders.price[i],
          amount: orders.amount[i],
          filled: orders.filled[i],
          remaining: orders.remaining[i],
          reduceOnly: enums.reduceOnly,
          lastUpdate: orders.lastUpdate[i],
          ...(enums.close !== undefined && { close: enums.close }),
          ...(orders.clientOrderId(i) !== undefined && { clientOrderId: orders.clientOrderId(i) }),
          ...(orders.parentId(i) !== undefined && { parentId: orders.parentId(i) }),
          ...(enums.timeInForce !== undefined && { timeInForce: enums.timeInForce as Order['timeInForce'] }),
          ...(enums.type === 'take_profit_market' && trigger && { tpTriggerType: trigger }),
          ...(enums.type === 'stop_market' && trigger && { slTriggerType: trigger }),
          ...(finite(orders.takeProfit[i]) !== undefined && { takeProfit: orders.takeProfit[i] }),
          ...(finite(orders.stopLoss[i]) !== undefined && { stopLoss: orders.stopLoss[i] }),
          ...(enums.rejectReason && { extra: { rejectReason: enums.rejectReason as never } }),
        } as Order);
      }
      update.orders = rows;
    }

    const positions = frame.positions();
    if (positions) {
      const rows: Position[] = [];
      for (let i = 0; i < positions.count; i++) {
        const enums = positionEnums(positions.enums[i]);
        const symbol = this.symbol(positions.market[i]);
        const mode = enums.positionMode as PositionMode | undefined;
        rows.push({
          id: mode === PositionMode.Hedged ? `${symbol}:${enums.side}` : symbol,
          symbol,
          side: enums.side as Position['side'],
          entryPrice: positions.entryPrice[i],
          notional: positions.notional[i],
          leverage: positions.leverage[i],
          unrealizedPnl: positions.unrealizedPnl[i],
          contracts: positions.contracts[i],
          liquidationPrice: positions.liquidationPrice[i],
          ...(mode !== undefined && { positionMode: mode }),
          ...(enums.marginType !== undefined && { marginType: enums.marginType as MarginType }),
          ...(finite(positions.realizedPnl[i]) !== undefined && { realizedPnl: positions.realizedPnl[i] }),
          ...(finite(positions.marginSize[i]) !== undefined && { marginSize: positions.marginSize[i] }),
          ...(finite(positions.lastUpdate[i]) !== undefined && { lastUpdate: positions.lastUpdate[i] }),
        } as Position);
      }
      update.positions = rows;
    }

    const balance = frame.balance();
    if (balance) update.balance = { ...this.store.balance, ...balance };

    const status = frame.status();
    if (status) {
      const loaded = status.loaded;
      update.loaded = {
        ...this.store.loaded,
        balance: (loaded & LOADED_FLAG.BALANCE) !== 0,
        orders: (loaded & LOADED_FLAG.ORDERS) !== 0,
        markets: (loaded & LOADED_FLAG.MARKETS) !== 0,
        tickers: (loaded & LOADED_FLAG.TICKERS) !== 0,
        positions: (loaded & LOADED_FLAG.POSITIONS) !== 0,
      };
      update.latency = status.latency;
      update.options = { ...this.store.options, isHedged: status.hedged };
    }

    // Per-market leverage and margin type: the order form falls back to
    // `fetchedLeverage` when a market has no position.
    const settings = frame.settings();
    if (settings) {
      const hedged = (update.options ?? this.store.options).isHedged;
      const leverage: Record<string, FetchedLeverage> = {};
      for (let i = 0; i < settings.count; i++) {
        const value = settings.leverage[i];
        leverage[this.symbol(settings.market[i])] = {
          buyLeverage: value,
          sellLeverage: value,
          marginType: settings.marginType(i) as MarginType | undefined,
          positionMode: hedged ? PositionMode.Hedged : PositionMode.OneWay,
        };
      }
      update.fetchedLeverage = leverage;
    }

    let changed = false;
    for (const _key in update) {
      changed = true;
      break;
    }
    if (changed) this.store.update(update);

    return {
      books: this.books(frame),
      candles: this.candles(frame),
      fills: this.fills(frame),
    };
  }

  private symbol(market: number): string {
    return this.marketSymbols[market] ?? '';
  }

  private books(frame: Frame): FrameEvents['books'] {
    const books: FrameEvents['books'] = [];
    for (const book of frame.books()) {
      const side = (rows: Float64Array) => {
        const levels = new Array(rows.length / 3);
        for (let i = 0, j = 0; i < rows.length; i += 3, j++) {
          levels[j] = { price: rows[i], amount: rows[i + 1], total: rows[i + 2] };
        }
        return levels;
      };
      books.push({ symbol: this.symbol(book.market), orderbook: { bids: side(book.bids), asks: side(book.asks) } });
    }
    return books;
  }

  private candles(frame: Frame): FrameEvents['candles'] {
    const rows = frame.candles();
    const candles: FrameEvents['candles'] = [];
    for (let i = 0; i < (rows?.count ?? 0); i++) {
      const at = i * 6;
      candles.push({
        symbol: this.symbol(rows!.market[i]),
        interval: rows!.interval(i),
        candle: {
          timestamp: rows!.rows[at],
          open: rows!.rows[at + 1],
          high: rows!.rows[at + 2],
          low: rows!.rows[at + 3],
          close: rows!.rows[at + 4],
          volume: rows!.rows[at + 5],
        },
      });
    }
    return candles;
  }

  private fills(frame: Frame): OrderFillEvent[] {
    const rows = frame.fills();
    const fills: OrderFillEvent[] = [];
    for (let i = 0; i < (rows?.count ?? 0); i++) {
      fills.push({
        id: rows!.orderId(i),
        symbol: this.symbol(rows!.market[i]),
        side: rows!.side[i] === 0 ? OrderSide.Buy : OrderSide.Sell,
        price: rows!.rows[i * 2],
        amount: rows!.rows[i * 2 + 1],
      } as OrderFillEvent);
    }
    return fills;
  }
}
