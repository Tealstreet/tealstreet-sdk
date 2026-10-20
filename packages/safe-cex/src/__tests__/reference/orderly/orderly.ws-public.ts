import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { OrderlyExchange } from './orderly.exchange';

import flatten from 'lodash/flatten';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { v } from '../../../utils/get-key';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS } from './orderly.types';
import { normalizeSymbol, reverseSymbol } from './orderly.utils';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};
type SubscribedTopics = {
  [id: string]: string;
};

export class OrderlyPublicWebsocket extends BaseWebSocket<OrderlyExchange> {
  topics: SubscribedTopics = {};
  replaySnapshots = false;
  fundingOnly?: string[];
  fundingSockets: OrderlyPublicWebsocket[] = [];
  snapshotSequence = 0;
  snapshots = new Map<string, (data: Data) => void>();
  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePingEvent()],
    pong: [() => this.handlePongEvent()],
    tickers: [({ data }: Data) => this.handleTickersStreamEvents(data)],
    bbos: [({ data }: Data) => this.handleBBOStreamEvents(data)],
    markprices: [({ data }: Data) => this.handleMarkPricesStreamEvents(data)],
    estfundingrate: [({ data }: Data) => this.handleFundingRateStreamEvents(data)],
  };

  dispose() {
    for (const socket of this.fundingSockets) socket.dispose();
    this.fundingSockets = [];
    super.dispose();
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const baseURL = BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet'];

      this.ws = this.createWebsocket(`${baseURL}/${this.parent.options.applicationId}`);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.ping();
      if (this.fundingOnly) {
        for (const topic of this.fundingOnly) this.send(JSON.stringify({ event: 'subscribe', topic }));
        return;
      }
      this.send(JSON.stringify({ event: 'subscribe', topic: 'tickers' }));
      this.send(JSON.stringify({ event: 'subscribe', topic: 'bbos' }));
      this.send(JSON.stringify({ event: 'subscribe', topic: 'markprices' }));
      if (this.replaySnapshots) {
        if (!this.fundingSockets.length) {
          const topics = this.parent.store.markets.map((market) => `${reverseSymbol(market.symbol)}@estfundingrate`).sort();
          for (let at = 0; at < topics.length; at += 90) {
            const socket = new OrderlyPublicWebsocket(this.parent);
            socket.replaySnapshots = true;
            socket.fundingOnly = topics.slice(at, at + 90);
            this.fundingSockets.push(socket);
            socket.connectAndSubscribe();
          }
        }
      } else this.send(JSON.stringify({ event: 'subscribe', topic: 'estfundingrate' }));
      this.subscribe();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  subscribe = () => {
    const topics = flatten(Object.values(this.topics));
    for (const topic of topics) {
      const payload = { event: 'subscribe', topic };
      this.send(JSON.stringify(payload));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const response = jsonParse(data);
      if (this.replaySnapshots && response?.topic?.endsWith('@estfundingrate')) {
        this.handleFundingRateStreamEvents([response.data]);
        return;
      }
      if (response?.event === 'request' && this.snapshots.has(String(response.id))) {
        this.snapshots.get(String(response.id))!(response.data);
        this.snapshots.delete(String(response.id));
        return;
      }
      const handlers = Object.entries(this.messageHandlers);

      for (const [topic, handler] of handlers) {
        if (data.includes(`event":"${topic}`) || data.includes(`topic":"${topic}`)) {
          const json = jsonParse(data);
          for (const cb of handler) {
            if (json) cb(json);
          }
          break;
        }
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ event: 'ping' }));
    }
  };

  handlePingEvent = () => {
    this.send(JSON.stringify({ event: 'pong' }));
  };

  handleTickersStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            last: row.close,
            quoteVolume: row.amount,
            volume: row.volume,
          });
        }
      }
    });
  };

  handleBBOStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            bid: row.bid,
            ask: row.ask,
          });
        }
      }
    });
  };

  handleMarkPricesStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            mark: row.price,
          });
        }
      }
    });
  };

  handleFundingRateStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            fundingRate: parseFloat(v(row, 'fundingRate')),
            fundingTime: parseFloat(v(row, 'fundingTs')),
          });
        }
      }
    });
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `${reverseSymbol(opts.symbol)}@kline_${opts.interval}`;

    const parser = (json: Data) => {
      callback({
        timestamp: json.data.startTime / 1000,
        open: json.data.open,
        high: json.data.high,
        low: json.data.low,
        close: json.data.close,
        volume: json.data.volume,
      });
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          this.topics[topic] = topic;

          const payload = { event: 'subscribe', topic };
          this.send(JSON.stringify(payload));
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
        this.cleanupMessageHandlerStats(topic);
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { event: 'unsubscribe', topic };
        this.send(JSON.stringify(payload));
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    const topic = `${reverseSymbol(symbol)}@orderbookupdate`;
    const orderBook: OrderBook = { bids: [], asks: [] };
    const innerState = {
      updates: [] as any[],
      isSnapshotLoaded: false,
    };

    const fetchSnapshot = async () => {
      try {
        const data = this.replaySnapshots ? await new Promise<Data>((resolve) => {
          const id = `reference-${++this.snapshotSequence}`;
          this.snapshots.set(id, resolve);
          this.send(JSON.stringify({ id, event: 'request', params: { type: 'orderbook', symbol: reverseSymbol(symbol) } }));
        }) : await this.parent.xhr.get(`${ENDPOINTS.ORDERBOOK}/${reverseSymbol(symbol)}`).then(({ data: { data } }) => ({
          ts: data.timestamp,
          bids: data.bids.map((row: Data) => [row.price, row.quantity]),
          asks: data.asks.map((row: Data) => [row.price, row.quantity]),
        }));

        if (!this.isDisposed) {
          orderBook.bids = data.bids.map(([price, amount]: [number, number]) => ({
            price,
            amount,
            total: 0,
          }));

          orderBook.asks = data.asks.map(([price, amount]: [number, number]) => ({
            price,
            amount,
            total: 0,
          }));

          // drop events where timestamp is older than the snapshot
          innerState.updates = innerState.updates.filter((update: Record<string, any>) => update.ts > data.ts);

          // apply all updates
          innerState.updates.forEach((update: Record<string, any>) => {
            this.processOrderBookUpdate(orderBook, update);
          });

          calcOrderBookTotal(orderBook);

          innerState.isSnapshotLoaded = true;
          innerState.updates = [];

          callback(orderBook);
        }
      } catch (err: any) {
        // Re-arm the first-update branch so a transient REST failure cannot
        // leave the book buffering forever without another snapshot attempt.
        innerState.updates = [];
        this.emitAndLogError(err?.response?.data?.message || err?.message || 'Orderly orderbook snapshot failed');
      }
    };

    const parser = (json: Data) => {
      if (!this.replaySnapshots && !innerState.isSnapshotLoaded && innerState.updates.length === 0) {
        void fetchSnapshot();
        innerState.updates = [json];
        return;
      }
      if (!innerState.isSnapshotLoaded) {
        innerState.updates.push(json);
        return;
      }

      // do updates
      this.processOrderBookUpdate(orderBook, json);
      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        // 1. subscribe to the topic
        // 2. wait for the first message and send request to snapshot
        // 3. store all incoming updates in an array
        // 4. when the snapshot is received, apply all updates and send the order book to the callback
        // 5. then on each update, apply it to the order book and send it to the callback
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          this.topics[topic] = topic;

          const payload = { event: 'subscribe', topic };
          this.send(JSON.stringify(payload));
          if (this.replaySnapshots) void fetchSnapshot();
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
        this.cleanupMessageHandlerStats(topic);
      }

      orderBook.asks = [];
      orderBook.bids = [];
      innerState.updates = [];
      innerState.isSnapshotLoaded = false;

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { event: 'unsubscribe', topic };
        this.send(JSON.stringify(payload));
      }
    };
  };

  private processOrderBookUpdate = (orderBook: OrderBook, update: Record<string, any>) => {
    const sides = { bids: update.data.bids, asks: update.data.asks };

    Object.entries(sides).forEach(([side, data]) => {
      // we need this for ts compile
      if (side !== 'bids' && side !== 'asks') return;

      data.forEach(([price, amount]: [number, number]) => {
        const update = { price, amount, total: 0 };
        updateOrderBook(orderBook, side, update);
      });
    });
  };
}
