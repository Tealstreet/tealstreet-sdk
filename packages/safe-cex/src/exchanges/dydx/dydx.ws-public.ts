import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, LogSeverity, OHLCVOptions, OrderBook, OrderSide, Ticker, Writable } from '../../types';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { BaseWebSocket } from '../base.ws';
import { DydxExchange } from './dydx.exchange';
import {
  CandleResolution,
  DydxCandle,
  DydxOrderbookLevel,
  DydxWsCandlesUpdate,
  DydxWsChannelBatchData,
  DydxWsChannelData,
  DydxWsMarketsUpdate,
  DydxWsOrderbookUpdate,
  DydxWsSubscribeMessage,
  DydxWsTradesUpdate,
  WS_CHANNELS,
  WSS_URLS,
} from './dydx.types';

type MessageHandlers = {
  [topic: string]: Array<(json: any) => void>;
};

type SubscribedTopics = {
  [id: string]: number;
};

/**
 * dYdX v4 Public WebSocket client
 * Handles real-time public market data: tickers, orderbooks, trades, candles
 */
export class DydxPublicWebSocket extends BaseWebSocket<DydxExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};
  private subscriptions = new Set<string>();

  constructor(parent: DydxExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      const wsUrl = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(wsUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', () => {
        this.onClose();
      });
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.subscriptions.clear();
      this.subscribe();
      this.emitWsReconnected();
    }
  };

  subscribe = () => {
    // Subscribe to public channels only
    this.subscribeToMarkets();
    this.subscribeToBlockHeight();
  };

  /**
   * Subscribe to all markets channel
   * Receives updates for all market data including prices and funding rates
   */
  subscribeToMarkets = () => {
    const channel = WS_CHANNELS.MARKETS;
    const subscriptionKey = channel;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to ${channel}`, LogSeverity.Info);
  };

  /**
   * Subscribe to orderbook updates for a specific market
   */
  subscribeToOrderbook = (market: string) => {
    const channel = WS_CHANNELS.ORDERBOOK;
    const subscriptionKey = `${channel}:${market}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id: market,
      batched: true,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to orderbook for ${market}`, LogSeverity.Info);
  };

  /**
   * Subscribe to trades for a specific market
   */
  subscribeToTrades = (market: string) => {
    const channel = WS_CHANNELS.TRADES;
    const subscriptionKey = `${channel}:${market}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id: market,
      batched: true,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to trades for ${market}`, LogSeverity.Info);
  };

  /**
   * Subscribe to candles for a specific market and resolution
   */
  subscribeToCandles = (market: string, resolution: CandleResolution) => {
    const channel = WS_CHANNELS.CANDLES;
    const id = `${market}/${resolution}`;
    const subscriptionKey = `${channel}:${id}`;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
      id,
      batched: true,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to candles for ${market} ${resolution}`, LogSeverity.Info);
  };

  /**
   * Subscribe to block height updates
   */
  subscribeToBlockHeight = () => {
    const channel = WS_CHANNELS.BLOCK_HEIGHT;
    const subscriptionKey = channel;

    if (this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'subscribe',
      channel,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.add(subscriptionKey);
    this.parent.log(`Subscribed to ${channel}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from orderbook updates
   */
  unsubscribeFromOrderbook = (market: string) => {
    const channel = WS_CHANNELS.ORDERBOOK;
    const subscriptionKey = `${channel}:${market}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id: market,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from orderbook for ${market}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from trades
   */
  unsubscribeFromTrades = (market: string) => {
    const channel = WS_CHANNELS.TRADES;
    const subscriptionKey = `${channel}:${market}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id: market,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from trades for ${market}`, LogSeverity.Info);
  };

  /**
   * Unsubscribe from candles
   */
  unsubscribeFromCandles = (market: string, resolution: CandleResolution) => {
    const channel = WS_CHANNELS.CANDLES;
    const id = `${market}/${resolution}`;
    const subscriptionKey = `${channel}:${id}`;

    if (!this.subscriptions.has(subscriptionKey)) {
      return;
    }

    const message: DydxWsSubscribeMessage = {
      type: 'unsubscribe',
      channel,
      id,
    };

    this.send(JSON.stringify(message));
    this.subscriptions.delete(subscriptionKey);
    this.parent.log(`Unsubscribed from candles for ${market} ${resolution}`, LogSeverity.Info);
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        const messageType = json.type;
        const channel = json.channel;

        // Record stats for non-orderbook channels (orderbook is too frequent)
        if (channel !== WS_CHANNELS.ORDERBOOK) {
          this.recordMessageHandlerStats(channel);
        }

        // Handle subscription responses
        if (messageType === 'subscribed') {
          this.parent.log(`Subscription confirmed for ${channel}`, LogSeverity.Info);
          return;
        }

        if (messageType === 'unsubscribed') {
          this.parent.log(`Unsubscription confirmed for ${channel}`, LogSeverity.Info);
          return;
        }

        if (messageType === 'error') {
          this.parent.log(`WebSocket error: ${json.message || 'Unknown error'}`, LogSeverity.Error);
          return;
        }

        // Handle channel data
        if (messageType === 'channel_data') {
          this.handleChannelData(json as DydxWsChannelData);
        } else if (messageType === 'channel_batch_data') {
          this.handleChannelBatchData(json as DydxWsChannelBatchData);
        }
      } catch (e) {
        this.parent.log(`Error parsing WebSocket message: ${e}`, LogSeverity.Error);
      }
    }
  };

  /**
   * Handle individual channel data messages
   */
  private handleChannelData = (message: DydxWsChannelData) => {
    const { channel, contents, id } = message;

    switch (channel) {
      case WS_CHANNELS.BLOCK_HEIGHT:
        this.handleBlockHeight(contents);
        break;
      default:
        // Execute registered message handlers
        const handlerKey = id ? `${channel}.${id}` : channel;
        if (this.messageHandlers[handlerKey]) {
          for (const fn of this.messageHandlers[handlerKey]) {
            fn(contents);
          }
        }
    }
  };

  /**
   * Handle batched channel data messages
   */
  private handleChannelBatchData = (message: DydxWsChannelBatchData) => {
    const { channel, contents, id } = message;

    if (!Array.isArray(contents)) {
      this.parent.log(`Expected array for batch data in channel ${channel}`, LogSeverity.Warning);
      return;
    }

    for (const item of contents) {
      switch (channel) {
        case WS_CHANNELS.MARKETS:
          this.handleMarketsUpdate(item as DydxWsMarketsUpdate);
          break;
        case WS_CHANNELS.TRADES:
          this.handleTradesUpdate(item as DydxWsTradesUpdate);
          break;
        case WS_CHANNELS.ORDERBOOK:
          this.handleOrderbookUpdate(id || '', item as DydxWsOrderbookUpdate);
          break;
        case WS_CHANNELS.CANDLES:
          this.handleCandlesUpdate(item as DydxWsCandlesUpdate);
          break;
        default:
          const handlerKey = id ? `${channel}.${id}` : channel;
          if (this.messageHandlers[handlerKey]) {
            for (const fn of this.messageHandlers[handlerKey]) {
              fn(item);
            }
          }
      }
    }
  };

  /**
   * Handle markets channel updates
   * Updates ticker data for all markets
   */
  private handleMarketsUpdate = (data: DydxWsMarketsUpdate) => {
    if (!data.trading) return;

    // O(1) hashmap lookup. v4_markets batch frames carry updates for many
    // tickers per frame; the prior tickers.find was N*M per frame.
    const tickersById = this.parent.store.tickersById;
    for (const tickerSymbol in data.trading) {
      const marketData = data.trading[tickerSymbol];
      const ticker = tickersById[tickerSymbol];

      if (ticker) {
        const update: Partial<Writable<Ticker>> = {};

        if (marketData.oraclePrice) {
          update.index = parseFloat(marketData.oraclePrice);
        }
        if (marketData.priceChange24H) {
          update.percentage = parseFloat(marketData.priceChange24H);
        }
        if (marketData.volume24H) {
          update.volume = parseFloat(marketData.volume24H);
        }
        if (marketData.nextFundingRate) {
          update.fundingRate = parseFloat(marketData.nextFundingRate);
        }
        if (marketData.openInterest) {
          update.openInterest = parseFloat(marketData.openInterest);
        }

        this.parent.store.updateTicker(ticker, update);
      }
    }
  };

  /**
   * Handle trades channel updates
   */
  private handleTradesUpdate = (data: DydxWsTradesUpdate) => {
    if (!data.trades || !Array.isArray(data.trades)) return;

    // O(1) hashmap lookup on the trades hot path.
    const marketsById = this.parent.store.marketsById;
    for (const trade of data.trades) {
      const market = marketsById[trade.id];
      if (!market) continue;

      this.parent.emitter.emit('fill', {
        id: trade.id,
        side: trade.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        symbol: market.symbol,
        price: parseFloat(trade.price),
        amount: parseFloat(trade.size),
      });
    }
  };

  /**
   * Handle orderbook channel updates
   */
  private handleOrderbookUpdate = (marketId: string, data: DydxWsOrderbookUpdate) => {
    const handlerKey = `${WS_CHANNELS.ORDERBOOK}.${marketId}`;

    if (this.messageHandlers[handlerKey]) {
      for (const fn of this.messageHandlers[handlerKey]) {
        fn(data);
      }
    }
  };

  /**
   * Handle candles channel updates
   */
  private handleCandlesUpdate = (data: DydxWsCandlesUpdate) => {
    if (!data.candles || !Array.isArray(data.candles)) return;

    for (const candle of data.candles) {
      const handlerKey = `${WS_CHANNELS.CANDLES}.${candle.ticker}.${candle.resolution}`;

      if (this.messageHandlers[handlerKey]) {
        for (const fn of this.messageHandlers[handlerKey]) {
          fn(candle);
        }
      }
    }
  };

  /**
   * Handle block height updates
   */
  private handleBlockHeight = (data: any) => {
    if (data.height) {
      this.parent.log(`Block height: ${data.height}`, LogSeverity.Debug);
    }
  };

  /**
   * Listen to OHLCV candles for a specific market and interval
   */
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) {
      this.parent.log(`Market with symbol ${opts.symbol} not found`, LogSeverity.Error);
      return () => {};
    }

    const marketId = market.id;
    const resolution = this.mapIntervalToResolution(opts.interval);
    const topic = `${WS_CHANNELS.CANDLES}.${marketId}.${resolution}`;
    const handler = topic;

    const parser = (data: DydxCandle) => {
      if (data.ticker === marketId && data.resolution === resolution) {
        this.recordMessageHandlerStats(topic);
        callback({
          timestamp: new Date(data.startedAt).getTime(),
          open: parseFloat(data.open),
          high: parseFloat(data.high),
          low: parseFloat(data.low),
          close: parseFloat(data.close),
          volume: parseFloat(data.baseTokenVolume),
        });
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.subscribeToCandles(marketId, resolution);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.unsubscribeFromCandles(marketId, resolution);
      }
    };
  };

  /**
   * Listen to orderbook updates for a specific market
   */
  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    if (!market) {
      this.parent.log(`Market with symbol ${symbol} not found`, LogSeverity.Error);
      return () => {};
    }

    const marketId = market.id;
    const topic = `${WS_CHANNELS.ORDERBOOK}.${marketId}`;
    const handler = topic;
    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const parser = (data: DydxWsOrderbookUpdate) => {
      if (!data || (!data.bids && !data.asks)) return;

      const orderBook: OrderBook = {
        bids: [],
        asks: [],
      };

      if (data.bids && Array.isArray(data.bids)) {
        orderBook.bids = data.bids
          .map((level: DydxOrderbookLevel) => ({
            price: parseFloat(level.price),
            amount: parseFloat(level.size),
            total: 0,
          }))
          .filter((level) => level.amount > 0);
      }

      if (data.asks && Array.isArray(data.asks)) {
        orderBook.asks = data.asks
          .map((level: DydxOrderbookLevel) => ({
            price: parseFloat(level.price),
            amount: parseFloat(level.size),
            total: 0,
          }))
          .filter((level) => level.amount > 0);
      }

      this.orderBookInnerState[innerId].isSnapshotLoaded = true;

      // dYdX v4 indexer delivers orderbook levels pre-sorted (bids DESC,
      // asks ASC). The arrays are .map'd from the source array preserving
      // order, so the trailing sortOrderBook is redundant.
      if (orderBook.asks.length !== 0 || orderBook.bids.length !== 0) {
        calcOrderBookTotal(orderBook);
        callback(orderBook);
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.subscribeToOrderbook(marketId);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      delete this.orderBookInnerState[innerId];
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.unsubscribeFromOrderbook(marketId);
      }
    };
  };

  /**
   * Map interval string to dYdX CandleResolution
   */
  private mapIntervalToResolution(interval: string): CandleResolution {
    const mapping: Record<string, CandleResolution> = {
      '1m': CandleResolution.ONE_MIN,
      '5m': CandleResolution.FIVE_MINS,
      '15m': CandleResolution.FIFTEEN_MINS,
      '30m': CandleResolution.THIRTY_MINS,
      '1h': CandleResolution.ONE_HOUR,
      '4h': CandleResolution.FOUR_HOURS,
      '1d': CandleResolution.ONE_DAY,
    };

    return mapping[interval] || CandleResolution.ONE_MIN;
  }

  /**
   * dYdX doesn't require explicit ping, connection is maintained by server
   */
  ping = (): void => {
    // No-op: dYdX maintains the connection automatically
  };
}
