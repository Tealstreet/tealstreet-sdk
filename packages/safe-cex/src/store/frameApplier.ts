import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Candle, FetchedLeverage, Market, Order, OrderBook, OrderFillEvent, Position, Ticker, Writable } from '../types';
import type { PackedTickerDelta, Store, StoreUpdate } from './store.interface';

import { LOADED_FLAG, MARKET_FLAG, TICKER_FIELDS, orderEnums, positionEnums } from '@tealstreet/safe-cex-wasm/wire';
import { MarginType, OrderSide, PositionMode } from '../types';
import { divide, subtract } from '../utils/safe-math';

export type FrameBook = { symbol: string; orderbook: OrderBook };
type BookLevels = Map<number, number> | Float64Array;
type BookState = { id: string; bids: BookLevels; asks: BookLevels };
export type FrameCandle = { symbol: string; interval: string; candle: Candle };

const finite = (value: number) => (Number.isNaN(value) ? undefined : value);

/**
 * Applies the Rust engine's flush frames to a `DefaultStore`, so every reader
 * of the store keeps working: one `store.update` per frame, tickers patched in
 * place from deltas, everything else replaced by section. Frame row indices
 * refer to the latest `MARKETS` section, so the applier keeps that table.
 */
export class FrameApplier {
  private marketSymbols: string[] = [];
  private marketIds: string[] = [];
  private bookStates = new Map<string, BookState>();
  private readonly frameBooks = new WeakMap<Frame, FrameBook[]>();
  private readonly frameMarkets = new WeakMap<Frame, { symbols: string[]; ids: string[] }>();
  private contractSizes: number[] = [];

  constructor(private readonly store: Store, private readonly nativePlacementAmounts = false, private readonly inverseNativeAmounts = false) {}

  /** Forwarding workers skip book reconstruction before transferring the frame. */
  apply(frame: Frame, applyBooks = true): void {
    const update: Writable<StoreUpdate> = {};

    const markets = frame.markets();
    const symbols = markets ? Array.from({ length: markets.count }, (_, i) => markets.symbol(i)) : this.marketSymbols;
    const ids = markets ? Array.from({ length: markets.count }, (_, i) => markets.id(i)) : this.marketIds;
    this.frameMarkets.set(frame, { symbols, ids });
    if (applyBooks) this.ingestBooks(frame, symbols, ids);
    else this.bookStates.clear();
    if (markets) {
      this.marketIds = ids;
      const rows: Market[] = [];
      this.marketSymbols = new Array(markets.count);
      this.contractSizes = Array.from(markets.contractSize, (size) => size > 0 ? size : 1);
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
        const identity = tickers.identity?.(i);
        const ticker: Record<string, number | string> = { id: market?.id ?? identity?.id ?? '', symbol: market?.symbol ?? identity?.symbol ?? '' };
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
        const symbol = this.symbol(orders.market[i]);
        const market = update.markets?.[orders.market[i]] ?? this.store.marketsBySymbol[symbol];
        const native = orders.remainingNative?.[i];
        const nativeExtra = market?.isInverse && native !== undefined && Number.isFinite(native)
          ? { remainingNative: native, remainingNotional: native }
          : this.inverseNativeAmounts
            ? { remainingNative: Math.max(0, subtract(orders.amount[i], orders.filled[i])) }
            : undefined;
        const extra = nativeExtra || enums.rejectReason || this.nativePlacementAmounts
          ? {
              ...nativeExtra,
              ...(enums.rejectReason && { rejectReason: enums.rejectReason as never }),
              ...(this.nativePlacementAmounts && {
                remainingNative: native !== undefined && Number.isFinite(native)
                  ? native
                  : divide(orders.remaining[i], this.contractSizes[orders.market[i]]),
                filledInPlacementUnits: divide(orders.filled[i], this.contractSizes[orders.market[i]]),
              }),
            }
          : undefined;
        rows.push({
          id: orders.id(i),
          status: enums.status as Order['status'],
          symbol,
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
          ...((enums.type === 'stop_market' || enums.type === 'trailing_stop_market') && trigger && { slTriggerType: trigger }),
          ...(finite(orders.takeProfit[i]) !== undefined && { takeProfit: orders.takeProfit[i] }),
          ...(finite(orders.stopLoss[i]) !== undefined && { stopLoss: orders.stopLoss[i] }),
          ...(extra && { extra }),
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
        const market = update.markets?.[positions.market[i]] ?? this.store.marketsBySymbol[symbol];
        const mode = enums.positionMode as PositionMode | undefined;
        rows.push({
          id: positions.id(i) ?? (mode === PositionMode.Hedged ? `${symbol}:${enums.side}` : symbol),
          symbol,
          side: enums.side as Position['side'],
          entryPrice: positions.entryPrice[i],
          notional: positions.notional[i],
          leverage: positions.leverage[i],
          unrealizedPnl: positions.unrealizedPnl[i],
          contracts: positions.contracts[i],
          ...((this.nativePlacementAmounts || market?.isInverse || this.inverseNativeAmounts) && { extra: {
            ...(this.nativePlacementAmounts && {
              contractsNative: divide(positions.contracts[i], this.contractSizes[positions.market[i]]),
            }),
            ...((market?.isInverse || this.inverseNativeAmounts) && { contractsNative: positions.notional[i], unrealizedPnlNative: positions.unrealizedPnl[i] }),
          } }),
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
          buyLeverage: finite(settings.buyLeverage?.[i] ?? NaN) ?? value,
          sellLeverage: finite(settings.sellLeverage?.[i] ?? NaN) ?? value,
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
  }

  private symbol(market: number): string {
    return this.marketSymbols[market] ?? '';
  }

  books(frame: Frame): FrameBook[] {
    const captured = this.frameMarkets.get(frame);
    const markets = captured ? undefined : frame.markets();
    return this.ingestBooks(
      frame,
      captured?.symbols ?? (markets ? Array.from({ length: markets.count }, (_, i) => markets.symbol(i)) : this.marketSymbols),
      captured?.ids ?? (markets ? Array.from({ length: markets.count }, (_, i) => markets.id(i)) : this.marketIds),
    );
  }

  private ingestBooks(frame: Frame, symbols: string[], ids: string[]): FrameBook[] {
    const cached = this.frameBooks.get(frame);
    if (cached) return cached;
    const fullBooks = [...(frame.books?.() ?? [])];
    const deltaBooks = [...(frame.bookDeltas?.() ?? [])];
    if (!fullBooks.length && !deltaBooks.length && symbols === this.marketSymbols) {
      const empty: FrameBook[] = [];
      this.frameBooks.set(frame, empty);
      return empty;
    }
    // Stage every change: one invalid delta must not partially advance a baseline.
    const states = new Map(this.bookStates);
    if (symbols !== this.marketSymbols) {
      const identities = new Map(symbols.map((symbol, i) => [symbol, ids[i]]));
      for (const [symbol, state] of states) if (identities.get(symbol) !== state.id) states.delete(symbol);
    }
    const changed = new Set<string>();
    const identity = (market: number) => {
      const symbol = symbols[market];
      const id = ids[market];
      if (!symbol || id === undefined) throw new Error('Order book snapshot required: unknown market');
      return { symbol, id };
    };
    const applyLevels = (levels: Map<number, number>, rows: Float64Array, width: number) => {
      for (let i = 0; i < rows.length; i += width) {
        const price = rows[i];
        const amount = rows[i + 1];
        if (!Number.isFinite(price) || price <= 0 || !Number.isFinite(amount) || amount < 0) {
          throw new Error('Order book snapshot required: invalid level');
        }
        if (amount === 0) levels.delete(price);
        else levels.set(price, amount);
      }
    };
    for (const book of fullBooks) {
      const { symbol, id } = identity(book.market);
      // A worker can transfer the source frame after reading it. Keep compact
      // numeric baselines, without allocating price maps or sorting full books.
      states.set(symbol, { id, bids: book.bids.slice(), asks: book.asks.slice() });
      changed.add(symbol);
    }
    const levelMap = (levels: BookLevels) => {
      if (levels instanceof Map) return new Map(levels);
      const result = new Map<number, number>();
      applyLevels(result, levels, 3);
      return result;
    };
    for (const book of deltaBooks) {
      const { symbol, id } = identity(book.market);
      const previous = states.get(symbol);
      if (!previous || previous.id !== id) throw new Error(`Order book snapshot required for ${symbol}`);
      const state = { id, bids: levelMap(previous.bids), asks: levelMap(previous.asks) };
      applyLevels(state.bids, book.bids, 2);
      applyLevels(state.asks, book.asks, 2);
      states.set(symbol, state);
      changed.add(symbol);
    }
    const side = (levels: BookLevels, descending: boolean): OrderBook['bids'] => {
      let total = 0;
      if (levels instanceof Float64Array) {
        const rows: OrderBook['bids'] = [];
        let previous = descending ? Infinity : -Infinity;
        for (let i = 0; i < levels.length; i += 3) {
          const price = levels[i];
          const amount = levels[i + 1];
          if (!Number.isFinite(price) || price <= 0 || !Number.isFinite(amount) || amount < 0) {
            throw new Error('Order book snapshot required: invalid level');
          }
          if (descending ? price >= previous : price <= previous) return side(levelMap(levels), descending);
          previous = price;
          if (amount === 0) continue;
          total += amount;
          if (!Number.isFinite(total)) throw new Error('Order book snapshot required: invalid total');
          rows.push({ price, amount, total });
        }
        return rows;
      }
      return [...levels].sort(([a], [b]) => descending ? b - a : a - b).map(([price, amount]) => {
        total += amount;
        if (!Number.isFinite(total)) throw new Error('Order book snapshot required: invalid total');
        return { price, amount, total };
      });
    };
    const books = [...changed].map((symbol) => {
      const state = states.get(symbol)!;
      return { symbol, orderbook: { bids: side(state.bids, true), asks: side(state.asks, false) } };
    });
    this.bookStates = states;
    this.frameBooks.set(frame, books);
    return books;
  }

  candles(frame: Frame): FrameCandle[] {
    const rows = frame.candles();
    const candles: FrameCandle[] = [];
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

  fills(frame: Frame): OrderFillEvent[] {
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
