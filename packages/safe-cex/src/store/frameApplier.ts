import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { FetchedLeverage, OrderFillEvent, Writable } from '../types';
import type { FrameBook } from './bookFrames';
import type { FrameCandle } from './candleFrames';
import type { Store, StoreUpdate } from './store.interface';

import { LOADED_FLAG } from '@tealstreet/safe-cex-wasm/wire';
import { BookFrames } from './bookFrames';
import { readCandleFrame } from './candleFrames';
import { readFillFrame } from './fillFrames';
import { readBalanceFrame } from './balanceFrames';
import { readMarketFrame } from './marketFrames';
import { readOrderFrame } from './orderFrames';
import { readPositionFrame } from './positionFrames';
import { readStoreStatus, readStoreSettings } from './storeFrames';
import { readTickerFrame, readTickerDeltaFrame, readTickerObservationFrame } from './tickerFrames';

export type { FrameBook } from './bookFrames';
export type { FrameCandle } from './candleFrames';

export class FrameApplyError extends Error {
  constructor(
    message: string,
    readonly bookRejected = false,
    readonly otherRejected = !bookRejected,
  ) {
    super(message);
  }
}

/**
 * Applies the Rust engine's flush frames to a `DefaultStore`, so every reader
 * of the store keeps working: one `store.update` per frame, tickers patched in
 * place from deltas, everything else replaced by section. Frame row indices
 * refer to the latest `MARKETS` section, so the applier keeps that table.
 */
export class FrameApplier {
  private marketSymbols: string[] = [];
  private marketIds: string[] = [];
  private readonly bookFrames = new BookFrames();
  private readonly bookErrors = new WeakMap<Frame, unknown>();
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
    let bookRejected = false;
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
      if (balance) update.balance = { ...this.store.balance, ...balance };
    }, LOADED_FLAG.BALANCE);

    attempt(() => {
      const status = frame.status();
      if (status) {
        const statusUpdate = readStoreStatus(status);
        update.loaded = { ...this.store.loaded, ...statusUpdate.loaded };
        update.latency = statusUpdate.latency;
        update.options = { ...this.store.options, ...statusUpdate.options };
      }
    });

    attempt(() => {
      if (marketsFailed) return;
      // Per-market leverage and margin type: the order form falls back to
      // `fetchedLeverage` when a market has no position.
      const settings = frame.settings();
      if (settings) {
        this.writeFetchedLeverage(update, readStoreSettings(settings,
          (update.options ?? this.store.options).isHedged, resolveSymbol));
      }
    });

    this.frameMarkets.set(frame, { symbols, ids });
    if (applyBooks && !marketsFailed) {
      try {
        this.ingestBooks(frame, symbols, ids);
      } catch (error) {
        bookRejected = true;
        // A rejected batch may discard valid deltas too; require fresh baselines.
        this.bookFrames.clear();
        this.bookErrors.set(frame, error);
        this.frameBooks.set(frame, []);
        errors.push(error);
      }
    } else if (marketsFailed) this.frameBooks.set(frame, []);
    else this.bookFrames.clear();

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
    if (errors.length)
      throw new FrameApplyError(errors.map(String).join('; '), bookRejected, errors.length > (bookRejected ? 1 : 0));
  }

  invalidateMarketIdentity(): void {
    this.appliedSections &= ~(LOADED_FLAG.MARKETS | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS | LOADED_FLAG.TICKERS);
    this.marketsNeedSnapshot = true;
    this.bookFrames.clear();
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

  marketSymbolsFor(frame: Frame): readonly string[] {
    return this.frameMarkets.get(frame)?.symbols ?? this.marketSymbols;
  }

  books(frame: Frame): FrameBook[] {
    if (this.bookErrors.has(frame)) throw this.bookErrors.get(frame);
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
    return this.frameBooks.get(frame) ?? this.bookFrames.read(frame, symbols, ids);
  }

  candles(frame: Frame): FrameCandle[] {
    if (this.invalidMarketFrames.has(frame)) throw new FrameApplyError('Candle identity requires a market snapshot');
    return readCandleFrame(frame, this.frameMarkets.get(frame)?.symbols ?? this.marketSymbols);
  }

  fills(frame: Frame): OrderFillEvent[] {
    try {
      return readFillFrame(frame);
    } catch (error) {
      throw new FrameApplyError(error instanceof Error ? error.message : String(error));
    }
  }
}
