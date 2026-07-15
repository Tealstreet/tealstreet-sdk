import type { BitmexExchange } from './bitmex.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { Candle, Market, OHLCVOptions, OrderBook } from '../../types';
import { v } from '../../utils/get-key';
import { getCandleStartTime } from '../../utils/getCandleStartTime';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { updateOrderBook } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { getBitmexOHLCVPlan } from './bitmex.ohlcv-helpers';
import { BASE_WS_URL } from './bitmex.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class BitmexPublicWebsocket extends BaseWebSocket<BitmexExchange> {
  messageHandlers: MessageHandlers = {};

  topics: { [key: string]: number } = {};

  private static readonly OHLCV_POLL_INTERVAL_MS = 5000;
  private static readonly OHLCV_WS_STALE_THRESHOLD_MS = 12000;

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
      this.resetOrderbook();
      const payload = {
        op: 'subscribe',
        args: ['instrument:CONTRACTS'],
      };

      this.send(JSON.stringify(payload));
      this.subscribe();
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  subscribe = () => {
    for (const topic of Object.keys(this.topics)) {
      this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send('ping');
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const handlers = Object.entries(this.messageHandlers);

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      const json = jsonParse(data);
      if (json?.table === 'instrument') {
        const rows = json?.data || [];
        this.recordMessageHandlerStats('ticker');

        // O(1) hashmap lookup. The instrument table arrives at high
        // frequency for all subscribed symbols.
        const tickersBySymbol = this.parent.store.tickersBySymbol;
        for (const row of rows) {
          const ticker = tickersBySymbol[row.symbol];

          if (ticker) {
            const bidPrice = parseFloat(v(row, 'bidPrice'));
            const askPrice = parseFloat(v(row, 'askPrice'));
            const lastPrice = parseFloat(v(row, 'lastPrice'));
            const tradePrice = parseFloat(v(row, 'tradePrice'));
            const changePercentage = parseFloat(v(row, 'changePercentage'));
            const volumeF = parseFloat(v(row, 'volumeF'));
            const valueF = parseFloat(v(row, 'valueF'));
            const openValue = parseFloat(v(row, 'openValue'));
            const markPrice = parseFloat(v(row, 'markPrice'));
            const fairPrice = parseFloat(v(row, 'fairPrice'));
            const indexPrice = parseFloat(v(row, 'indexPrice'));
            const fundingRate = parseFloat(v(row, 'fundingRate'));

            const nextLast = Number.isFinite(tradePrice)
              ? tradePrice
              : Number.isFinite(lastPrice)
                ? lastPrice
                : undefined;
            const nextMark = Number.isFinite(markPrice)
              ? markPrice
              : Number.isFinite(fairPrice)
                ? fairPrice
                : undefined;

            this.store.updateTicker(ticker, {
              ...(Number.isFinite(bidPrice) ? { bid: bidPrice } : {}),
              ...(Number.isFinite(askPrice) ? { ask: askPrice } : {}),
              ...(nextLast !== undefined ? { last: nextLast } : {}),
              ...(Number.isFinite(changePercentage) ? { percentage: changePercentage } : {}),
              ...(Number.isFinite(volumeF) ? { volume: volumeF } : {}),
              ...(Number.isFinite(valueF) ? { quoteVolume: valueF } : {}),
              // unsure if I should or not divide it
              ...(Number.isFinite(openValue) ? { openInterest: openValue } : {}),
              ...(nextMark !== undefined ? { mark: nextMark } : {}),
              ...(Number.isFinite(indexPrice) ? { index: indexPrice } : {}),
              ...(Number.isFinite(fundingRate) ? { fundingRate } : {}),
              info: row,
            });
          }
        }
      } else {
        for (const [topic, callbacks] of handlers) {
          this.recordMessageHandlerStats(topic);
          const [leftmost, _symbol] = topic.split(':');
          if (data.includes(`${leftmost}`)) {
            for (const callback of callbacks) {
              if (json) callback(Array.isArray(json) ? json : [json]);
            }
          }
        }
      }
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    // BitMEX public clients default to Aggregated today, but making the pool
    // explicit keeps the local price-keyed book on one coherent liquidity
    // view if account/persona defaults change upstream.
    const topic = `orderBookL2:${symbol.toLowerCase()}:Aggregated`;
    const handler = topic;
    const orderBook: OrderBook = { bids: [], asks: [] };

    const market = this.store.markets.find((m) => m.symbol === symbol);

    const parser = ([rawData]: Data) => {
      // incorrect symbol, we don't take account
      const relevantData = rawData.data?.filter(
        (e: any) => e.symbol === symbol && (e.pool === undefined || e.pool === 'Aggregated'),
      );

      if (!relevantData?.length) {
        return;
      }

      const data = {
        bids: relevantData.filter((e: any) => e.side === 'Buy'),
        asks: relevantData.filter((e: any) => e.side === 'Sell'),
      };

      if (relevantData.action === 'partial' || rawData.action === 'partial') {
        orderBook.bids = data.bids.map(({ price, size }: { price: number; size: number }) => ({
          price,
          amount:
            market && this.parent.convertOutputsFromMicro ? this.parent.convertFromMicroValue(size, market) : size,
          total: 0,
        }));
        orderBook.asks = data.asks.map(({ price, size }: { price: number; size: number }) => ({
          price,
          amount:
            market && this.parent.convertOutputsFromMicro ? this.parent.convertFromMicroValue(size, market) : size,
          total: 0,
        }));
      } else {
        // snapshot is loaded, apply updates and callback
        this.processOrderBookUpdate(orderBook, data, market);
      }
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

        const payload = { op: 'subscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      orderBook.asks = [];
      orderBook.bids = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      this.topics[topic] = Math.max((this.topics[topic] || 1) - 1, 0);

      if (!this.messageHandlers[handler].length) {
        this.cleanupMessageHandlerStats(handler);
        if (this.isConnected) {
          const payload = { op: 'unsubscribe', args: [topic] };
          this.send(JSON.stringify(payload));
        }
      }

      if (this.topics[topic] === 0) {
        delete this.topics[topic];
      }
      if (!this.messageHandlers[handler].length) {
        delete this.messageHandlers[handler];
      }
    };
  };

  private processOrderBookUpdate = (orderBook: OrderBook, update: Record<string, any>, market?: Market) => {
    const shouldConvert = !!(
      market &&
      !market.isInverse &&
      !market.info.isQuanto &&
      this.parent.convertOutputsFromMicro
    );

    const bids = update.bids;
    if (bids) {
      for (let i = 0; i < bids.length; i++) {
        const { price, size: rawSize } = bids[i];
        const safeSize = Math.abs(parseFloat(rawSize as any));
        const size = shouldConvert ? this.parent.convertFromMicroValue(safeSize, market!) : safeSize;
        updateOrderBook(orderBook, 'bids', { price, amount: size || 0, total: 0 });
      }
    }
    const asks = update.asks;
    if (asks) {
      for (let i = 0; i < asks.length; i++) {
        const { price, size: rawSize } = asks[i];
        const safeSize = Math.abs(parseFloat(rawSize as any));
        const size = shouldConvert ? this.parent.convertFromMicroValue(safeSize, market!) : safeSize;
        updateOrderBook(orderBook, 'asks', { price, amount: size || 0, total: 0 });
      }
    }
  };

  private listenOHLCVWithPolling = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let isActive = true;
    let lastCandle: Candle | null = null;

    const poll = async () => {
      if (!isActive) return;

      try {
        // Fetch the latest 2 candles to get the most recent complete and current candle.
        const candles = await this.parent.fetchOHLCV({ ...opts, limit: 2 });

        if (candles.length > 0 && isActive) {
          const latestCandle = candles[candles.length - 1];

          // Only call the callback if the candle has changed.
          if (
            !lastCandle ||
            lastCandle.timestamp !== latestCandle.timestamp ||
            lastCandle.close !== latestCandle.close ||
            lastCandle.volume !== latestCandle.volume
          ) {
            lastCandle = latestCandle;
            callback(latestCandle);
          }
        }
      } catch (_err) {
        // Silently handle errors - the parent's fetchOHLCV already logs them.
      }

      // Schedule next poll if still active.
      if (isActive) {
        timeoutId = setTimeout(poll, BitmexPublicWebsocket.OHLCV_POLL_INTERVAL_MS);
      }
    };

    poll();

    return () => {
      isActive = false;
      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
      lastCandle = null;
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const ohlcvPlan = getBitmexOHLCVPlan(opts.interval);
    if (!ohlcvPlan) {
      return this.listenOHLCVWithPolling(opts, callback);
    }

    const topic = `trade:${opts.symbol}`;
    const handler = `${topic}:${opts.interval}`;
    let waitForConnectTimeoutId: NodeJS.Timeout | null = null;
    let wsWatchdogTimeoutId: NodeJS.Timeout | null = null;
    let fallbackDisposer: (() => void) | null = null;
    let isActive = true;
    let wsLastTradeAt = 0;
    let lastCandleSignature = '';
    let lastEmittedCandle: Candle | null = null;
    let currentCandle: Candle | null = null;
    let hasReceivedPoll = false;

    const emitIfChanged = (candle: Candle): 'emitted' | 'duplicate' | 'stale' => {
      if (lastEmittedCandle) {
        // Never move backwards in time on startup/reconnect snapshots.
        if (candle.timestamp < lastEmittedCandle.timestamp) {
          return 'stale';
        }
        // Ignore regressive same-bucket updates (volume should be monotonic within a live candle).
        if (candle.timestamp === lastEmittedCandle.timestamp && candle.volume < lastEmittedCandle.volume) {
          return 'stale';
        }
      }

      const signature = [candle.timestamp, candle.open, candle.high, candle.low, candle.close, candle.volume].join('|');
      if (signature === lastCandleSignature) {
        return 'duplicate';
      }
      lastCandleSignature = signature;
      lastEmittedCandle = candle;
      callback(candle);
      return 'emitted';
    };

    const stopFallbackPolling = () => {
      if (fallbackDisposer) {
        fallbackDisposer();
        fallbackDisposer = null;
      }
    };

    const startFallbackPolling = () => {
      if (!isActive || fallbackDisposer) {
        return;
      }
      fallbackDisposer = this.listenOHLCVWithPolling(opts, (candle) => {
        const expectedCurrentBucket = getCandleStartTime(opts.interval as any, Math.floor(Date.now() / 1000));
        if (!currentCandle && candle.timestamp < expectedCurrentBucket) {
          // Ignore initial stale previous-bucket snapshots during fresh connect.
          return;
        }

        let didIntegratePoll = false;
        if (!currentCandle) {
          currentCandle = candle;
          didIntegratePoll = true;
        } else if (candle.timestamp > currentCandle.timestamp) {
          currentCandle = candle;
          didIntegratePoll = true;
        } else if (candle.timestamp === currentCandle.timestamp) {
          // Poll candle is authoritative for current bucket shape, but keep WS close freshest.
          // This avoids locking in a bad open when WS trades arrive before first poll seed.
          const merged: Candle = {
            timestamp: currentCandle.timestamp,
            open: candle.open,
            high: Math.max(currentCandle.high, candle.high, candle.open, candle.close),
            low: Math.min(currentCandle.low, candle.low, candle.open, candle.close),
            close: currentCandle.close,
            volume: Math.max(currentCandle.volume, candle.volume),
          };
          if (
            merged.open !== currentCandle.open ||
            merged.high !== currentCandle.high ||
            merged.low !== currentCandle.low ||
            merged.close !== currentCandle.close ||
            merged.volume !== currentCandle.volume
          ) {
            currentCandle = merged;
            didIntegratePoll = true;
          }
        }

        if (didIntegratePoll) {
          hasReceivedPoll = true;
        }
        emitIfChanged(currentCandle);
      });
    };

    const scheduleWatchdog = () => {
      if (!isActive) return;
      if (wsWatchdogTimeoutId) {
        clearTimeout(wsWatchdogTimeoutId);
      }
      wsWatchdogTimeoutId = setTimeout(() => {
        if (!isActive) return;
        const wsIsStale =
          wsLastTradeAt === 0 || Date.now() - wsLastTradeAt > BitmexPublicWebsocket.OHLCV_WS_STALE_THRESHOLD_MS;
        if (wsIsStale) {
          startFallbackPolling();
        }
        scheduleWatchdog();
      }, BitmexPublicWebsocket.OHLCV_POLL_INTERVAL_MS);
    };

    const parser = ([rawData]: Data) => {
      if (!rawData || !Array.isArray(rawData.data)) {
        return;
      }

      const rows = rawData.data.filter((row: any) => row.symbol === opts.symbol);
      if (!rows.length) {
        return;
      }

      for (const row of rows) {
        const timestampMs = Date.parse(row.timestamp);
        if (!Number.isFinite(timestampMs)) {
          continue;
        }

        const tradePrice = parseFloat(v(row, 'price') || 0);
        if (!Number.isFinite(tradePrice) || tradePrice <= 0) {
          continue;
        }

        const tradeSizeRaw = Math.abs(parseFloat(v(row, 'size') || 0));
        const tradeSize = Number.isFinite(tradeSizeRaw) ? tradeSizeRaw : 0;
        const candleTimestamp = getCandleStartTime(opts.interval as any, Math.floor(timestampMs / 1000));

        if (!currentCandle || candleTimestamp > currentCandle.timestamp) {
          currentCandle = {
            timestamp: candleTimestamp,
            open: tradePrice,
            high: tradePrice,
            low: tradePrice,
            close: tradePrice,
            volume: tradeSize,
          };
        } else if (candleTimestamp === currentCandle.timestamp) {
          currentCandle = {
            ...currentCandle,
            high: Math.max(currentCandle.high, tradePrice),
            low: Math.min(currentCandle.low, tradePrice),
            close: tradePrice,
            volume: currentCandle.volume + tradeSize,
          };
        } else {
          continue;
        }

        const emitStatus = emitIfChanged(currentCandle);
        if (emitStatus === 'emitted' || emitStatus === 'duplicate') {
          wsLastTradeAt = Date.now();
          // Don't stop poll until first poll has seeded currentCandle with full candle history
          if (hasReceivedPoll) {
            stopFallbackPolling();
          }
        } else if (emitStatus === 'stale') {
          startFallbackPolling();
        }
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (!isActive) return;

      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
      } else {
        waitForConnectTimeoutId = setTimeout(waitForConnectedAndSubscribe, 100);
      }
    };

    startFallbackPolling();
    scheduleWatchdog();
    waitForConnectedAndSubscribe();

    return () => {
      isActive = false;

      if (waitForConnectTimeoutId) {
        clearTimeout(waitForConnectTimeoutId);
        waitForConnectTimeoutId = null;
      }

      if (wsWatchdogTimeoutId) {
        clearTimeout(wsWatchdogTimeoutId);
        wsWatchdogTimeoutId = null;
      }

      stopFallbackPolling();

      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic] = Math.max((this.topics[topic] || 1) - 1, 0);

      if (this.topics[topic] === 0) {
        delete this.topics[topic];
        if (this.isConnected) {
          this.cleanupMessageHandlerStats(handler);
          this.send(JSON.stringify({ op: 'unsubscribe', args: [topic] }));
        }
      }
    };
  };
}
