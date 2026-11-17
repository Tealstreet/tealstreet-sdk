import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { FetchedLeverage, MessageHandlerStatsType, OrderFillEvent } from '../types';
import type { FrameBook } from './bookFrames';
import type { FrameCandle } from './candleFrames';
import type { StoreData } from '../generated/StoreData';
import type { NativeDerivedFrame } from './nativeDerivedFrames';

import { initialStoreData } from '../generated/StoreData';
import { dequal } from 'dequal';
import { TICKER_FIELDS } from '@tealstreet/safe-cex-wasm/wire';
import { WorkerFrameRecoveryScopes } from './workerFrameRecoveryScopes';
import { readNativeDerivedSections } from './nativeDerivedFrames';

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

/** Native SCXF state, with immutable section identities and versioned subscriptions. */
export type FrameSection = keyof StoreData;
export type FrameVersions = Readonly<Partial<Record<FrameSection, number>>>;
export type FrameListener = (data: StoreData, versions: FrameVersions) => void;

export class FrameStore {
  private state: StoreData = { ...initialStoreData };
  get latency(): StoreData['latency'] { return this.state.latency; }
  get balance(): StoreData['balance'] { return this.state.balance; }
  get markets(): StoreData['markets'] { return this.state.markets; }
  get tickers(): StoreData['tickers'] { return this.state.tickers; }
  get orders(): StoreData['orders'] { return this.state.orders; }
  get starkConfig(): StoreData['starkConfig'] { return this.state.starkConfig; }
  get zkLinkConfig(): StoreData['zkLinkConfig'] { return this.state.zkLinkConfig; }
  get userData(): StoreData['userData'] { return this.state.userData; }
  get positions(): StoreData['positions'] { return this.state.positions; }
  get positionSettings(): StoreData['positionSettings'] { return this.state.positionSettings; }
  get displayedPositions(): StoreData['displayedPositions'] { return this.state.displayedPositions; }
  get displayedMargin(): StoreData['displayedMargin'] { return this.state.displayedMargin; }
  get ordersRisk(): StoreData['ordersRisk'] { return this.state.ordersRisk; }
  get activeChasers(): StoreData['activeChasers'] { return this.state.activeChasers; }
  get activeTWAPs(): StoreData['activeTWAPs'] { return this.state.activeTWAPs; }
  get loaded(): StoreData['loaded'] { return this.state.loaded; }
  get options(): StoreData['options'] { return this.state.options; }
  get updateIds(): StoreData['updateIds'] { return this.state.updateIds; }
  get messageHandlerStats(): StoreData['messageHandlerStats'] { return this.state.messageHandlerStats; }
  get requestStats(): StoreData['requestStats'] { return this.state.requestStats; }
  get fetchedLeverage(): StoreData['fetchedLeverage'] { return this.state.fetchedLeverage; }
  get socketHealth(): StoreData['socketHealth'] { return this.state.socketHealth; }
  private marketIndexRows = this.state.markets;
  private tickerIndexRows = this.state.tickers;
  private marketSymbolIndex: Record<string, StoreData['markets'][number]> = {};
  private marketIdIndex: Record<string, StoreData['markets'][number]> = {};
  private tickerSymbolIndex: Record<string, StoreData['tickers'][number]> = {};
  private tickerIdIndex: Record<string, StoreData['tickers'][number]> = {};
  get marketsBySymbol() { this.indexMarkets(); return this.marketSymbolIndex; }
  get marketsById() { this.indexMarkets(); return this.marketIdIndex; }
  get tickersBySymbol() { this.indexTickers(); return this.tickerSymbolIndex; }
  get tickersById() { this.indexTickers(); return this.tickerIdIndex; }
  private indexMarkets() {
    if (this.marketIndexRows === this.markets) return;
    this.marketIndexRows = this.markets;
    this.marketSymbolIndex = Object.fromEntries(this.markets.map(row => [row.symbol, row]));
    this.marketIdIndex = Object.fromEntries(this.markets.map(row => [row.id, row]));
  }
  private indexTickers() {
    if (this.tickerIndexRows === this.tickers) return;
    this.tickerIndexRows = this.tickers;
    this.tickerSymbolIndex = Object.fromEntries(this.tickers.map(row => [row.symbol, row]));
    this.tickerIdIndex = Object.fromEntries(this.tickers.map(row => [row.id, row]));
  }
  getMarketsVersion() { return this.version('markets'); }
  getTickersVersion() { return this.version('tickers'); }

  private patchTickers(rows: StoreData['tickers'], deltas: readonly (readonly number[])[]): StoreData['tickers'] {
    const result = [...rows];
    for (const [index, mask, ...values] of deltas) {
      let row = { ...result[index] };
      let offset = 0;
      for (let field = 0; field < TICKER_FIELDS.length; field++) {
        if (!(mask & (1 << field))) continue;
        const key = TICKER_FIELDS[field];
        const value = values[offset++];
        if (key === 'midPrice' && Number.isNaN(value)) {
          const { midPrice: _, ...rest } = row;
          row = rest;
        } else row = { ...row, [key]: value };
      }
      result[index] = row;
    }
    return result;
  }

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

  private nativeStatsGroups = new Set<MessageHandlerStatsType>();

  private readonly listeners = new Set<FrameListener>();
  private readonly sectionVersions: Partial<Record<FrameSection, number>> = {};
  readonly recovery = new WorkerFrameRecoveryScopes();
  private sequence: number | undefined;
  private needsSnapshot = false;

  get data(): StoreData { return this.state; }
  get seq(): number | undefined { return this.sequence; }
  get snapshotRequired(): boolean { return this.needsSnapshot; }
  version(section: FrameSection): number { return this.sectionVersions[section] ?? 0; }
  get versions(): FrameVersions {
    return { ...this.sectionVersions };
  }

  subscribe(listener: FrameListener): () => void;
  subscribe<K extends FrameSection>(section: K, listener: (value: StoreData[K], version: number, data: StoreData) => void): () => void;
  subscribe<K extends FrameSection>(section: K | FrameListener, listener?: (value: StoreData[K], version: number, data: StoreData) => void): () => void {
    const callback: FrameListener = typeof section === 'function' ? section : (data) => {
      const version = this.version(section);
      if (version !== previous) {
        previous = version;
        listener?.(data[section], version, data);
      }
    };
    let previous = typeof section === 'function' ? 0 : this.version(section);
    this.listeners.add(callback);
    return () => { this.listeners.delete(callback); };
  }

  private publish(changes: Partial<StoreData>, accountRead = false): void {
    let next = this.state;
    const replace = <K extends FrameSection>(key: K) => {
      const value = changes[key];
      if (dequal(this.state[key], value)) return;
      next = { ...next, [key]: value };
      this.sectionVersions[key] = this.version(key) + 1;
    };
    for (const key of Object.keys(changes)) replace(key as FrameSection);
    if (next === this.state) return;
    if (!accountRead && (next.orders !== this.state.orders || next.positions !== this.state.positions)) {
      next = { ...next, updateIds: {
        orders: this.state.updateIds.orders + Number(next.orders !== this.state.orders),
        positions: this.state.updateIds.positions + Number(next.positions !== this.state.positions),
      } };
      this.sectionVersions.updateIds = this.version('updateIds') + 1;
    }
    this.state = next;
    const errors: unknown[] = [];
    const versions = this.versions;
    for (const listener of this.listeners) {
      try { listener(next, versions); } catch (error) { errors.push(error); }
    }
    if (errors.length) throw new AggregateError(errors, 'FrameStore subscriber failed');
  }

  /** A transport snapshot establishes a new sequence domain; gaps refuse deltas. */
  ingest(frame: NativeDerivedFrame, { snapshot = false, seq }: { snapshot?: boolean; seq?: number } = {}): void {
    if (seq !== undefined && (!Number.isSafeInteger(seq) || seq < 0)) throw new FrameApplyError('Invalid frame sequence');
    if (!snapshot && (this.needsSnapshot || (seq !== undefined && this.sequence !== undefined && seq !== this.sequence + 1))) {
      this.needsSnapshot = true;
      this.invalidateMarketIdentity();
      throw new FrameApplyError('Frame sequence gap requires snapshot');
    }
    if (snapshot) {
      this.bookFrames.clear();
      this.appliedSections = 0;
      this.marketSymbols = [];
      this.marketIds = [];
      this.marketsNeedSnapshot = false;
      this.recovery.clear();
    }
    this.sequence = seq;
    this.needsSnapshot = false;
    this.apply(frame, true, snapshot);
  }

  reset(): void {
    this.sequence = undefined;
    this.needsSnapshot = false;
    this.appliedSections = 0;
    this.marketSymbols = [];
    this.marketIds = [];
    this.marketsNeedSnapshot = false;
    this.nativeStatsGroups.clear();
    this.bookFrames.clear();
    this.recovery.clear();
    this.publish(initialStoreData, true);
  }


  /** A frame-fed trading engine does not own its source's subscription diagnostics. */
  applyMessageHandlerStats(frame: Frame): void {
    const stats = this.readMessageHandlerStats(frame);
    if (stats) this.publish({ messageHandlerStats: stats });
  }

  private readMessageHandlerStats(frame: Frame): StoreData['messageHandlerStats'] {
    const stats = frame.messageHandlerStats?.();
    if (!stats) return undefined;
    const groups: MessageHandlerStatsType[] = ['public', 'business', 'futures', 'private'];
    const buckets: NonNullable<StoreData['messageHandlerStats']> = {
      public: {},
      business: {},
      futures: {},
      private: {},
      ...this.messageHandlerStats,
    };
    const owned = new Set<MessageHandlerStatsType>();
    for (let i = 0; i < stats.count; i++) owned.add(groups[stats.group(i)]);
    if (stats.replace) {
      for (const group of this.nativeStatsGroups) buckets[group] = {};
      for (const group of owned) buckets[group] = {};
    }
    const copied = new Set<MessageHandlerStatsType>();
    const now = Date.now();
    for (let i = 0; i < stats.count; i++) {
      const group = groups[stats.group(i)];
      const age = stats.ageMs(i);
      if (!copied.has(group)) {
        buckets[group] = { ...buckets[group] };
        copied.add(group);
      }
      buckets[group]![stats.topic(i)] = {
        lastMessageTimestamp: Number.isFinite(age) ? now - age : 0,
        lastCallbacksCount: stats.subscribed(i) && Number.isFinite(age) ? 1 : 0,
        hadHandlers: true,
      };
    }
    if (stats.replace) this.nativeStatsGroups = owned;
    return buckets;
  }

  /** Forwarding workers skip book reconstruction before transferring the frame. */
  apply(frame: NativeDerivedFrame, applyBooks = true, snapshot = false): void {
    const update: { -readonly [K in keyof StoreData]?: StoreData[K] } = snapshot ? { ...initialStoreData } : {};

    const errors: unknown[] = [];
    let accepted = 0;
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
        accepted |= LOADED_FLAG.MARKETS;
        symbols = nextSymbols;
        ids = rows.map((row) => row.id);
      }
    }, LOADED_FLAG.MARKETS);

    const marketsFailed = errors.length > 0 || (this.marketsNeedSnapshot && !(accepted & LOADED_FLAG.MARKETS));
    attempt(() => {
      const stats = this.readMessageHandlerStats(frame);
      if (stats) update.messageHandlerStats = stats;
    });

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
        : this.marketsBySymbol[resolveSymbol(index)]);
      if (rows) { update.tickers = rows; accepted |= LOADED_FLAG.TICKERS; }
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const deltas = readTickerDeltaFrame(frame,
        (update.tickers ?? this.tickers).length, Boolean(tickers));
      if (deltas) update.tickers = this.patchTickers(update.tickers ?? this.tickers, deltas);
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const observations = readTickerObservationFrame(frame, update.tickers ?? this.tickers);
      if (observations) {
        const rows = [...(update.tickers ?? this.tickers)];
        for (const [index, observedAt] of observations) rows[index] = { ...rows[index], observedAt };
        update.tickers = rows;
      }
    }, LOADED_FLAG.TICKERS);

    attempt(() => {
      if (marketsFailed) return;
      const rows = readOrderFrame(frame, this.data, update.markets);
      if (rows) { update.orders = rows; accepted |= LOADED_FLAG.ORDERS; }
    }, LOADED_FLAG.ORDERS);

    attempt(() => {
      if (marketsFailed) return;
      const positions = readPositionFrame(frame, this.data, update.markets);
      if (positions) { update.positions = positions; accepted |= LOADED_FLAG.POSITIONS; }
    }, LOADED_FLAG.POSITIONS);

    attempt(() => {
      const balance = readBalanceFrame(frame);
      if (balance) accepted |= LOADED_FLAG.BALANCE;
      if (balance) update.balance = { ...(update.balance ?? this.balance), ...balance };
    }, LOADED_FLAG.BALANCE);

    attempt(() => {
      const info = frame.balanceInfo?.();
      if (info !== undefined) update.balance = { ...(update.balance ?? this.balance), info };
    });

    attempt(() => {
      const status = frame.status();
      if (status) {
        const statusUpdate = readStoreStatus(status);
        update.loaded = { ...this.loaded, ...statusUpdate.loaded };
        update.latency = statusUpdate.latency;
        update.options = { ...this.options, ...statusUpdate.options };
      }
    });

    attempt(() => {
      if (marketsFailed) return;
      // Per-market leverage and margin type: the order form falls back to
      // `fetchedLeverage` when a market has no position.
      const settings = frame.settings();
      if (settings) {
        update.fetchedLeverage = readStoreSettings(settings,
          (update.options ?? this.options).isHedged, resolveSymbol);
      }
    });

    readNativeDerivedSections(frame, (key, value) => { update[key] = value; }, error => { errors.push(error); });
    this.frameMarkets.set(frame, { symbols, ids });
    if (applyBooks && !marketsFailed) {
      try {
        this.ingestBooks(frame, symbols, ids);
      } catch (error) {
        bookRejected = true;
        // A rejected batch may discard valid deltas too; require fresh baselines.
        this.bookFrames.clear();
        this.bookErrors.set(frame, error);
        this.recovery.reject('books', error, frame, symbols);
        this.frameBooks.set(frame, []);
        errors.push(error);
      }
    } else if (marketsFailed) this.frameBooks.set(frame, []);
    else this.bookFrames.clear();

    this.marketSymbols = symbols;
    this.marketIds = ids;
    // Install proof before notifying startup waiters. Empty validated tables
    // count; rejected sections leave independent accepted peers usable.
    if (accepted & LOADED_FLAG.MARKETS) this.recovery.acceptMarketSnapshot();
    this.appliedSections |= accepted;

    let changed = false;
    for (const _key in update) {
      changed = true;
      break;
    }
    // A current-state read is not another independent venue observation. Its
    // pending stream frame still owns order/position freshness when it arrives.
    if (changed) {
      try {
        this.publish(update, frame.accountRead?.());
      } catch (error) {
        if (!errors.length) throw error;
        errors.push(error);
      }
    }
    if (errors.length) {
      const error = new FrameApplyError(errors.map(String).join('; '), bookRejected, errors.length > (bookRejected ? 1 : 0));
      if (error.otherRejected) this.recovery.reject('frame', error);
      throw error;
    }
  }

  invalidateMarketIdentity(): void {
    this.appliedSections &= ~(LOADED_FLAG.MARKETS | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS | LOADED_FLAG.TICKERS);
    this.marketsNeedSnapshot = true;
    this.bookFrames.clear();
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
    const cached = this.frameBooks.get(frame);
    if (cached) return cached;
    const rows = this.bookFrames.read(frame, symbols, ids);
    this.frameBooks.set(frame, rows);
    this.recovery.acceptBooks(rows);
    return rows;
  }

  candles(frame: Frame): FrameCandle[] {
    if (this.invalidMarketFrames.has(frame)) throw new FrameApplyError('Candle identity requires a market snapshot');
    try {
      const rows = readCandleFrame(frame, this.frameMarkets.get(frame)?.symbols ?? this.marketSymbols);
      this.recovery.acceptCandles(rows);
      return rows;
    } catch (error) {
      this.recovery.reject('candles', error, frame, this.marketSymbolsFor(frame));
      throw error;
    }
  }

  fills(frame: Frame): OrderFillEvent[] {
    try {
      const rows = readFillFrame(frame);
      this.recovery.acceptFills(rows.length > 0);
      return rows;
    } catch (error) {
      this.recovery.reject('fills', error);
      throw new FrameApplyError(error instanceof Error ? error.message : String(error));
    }
  }
}
