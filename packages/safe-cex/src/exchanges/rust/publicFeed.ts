import type { Candle, ExchangeName, ExchangeOptions, Market, OHLCVOptions, OrderBook, Ticker } from '../../types';
import type { RustEngineExchange } from './rust.exchange';

/** Explicitly transfer the shared port when options cross a worker boundary. */
export type RustPublicFeedPolicy = { mode: 'direct' | 'disabled' } | { mode: 'shared'; port: MessagePort };

type FeedUpdate = {
  op: 'publicFeedUpdate';
  tickers?: Array<Partial<Record<Exclude<keyof Ticker, 'id' | 'symbol' | 'info'>, number>> & { symbol: string }>;
  books?: Array<{ symbol: string; bids: number[][]; asks: number[][] }>;
  candles?: Array<Candle & { symbol: string; interval: string }>;
};
type Request =
  | { op: 'hello'; identity: string }
  | { op: 'tickerSymbols'; symbols: string[] }
  | { op: 'snapshot' | 'dispose' | 'ping' }
  | { op: 'book'; symbol: string; subscribe: boolean }
  | { op: 'candle'; symbol: string; interval: OHLCVOptions['interval']; subscribe: boolean };
type FeedMarket = Pick<
  Market,
  'id' | 'symbol' | 'contractSize' | 'isSpot' | 'isPerpetual' | 'isInverse' | 'isExpiring'
>;
const feedMarket = (market: Market): FeedMarket => ({
  id: market.id,
  symbol: market.symbol,
  contractSize: market.contractSize ?? 1,
  isSpot: Boolean(market.isSpot),
  isPerpetual: Boolean(market.isPerpetual),
  isInverse: Boolean(market.isInverse),
  isExpiring: Boolean(market.isExpiring),
});
type Reply = { op: 'ready'; markets: FeedMarket[] } | { op: 'pong' } | { op: 'error'; message: string } | FeedUpdate;

const tickerFields = [
  'bid',
  'ask',
  'last',
  'mark',
  'index',
  'percentage',
  'openInterest',
  'fundingRate',
  'fundingTime',
  'volume',
  'quoteVolume',
] as const;

export const hasSharedTickerValues = (ticker: NonNullable<FeedUpdate['tickers']>[number]): boolean =>
  tickerFields.some((field) => Number.isFinite(ticker[field]));

/** Excludes credentials/account settings; includes all public-product selectors. */
export const rustPublicFeedIdentity = (name: ExchangeName, options: ExchangeOptions) =>
  JSON.stringify([
    name,
    Boolean(options.testnet),
    Boolean(options.enableSpotMarkets),
    Boolean(options.enableInverseMarkets),
    Boolean(options.enableOptionMarkets),
    options.bybitSiteId ?? '',
    options.extra?.hyrotraderMode ?? 'production',
    options.polymarket?.marketSlugs?.slice().sort() ?? null,
  ]);

/** A running public Rust engine owns parsing/subscriptions; ports carry public fields only. */
const sourcesWithProvider = new WeakSet<RustEngineExchange>();

export class RustPublicFeedProvider {
  private readonly clients = new Map<MessagePort, { cleanup(): void; expired(): boolean }>();
  private disposed = false;
  private sweep?: ReturnType<typeof setInterval>;
  private readonly tickerDemands = new Map<MessagePort, string[]>();
  private readonly originalTickerSymbols: string[];

  constructor(private readonly source: RustEngineExchange) {
    if (!source.hasStarted || !source.store.loaded.markets || !source.store.loaded.tickers || source.isDisposed)
      throw new Error('Shared public feed requires a started, loaded Rust source');
    if (!source.store.markets.length || !source.store.tickers.length)
      throw new Error('Shared public feed source has no instruments or tickers');
    if (!source.isPublicDataOnly) throw new Error('Shared public feed requires a public-data-only Rust source');
    if (source.publicFeedMode !== 'direct')
      throw new Error('Shared public feed source must own its direct public feed');
    if (sourcesWithProvider.has(source)) throw new Error('A Rust source already has a public feed provider');
    sourcesWithProvider.add(source);
    this.originalTickerSymbols = [...source.getActiveTickerSymbols()];
  }

  private updateTickerDemand() {
    if (this.source.isDisposed) return;
    const demands = [this.originalTickerSymbols, ...this.tickerDemands.values()];
    this.source.setActiveTickerSymbols(
      demands.some((symbols) => symbols.length === 0) ? [] : [...new Set(demands.flat())],
    );
  }

  attach(port: MessagePort): () => void {
    if (this.disposed || this.source.isDisposed) throw new Error('Shared public feed provider disposed');
    if (this.clients.has(port)) throw new Error('Shared public feed port already attached');
    let ready = false;
    let lastSeen = Date.now();
    const catalog = () =>
      JSON.stringify(this.source.store.markets.map(feedMarket).sort((a, b) => a.symbol.localeCompare(b.symbol)));
    const initialCatalog = catalog();
    const subscriptions = new Map<string, () => void>();
    const send = (message: Reply) => {
      try {
        port.postMessage(message);
      } catch {
        cleanup();
      }
    };
    const seed = () => {
      if (!ready || !this.source.store.loaded.tickers) return;
      const tickers = this.source.store.tickers.map((ticker) => {
        const row: NonNullable<FeedUpdate['tickers']>[number] = { symbol: ticker.symbol };
        for (const field of tickerFields) if (Number.isFinite(ticker[field])) row[field] = ticker[field];
        return row;
      });
      send({ op: 'publicFeedUpdate', tickers });
    };
    const unsubscribe = this.source.observePublicFeed({
      updated: (catalogChanged) => {
        if (catalogChanged && catalog() !== initialCatalog) {
          send({ op: 'error', message: 'Shared public feed catalog changed; restart provider and accounts' });
          cleanup();
        } else seed();
      },
      closed: () => {
        send({ op: 'error', message: 'Shared public feed source disposed' });
        cleanup();
      },
    });
    const cleanup = () => {
      if (!this.clients.delete(port)) return;
      if (!this.clients.size) {
        clearInterval(this.sweep);
        this.sweep = undefined;
      }
      unsubscribe();
      this.tickerDemands.delete(port);
      this.updateTickerDemand();
      for (const stop of subscriptions.values()) stop();
      subscriptions.clear();
      port.removeEventListener('message', onMessage);
      port.removeEventListener('messageerror', onError);
      port.close();
    };
    const onError = () => cleanup();
    const onMessage = (event: MessageEvent<Request>) => {
      const data = event.data;
      if (!data || typeof data !== 'object') return;
      lastSeen = Date.now();
      if (data.op === 'hello') {
        if (data.identity !== rustPublicFeedIdentity(this.source.name, this.source.options)) {
          send({ op: 'error', message: 'Shared public feed venue/network/product mismatch' });
          cleanup();
          return;
        }
        ready = true;
        send({ op: 'ready', markets: this.source.store.markets.map(feedMarket) });
        seed();
      } else if (data.op === 'dispose') cleanup();
      else if (ready && data.op === 'tickerSymbols') {
        if (!Array.isArray(data.symbols) || data.symbols.some((symbol) => typeof symbol !== 'string')) return;
        this.tickerDemands.set(port, data.symbols);
        this.updateTickerDemand();
      } else if (ready && data.op === 'ping') send({ op: 'pong' });
      else if (ready && data.op === 'snapshot') seed();
      else if (ready && (data.op === 'book' || data.op === 'candle')) {
        if (typeof data.symbol !== 'string' || !this.source.store.marketsBySymbol[data.symbol]) {
          send({ op: 'error', message: 'Shared public feed symbol unavailable' });
          return;
        }
        const interval = data.op === 'candle' ? this.source.candleIntervalForFeed(data.interval) : '1m';
        const key = data.op === 'book' ? `book:${data.symbol}` : `candle:${data.symbol}:${interval}`;
        if (!data.subscribe) {
          subscriptions.get(key)?.();
          subscriptions.delete(key);
          return;
        }
        if (subscriptions.has(key)) return;
        if (data.op === 'book') {
          const symbol = data.symbol;
          const forward = (book: OrderBook) =>
            send({
              op: 'publicFeedUpdate',
              books: [
                {
                  symbol,
                  bids: book.bids.map(({ price, amount }) => [price, amount]),
                  asks: book.asks.map(({ price, amount }) => [price, amount]),
                },
              ],
            });
          subscriptions.set(key, this.source.listenOrderBook(symbol, forward));
          const snapshot = this.source.snapshotOrderBook(symbol);
          if (snapshot) forward(snapshot);
        } else {
          const { symbol } = data;
          subscriptions.set(
            key,
            this.source.listenOHLCV({ symbol, interval }, (candle) =>
              send({
                op: 'publicFeedUpdate',
                candles: [
                  {
                    symbol,
                    interval: this.source.candleIntervalForFeed(interval),
                    timestamp: candle.timestamp,
                    open: candle.open,
                    high: candle.high,
                    low: candle.low,
                    close: candle.close,
                    volume: candle.volume,
                  },
                ],
              }),
            ),
          );
        }
      }
    };
    this.clients.set(port, { cleanup, expired: () => Date.now() - lastSeen > (ready ? 20_000 : 10_000) });
    this.sweep ??= setInterval(() => {
      for (const client of this.clients.values()) if (client.expired()) client.cleanup();
    }, 5_000);
    port.addEventListener('message', onMessage);
    port.addEventListener('messageerror', onError);
    port.start();
    return cleanup;
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    sourcesWithProvider.delete(this.source);
    for (const [port, client] of this.clients) {
      try {
        port.postMessage({ op: 'error', message: 'Shared public feed provider disposed' } satisfies Reply);
      } catch {
        // A closed port still needs its subscriptions released.
      } finally {
        client.cleanup();
      }
    }
  }
}

/** Account-side lifecycle; feed data always enters the native engine before frames. */
export class RustPublicFeedClient {
  private disposed = false;
  private ready = false;
  private catalog = new Map<string, FeedMarket>();
  private timer?: ReturnType<typeof setTimeout>;
  private heartbeat?: ReturnType<typeof setInterval>;
  private lastPong = 0;
  private reject?: (error: Error) => void;
  private resolve?: () => void;
  constructor(
    private readonly port: MessagePort,
    private readonly identity: string,
    private readonly ingest: (update: FeedUpdate) => Promise<unknown>,
    private readonly fail: (error: Error) => void,
  ) {}

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
      this.port.addEventListener('message', this.receive);
      this.port.addEventListener('messageerror', this.messageError);
      this.port.start();
      this.timer = setTimeout(() => this.failed(new Error('Shared public feed handshake timed out')), 10_000);
      this.send({ op: 'hello', identity: this.identity });
    });
  }
  private failed = (error: Error) => {
    if (this.disposed) return;
    this.reject?.(error);
    this.fail(error);
    this.dispose();
  };
  private messageError = () => this.failed(new Error('Shared public feed message error'));
  private receive = (event: MessageEvent<Reply>) => {
    if (this.disposed) return;
    const message = event.data;
    if (!message || typeof message !== 'object') return;
    if (message.op === 'ready') {
      if (
        !Array.isArray(message.markets) ||
        message.markets.some((market) => !market || typeof market.symbol !== 'string')
      ) {
        this.failed(new Error('Invalid shared public feed catalog'));
        return;
      }
      this.catalog = new Map(message.markets.map((market) => [market.symbol, market]));
      this.ready = true;
      this.lastPong = Date.now();
      if (!this.heartbeat)
        this.heartbeat = setInterval(() => {
          if (Date.now() - this.lastPong > 15_000) this.failed(new Error('Shared public feed connection lost'));
          else this.send({ op: 'ping' });
        }, 5_000);
      clearTimeout(this.timer);
      this.resolve?.();
      this.reject = undefined;
    } else if (message.op === 'pong') this.lastPong = Date.now();
    else if (message.op === 'error') this.failed(new Error(message.message));
    else if (this.ready && message.op === 'publicFeedUpdate') void this.ingest(message).catch(this.failed);
  };
  validateMarkets(markets: Market[]): void {
    for (const market of markets) {
      if (JSON.stringify(this.catalog.get(market.symbol)) !== JSON.stringify(feedMarket(market))) {
        throw new Error(`Shared public feed catalog/product mismatch: ${market.symbol}`);
      }
    }
  }
  send(message: Request) {
    if (!this.disposed) {
      try {
        this.port.postMessage(message);
      } catch (error) {
        this.failed(error instanceof Error ? error : new Error(String(error)));
      }
    }
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    try {
      this.port.postMessage({ op: 'dispose' } satisfies Request);
    } catch {
      /* Already disconnected. */
    }
    clearTimeout(this.timer);
    clearInterval(this.heartbeat);
    this.reject?.(new Error('Shared public feed disposed during startup'));
    this.port.removeEventListener('message', this.receive);
    this.port.removeEventListener('messageerror', this.messageError);
    this.port.close();
  }
}
