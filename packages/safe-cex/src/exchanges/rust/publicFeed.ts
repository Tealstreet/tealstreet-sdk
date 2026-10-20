import type {
  Candle,
  ExchangeName,
  ExchangeOptions,
  Market,
  OHLCVOptions,
  OrderBook,
  PublicDataSource,
  Ticker,
} from '../../types';
import type { RustEngineExchange } from './rust.exchange';

/** Explicitly transfer the shared port when options cross a worker boundary. */
export type RustPublicFeedPolicy = { mode: 'direct' | 'disabled' } | { mode: 'shared'; port: MessagePort };

type FeedUpdate = {
  op: 'publicFeedUpdate';
  tickers?: Array<
    Partial<Record<Exclude<keyof Ticker, 'id' | 'symbol' | 'info' | 'midPrice'>, number>> & {
      symbol: string;
      midPrice?: number | null;
    }
  >;
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
> & {
  symbolType?: string;
  marketRegion?: string;
  underlyingTicker?: string;
  instrumentKind?: string;
  underlying?: string;
  expiry?: number;
  strike?: number;
  option?: Market['option'];
};
const stringValue = (value: unknown) => (typeof value === 'string' ? value : undefined);
const numberValue = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : undefined);
const feedMarket = (market: Market): FeedMarket => ({
  id: market.id,
  symbol: market.symbol,
  contractSize: market.contractSize ?? 1,
  isSpot: Boolean(market.isSpot),
  isPerpetual: Boolean(market.isPerpetual),
  isInverse: Boolean(market.isInverse),
  isExpiring: Boolean(market.isExpiring),
  symbolType: stringValue(market.info?.symbolType),
  marketRegion: stringValue(market.info?.marketRegion),
  underlyingTicker: stringValue(market.info?.underlyingTicker),
  instrumentKind: stringValue(market.info?.instrumentKind),
  underlying: stringValue(market.info?.underlying),
  expiry: numberValue(market.info?.expiry),
  strike: numberValue(market.info?.strike),
  ...(market.option && {
    option: {
      underlying: market.option.underlying,
      expiry: market.option.expiry,
      strike: market.option.strike,
      right: market.option.right,
      style: market.option.style,
      settlement: market.option.settlement,
      settleAsset: market.option.settleAsset,
      premiumAsset: market.option.premiumAsset,
    },
  }),
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
        const row: NonNullable<FeedUpdate['tickers']>[number] = {
          symbol: ticker.symbol,
          midPrice: Number.isFinite(ticker.midPrice) ? ticker.midPrice! : null,
        };
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
        const interval = data.op === 'candle' ? data.interval : '1m';
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
          subscriptions.set(key, this.source.listenOrderBookForFeed(symbol, forward));
          const snapshot = this.source.snapshotOrderBook(symbol);
          if (snapshot) forward(snapshot);
        } else {
          const { symbol } = data;
          subscriptions.set(
            key,
            this.source.listenOHLCVForFeed({ symbol, interval }, (candle) =>
              send({
                op: 'publicFeedUpdate',
                candles: [
                  {
                    symbol,
                    interval,
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
    private readonly policy: { handshakeTimeoutMs?: number; inProcess?: boolean } = {},
  ) {}

  get isReady() {
    return this.ready;
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
      this.port.addEventListener('message', this.receive);
      this.port.addEventListener('messageerror', this.messageError);
      this.port.start();
      // An in-process source that has not loaded must not take private startup with it, as in TS.
      this.timer = setTimeout(() => {
        if (!this.policy.inProcess) this.failed(new Error('Shared public feed handshake timed out'));
        else {
          this.resolve?.();
          this.reject = undefined;
        }
      }, this.policy.handshakeTimeoutMs ?? 10_000);
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
      const shared = this.catalog.get(market.symbol);
      // exchange-server's source can trail the account's own catalog by a listing.
      if (!shared && this.policy.inProcess) continue;
      if (JSON.stringify(shared) !== JSON.stringify(feedMarket(market))) {
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

type Listener = (event: MessageEvent) => void;

/** One end of an in-memory port pair: same-realm hosts need no structured clone. */
class InProcessPort {
  peer?: InProcessPort;
  private closed = false;
  private readonly listeners = new Set<Listener>();
  postMessage(data: unknown) {
    if (this.closed) throw new Error('In-process public feed port closed');
    const peer = this.peer;
    // Asynchronous like a MessagePort, so neither side re-enters the other.
    queueMicrotask(() => {
      if (!peer || peer.closed) return;
      for (const listener of [...peer.listeners]) listener({ data } as MessageEvent);
    });
  }
  addEventListener(type: string, listener: Listener) {
    if (type === 'message') this.listeners.add(listener);
  }
  removeEventListener(type: string, listener: Listener) {
    if (type === 'message') this.listeners.delete(listener);
  }
  start() {}
  close() {
    this.closed = true;
    this.listeners.clear();
  }
}

/** A connected pair of in-memory ports for a same-realm public feed. */
export const inProcessPortPair = (): [MessagePort, MessagePort] => {
  const a = new InProcessPort();
  const b = new InProcessPort();
  a.peer = b;
  b.peer = a;
  return [a as unknown as MessagePort, b as unknown as MessagePort];
};

/** In-process sources load before handing data over; TS waits as long. */
export const IN_PROCESS_PUBLIC_FEED_TIMEOUT_MS = 30_000;
const IN_PROCESS_TICKER_INTERVAL_MS = 250;
const IN_PROCESS_CLIENT_EXPIRY_MS = 20_000;

type TickerRow = NonNullable<FeedUpdate['tickers']>[number];
const tickerRow = (ticker: Ticker): TickerRow => {
  const row: TickerRow = {
    symbol: ticker.symbol,
    midPrice: Number.isFinite(ticker.midPrice) ? ticker.midPrice! : null,
  };
  for (const field of tickerFields) if (Number.isFinite(ticker[field])) row[field] = ticker[field];
  return row;
};
const sameRow = (a: TickerRow, b: TickerRow) =>
  a.midPrice === b.midPrice && tickerFields.every((field) => a[field] === b[field]);

type HubClient = {
  ready: boolean;
  lastSeen: number;
  send(message: Reply): void;
  resubscribe(): void;
  cleanup(): void;
};

/** One per source: a single store subscription and one ticker delta per tick for every account. */
class InProcessHub {
  readonly clients = new Set<HubClient>();
  catalogSymbols = new Set<string>();
  private catalog = '';
  private markets: FeedMarket[] = [];
  private lastMarkets?: Market[];
  private rows = new Map<string, TickerRow>();
  private tickerTimer?: ReturnType<typeof setTimeout>;
  private sweep?: ReturnType<typeof setInterval>;
  private readonly unsubscribe: () => void;

  constructor(
    private readonly source: PublicDataSource,
    private readonly release: () => void,
  ) {
    this.unsubscribe = source.store.subscribe(() => this.updated());
    this.sweep = setInterval(() => {
      const now = Date.now();
      for (const client of this.clients)
        if (client.ready && now - client.lastSeen > IN_PROCESS_CLIENT_EXPIRY_MS) client.cleanup();
    }, 5_000);
  }

  read<T>(fn: () => T): T | undefined {
    try {
      return fn();
    } catch {
      // The source throws while it restarts; the next notify retries.
      return undefined;
    }
  }

  get loaded() {
    return Boolean(
      this.read(
        () =>
          this.source.store.loaded.markets && this.source.store.loaded.tickers && this.source.store.markets.length > 0,
      ),
    );
  }

  /** Re-reads the catalog; true when it changed and every ready client was re-announced. */
  private refreshCatalog(): boolean {
    const markets = this.read(() => this.source.store.markets);
    if (!markets?.length || markets === this.lastMarkets) return false;
    this.lastMarkets = markets;
    const next = markets.map(feedMarket);
    const text = JSON.stringify(next);
    if (text === this.catalog) return false;
    this.catalog = text;
    this.markets = next;
    this.catalogSymbols = new Set(next.map((market) => market.symbol));
    return true;
  }

  private fullSeed(): Reply | undefined {
    const tickers = this.read(() => this.source.store.tickers);
    if (!tickers?.length) return undefined;
    this.rows = new Map(tickers.map((ticker) => [ticker.symbol, tickerRow(ticker)]));
    return { op: 'publicFeedUpdate', tickers: [...this.rows.values()] };
  }

  /** Announces the catalog and a full seed to one client, once the source has loaded. */
  greet(client: HubClient) {
    if (!this.loaded) return;
    this.refreshCatalog();
    if (!this.catalog) return;
    client.ready = true;
    client.lastSeen = Date.now();
    client.send({ op: 'ready', markets: this.markets });
    this.seed(client);
    client.resubscribe();
  }

  seed(client: HubClient) {
    const tickers = this.read(() => this.source.store.tickers);
    if (!tickers?.length) return;
    client.send({ op: 'publicFeedUpdate', tickers: tickers.map(tickerRow) });
  }

  private updated() {
    if (!this.loaded) return;
    if (this.refreshCatalog()) {
      const seed = this.fullSeed();
      for (const client of this.clients) {
        client.ready = true;
        client.send({ op: 'ready', markets: this.markets });
        if (seed) client.send(seed);
        client.resubscribe();
      }
      return;
    }
    for (const client of this.clients) if (!client.ready) this.greet(client);
    this.tickerTimer ??= setTimeout(this.tick, IN_PROCESS_TICKER_INTERVAL_MS);
  }

  private tick = () => {
    this.tickerTimer = undefined;
    const tickers = this.read(() => this.source.store.tickers);
    if (!tickers?.length) return;
    const changed: TickerRow[] = [];
    for (const ticker of tickers) {
      const row = tickerRow(ticker);
      const previous = this.rows.get(row.symbol);
      if (previous && sameRow(previous, row)) continue;
      this.rows.set(row.symbol, row);
      changed.push(row);
    }
    if (!changed.length) return;
    const update: Reply = { op: 'publicFeedUpdate', tickers: changed };
    for (const client of this.clients) if (client.ready) client.send(update);
  };

  remove(client: HubClient) {
    this.clients.delete(client);
    if (this.clients.size) return;
    clearTimeout(this.tickerTimer);
    clearInterval(this.sweep);
    this.unsubscribe();
    this.release();
  }
}

const inProcessHubs = new WeakMap<PublicDataSource, InProcessHub>();

/**
 * Serves the shared-feed protocol from an in-process `PublicDataSource`
 * (exchange-server's per-venue public exchange). The source survives its own
 * restarts, so accounts keep running; catalog changes re-send `ready`, and
 * book/candle requests for symbols the source has not announced wait for it.
 */
export const serveInProcessPublicData = (source: PublicDataSource, port: MessagePort): (() => void) => {
  let hub = inProcessHubs.get(source);
  if (!hub) {
    hub = new InProcessHub(source, () => inProcessHubs.delete(source));
    inProcessHubs.set(source, hub);
  }
  const owner = hub;
  let disposed = false;
  // key -> request; a null stop means the symbol is not in the source catalog yet.
  const subscriptions = new Map<string, { request: Extract<Request, { op: 'book' | 'candle' }>; stop?: () => void }>();
  const subscribe = (entry: { request: Extract<Request, { op: 'book' | 'candle' }>; stop?: () => void }) => {
    const { request } = entry;
    const symbol = request.symbol;
    if (entry.stop || !owner.catalogSymbols.has(symbol)) return;
    entry.stop =
      request.op === 'book'
        ? owner.read(() =>
            source.listenOrderBook(symbol, (book) =>
              client.send({
                op: 'publicFeedUpdate',
                books: [
                  {
                    symbol,
                    bids: book.bids.map(({ price, amount }) => [price, amount]),
                    asks: book.asks.map(({ price, amount }) => [price, amount]),
                  },
                ],
              }),
            ),
          )
        : owner.read(() =>
            source.listenOHLCV({ symbol, interval: request.interval }, (candle) =>
              client.send({
                op: 'publicFeedUpdate',
                candles: [
                  {
                    symbol,
                    interval: request.interval,
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
  };
  const client: HubClient = {
    ready: false,
    lastSeen: Date.now(),
    send: (message) => {
      try {
        port.postMessage(message);
      } catch {
        cleanup();
      }
    },
    resubscribe: () => {
      for (const entry of subscriptions.values()) subscribe(entry);
    },
    cleanup: () => cleanup(),
  };
  const cleanup = () => {
    if (disposed) return;
    disposed = true;
    for (const { stop } of subscriptions.values()) if (stop) owner.read(stop);
    subscriptions.clear();
    port.removeEventListener('message', onMessage);
    port.close();
    owner.remove(client);
  };
  const onMessage = (event: MessageEvent<Request>) => {
    const data = event.data;
    if (disposed || !data || typeof data !== 'object') return;
    client.lastSeen = Date.now();
    if (data.op === 'hello') owner.greet(client);
    else if (data.op === 'dispose') cleanup();
    else if (data.op === 'ping') client.send({ op: 'pong' });
    else if (client.ready && data.op === 'snapshot') owner.seed(client);
    else if (data.op === 'book' || data.op === 'candle') {
      if (typeof data.symbol !== 'string') return;
      const key = data.op === 'book' ? `book:${data.symbol}` : `candle:${data.symbol}:${data.interval}`;
      const existing = subscriptions.get(key);
      if (!data.subscribe) {
        if (existing?.stop) owner.read(existing.stop);
        subscriptions.delete(key);
        return;
      }
      if (existing) return;
      const entry = { request: data };
      subscriptions.set(key, entry);
      subscribe(entry);
    }
  };
  owner.clients.add(client);
  port.addEventListener('message', onMessage);
  port.start();
  return cleanup;
};
