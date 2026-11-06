import type { Market, OrderBook, OrderBookOrders } from '../../../types';
import type { OKXExchange } from './okx.exchange';

import flatten from 'lodash/flatten';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { roundUSD } from '../../../utils/roundUSD';
import { multiply } from '../../../utils/safe-math';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URLS } from '../../../exchanges/okx/okx.types';
import { resolveTransportRouting } from '../../../transport/registry';
import { WsDirectFallback } from '../../../utils/wsDirectFallback';

const tickerNumber = (value: unknown): number =>
  value === '' || value === undefined || value === null ? 0 : Number.parseFloat(String(value));

type Data = Record<string, any>;
type MessageHandlers = {
  [channel: string]: Array<(json: Data) => void>;
};
type SubscribedTopics = {
  [id: string]: Array<{ channel: string; instId: string }>;
};

export class OKXPublicWebsocket extends BaseWebSocket<OKXExchange> {
  private orderBookSequenceByHandler = new Map<string, string>();

  private recoveringOrderBooks = new Set<string>();

  private directFallback = new WsDirectFallback();

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
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsPublic;
      const directUrl = BASE_WS_URLS[this.parent.region].public[this.parent.options.testnet ? 'testnet' : 'livenet'];
      const baseUrl = this.directFallback.resolveUrl(directUrl, route.fallback);
      if (baseUrl === route.fallback) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet okx public proxy');
        } catch (_e) {}
      }
      this.ws = this.createWebsocket(baseUrl);

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
        instId: this.parent.getIndexTickerInstId(m),
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
      this.orderBookSequenceByHandler.clear();
      this.recoveringOrderBooks.clear();
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

  private onMarketsChanged = ({ added }: { added: Market[] }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    const newTickerTopics = added.map((m) => ({ channel: 'tickers', instId: m.id }));
    const newMarkPriceTopics = added.map((m) => ({ channel: 'mark-price', instId: m.id }));
    const newIndexTopics = added.map((m) => ({
      channel: 'index-tickers',
      instId: this.parent.getIndexTickerInstId(m),
    }));
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
      this.directFallback.onMessage();
      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      const json = bigIntAsStringJsonParse<Data>(data);
      if (!json || json.event === 'subscribe') return;

      const channel = typeof json.arg?.channel === 'string' ? json.arg.channel : '';
      const instId = typeof json.arg?.instId === 'string' ? json.arg.instId : '';
      if (!channel) return;

      if (instId) {
        this.dispatchMessageHandler(`${channel}.${instId}`, json);
      }
      this.dispatchMessageHandler(channel, json);
    }
  };

  private dispatchMessageHandler = (handler: string, json: Data) => {
    const callbacks = this.messageHandlers[handler];
    if (!callbacks?.length) return;

    if (handler.startsWith('books.') && !this.acceptOrderBookSequence(handler, json)) {
      return;
    }

    this.recordMessageHandlerStats(handler);
    for (const callback of callbacks) {
      callback(json);
    }
  };

  private acceptOrderBookSequence = (handler: string, data: Data) => {
    const update = data.data?.[0];
    const seqId = update?.seqId == null ? '' : String(update.seqId);

    if (data.action === 'snapshot' && seqId) {
      this.orderBookSequenceByHandler.set(handler, seqId);
      this.recoveringOrderBooks.delete(handler);
      return true;
    }

    if (this.recoveringOrderBooks.has(handler)) {
      return false;
    }

    const previousSeqId = update?.prevSeqId == null ? '' : String(update.prevSeqId);
    const expectedPreviousSeqId = this.orderBookSequenceByHandler.get(handler);

    if (!seqId || !previousSeqId || !expectedPreviousSeqId || previousSeqId !== expectedPreviousSeqId) {
      this.recoverOrderBook(handler);
      return false;
    }

    this.orderBookSequenceByHandler.set(handler, seqId);
    return true;
  };

  private recoverOrderBook = (handler: string) => {
    this.orderBookSequenceByHandler.delete(handler);
    this.recoveringOrderBooks.add(handler);

    const topic = this.topics[handler]?.[0];
    if (!topic || !this.isConnected || this.isDisposed) return;

    this.send(JSON.stringify({ op: 'unsubscribe', args: [topic] }));
    this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
  };

  handleTickerEvents = ({ data: [update] }: Data) => {
    // Empty live last/open fields carry no new trade; retain the proven values.
    const open = update.open24h === '' ? undefined : tickerNumber(update.open24h);
    const last = update.last === '' ? undefined : tickerNumber(update.last);
    const percentage =
      last === undefined || open === undefined ? undefined : open === 0 ? 0 : roundUSD(((last - open) / open) * 100);

    this.store.updateTicker(
      { id: update.instId },
      {
        bid: tickerNumber(update.bidPx),
        ask: tickerNumber(update.askPx),
        ...(last !== undefined && { last }),
        ...(percentage !== undefined && { percentage }),
        volume: tickerNumber(update.volCcy24h),
        quoteVolume: tickerNumber(update.vol24h),
      },
    );
  };

  handleMarkPriceEvents = ({ data: [update] }: Data) => {
    this.parent.markPriceLoaded(update.instId);
    this.store.updateTicker({ id: update.instId }, { mark: parseFloat(update.markPx) });
  };

  handleIndexTickerEvents = ({ data: [update] }: Data) => {
    const indexPrice = parseFloat(update.idxPx);
    for (const market of this.parent.getMarketsForIndexTickerInstId(update.instId)) {
      this.parent.indexPriceLoaded(market.id);
      this.store.updateTicker({ id: market.id }, { index: indexPrice });
    }
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

      // A trade price can lag the book; filtering by it deletes valid depth.
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
      this.messageHandlers[handler] = (this.messageHandlers[handler] || []).filter((f) => f !== parser);

      if (!this.messageHandlers[handler].length) {
        delete this.topics[handler];
        this.orderBookSequenceByHandler.delete(handler);
        this.recoveringOrderBooks.delete(handler);
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
