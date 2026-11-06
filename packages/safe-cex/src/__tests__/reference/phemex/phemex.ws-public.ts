import type { Candle, OHLCVOptions, OrderBook, Ticker } from '../../../types';
import type { PhemexExchange } from './phemex.exchange';

import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity } from '../../../types';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { updateOrderBookByPrice } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS } from '../../../exchanges/phemex/phemex.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: any) => void>;
};

export class BinancePublicWebsocket extends BaseWebSocket<PhemexExchange> {
  private static readonly MAX_SUBSCRIPTIONS = 20;
  private static readonly SUBSCRIPTION_WARNING_THRESHOLD = 18;
  private static readonly ORDERBOOK_SNAPSHOT_ERROR_LOG_INTERVAL_MS = 30_000;
  private hasWarnedSubscriptionLimit = false;
  private orderBookSnapshotErrorLogAt: Record<string, number> = {};
  messageHandlers: MessageHandlers = {
    'perp_market24h_pack_p.update': [(d: Data) => this.handleTickerStreamEvents(d)],
  };

  topics: { [key: string]: number } = {};

  requestId: number = 1200;

  private klineIntervalByResolution?: Map<number, string>;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      const payload = {
        id: 1,
        method: 'perp_market24h_pack_p.subscribe',
        params: [],
      };

      this.send(JSON.stringify(payload));

      this.resetOrderbook();
      this.subscribe();
      this.ping();
    }
  };

  subscribe = () => {
    let id = 10;
    for (const topic of Object.keys(this.topics)) {
      const [method, ...params] = topic.split('|||');
      this.send(
        JSON.stringify({
          method: method,
          // kline requires second arg to be a number
          params: method === 'kline_p.subscribe' ? [params[0], +params[1]] : method === 'orderbook_p.subscribe' ? [params[0], false, 0] : params,
          id: id++,
        }),
      );
    }
  };

  private updateSubscriptionLimitWarning = () => {
    const activeSubscriptions =
      1 + Object.values(this.topics).filter((referenceCount) => referenceCount > 0).length;
    if (
      activeSubscriptions >= BinancePublicWebsocket.SUBSCRIPTION_WARNING_THRESHOLD &&
      !this.hasWarnedSubscriptionLimit
    ) {
      this.hasWarnedSubscriptionLimit = true;
      this.parent.log(
        `[Phemex] Public WebSocket has ${activeSubscriptions}/${BinancePublicWebsocket.MAX_SUBSCRIPTIONS} active subscriptions; additional charts or orderbooks may be rejected.`,
        LogSeverity.Warning,
      );
    } else if (activeSubscriptions < BinancePublicWebsocket.SUBSCRIPTION_WARNING_THRESHOLD) {
      this.hasWarnedSubscriptionLimit = false;
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(`{"id":${this.requestId++},"method":"server.ping","params":[]}`);
    }
  };

  private dispatchTopic = (topic: string, payload: any, recordStatsPerCallback = false) => {
    const callbacks = this.messageHandlers[topic];
    if (!callbacks?.length) return;

    if (!recordStatsPerCallback) this.recordMessageHandlerStats(topic);
    for (const callback of callbacks) {
      if (recordStatsPerCallback) this.recordMessageHandlerStats(topic);
      callback(payload);
    }
  };

  private getKlineInterval = (resolution: unknown): string | undefined => {
    const numericResolution = Number(resolution);
    if (!Number.isFinite(numericResolution)) return undefined;

    if (!this.klineIntervalByResolution) {
      const timeframes: Record<string, string> = this.parent.timeframes;
      this.klineIntervalByResolution = new Map<number, string>();
      for (const interval of Object.keys(timeframes)) {
        this.klineIntervalByResolution.set(Number(timeframes[interval]), interval);
      }
    }

    return this.klineIntervalByResolution.get(numericResolution);
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const json = jsonParse(data);

      if (json?.method === 'server.ping' || json?.result === 'pong') {
        this.handlePongEvent();
        return;
      }

      const jsonSymbol = json?.symbol;

      if (json?.kline_p && typeof jsonSymbol === 'string') {
        const dispatchedTopics = new Set<string>();
        for (const row of json.kline_p) {
          const interval = Array.isArray(row) ? this.getKlineInterval(row[1]) : undefined;
          if (!interval) continue;

          const topic = `kline_p.${interval}.${jsonSymbol}`;
          if (!dispatchedTopics.has(topic)) {
            dispatchedTopics.add(topic);
            this.dispatchTopic(topic, json.kline_p);
          }
        }
        return;
      }

      if (json?.orderbook_p && typeof jsonSymbol === 'string') {
        this.dispatchTopic(`orderbook_p.${jsonSymbol}`, { ...json.orderbook_p, sequence: json.sequence, type: json.type, depth: json.depth });
        return;
      }

      if (typeof json?.method === 'string') {
        // The always-on packed ticker handler historically records one stats
        // hit per callback. Keep that metric contract while avoiding a raw
        // frame substring scan.
        this.dispatchTopic(json.method, Array.isArray(json) ? json : [json], true);
      }
    }
  };

  handleTickerStreamEvents = (events: Array<Record<string, any>>) => {
    events.forEach((event) => {
      if (event?.data?.length) {
        for (const row of event?.data) {
          const [
            rawSymbol,
            openRp,
            ,
            ,
            lastRp,
            volumeRq,
            ,
            openInterestRv,
            indexRp,
            markRp,
            fundingRateRr,
            ,
            bid,
            ask,
          ] = row;

          const symbol = this.parent.formatSymbolIn(rawSymbol);

          const ticker = this.parent.store.tickersBySymbol[symbol];
          const tickerChanges = this.mapPackedTickerChanges({
            ask,
            bid,
            fundingRateRr,
            indexRp,
            lastRp,
            markRp,
            openInterestRv,
            openRp,
            volumeRq,
          });
          if (ticker) {
            this.store.updateTicker(ticker, tickerChanges);
          } else {
            const market = this.parent.store.marketsBySymbol[symbol];
            if (!market) continue;

            this.store.update({
              tickers: [
                ...this.store.tickers.filter((existingTicker) => existingTicker.symbol !== symbol),
                {
                  id: market.id,
                  symbol,
                  bid: tickerChanges.bid ?? tickerChanges.last,
                  ask: tickerChanges.ask ?? tickerChanges.last,
                  last: tickerChanges.last,
                  mark: tickerChanges.mark,
                  index: tickerChanges.index,
                  percentage: tickerChanges.percentage,
                  openInterest: tickerChanges.openInterest,
                  fundingRate: tickerChanges.fundingRate,
                  fundingTime: 0,
                  volume: tickerChanges.volume,
                  quoteVolume: tickerChanges.quoteVolume,
                } as Ticker,
              ],
            });
          }
        }
      }
    });
  };

  private mapPackedTickerChanges = ({
    ask,
    bid,
    fundingRateRr,
    indexRp,
    lastRp,
    markRp,
    openInterestRv,
    openRp,
    volumeRq,
  }: {
    ask?: string;
    bid?: string;
    fundingRateRr: string;
    indexRp: string;
    lastRp: string;
    markRp: string;
    openInterestRv: string;
    openRp: string;
    volumeRq: string;
  }): Partial<Ticker> & Pick<Ticker, 'fundingRate' | 'index' | 'last' | 'mark' | 'openInterest' | 'percentage' | 'quoteVolume' | 'volume'> => {
    const last = parseFloat(lastRp);
    const volume = parseFloat(volumeRq);
    const open = parseFloat(openRp);

    return {
      last,
      volume,
      quoteVolume: volume * last,
      percentage: ((last - open) / open) * 100,
      fundingRate: parseFloat(fundingRateRr),
      mark: parseFloat(markRp),
      index: parseFloat(indexRp),
      openInterest: parseFloat(openInterestRv),
      ...(bid ? { bid: parseFloat(bid) } : {}),
      ...(ask ? { ask: parseFloat(ask) } : {}),
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `kline_p.subscribe|||${opts.symbol}|||${+this.parent.timeframes[opts.interval]}`;
    const handler = `kline_p.${opts.interval}.${opts.symbol}`;

    const parser = (allRows: number[][]) => {
      // [ <timestamp>, "<interval>", <lastClose>, <open>, <high>, <low>, <close>, <volume>, <turnover>, ]

      const rows = [];
      if (allRows?.[1]) rows.push(allRows[1]);
      if (allRows?.[0]) rows.push(allRows[0]);

      for (const json of rows) {
        const [timestamp, interval, , open, high, low, close, volume] = json;
        if (interval === +this.parent.timeframes[opts.interval]) {
          callback({
            timestamp,
            open,
            high,
            low,
            close,
            volume: volume,
          });
        }
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.updateSubscriptionLimitWarning();

        const payload = {
          method: 'kline_p.subscribe',
          params: [opts.symbol, +this.parent.timeframes[opts.interval]],
          id: 2,
        };
        this.send(JSON.stringify(payload));
        //this.parent.log(`Switched to [${opts.symbol}:${opts.interval}]`);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = {
          method: 'kline_p.unsubscribe',
          params: [opts.symbol, +this.parent.timeframes[opts.interval]],
          id: 2,
        };
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify(payload));
      }
      if (this.topics[topic] <= 0) delete this.topics[topic];
      this.updateSubscriptionLimitWarning();
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let isActive = true;
    let snapshotRequestId = 0;

    const topic = `orderbook_p.subscribe|||${symbol}`;
    const handler = `orderbook_p.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };

    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const innerState = this.orderBookInnerState[innerId];

    const logSnapshotFailure = (error: unknown) => {
      const now = Date.now();
      const lastLogAt = this.orderBookSnapshotErrorLogAt[symbol];
      if (
        lastLogAt !== undefined &&
        now - lastLogAt < BinancePublicWebsocket.ORDERBOOK_SNAPSHOT_ERROR_LOG_INTERVAL_MS
      ) {
        return;
      }

      this.orderBookSnapshotErrorLogAt[symbol] = now;
      const message = error instanceof Error ? error.message : String(error);
      this.parent.log(
        `[Phemex] Orderbook snapshot request failed for ${symbol}; waiting for next delta to retry: ${message}`,
        LogSeverity.Warning,
      );
    };

    const fetchSnapshot = async () => {
      const currentSnapshotRequestId = ++snapshotRequestId;

      try {
        const { data } = await this.parent.xhr.get(ENDPOINTS.ORDERBOOK, {
          params: { symbol, limit: 1000 },
        });

        const { orderbook_p, sequence } = data.result;

        if (!isActive || this.isDisposed || currentSnapshotRequestId !== snapshotRequestId) {
          return;
        }

        // save snapshot into orderBook object
        orderBook.bids = orderbook_p.bids.map(([price, amount]: string[]) => ({
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        }));

        orderBook.asks = orderbook_p.asks.map(([price, amount]: string[]) => ({
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        }));

        // drop events where u < lastUpdateId
        innerState.updates = innerState.updates.filter((update: Record<string, any>) => update.sequence > sequence);

        // apply all updates
        innerState.updates.forEach((update: Record<string, any>) => {
          this.processOrderBookUpdate(orderBook, update);
        });

        calcOrderBookTotal(orderBook);

        innerState.isSnapshotLoaded = true;
        innerState.updates = [];

        callback(orderBook);
      } catch (error) {
        if (!isActive || this.isDisposed || currentSnapshotRequestId !== snapshotRequestId) {
          return;
        }

        innerState.updates = [];
        innerState.isSnapshotLoaded = false;
        logSnapshotFailure(error);
      }
    };

    const parser = (data: Data) => {
      // first update, request snapshot
      if (!innerState.isSnapshotLoaded && innerState.updates.length === 0) {
        fetchSnapshot();
        innerState.updates = [data];
        return;
      }

      // more updates, but snapshot is not loaded yet
      if (!innerState.isSnapshotLoaded) {
        innerState.updates.push(data);
        return;
      }
      // snapshot is loaded, apply updates and callback
      this.processOrderBookUpdate(orderBook, data);
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
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.updateSubscriptionLimitWarning();

        const payload = { method: 'orderbook_p.subscribe', params: [symbol, false, 0], id: 3 };
        this.send(JSON.stringify(payload));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      isActive = false;
      snapshotRequestId += 1;
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      orderBook.asks = [];
      orderBook.bids = [];
      innerState.updates = [];
      innerState.isSnapshotLoaded = false;

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { method: 'orderbook_p.unsubscribe', params: [symbol], id: 3 };
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify(payload));
      }
      if (this.topics[topic] <= 0) delete this.topics[topic];
      this.updateSubscriptionLimitWarning();
    };
  };

  private processOrderBookUpdate = (orderBook: OrderBook, update: Record<string, any>) => {
    if (update.type === 'snapshot' && update.depth === 0) {
      orderBook.bids = [];
      orderBook.asks = [];
    }
    const bids = update.bids || [];
    for (let i = 0; i < bids.length; i++) {
      const [price, amount] = bids[i];
      updateOrderBookByPrice(orderBook, 'bids', parseFloat(price), parseFloat(amount));
    }

    const asks = update.asks || [];
    for (let i = 0; i < asks.length; i++) {
      const [price, amount] = asks[i];
      updateOrderBookByPrice(orderBook, 'asks', parseFloat(price), parseFloat(amount));
    }
  };
}
