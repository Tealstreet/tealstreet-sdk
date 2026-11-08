import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Candle, FetchedLeverage, OrderBook, OrderFillEvent, Writable } from '../types';
import type { Store, StoreUpdate } from './store.interface';

import { LOADED_FLAG } from '@tealstreet/safe-cex-wasm/wire';
import { MarginType, PositionMode } from '../types';
import { readFillFrame } from './fillFrames';
import { readBalanceFrame } from './balanceFrames';
import { readMarketFrame } from './marketFrames';
import { readOrderFrame } from './orderFrames';
import { readPositionFrame } from './positionFrames';
import { readTickerFrame, readTickerDeltaFrame, readTickerObservationFrame } from './tickerFrames';

export type FrameBook = { symbol: string; orderbook: OrderBook };
type BookLevels = Map<number, number> | Float64Array;
type BookState = { id: string; bids: BookLevels; asks: BookLevels };
export type FrameCandle = { symbol: string; interval: string; candle: Candle };

export class FrameApplyError extends Error {}

const finite = (value: number) => (Number.isFinite(value) ? value : undefined);
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
  private readonly invalidMarketFrames = new WeakSet<Frame>();
  private marketsNeedSnapshot = false;
  private appliedSections = 0;

  /** STATUS flags cannot prove that a section survived frame decoding. */
  get hasMarketIdentity(): boolean {
    return !this.marketsNeedSnapshot && (this.appliedSections & LOADED_FLAG.MARKETS) !== 0;
  }

  get hasAccountState(): boolean {
    const required = LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS;
    return (this.appliedSections & required) === required;
  }

  get hasTickerState(): boolean {
    return (this.appliedSections & LOADED_FLAG.TICKERS) !== 0;
  }

  constructor(private readonly store: Store) {}

  /** Forwarding workers skip book reconstruction before transferring the frame. */
  apply(frame: Frame, applyBooks = true): void {
    const update: Writable<StoreUpdate> = {};

    const errors: unknown[] = [];
    const attempt = (read: () => void, section = 0) => {
      try {
        read();
      } catch (error) {
        this.appliedSections &= ~section;
        errors.push(error);
      }
    };
    let markets: ReturnType<Frame['markets']>;
    let tickers: ReturnType<Frame['tickers']>;
    let symbols = this.marketSymbols;
    let ids = this.marketIds;
    const resolveSymbol = (market: number, fallback?: string) => symbols[market] ?? fallback ?? '';
    attempt(() => {
      markets = frame.markets();
    }, LOADED_FLAG.MARKETS);
    attempt(() => {
      if (markets) {
        const rows = readMarketFrame(frame, markets);
        if (rows === undefined) throw new Error('Missing Market output');
        const nextSymbols = rows.map((row) => row.symbol);
        update.markets = rows;
        symbols = nextSymbols;
        ids = rows.map((row) => row.id);
      }
    }, LOADED_FLAG.MARKETS);

    const marketsFailed = errors.length > 0 || (this.marketsNeedSnapshot && !update.markets);
    this.marketsNeedSnapshot = marketsFailed;
    if (marketsFailed) {
      this.invalidateMarketIdentity();
      this.invalidMarketFrames.add(frame);
      if (!errors.length) errors.push(new Error('Market identity requires a snapshot'));
    }
    attempt(() => {
      if (marketsFailed) return;
      tickers = frame.tickers();
      const rows = readTickerFrame(tickers, (index) => update.markets
        ? update.markets[index]
        : this.store.marketsBySymbol[resolveSymbol(index)]);
      if (rows) update.tickers = rows;
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const deltas = readTickerDeltaFrame(frame,
        (update.tickers ?? this.store.tickers).length, Boolean(tickers));
      if (deltas) update.tickerDeltas = deltas;
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const observations = readTickerObservationFrame(frame, update.tickers ?? this.store.tickers);
      if (observations) update.tickerObservations = observations;
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const rows = readOrderFrame(frame, this.store, update.markets);
      if (rows) update.orders = rows;
    }, LOADED_FLAG.ORDERS);

    attempt(() => {
      if (marketsFailed) return;
      const positions = readPositionFrame(frame, this.store, update.markets);
      if (positions) update.positions = positions;
    }, LOADED_FLAG.POSITIONS);

    attempt(() => {
      const balance = readBalanceFrame(frame);
      if (balance) {
        // Account info is fetched separately; frame aggregates and maps replace prior state.
        update.balance = 'info' in this.store.balance ? { ...balance, info: this.store.balance.info } : balance;
      }
    }, LOADED_FLAG.BALANCE);

    attempt(() => {
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
    });

    attempt(() => {
      if (marketsFailed) return;
      // Per-market leverage and margin type: the order form falls back to
      // `fetchedLeverage` when a market has no position.
      const settings = frame.settings();
      if (settings) {
        const hedged = (update.options ?? this.store.options).isHedged;
        const leverage: Record<string, FetchedLeverage> = {};
        for (let i = 0; i < settings.count; i++) {
          const value = finite(settings.leverage[i]);
          const buyLeverage = finite(settings.buyLeverage?.[i] ?? NaN) ?? value;
          const sellLeverage = finite(settings.sellLeverage?.[i] ?? NaN) ?? value;
          // An unknown side omits the symbol: UiStateHandler copies these onto positions.
          if (buyLeverage === undefined || sellLeverage === undefined) continue;
          leverage[resolveSymbol(settings.market[i], settings.symbol?.(i))] = {
            buyLeverage,
            sellLeverage,
            marginType: settings.marginType(i) as MarginType | undefined,
            positionMode: hedged ? PositionMode.Hedged : PositionMode.OneWay,
          };
        }
        this.writeFetchedLeverage(update, leverage);
      }
    });

    this.frameMarkets.set(frame, { symbols, ids });
    if (applyBooks && !marketsFailed) {
      try {
        this.ingestBooks(frame, symbols, ids);
      } catch (error) {
        // A rejected batch may discard valid deltas too; require fresh baselines.
        this.bookStates.clear();
        this.frameBooks.set(frame, []);
        errors.push(error);
      }
    } else if (marketsFailed) this.frameBooks.set(frame, []);
    else this.bookStates.clear();

    this.marketSymbols = symbols;
    this.marketIds = ids;
    // Install proof before notifying startup waiters. Empty validated tables
    // count; rejected sections leave independent accepted peers usable.
    if (update.markets && !marketsFailed) this.appliedSections |= LOADED_FLAG.MARKETS;
    if (update.orders) this.appliedSections |= LOADED_FLAG.ORDERS;
    if (update.positions) this.appliedSections |= LOADED_FLAG.POSITIONS;
    if (update.balance) this.appliedSections |= LOADED_FLAG.BALANCE;
    if (update.tickers) this.appliedSections |= LOADED_FLAG.TICKERS;

    let changed = false;
    for (const _key in update) {
      changed = true;
      break;
    }
    // A current-state read is not another independent venue observation. Its
    // pending stream frame still owns order/position freshness when it arrives.
    if (changed) {
      try {
        if (frame.accountRead?.()) this.store.update(update, false, 'account-read');
        else this.store.update(update);
      } catch (error) {
        if (!errors.length) throw error;
        errors.push(error);
      }
    }
    if (errors.length) throw new FrameApplyError(errors.map(String).join('; '));
  }

  invalidateMarketIdentity(): void {
    this.appliedSections &= ~(LOADED_FLAG.MARKETS | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS | LOADED_FLAG.TICKERS);
    this.marketsNeedSnapshot = true;
    this.bookStates.clear();
  }

  applyLeverage(symbol: string, leverage: FetchedLeverage): void {
    this.applyLeverageSnapshot({ ...this.store.fetchedLeverage, [symbol]: leverage });
  }

  applyLeverageSnapshot(leverage: Record<string, FetchedLeverage>): void {
    const update: Writable<StoreUpdate> = {};
    this.writeFetchedLeverage(update, leverage);
    this.store.update(update);
  }

  private writeFetchedLeverage(update: Writable<StoreUpdate>, leverage: Record<string, FetchedLeverage>): void {
    update.fetchedLeverage = leverage;
  }

  private symbol(market: number, fallback?: string): string {
    return this.marketSymbols[market] ?? fallback ?? '';
  }

  books(frame: Frame): FrameBook[] {
    const captured = this.frameMarkets.get(frame);
    const markets = captured ? undefined : frame.markets();
    return this.ingestBooks(
      frame,
      captured?.symbols ??
        (markets ? Array.from({ length: markets.count }, (_, i) => markets.symbol(i)) : this.marketSymbols),
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
        if (!Number.isFinite(price) || !Number.isFinite(amount) || amount < 0) {
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
          if (!Number.isFinite(price) || !Number.isFinite(amount) || amount < 0) {
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
      return [...levels]
        .sort(([a], [b]) => (descending ? b - a : a - b))
        .map(([price, amount]) => {
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
    if (this.invalidMarketFrames.has(frame)) throw new FrameApplyError('Candle identity requires a market snapshot');
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
    try {
      return readFillFrame(frame);
    } catch (error) {
      throw new FrameApplyError(error instanceof Error ? error.message : String(error));
    }
  }
}
