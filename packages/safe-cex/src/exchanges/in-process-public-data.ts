import type { Candle, OHLCVOptions, OrderBook, PublicDataSource, StoreData } from '../types';
import type { BaseExchange } from './base';

import { LogSeverity } from '../types';
import { EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS } from './external-public-data';

/**
 * In-process public data provider that forwards data from a running
 * Exchange instance (the "source") to a dependent exchange (the "parent").
 *
 * This enables sharing a single public-data exchange connection across N
 * authenticated instances. For example, 100 Bybit accounts can share one
 * public WebSocket connection instead of each opening their own.
 *
 * The provider:
 * - Subscribes to the source's store for tickers and markets
 * - Delegates orderbook/candle subscriptions to the source's listen* methods
 * - Provides the same interface shape as ExternalPublicDataProvider
 */
export class InProcessPublicDataProvider {
  private parent: BaseExchange;
  private source: PublicDataSource;
  private unsubscribeStore?: () => void;
  private isDisposed = false;
  private initialized = false;
  private lastReceived = 0;

  constructor(parent: BaseExchange, source: PublicDataSource) {
    this.parent = parent;
    this.source = source;
  }

  /**
   * Connect to the source exchange by subscribing to its store updates.
   * Immediately seeds the parent with current tickers/markets if available.
   */
  connect = () => {
    if (this.isDisposed) return;

    this.log('Connecting to in-process public data source', LogSeverity.Info);

    // Seed with current data if source already has it
    const { tickers, markets, loaded } = this.source.store;

    if (loaded.tickers && tickers.length > 0) {
      this.noteDataObserved();
      this.parent.store.update({
        tickers,
        loaded: { ...this.parent.store.loaded, tickers: true },
      });
    }

    if (loaded.markets && markets.length > 0) {
      this.noteDataObserved();
      this.parent.store.update({
        markets,
        loaded: { ...this.parent.store.loaded, markets: true },
      });
    }

    // Subscribe to ongoing updates.
    // Hot-path guard: the source store mutates ticker/market fields in
    // place and notifies on every delta. The parent's tickerIndexMap and
    // hashmaps already point to the same ticker objects (shared refs via
    // `stripTickerInfo` returning the same reference when there's no
    // `info`), so per-frame mutations are visible without re-running
    // `parent.store.update({ tickers })` — which would otherwise rebuild
    // the parent's maps from scratch on every source notify.
    //
    // Only propagate a full update when the array *reference* or length
    // actually changes (snapshot replacement, ticker added, ticker
    // removed). On Bybit linear this avoids ~600 full-rebuild
    // propagations/sec, each touching ~600 hashmap entries.
    let lastTickers: typeof this.source.store.tickers | undefined;
    let lastMarkets: typeof this.source.store.markets | undefined;

    this.unsubscribeStore = this.source.store.subscribe((data: StoreData) => {
      if (this.isDisposed) return;

      const sourceTickers = data.tickers;
      const sourceMarkets = data.markets;
      const hasTickers = !!sourceTickers && sourceTickers.length > 0;
      const hasMarkets = !!sourceMarkets && sourceMarkets.length > 0;
      if (!hasTickers && !hasMarkets) return;

      // Stamp liveness BEFORE the change filter below. The source mutates
      // ticker fields in place and only trips that filter when the array
      // reference or length changes, so a filtered notify is still proof the
      // source is delivering — stamping after it would report a live feed as
      // stale within seconds.
      this.noteDataObserved();

      const tickersChanged =
        hasTickers && (sourceTickers !== lastTickers || sourceTickers.length !== lastTickers?.length);
      const marketsChanged =
        hasMarkets && (sourceMarkets !== lastMarkets || sourceMarkets.length !== lastMarkets?.length);

      if (!tickersChanged && !marketsChanged) return;

      if (tickersChanged) lastTickers = sourceTickers;
      if (marketsChanged) lastMarkets = sourceMarkets;

      this.parent.store.update({
        ...(tickersChanged ? { tickers: sourceTickers } : {}),
        ...(marketsChanged ? { markets: sourceMarkets } : {}),
        loaded: {
          ...this.parent.store.loaded,
          ...(tickersChanged ? { tickers: true } : {}),
          ...(marketsChanged ? { markets: true } : {}),
        },
      });
    });

    this.log('Connected to in-process public data source', LogSeverity.Info);
  };

  /**
   * Subscribe to orderbook updates for a symbol.
   * Delegates to the source exchange's listenOrderBook method.
   */
  subscribeOrderbook = (symbol: string, callback: (orderbook: OrderBook) => void): (() => void) => {
    return this.source.listenOrderBook(symbol, callback);
  };

  /**
   * Subscribe to candle updates for a symbol/interval.
   * Delegates to the source exchange's listenOHLCV method.
   */
  subscribeCandle = (opts: OHLCVOptions, callback: (candle: Candle) => void): (() => void) => {
    return this.source.listenOHLCV(opts, callback);
  };

  /**
   * Check if the provider is connected (source store subscription is active).
   */
  get isConnected(): boolean {
    return !this.isDisposed && !!this.unsubscribeStore;
  }

  /**
   * These three mirror `ExternalPublicDataProvider` so the store's socket-health
   * bridge can treat both providers alike. Without them a hosted account
   * reported `publicInitialized: false` forever and the connection popover sat
   * on "Waiting" while public data was arriving normally, because the account
   * has no public socket of its own — the shared exchange in the same process
   * feeds it through here.
   */
  get isInitialized(): boolean {
    return this.initialized;
  }

  get isHealthy(): boolean {
    if (!this.isConnected || this.lastReceived === 0) return false;
    return Date.now() - this.lastReceived < EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS;
  }

  get lastMessageTimestamp(): number | undefined {
    return this.lastReceived > 0 ? this.lastReceived : undefined;
  }

  private noteDataObserved = () => {
    this.initialized = true;
    this.lastReceived = Date.now();
  };

  /**
   * Dispose the provider and clean up all subscriptions.
   */
  dispose = () => {
    this.isDisposed = true;

    if (this.unsubscribeStore) {
      this.unsubscribeStore();
      this.unsubscribeStore = undefined;
    }

    this.log('Disposed in-process public data provider', LogSeverity.Info);
  };

  private log = (message: string, severity: LogSeverity) => {
    this.parent.emitter.emit('log', `[${this.parent.name}:InProcess] ${message}`, severity);
  };
}
