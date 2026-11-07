import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, OHLCVOptions, OrderBook } from '../../../types';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { adjust } from '../../../utils/safe-math';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { HyperliquidSpotExchange } from './hyperliquidspot.exchange';
import { AllMids, HyperliquidCandle, WsBook, WSS_URLS } from './hyperliquidspot.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: any) => void>;
};

type SubscribedTopics = {
  [id: string]: number;
};

export class HyperliquidSpotPublicWebSocket extends BaseWebSocket<HyperliquidSpotExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};

  constructor(parent: HyperliquidSpotExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.resetOrderbook();
      this.subscribePublicTopics();
      this.ping();
    }
  };

  subscribePublicTopics = () => {
    // Subscribe to allMids (public market data)
    this.send(
      JSON.stringify({
        method: 'subscribe',
        subscription: {
          type: 'allMids',
        },
      }),
    );

    // Subscribe to user-requested topics (candles, orderbooks)
    this.subscribeUserTopics();
  };

  subscribeUserTopics = () => {
    const topics = flatten(Object.keys(this.topics));
    for (const topic of topics) {
      const [id, symbol, interval] = topic.split('.');

      if (id === 'candle') {
        const payload = {
          method: 'subscribe',
          subscription: {
            type: 'candle',
            coin: symbol,
            interval: interval,
          },
        };
        this.send(JSON.stringify(payload));
      } else if (id === 'l2Book') {
        const payload = {
          method: 'subscribe',
          subscription: {
            type: 'l2Book',
            coin: symbol,
          },
        };
        this.send(JSON.stringify(payload));
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ method: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        // The exclusion read 'l2book' while the channel is 'l2Book', so a bare
        // shared `l2Book` entry was recorded after all — matching no
        // `messageHandlers` key, therefore invisible to
        // `reconcileMessageHandlerStats` and reported as a live feed forever.
        // `candle` is the same shape and is recorded per (symbol, interval) in
        // the listenOHLCV parser instead.
        //
        // Recording `l2Book.{coin}` instead was tried and reverted: per-symbol
        // keys let `getWarningThreshold` resolve a ticker, and its
        // `Math.log(Math.max(1, quoteVolume / 10_000))` denominator is 0 for any
        // pair under ~$10k daily volume, giving an INFINITE threshold. That is
        // strictly worse than no signal. Until that is fixed this venue has no
        // orderbook liveness — the perp side is in the same position minus bbo,
        // which spot does not have.
        if (json?.channel !== 'l2Book' && json?.channel !== 'candle' && json?.channel !== 'error') {
          this.recordMessageHandlerStats(json?.channel);
        }
        this.recordMessageHandlerStats('pong');

        switch (json?.channel) {
          case 'pong':
            this.handlePongEvent();
            break;
          case 'allMids':
            this.handleAllMids(json?.data?.mids || {});
            break;
          case 'candle': {
            // Sample first entry → look up exact (coin, interval) handler.
            const candleData = json?.data;
            const sample = Array.isArray(candleData) ? candleData[0] : candleData;
            if (!sample?.s || !sample?.i) break;
            const handlers = this.messageHandlers[`candle.${sample.s}.${sample.i}`];
            if (handlers) {
              const payload = candleData || [];
              // Indexed for-loop avoids iterator allocation per frame.
              for (let i = 0; i < handlers.length; i++) handlers[i](payload);
            }
            break;
          }
          case 'l2Book': {
            const coin = json?.data?.coin;
            if (!coin) break;

            // Direct Map lookup, not Object.keys.filter.
            const handlers = this.messageHandlers[`l2Book.${coin}`];
            if (handlers) {
              const payload = json?.data || [];
              for (let i = 0; i < handlers.length; i++) {
                try {
                  handlers[i](payload);
                } catch (_e) {}
              }
            }
            break;
          }
        }
      } catch (_e) {}
    }
  };

  // Reusable two-field update buffer — same pattern as the perp
  // public WS's `_midsUpdateBuf`. Avoids per-ticker allocations on
  // the allMids hot path. `allMids` IS Hyperliquid's published
  // mid (`midPx`), so we set both `last` and `midPrice` for the
  // cli `$mid` resolver and any other consumer that prefers the
  // canonical mid.
  private _midsUpdateBuf = { last: 0, midPrice: 0 };

  handleAllMids = (data: AllMids) => {
    // O(1) hashmap lookup; for...in avoids the Object.entries tuple-array
    // allocation per allMids frame (~600+ entries).
    const tickersById = this.parent.store.tickersById;
    const updateBuf = this._midsUpdateBuf;
    for (const tickerPrefix in data) {
      const ticker = tickersById[tickerPrefix];
      if (!ticker) continue;
      const last = parseFloat(data[tickerPrefix]);
      updateBuf.last = last;
      updateBuf.midPrice = last;
      this.parent.store.updateTicker(ticker, updateBuf);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const symbol = this.parent.store.markets.find((market) => market.symbol === opts.symbol)?.id || '';
    const topic = `candle.${symbol}.${opts.interval}`;
    const handler = `candle.${symbol}.${opts.interval}`;

    const parser = (data: HyperliquidCandle[]) => {
      // Indexed for-loop + hoisted stats record (the dispatcher already
      // routes only matching frames here by exact `candle.{s}.{i}` key,
      // so any frame reaching this parser is "active"; record once).
      const entries = Array.isArray(data) ? data : [data];
      this.recordMessageHandlerStats(topic);
      for (let i = 0; i < entries.length; i++) {
        const entry = entries[i];
        if (symbol === entry.s && entry.i === opts.interval) {
          callback({
            timestamp: entry.t / 1000,
            open: parseFloat(entry.o),
            high: parseFloat(entry.h),
            low: parseFloat(entry.l),
            close: parseFloat(entry.c),
            volume: parseFloat(entry.v),
          });
        }
      }
    };

    let waitingTimeout: ReturnType<typeof setTimeout> | undefined;
    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;

        const payload = {
          method: 'subscribe',
          subscription: {
            type: 'candle',
            coin: symbol,
            interval: opts.interval,
          },
        };
        this.send(JSON.stringify(payload));
      } else {
        waitingTimeout = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      // Cancel any pending waitForConnectedAndSubscribe polling
      if (waitingTimeout) {
        clearTimeout(waitingTimeout);
        waitingTimeout = undefined;
      }

      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.cleanupMessageHandlerStats(handler);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const market = this.parent.store.markets.find((market) => market.symbol === symbol);
    if (!market) {
      this.parent.emitAndLogError(`Market with symbol ${symbol} not found`, 'listenOrderBook');
      throw new Error(`Market with symbol ${symbol} not found`);
    }
    const subSymbol = market.id;
    const pricePrecision = market.precision.price;
    if (!pricePrecision) {
      this.parent.emitAndLogError(`Market with symbol ${symbol} has no price precision`, 'listenOrderBook');
      throw new Error(`Market with symbol ${symbol} has no price precision`);
    }

    const topic = `l2Book.${subSymbol}`;
    const handler = `l2Book.${subSymbol}`;

    const innerId = nanoid();

    const groupingLevels = [0, 1, 2, 3, 4, 5, 6];

    const books: {
      [grouping: number]: OrderBook;
    } = groupingLevels.reduce(
      (acc, grouping) => {
        acc[grouping] = {
          bids: [],
          asks: [],
        };
        return acc;
      },
      {} as { [grouping: number]: OrderBook },
    );

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const parser = (json: Data) => {
      const event = json as WsBook;

      const bidPrice1 = event.levels[0][0]?.px;
      const bidPrice2 = event.levels[0][1]?.px;
      if (!bidPrice1 || !bidPrice2) {
        this.parent.log(`Order book for ${subSymbol} is empty`);
        return;
      }

      const apparentStep = adjust(Math.abs(parseFloat(bidPrice1) - parseFloat(bidPrice2)), pricePrecision);

      const precisionMult = Math.floor(apparentStep / pricePrecision);
      const n = Math.round(Math.log10(precisionMult));
      if (n < groupingLevels[0] || n > groupingLevels[groupingLevels.length - 1]) {
        this.emitAndLogError(`Order book for ${subSymbol} has invalid grouping level ${n}`, undefined, {
          book: JSON.stringify(event.levels),
        });
      }

      const orderBook = books[n];
      if (!orderBook) {
        this.parent.log(`Order book for ${subSymbol} with grouping ${n} not found`);
        return;
      }

      orderBook.bids = [];
      orderBook.asks = [];

      // bids
      event.levels[0].forEach(({ px, sz }) => {
        orderBook.bids.push({
          price: parseFloat(px),
          amount: parseFloat(sz),
          total: 0,
        });
      });

      // asks
      event.levels[1].forEach(({ px, sz }) => {
        orderBook.asks.push({
          price: parseFloat(px),
          amount: parseFloat(sz),
          total: 0,
        });
      });

      // Guard against stale parser from cleaned-up subscription
      if (this.orderBookInnerState[innerId]) {
        this.orderBookInnerState[innerId].isSnapshotLoaded = true;
      }

      if (orderBook.asks.length !== 0 || orderBook.bids.length !== 0) {
        // Hyperliquid l2Book frames arrive pre-sorted (bids DESC, asks
        // ASC). We push them in order, so the resulting arrays are
        // already sorted; no need for a trailing sortOrderBook re-sort.
        //
        // Build the meta book via direct push instead of
        // `reduce + [...acc, ...filtered]` (the prior shape was quadratic
        // in accumulator size, plus a per-iter filter alloc per side).
        // Mirror of the perp WS fix.
        let lastBid = Infinity;
        let lastAsk = -Infinity;
        const metaBook: OrderBook = { bids: [], asks: [] };
        for (let g = 0; g < groupingLevels.length; g++) {
          const groupBook = books[groupingLevels[g]];
          const gBids = groupBook.bids;
          for (let i = 0; i < gBids.length; i++) {
            const b = gBids[i];
            if (b.price < lastBid) {
              lastBid = b.price;
              metaBook.bids.push(b);
            }
          }
          const gAsks = groupBook.asks;
          for (let i = 0; i < gAsks.length; i++) {
            const a = gAsks[i];
            if (a.price > lastAsk) {
              lastAsk = a.price;
              metaBook.asks.push(a);
            }
          }
        }
        calcOrderBookTotal(metaBook);
        callback(metaBook);
      }
    };

    let waitingTimeout: ReturnType<typeof setTimeout> | undefined;
    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        const shouldSubscribe = !this.messageHandlers[handler]?.length;
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;

        if (shouldSubscribe) {
          const payloads = [0, 2, 3, 4, 5].map((n) => ({
            method: 'subscribe',
            subscription: {
              type: 'l2Book',
              coin: subSymbol,
              nSigFigs: n === 0 ? null : n,
            },
          }));

          payloads.forEach((payload) => {
            this.send(JSON.stringify(payload));
          });
        }
      } else {
        waitingTimeout = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      // Cancel any pending waitForConnectedAndSubscribe polling to prevent
      // orphaned parsers from being added after cleanup
      if (waitingTimeout) {
        clearTimeout(waitingTimeout);
        waitingTimeout = undefined;
      }

      // No `cleanupMessageHandlerStats` here: l2Book is not stats-tracked on this
      // venue at all, so there is no entry to clear. The call that used to sit
      // here targeted `l2Book.<symbol>`, a key that was never written.

      // Clean up orderbook inner state to prevent memory leak
      delete this.orderBookInnerState[innerId];

      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payloads = [0, 2, 3, 4, 5].map((n) => ({
          method: 'unsubscribe',
          subscription: {
            type: 'l2Book',
            coin: subSymbol,
            nSigFigs: n === 0 ? null : n,
          },
        }));
        payloads.forEach((payload) => {
          this.send(JSON.stringify(payload));
        });
      }
    };
  };
}
