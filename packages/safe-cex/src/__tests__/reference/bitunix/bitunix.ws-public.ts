import type { Candle, OHLCVOptions, OrderBook, Timeframe } from '../../../types';
import type { BitunixExchange } from './bitunix.exchange';

import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { resolveTransportRouting } from '../../../transport/registry';
import { BASE_WS_URL, KlineInterval } from './bitunix.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export const BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION = 300;
const BITUNIX_SUBSCRIBE_BATCH_SIZE = 20;
const BITUNIX_SUBSCRIBE_INTERVAL_MS = 250;

export const INTERVAL: Record<Timeframe, KlineInterval> = {
  '1m': '1min',
  '3m': '3min',
  '5m': '5min',
  '15m': '15min',
  '30m': '30min',
  '1h': '60min',
  '2h': '2h',
  '4h': '4h',
  '6h': '6h',
  '12h': '12h',
  '1d': '1day',
  '1w': '1week',
};

export class BitunixPublicWebsocket extends BaseWebSocket<BitunixExchange> {
  messageHandlers: MessageHandlers = {};

  topics: { [key: string]: number } = {};

  pingLoopTimeoutId?: NodeJS.Timeout;

  subscriptionTimeoutId?: NodeJS.Timeout;

  isSecondary = false;

  private messageCount = 0;
  private useFallbackUrl = this.parent.options.tealstreetWsRoute;
  private lastMessageCount = 0;
  private connectionAttempts = 0;
  private readonly MAX_ATTEMPTS = 2;
  private marketSymbols?: Set<string>;
  private subscriptionQueue: Array<Array<Record<string, string>>> = [];

  constructor(parent: BitunixExchange, isSecondary = false, marketSymbols?: string[]) {
    super(parent);
    this.isSecondary = isSecondary;
    this.setMarketSymbols(marketSymbols);
  }

  setMarketSymbols = (marketSymbols?: string[]) => {
    this.marketSymbols = marketSymbols ? new Set(marketSymbols) : undefined;
  };

  private clearSubscriptionQueue = () => {
    clearTimeout(this.subscriptionTimeoutId);
    this.subscriptionTimeoutId = undefined;
    this.subscriptionQueue = [];
  };

  private drainSubscriptionQueue = () => {
    if (this.isDisposed || this.subscriptionQueue.length === 0) {
      this.subscriptionTimeoutId = undefined;
      return;
    }

    const args = this.subscriptionQueue.shift();
    if (args?.length) {
      this.send(JSON.stringify({ op: 'subscribe', args }));
    }

    // Keep a cooldown timer even after the current queue drains. A dynamic
    // subscription arriving immediately afterwards must not create a burst.
    this.subscriptionTimeoutId = setTimeout(this.drainSubscriptionQueue, BITUNIX_SUBSCRIBE_INTERVAL_MS);
  };

  private enqueueSubscriptions = (args: Array<Record<string, string>>, batchSize = BITUNIX_SUBSCRIBE_BATCH_SIZE) => {
    for (let index = 0; index < args.length; index += batchSize) {
      this.subscriptionQueue.push(args.slice(index, index + batchSize));
    }

    if (!this.subscriptionTimeoutId) {
      this.drainSubscriptionQueue();
    }
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.clearSubscriptionQueue();
      // Only switch to fallback URL if we've tried the normal connection multiple times without success
      if (this.connectionAttempts >= this.MAX_ATTEMPTS && this.lastMessageCount === 0 && this.messageCount === 0) {
        this.useFallbackUrl = true;
        this.connectionAttempts = 0; // Reset attempts when switching to fallback
      } else if (!this.useFallbackUrl) {
        this.connectionAttempts++;
      }

      this.lastMessageCount = this.messageCount;
      this.messageCount = 0; // Reset message counter on new connection

      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const fallbackUrl = resolveTransportRouting(this.parent.name, this.parent.options).wsPublic.fallback;
      const url = this.useFallbackUrl && fallbackUrl ? fallbackUrl : BASE_WS_URL.public[key];

      if (this.useFallbackUrl && fallbackUrl) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bitunix proxy (public)');
        } catch (_e) {}
      }

      this.ws = this.createWebsocket(url);

      const handleClose = () => {
        clearTimeout(this.pingLoopTimeoutId);
        this.clearSubscriptionQueue();
        this.onClose();
      };

      const handleError = () => {
        if (this.ws && this.ws.readyState !== WebSocket.CLOSED) {
          try {
            this.ws.close();
          } catch (_e) {}
        }
        handleClose();
      };

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', handleClose);
      this.ws.addEventListener('error', handleError);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      if (!this.isSecondary) {
        this.subscribeMarketsToChannel('ticker');
      }

      if (this.isSecondary) {
        this.subscribeMarketsToChannel('price');
      }

      // this.subscribeToBook('BTCUSDT', 'depth_book1');

      this.resetOrderbook();
      this.subscribe();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);

      if (this.pingLoopTimeoutId) {
        clearTimeout(this.pingLoopTimeoutId);
      }

      const doPingLoop = () => {
        if (!this.isDisposed) {
          this.ping();
          this.pingLoopTimeoutId = setTimeout(doPingLoop, 15_000);
        }
      };
      doPingLoop();
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    const channel = this.isSecondary ? 'price' : 'ticker';
    const args = added
      .filter((market) => !this.marketSymbols || this.marketSymbols.has(market.symbol))
      .map((market) => ({ symbol: market.symbol, ch: channel }));

    // Track topics
    for (const arg of args) {
      const key = JSON.stringify(arg);
      this.topics[key] = 1;
    }

    this.enqueueSubscriptions(args);
  };

  subscribe = () => {
    const topics = Object.keys(this.topics);
    const depthTopics = topics.filter((topic) => JSON.parse(topic).ch.startsWith('depth_'));
    const otherTopics = topics.filter((topic) => !JSON.parse(topic).ch.startsWith('depth_'));

    // Send depth topics individually
    for (const topic of depthTopics) {
      this.enqueueSubscriptions([JSON.parse(topic)], 1);
    }

    this.enqueueSubscriptions(otherTopics.map((topic) => JSON.parse(topic)));
  };

  subscribeMarketsToChannel = (channel: string) => {
    const args = this.parent.store.markets
      .filter((market) => !this.marketSymbols || this.marketSymbols.has(market.symbol))
      .slice(0, BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION)
      .map((market) => ({ symbol: market.symbol, ch: channel }));

    // Track topics
    for (const arg of args) {
      const key = JSON.stringify(arg);
      this.topics[key] = 1;
    }

  };

  subscribeToBook = (symbol: string, sub: 'depth_book1' | 'depth_book5' | 'depth_book15' | 'depth_books') => {
    const key = JSON.stringify({ symbol, ch: sub });
    this.topics[key] = 1;

    this.enqueueSubscriptions([{ symbol, ch: sub }], 1);
  };

  dispose = () => {
    this.parent.emitter.off('markets:changed', this.onMarketsChanged);
    clearTimeout(this.pingLoopTimeoutId);
    this.clearSubscriptionQueue();
    super.dispose();
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.messageCount++; // Increment message counter

      try {
        const parsedData = JSON.parse(data);
        const topic = parsedData.ch;

        if (topic) {
          this.recordMessageHandlerStats(topic);
        }

        if (!topic) {
          //console.log('bitunix: data without topic ', parsedData);
        } else if (topic === 'ticker') {
          this.handleTickerStreamEvent(parsedData);
        } else if (topic === 'price') {
          this.handlePriceStreamEvent(parsedData);
        } else if (topic === 'depth_book1') {
          this.handleDepthBookStreamEvent(parsedData);
        } else if (topic.startsWith('market_kline_')) {
          const arg = { symbol: parsedData.symbol, ch: topic };
          for (const handler of this.messageHandlers[JSON.stringify(arg)]) {
            handler([parsedData]);
          }
        }
      } catch (e) {
        console.error(e);
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ ping: virtualClock.getCurrentTimeMs() }));
    }
  };

  // {"ch":"ticker","symbol":"APTUSDT","ts":1742947821363,"data":{"o":"6.0364","la":"5.9182","h":"6.0996","l":"5.8194","b":"4321250.3","q":"25615096.2681","r":"-1.9564965293"}}
  handleTickerStreamEvent = (event: Record<string, any>) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[event.symbol];

    if (ticker) {
      const data = event.data;
      const lastPrice = parseFloat(v(data, 'la'));
      const openPrice = parseFloat(v(data, 'o'));
      this.store.updateTicker(ticker, {
        last: lastPrice,
        // bid: lastPrice,
        // ask: lastPrice,
        percentage: ((lastPrice - openPrice) / openPrice) * 100,
        volume: parseFloat(v(data, 'b')),
        quoteVolume: parseFloat(v(data, 'q')),
      });
    }
  };

  handlePriceStreamEvent = (event: Record<string, any>) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[event.symbol];

    if (ticker) {
      const data = event.data;
      this.store.updateTicker(ticker, {
        mark: parseFloat(v(data, 'mp')),
        index: parseFloat(v(data, 'ip')),
        fundingRate: parseFloat(v(data, 'fr')),
        fundingTime: nextFundingTime(v(data, 'nft') || v(data, 'ft')),
      });
    }
  };

  handleDepthBookStreamEvent = (event: Record<string, any>) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[event.symbol];

    if (ticker) {
      const data = event.data;
      this.store.updateTicker(ticker, {
        bid: parseFloat(data?.b?.[0]?.[0]) || 0,
        ask: parseFloat(data?.a?.[0]?.[0]) || 0,
      });
    }
  };

  // {"ch":"market_kline_60min","symbol":"BTCUSDT","ts":1743123528882,"data":{"o":"87191.2","c":"87318.9","h":"87364.2","l":"86856.2","b":"2133.1768","q":"185745169.6337"}}

  listenOHLCV = (_opts: OHLCVOptions, _callback: (candle: Candle) => void) => {
    throw new Error('Not implemented');
  };

  listenOrderBook = (_symbol: string, _callback: (orderBook: OrderBook) => void) => {
    throw new Error('Not implemented');
  };
}

/** `nft` is the next settlement as ISO text or epoch milliseconds; parseFloat
 * of ISO text would return only its year. */
function nextFundingTime(value: unknown): number {
  const numeric = Number(value);
  if (Number.isFinite(numeric)) return numeric;
  const parsed = Date.parse(String(value));
  return Number.isFinite(parsed) ? parsed : 0;
}
