import type { OrderBook, OrderBookOrders } from '../../types';
import type { OKXExchange } from './okx.exchange';

import flatten from 'lodash/flatten';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { roundUSD } from '../../utils/roundUSD';
import { multiply } from '../../utils/safe-math';
import { updateOrderBook } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URLS } from './okx.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [channel: string]: Array<(json: Data) => void>;
};
type SubscribedTopics = {
  [id: string]: Array<{ channel: string; instId: string }>;
};

export class OKXPublicWebsocket extends BaseWebSocket<OKXExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {
    tickers: [(d: Data) => this.handleTickerEvents(d)],
    'mark-price': [(d: Data) => this.handleMarkPriceEvents(d)],
    'index-tickers': [(d: Data) => this.handleIndexTickerEvents(d)],
    'open-interest': [(d: Data) => this.handleOpenInterestEvents(d)],
    'funding-rate': [(d: Data) => this.handleFundingRateEvents(d)],
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(
        BASE_WS_URLS[this.parent.region].public[this.parent.options.testnet ? 'testnet' : 'livenet'],
      );

      this.topics.tickers = this.store.markets.map((m) => ({
        channel: 'tickers',
        instId: m.id,
      }));

      this.topics['mark-price'] = this.store.markets.map((m) => ({
        channel: 'mark-price',
        instId: m.id,
      }));

      this.topics['index-tickers'] = this.store.markets.map((m) => ({
        channel: 'index-tickers',
        instId: m.id.replace('-SWAP', ''),
      }));

      this.topics['open-interest'] = this.store.markets.map((m) => ({
        channel: 'open-interest',
        instId: m.id,
      }));

      this.topics['funding-rate'] = this.store.markets.map((m) => ({
        channel: 'funding-rate',
        instId: m.id,
      }));

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
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

  private onMarketsChanged = ({ added }: { added: Array<{ id: string; symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    const newTickerTopics = added.map((m) => ({ channel: 'tickers', instId: m.id }));
    const newMarkPriceTopics = added.map((m) => ({ channel: 'mark-price', instId: m.id }));
    const newIndexTopics = added.map((m) => ({ channel: 'index-tickers', instId: m.id.replace('-SWAP', '') }));
    const newOiTopics = added.map((m) => ({ channel: 'open-interest', instId: m.id }));
    const newFundingTopics = added.map((m) => ({ channel: 'funding-rate', instId: m.id }));

    this.topics.tickers = [...(this.topics.tickers || []), ...newTickerTopics];
    this.topics['mark-price'] = [...(this.topics['mark-price'] || []), ...newMarkPriceTopics];
    this.topics['index-tickers'] = [...(this.topics['index-tickers'] || []), ...newIndexTopics];
    this.topics['open-interest'] = [...(this.topics['open-interest'] || []), ...newOiTopics];
    this.topics['funding-rate'] = [...(this.topics['funding-rate'] || []), ...newFundingTopics];

    const allNewTopics = [
      ...newTickerTopics,
      ...newMarkPriceTopics,
      ...newIndexTopics,
      ...newOiTopics,
      ...newFundingTopics,
    ];

    const batchSize = 100;
    for (let i = 0; i < allNewTopics.length; i += batchSize) {
      const batch = allNewTopics.slice(i, i + batchSize);
      this.send(JSON.stringify({ op: 'subscribe', args: batch }));
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send('ping');
    }
  };

  subscribe = () => {
    const topics = flatten(Object.values(this.topics));

    // Batch subscriptions to avoid exceeding OKX's WebSocket frame size limit
    // (sending all ~1500 topics at once triggers code 1009 "Message too big")
    const batchSize = 100;
    for (let i = 0; i < topics.length; i += batchSize) {
      const batch = topics.slice(i, i + batchSize);
      this.send(JSON.stringify({ op: 'subscribe', args: batch }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      for (const [channel, handler] of Object.entries(this.messageHandlers)) {
        let [leftmost, _mid, instId] = channel.split('.');
        if (leftmost === 'books') {
          instId = _mid;
        }
        if (
          data.includes(`channel":"${leftmost}`) &&
          (!instId || data.includes(`instId":"${instId}`)) &&
          !data.includes('event":"subscribe"')
        ) {
          const json = jsonParse(data);
          for (const cb of handler) {
            if (json) cb(json);
          }
        }
      }
    }
  };

  handleTickerEvents = ({ data: [update] }: Data) => {
    const open = parseFloat(update.open24h);
    const last = parseFloat(update.last);
    const percentage = roundUSD(((last - open) / open) * 100);

    this.store.updateTicker(
      { id: update.instId },
      {
        bid: parseFloat(update.bidPx),
        ask: parseFloat(update.askPx),
        last,
        percentage,
        volume: parseFloat(update.volCcy24h),
        quoteVolume: parseFloat(update.vol24h),
      },
    );
  };

  handleMarkPriceEvents = ({ data: [update] }: Data) => {
    this.store.updateTicker({ id: update.instId }, { mark: parseFloat(update.markPx) });
  };

  handleIndexTickerEvents = ({ data: [update] }: Data) => {
    this.store.updateTicker({ id: `${update.instId}-SWAP` }, { index: parseFloat(update.idxPx) });
  };

  handleOpenInterestEvents = ({ data: [update] }: Data) => {
    this.store.updateTicker({ id: update.instId }, { openInterest: parseFloat(update.oiCcy) });
  };

  handleFundingRateEvents = ({ data: [update] }: Data) => {
    this.store.updateTicker(
      { id: update.instId },
      { fundingRate: parseFloat(update.fundingRate), fundingTime: parseFloat(update.fundingTime) },
    );
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    if (!this.store.loaded.markets) {
      timeoutId = setTimeout(() => this.listenOrderBook(symbol, callback), 100);

      return () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      };
    }

    const market = this.store.markets.find((m) => m.symbol === symbol);
    if (!market) return () => {};

    const handler = `books.${market.id}`;

    const sides = ['bids', 'asks'] as const;
    const orderBook: OrderBook = { bids: [], asks: [] };
    const topic = { channel: 'books', instId: market.id };

    const parser = (data: Data) => {
      if (data.action === 'snapshot') {
        const {
          data: [snapshot],
        } = data;

        sides.forEach((side) => {
          orderBook[side] = snapshot[side].reduce((acc: OrderBookOrders[], [price, amount]: string[]) => {
            if (parseFloat(amount) === 0) return acc;
            return [
              ...acc,
              {
                price: parseFloat(price),
                amount: multiply(parseFloat(amount), market.precision.amount),
              },
            ];
          }, []);
        });
      }

      if (data.action === 'update') {
        const {
          data: [update],
        } = data;

        sides.forEach((side) => {
          for (const [rPrice, rAmount] of update[side]) {
            const price = parseFloat(rPrice);
            const amount = parseFloat(rAmount);
            const update = {
              price,
              amount: multiply(amount, market.precision.amount),
              total: 0,
            };
            updateOrderBook(orderBook, side, update);
          }
        });
      }

      // Use O(1) hashmap lookup instead of O(n) find
      const ticker = this.store.tickersBySymbol[market.symbol];

      const lastPrice = ticker?.last || 0;
      orderBook.asks = orderBook.asks.filter((a) => a.price >= lastPrice);
      orderBook.bids = orderBook.bids.filter((b) => b.price <= lastPrice);

      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

          const payload = JSON.stringify({ op: 'subscribe', args: [topic] });
          this.send(payload);

          // store subscribed topic to re-subscribe on reconnect
          this.topics[handler] = [topic];
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

      if (!this.messageHandlers[handler].length) {
        delete this.topics[handler];
        this.cleanupMessageHandlerStats(handler);
      }

      orderBook.bids = [];
      orderBook.asks = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }
    };
  };
}
