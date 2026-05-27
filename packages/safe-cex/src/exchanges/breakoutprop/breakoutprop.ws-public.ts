import dayjs from 'dayjs';
import { nanoid } from 'nanoid';
import { Candle, OHLCVOptions, OrderBook, Ticker } from '../../types';
import { BaseWebSocket } from '../base.ws';
import type { BreakoutpropExchange } from './breakoutprop.exchange';
import { TICKER_WS_UPDATE } from './breakoutprop.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: number;
};

export class BreakoutpropPublicWebsocket extends BaseWebSocket<BreakoutpropExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {
    pong: [() => this.handlePongEvent()],
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const { symbol, interval: _interval } = opts;
    const handler = `poll_kline.${symbol}`;
    const parser = (json: Data) => {
      const ticker = json as Ticker;
      // const info: INTX_INSTRUMENTS_RESPONSE[0] = ticker.info;
      const candle: Candle = {
        timestamp: dayjs().utcOffset(0).startOf('second').valueOf() / 1000,
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

  //  COPY

  connectAndSubscribe = (_isReconnect = false) => {};

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribe();
      this.ping();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ op: 'ping' }));
    }
  };

  subscribe = () => {
    // {"event":"sub","params":{"channel":"market_e_chrusdt_ticker","cb_id":"e_chrusdt"}}
    Object.keys(this.topics).forEach((topic) => {
      const payload = { event: 'sub', params: { channel: topic, cb_id: nanoid() } };
      this.send(JSON.stringify(payload));
    });
  };

  onMessage = async ({ data: _data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
    }
  };

  handleTickersEvent = (_json: Record<string, any>, symbol: string) => {
    const json: TICKER_WS_UPDATE = _json as any;

    const ticker: Partial<Ticker> = {
      id: symbol,
      symbol: symbol,
      last: +json.close,
      percentage: +json.rose,
      volume: +json.vol,
      quoteVolume: +json.amount,
      info: json,
    };
    this.parent.store.updateTicker({ id: symbol }, ticker);
  };
}
