import dayjs from 'dayjs';
import { nanoid } from 'nanoid';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { Candle, OHLCVOptions, OrderBook, timeframeToSeconds } from '../../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { calcOrderBookTotal, sortOrderBook } from '../../../utils/orderbook';
import { add } from '../../../utils/safe-math';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import type { CoinbaseExchange } from './coinbase.exchange';
import { BASE_WS_URL, CANDLE_EVENT, LEVEL2_EVENT, TRADE_EVENT } from './coinbase.types';
import { normalizeCoinbaseUsdEquivalentProductId } from './coinbase.utils';

export type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class CoinbasePublicWebsocket extends BaseWebSocket<CoinbaseExchange> {
  messageHandlers: MessageHandlers = {
    // '24hrTicker': [(d: Data) => this.handleTickerStreamEvents(d)],
    // bookTicker: [(d: Data) => this.handleBookTickersStreamEvents(d)],
    // markPriceUpdate: [(d: Data) => this.handleMarkPriceStreamEvents(d)],
    candles: [(d: Data) => this.handleOhlcvStreamEvents(d)],
    market_trades: [(d: Data) => this.handleTradeStreamEvents(d)],
    // ticker_batch: [(_d: Data) => this.handleBatchTickerStreamEvents(_d)],
    l2_data: [(d: Data) => this.handleOrderBookStreamEvents(d)],
    heartbeats: [() => this.handlePongEvent()],
    subscriptions: [() => {}],
  };
  topics: { [key: string]: number } = {
    heartbeats: 1,
  };

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
      this.subscribe();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  subscribe = () => {
    for (const topic of Object.keys(this.topics)) {
      const [channel, symbol] = topic.split('.');
      this.send(
        JSON.stringify({
          type: 'subscribe',
          product_ids: symbol ? [symbol] : this.parent.store.markets.map((m) => m.symbol),
          channel,
        }),
      );
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    this.trackLastReceived();
    if (!this.isDisposed) {
      // const handlers = Object.entries(this.messageHandlers);
      const json = jsonParse(data);
      if (!json) return;
      if (json.type === 'error') {
        this.parent.emitter.emit('error', json.message);
        return;
      }
      const channel = json!.channel;
      if (!this.messageHandlers[channel]) {
        console.error(`No handler for channel ${channel}`);
        console.error(json);
        return;
      }

      for (const callback of this.messageHandlers[channel]) {
        if (json) callback(json);
      }
    }
  };

  // handleBatchTickerStreamEvents = (data: Data) => {
  //   const events = data['events'] as any as TICKER_EVENT[];
  //   const symbols = new Set<string>();
  //   for (const event of events) {
  //     if (event.type === 'update') {
  //       for (const coinbaseTicker of event.tickers) {
  //         if (!coinbaseTicker) continue;
  //
  //         const symbol = coinbaseTicker.product_id;
  //         const prevTicker = this.parent.store.tickers.find((t) => t.symbol === symbol);
  //         if (prevTicker) {
  //           symbols.add(symbol);
  //           // TODO: fix bid, ask, mark, index, openInterest, fundingRate, fundingTime, quoteVolume
  //           const update: Partial<Writable<Ticker>> = {
  //             id: symbol,
  //             symbol,
  //             bid: parseFloat(coinbaseTicker.price),
  //             ask: parseFloat(coinbaseTicker.price),
  //             last: parseFloat(coinbaseTicker.price),
  //             mark: parseFloat(coinbaseTicker.price),
  //             index: parseFloat(coinbaseTicker.price),
  //             percentage: parseFloat(coinbaseTicker.price_percent_chg_24_h),
  //             volume: parseFloat(coinbaseTicker.volume_24_h),
  //           };
  //           this.parent.store.updateTicker(prevTicker, update);
  //         }
  //       }
  //     } else {
  //       // do nothing
  //     }
  //   }
  //   for (const symbol of symbols) {
  //     this.recordMessageHandlerStats(`ticker_batch.${symbol}`);
  //   }
  // };

  handleOhlcvStreamEvents = (data: Data) => {
    const events = data['events'] as any as CANDLE_EVENT[];
    const symbols = new Set<string>();
    for (const event of events) {
      if (event.type === 'update') {
        const updatesBySymbol: Record<string, Candle[]> = {};
        for (const candle of event.candles) {
          const symbol = candle.product_id;
          updatesBySymbol[symbol] = updatesBySymbol[symbol] || [];
          updatesBySymbol[symbol].push({
            timestamp: parseInt(candle.start),
            open: parseFloat(candle.open),
            high: parseFloat(candle.high),
            low: parseFloat(candle.low),
            close: parseFloat(candle.close),
            volume: parseFloat(candle.volume),
          });
        }
        for (let [symbol, candles] of Object.entries(updatesBySymbol)) {
          symbol = normalizeCoinbaseUsdEquivalentProductId(symbol);
          const handler = `candles.${symbol}`;
          const callbacks = this.messageHandlers[handler];
          if (!callbacks?.length) continue;
          symbols.add(symbol);
          for (const callback of callbacks) {
            for (const candle of candles) {
              callback(candle);
            }
          }
        }
      } else if (event.type === 'snapshot') {
        // do nothing
      }
    }
    for (const symbol of symbols) {
      this.recordMessageHandlerStats(`candles.${symbol}`);
    }
  };

  handleTradeStreamEvents = (data: Data) => {
    const events = data['events'] as any as TRADE_EVENT[];
    const symbols = new Set<string>();
    for (const event of events) {
      // only use first trade in event for now as this is just to patch candles
      // TODO: if you want actual trade tape without holes fix this...
      // also skipping snapshots...
      if (event.type === 'update') {
        const latestTrade = event.trades[0];
        if (latestTrade) {
          const symbol = latestTrade.product_id;
          symbols.add(symbol);
          const handler = `market_trades.${symbol}`;
          this.messageHandlers[handler]?.forEach((callback) => callback(latestTrade));
        }
      }
    }
    for (const symbol of symbols) {
      this.recordMessageHandlerStats(`market_trades.${symbol}`);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const symbol = normalizeCoinbaseUsdEquivalentProductId(opts.symbol);
    // {
    //   "type": "subscribe",
    //   "product_ids": [
    //   "ETH-USD"
    // ],
    //   "channel": "candles",
    //   "jwt": "XYZ"
    // }
    const topic = `candles.${symbol}`;
    const handler = `candles.${symbol}`;

    const secondaryTopic = `market_trades.${symbol}`;
    const secondaryHandler = `market_trades.${symbol}`;

    const intervalInSeconds = timeframeToSeconds[opts.interval] || Infinity;
    const relyOnTrades = intervalInSeconds < 5 * 60;

    let lastCandle: Candle | undefined = undefined;

    const parser = (json: Data) => {
      // if < 5m drop and rely on trades as TV cant interpolate
      if (relyOnTrades) return;

      lastCandle = json as Candle;

      // if is daily candle or greater we have to format it ourselves
      // tradingview only sends seconds candles, TV only interpolates candles less than daily
      // https://github.com/tradingview/charting_library/issues/8427#issuecomment-1923658340
      // TODO handle new daily candles during session
      if (opts.interval.endsWith('d') || opts.interval.endsWith('w') || opts.interval.endsWith('M')) {
        if (!this.parent.dailyCandles[opts.symbol]) {
          console.warn('fetching daily candle for formatting');
          // fetch last 2 candles in case our date parsing is off. 0th candle will be newest
          this.parent.fetchOHLCV({
            to: dayjs().utcOffset(0).endOf('day').valueOf(),
            symbol: opts.symbol,
            interval: '1d',
            limit: 2,
          });
          return;
        }
        const dailyCandle = this.parent.dailyCandles[opts.symbol];
        lastCandle = {
          ...this.parent.dailyCandles[opts.symbol],
          high: Math.max(dailyCandle.high, lastCandle.close),
          low: Math.min(dailyCandle.low, lastCandle.close),
          close: lastCandle.close,
          volume: dailyCandle.volume + lastCandle.volume,
        };
        this.parent.dailyCandles[opts.symbol] = lastCandle;
      }
      callback(lastCandle);
    };

    const secondaryParser = (json: Data) => {
      const trade: TRADE_EVENT['trades'][0] = json as any;
      const price = parseFloat(trade.price);
      // "2024-10-25T17:18:53.106855195Z"
      const rawTime = trade.time;
      const time = dayjs(rawTime).unix();
      if (relyOnTrades && !lastCandle) {
        lastCandle = {
          timestamp: time - (time % intervalInSeconds),
          open: price,
          high: price,
          low: price,
          close: price,
          volume: 0,
        };
      }
      if (!lastCandle) return;
      if (time < lastCandle.timestamp) {
        return;
      }
      if (relyOnTrades) {
        const timestamp = time - (time % intervalInSeconds);
        if (timestamp !== lastCandle.timestamp) {
          lastCandle = {
            timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0,
          };
        }
      }

      const volume = parseFloat(trade.size);
      lastCandle = {
        ...lastCandle,
        high: Math.max(lastCandle.high, price),
        low: Math.min(lastCandle.low, price),
        close: price,
        volume: add(lastCandle.volume, volume / price),
      };
      callback(lastCandle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        const shouldSubscribe = !this.messageHandlers[handler]?.length;
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        if (shouldSubscribe) {
          const payload = {
            type: 'subscribe',
            product_ids: [symbol],
            channel: 'candles',
          };
          this.send(JSON.stringify(payload));
        }

        if (!this.parent.options.publicDataOnly) {
          const shouldSubscribeSecondary = !this.messageHandlers[secondaryHandler]?.length;
          this.messageHandlers[secondaryTopic] = [...(this.messageHandlers[secondaryTopic] || []), secondaryParser];
          this.topics[secondaryTopic] = (this.topics[secondaryTopic] || 0) + 1;

          if (shouldSubscribeSecondary) {
            const secondaryPayload = {
              type: 'subscribe',
              product_ids: [symbol],
              channel: 'market_trades',
            };
            this.send(JSON.stringify(secondaryPayload));
          }
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.cleanupMessageHandlerStats(`candles.${symbol}`);
      this.cleanupMessageHandlerStats(`market_trades.${symbol}`);

      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.messageHandlers[secondaryHandler] = [
        ...(this.messageHandlers[secondaryHandler] || []).filter((f) => f !== secondaryParser),
      ];
      this.topics[topic]--;
      this.topics[secondaryTopic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = {
          type: 'unsubscribe',
          product_ids: [symbol],
          channel: 'candles',
        };
        this.send(JSON.stringify(payload));
      }

      if (this.isConnected && !this.messageHandlers[secondaryHandler].length) {
        const secondaryPayload = {
          type: 'unsubscribe',
          product_ids: [symbol],
          channel: 'market_trades',
        };
        this.send(JSON.stringify(secondaryPayload));
      }
    };
  };

  handleOrderBookStreamEvents = (data: Data) => {
    const events = data['events'] as any as LEVEL2_EVENT[];
    const symbols = new Set<string>();
    for (const event of events) {
      const symbol = event.product_id;
      symbols.add(symbol);
      const handler = `level2.${symbol}`;
      this.messageHandlers[handler]?.forEach((callback) => callback(event));
    }
    for (const symbol of symbols) {
      this.recordMessageHandlerStats(`level2.${symbol}`);
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    symbol = normalizeCoinbaseUsdEquivalentProductId(symbol);
    const topic = `level2.${symbol}`;
    const handler = `level2.${symbol}`;

    const innerId = nanoid();
    const orderBook: OrderBook = { bids: [], asks: [] };

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const parser = (json: Data) => {
      const event = json as LEVEL2_EVENT;
      if (event.type === 'snapshot') {
        orderBook.bids = [];
        orderBook.asks = [];

        event.updates.forEach(({ side, price_level, new_quantity }) => {
          if (side !== 'bid' && side !== 'offer') return;

          const key = side === 'offer' ? 'asks' : 'bids';
          orderBook[key].push({
            price: parseFloat(price_level),
            amount: parseFloat(new_quantity),
            total: 0,
          });
        });
        this.orderBookInnerState[innerId].isSnapshotLoaded = true;
      } else if (event.type === 'update') {
        if (this.orderBookInnerState[innerId].isSnapshotLoaded) {
          event.updates.forEach(({ side, price_level, new_quantity }) => {
            if (side !== 'offer' && side !== 'bid') return;
            const key: 'bids' | 'asks' = side === 'offer' ? 'asks' : 'bids';
            const price = parseFloat(price_level);
            const amount = parseFloat(new_quantity);
            const update = { price, amount, total: 0 };
            updateOrderBook(orderBook, key, update);
          });
        }
      } else {
        console.warn('Unknown event type', event.type);
        return;
      }

      if (orderBook.asks.length !== 0 || orderBook.bids.length !== 0) {
        sortOrderBook(orderBook);
        calcOrderBookTotal(orderBook);
        callback(orderBook);
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        const shouldSubscribe = !this.messageHandlers[handler]?.length;
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;

        if (shouldSubscribe) {
          const payload = {
            type: 'subscribe',
            product_ids: [symbol],
            channel: 'level2',
          };
          this.send(JSON.stringify(payload));
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.cleanupMessageHandlerStats(`level2.${symbol}`);
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = {
          type: 'unsubscribe',
          product_ids: [symbol],
          channel: 'level2',
          // jwt: sign(this.parent.options.key, this.parent.options.secret),
        };
        this.send(JSON.stringify(payload));
      }
    };
  };

  // no ping for coinbase, only heartbeats
  ping = (): void => {};
}
