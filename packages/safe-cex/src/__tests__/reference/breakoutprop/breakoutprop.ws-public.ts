import { Candle, OHLCVOptions, OrderBook, Ticker, timeframeToSeconds } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import type { BreakoutpropExchange } from './breakoutprop.exchange';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class BreakoutpropPublicWebsocket extends BaseWebSocket<BreakoutpropExchange> {
  messageHandlers: MessageHandlers = {};

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const { symbol, interval } = opts;
    const intervalSeconds = timeframeToSeconds[interval];
    const handler = `poll_kline.${symbol}`;
    const parser = (json: Data) => {
      const ticker = json as Ticker;
      // const info: INTX_INSTRUMENTS_RESPONSE[0] = ticker.info;
      const candle: Candle = {
        timestamp: Math.floor(Date.now() / 1000 / intervalSeconds) * intervalSeconds,
        open: ticker.last,
        high: ticker.last,
        low: ticker.last,
        close: ticker.last,
        volume: 0,
      };

      callback(candle);
    };
    this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      if (!this.messageHandlers[handler].length) {
        this.cleanupMessageHandlerStats(handler);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const handler = `poll_book.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };
    const parser = (json: Data) => {
      const ticker = json as Ticker;
      orderBook.bids = [
        {
          price: ticker.bid,
          amount: 1,
          total: 1,
        },
      ];
      orderBook.asks = [
        {
          price: ticker.ask,
          amount: 1,
          total: 1,
        },
      ];

      callback(orderBook);
    };
    this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      if (!this.messageHandlers[handler].length) {
        this.cleanupMessageHandlerStats(handler);
      }
    };
  };

}
