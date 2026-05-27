import type { Candle, OHLCVOptions, OrderBook, Ticker, Writable } from '../../types';
import type { KucoinExchange } from './kucoin.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { updateOrderBookByPrice } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { ENDPOINTS, WS_INTERVAL } from './kucoin.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: { [instId: string]: number };
};

export class KucoinPublicWebsocket extends BaseWebSocket<KucoinExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};
  token: string | null = null;
  socketEndpoint: string | null = null;

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      // Set up ticker topics for all available markets
      this.topics.ticker = this.parent.store.markets.reduce(
        (acc, m) => ({
          ...acc,
          [m.id || this.parent.toApiSymbol(m.symbol)]: 1,
        }),
        {},
      );

      try {
        // Get WebSocket token
        const { data } = await this.parent.xhr.post(ENDPOINTS.GET_PUBLIC_WS_TOKEN);

        if (data.code !== '200000' || !data.data) {
          this.parent.emitAndLogError('Failed to get WebSocket token');
          return;
        }

        this.token = data.data.token;

        // Construct WebSocket endpoint from the response
        const { instanceServers } = data.data;
        if (!instanceServers || !instanceServers.length) {
          this.parent.emitAndLogError('No WebSocket servers available');
          return;
        }

        // Use the first server in the list
        const server = instanceServers[0];
        this.socketEndpoint = `${server.endpoint}?token=${this.token}&connectId=${Date.now()}`;

        // Create WebSocket connection
        this.ws = this.createWebsocket(this.socketEndpoint);
        this.ws.addEventListener('open', this.onOpen);
        this.ws.addEventListener('message', this.onMessage);
        this.ws.addEventListener('close', this.onClose);
      } catch (err: any) {
        this.parent.emitAndLogError(err?.response?.data?.msg || err?.message || 'Failed to connect to WebSocket');
      }
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribe();
      this.ping();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ id?: string; symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;
    if (!this.isConnected) return;

    // Add new symbols to tracked topics
    for (const m of added) {
      if (!this.topics.ticker) this.topics.ticker = {};
      this.topics.ticker[m.id || this.parent.toApiSymbol(m.symbol)] = 1;
    }

    // Subscribe to just the new symbols
    const newSymbols = added.map((m) => m.id || this.parent.toApiSymbol(m.symbol));
    this.subscribeTopics('tickerV2', newSymbols);
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      // KuCoin requires ping to be a JSON object
      this.send(JSON.stringify({ id: Date.now(), type: 'ping' }));
    }
  };

  subscribe = () => {
    const tickerSymbols = Object.keys(this.topics.ticker || {});
    this.subscribeTopics('tickerV2', tickerSymbols);
  };

  private subscribeTopics = (channel: 'tickerV2', symbols: string[]) => {
    for (const symbol of symbols) {
      this.send(
        JSON.stringify({
          id: Date.now(),
          type: 'subscribe',
          topic: `/contractMarket/${channel}:${symbol}`,
          privateChannel: false,
          response: true,
        }),
      );
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const json = jsonParse(data);
      if (!json) return;

      // Handle ping/pong
      if (json.type === 'pong') {
        this.handlePongEvent();
        return;
      }

      // Handle ticker updates
      if (json.topic && json.topic.startsWith('/contractMarket/ticker')) {
        this.recordMessageHandlerStats('ticker');
        this.handleTickerSnapshot(json);
        return;
      }

      // Handle candle updates
      if (json.topic && json.topic.startsWith('/contractMarket/limitCandle')) {
        const topicArg = json.topic.split(':')[1] || '';
        const [symbol, interval] = topicArg.split('_');
        const internalSymbol = this.parent.toInternalSymbol(symbol) || symbol;
        const topic = `candle_${internalSymbol}_${interval}`;

        this.recordMessageHandlerStats(topic);

        if (this.messageHandlers[topic]) {
          for (const cb of this.messageHandlers[topic]) {
            cb(json);
          }
        }
        return;
      }

      // Handle orderbook updates
      if (json.topic && json.topic.startsWith('/contractMarket/level2')) {
        const symbol = json.topic.split(':')[1];
        const internalSymbol = this.parent.toInternalSymbol(symbol) || symbol;
        const topic = `orderBook_${internalSymbol}`;

        this.recordMessageHandlerStats(topic);

        if (this.messageHandlers[topic]) {
          for (const cb of this.messageHandlers[topic]) {
            cb(json);
          }
        }
      }
    }
  };

  handleTickerSnapshot = (json: Record<string, any>) => {
    const data = json.data;
    if (!data || !data.symbol) return;

    const symbol = this.parent.toInternalSymbol(data.symbol);
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[symbol];

    if (ticker) {
      const update: Partial<Writable<Ticker>> = {
        bid: parseFloat(data.bestBidPrice || '0'),
        ask: parseFloat(data.bestAskPrice || '0'),
        last: parseFloat(data.lastPrice || '0'),
        index: parseFloat(data.indexPrice || '0'),
        percentage: parseFloat(data.priceChangePercent || '0') * 100,
        fundingRate: parseFloat(data.fundingRate || '0'),
        fundingTime: parseFloat(data.nextFundingTime || '0'),
        volume: parseFloat(data.volume24h || '0'),
        quoteVolume: parseFloat(data.turnover24h || '0'),
      };

      this.parent.store.updateTicker(ticker, update);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = WS_INTERVAL[opts.interval];
    const symbol = opts.symbol;
    const apiSymbol = this.parent.toApiSymbol(symbol);
    const topic = `candle_${symbol}_${interval}`;

    const parser = (data: Data) => {
      if (!data.data) return;

      const rawCandle = data.data.candles;
      if (Array.isArray(rawCandle)) {
        callback({
          timestamp: parseInt(rawCandle[0], 10),
          open: parseFloat(rawCandle[1]),
          close: parseFloat(rawCandle[2]),
          high: parseFloat(rawCandle[3]),
          low: parseFloat(rawCandle[4]),
          volume: parseFloat(rawCandle[5]),
        });
        return;
      }

      const candle: Candle = {
        timestamp: data.data.time / 1000, // Convert milliseconds to seconds
        open: parseFloat(data.data.open),
        high: parseFloat(data.data.high),
        low: parseFloat(data.data.low),
        close: parseFloat(data.data.close),
        volume: parseFloat(data.data.volume),
      };
      callback(candle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];

          const payload = {
            id: Date.now(),
            type: 'subscribe',
            topic: `/contractMarket/limitCandle:${apiSymbol}_${interval}`,
            privateChannel: false,
            response: true,
          };

          this.send(JSON.stringify(payload));
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      if (this.isConnected) {
        this.send(
          JSON.stringify({
            id: Date.now(),
            type: 'unsubscribe',
            topic: `/contractMarket/limitCandle:${apiSymbol}_${interval}`,
            privateChannel: false,
            response: true,
          }),
        );
        this.cleanupMessageHandlerStats(topic);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let snapshotLoaded = false;
    let snapshotLoading = false;
    let snapshotRefetchRequested = false;
    let lastSequence = 0;
    let bufferedUpdates: Data[] = [];

    const orderBook: OrderBook = { bids: [], asks: [] };
    const apiSymbol = this.parent.toApiSymbol(symbol);
    const topic = `orderBook_${symbol}`;
    const recoverFromGap = (message: Data) => {
      snapshotLoaded = false;
      bufferedUpdates = [message];
      if (snapshotLoading) {
        snapshotRefetchRequested = true;
        return;
      }
      void fetchSnapshot();
    };

    const applySnapshot = (snapshot: Record<string, any>) => {
      orderBook.bids = (snapshot.bids || []).map(([price, amount]: [string | number, string | number]) => ({
        price: parseFloat(`${price}`),
        amount: parseFloat(`${amount}`),
        total: 0,
      }));
      orderBook.asks = (snapshot.asks || []).map(([price, amount]: [string | number, string | number]) => ({
        price: parseFloat(`${price}`),
        amount: parseFloat(`${amount}`),
        total: 0,
      }));
      lastSequence = parseInt(snapshot.sequence || snapshot.sn || '0', 10) || lastSequence;
    };

    const applyIncrement = (message: Data) => {
      const payload = message.data || {};
      const sequence = parseInt(payload.sequence || message.sn || '0', 10);
      if (sequence && lastSequence && sequence <= lastSequence) return false;
      if (sequence && lastSequence && sequence > lastSequence + 1) {
        recoverFromGap(message);
        return false;
      }

      if (typeof payload.change === 'string') {
        const [priceRaw, sideRaw, amountRaw] = payload.change.split(',');
        const side = sideRaw === 'buy' ? 'bids' : 'asks';
        updateOrderBookByPrice(orderBook, side, parseFloat(priceRaw), parseFloat(amountRaw));
        if (sequence) lastSequence = sequence;
        return true;
      }

      if (message.type === 'snapshot') {
        applySnapshot(payload);
        return true;
      }

      if (message.type === 'update') {
        for (const bid of payload.bids || []) {
          updateOrderBookByPrice(orderBook, 'bids', parseFloat(bid[0]), parseFloat(bid[1]));
        }
        for (const ask of payload.asks || []) {
          updateOrderBookByPrice(orderBook, 'asks', parseFloat(ask[0]), parseFloat(ask[1]));
        }
        if (sequence) lastSequence = sequence;
        return true;
      }

      return false;
    };

    const emitOrderBook = () => {
      calcOrderBookTotal(orderBook);
      callback(orderBook);
    };

    const fetchSnapshot = async () => {
      if (snapshotLoading || this.isDisposed) return;
      snapshotLoading = true;
      try {
        const { data } = await this.parent.xhr.get(ENDPOINTS.ORDERBOOK, { params: { symbol: apiSymbol } });
        if (data.code !== '200000' || !data.data) {
          this.parent.emitAndLogError(data.msg || `Failed to fetch orderbook snapshot for ${symbol}`);
          return;
        }

        applySnapshot(data.data);
        snapshotLoaded = true;

        const replay = bufferedUpdates.filter((update) => {
          const sequence = parseInt(update.data?.sequence || update.sn || '0', 10);
          return !sequence || !lastSequence || sequence > lastSequence;
        });
        bufferedUpdates = [];
        for (const update of replay) applyIncrement(update);

        if (snapshotLoaded) emitOrderBook();
      } catch (err: any) {
        this.parent.emitAndLogError(err?.response?.data?.msg || err?.message || `Failed to fetch orderbook ${symbol}`);
      } finally {
        snapshotLoading = false;
        if (snapshotRefetchRequested && !this.isDisposed) {
          snapshotRefetchRequested = false;
          void fetchSnapshot();
        }
      }
    };

    const parser = (data: Data) => {
      if (!data.data) return;

      if (!snapshotLoaded) {
        bufferedUpdates.push(data);
        void fetchSnapshot();
        return;
      }

      if (applyIncrement(data)) emitOrderBook();
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];

          const payload = {
            id: Date.now(),
            type: 'subscribe',
            topic: `/contractMarket/level2:${apiSymbol}`,
            privateChannel: false,
            response: true,
          };

          this.send(JSON.stringify(payload));
          void fetchSnapshot();
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      orderBook.asks = [];
      orderBook.bids = [];

      this.cleanupMessageHandlerStats(topic);

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected) {
        const payload = {
          id: Date.now(),
          type: 'unsubscribe',
          topic: `/contractMarket/level2:${apiSymbol}`,
          privateChannel: false,
          response: true,
        };

        this.send(JSON.stringify(payload));
      }
    };
  };
}
