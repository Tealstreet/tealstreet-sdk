import type { Candle, OHLCVOptions, OrderBook, Ticker, Writable } from '../../../types';
import type { KucoinExchange } from './kucoin.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { updateOrderBookByPrice } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ENDPOINTS, WS_INTERVAL } from './kucoin.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

// KuCoin closes the socket if a SESSION exceeds 400 topic subscriptions
// ("exceed max subscription count limitation of 400 per session"). With ~600
// USDT markets, even one ticker channel for every symbol overflows one
// connection — so the public feed is a small POOL of sockets (like
// bitget/bitunix), each staying under the cap and differing only by ROLE:
//   • 'detail'   — orderbook + candles + the actively-viewed symbol's bid/ask +
//     mark/index, all on-demand (a user only views a handful at once).
//   • 'snapshot' — bulk-subscribes /contractMarket/snapshot (last / 24h% /
//     funding / volume) for an interleaved shard of all symbols, giving the
//     screener live data for every market.
// All sockets share one store through the handlers; see CLAUDE.md "400-topic
// per-connection limit".
const MAX_SNAPSHOT_SYMBOLS_PER_CONNECTION = 380; // < 400, leaves room for ack/control frames
const SNAPSHOT_CHANNEL = '/contractMarket/snapshot';
// Per-viewed-symbol channels carried by the detail socket alongside the orderbook.
const ACTIVE_DETAIL_TICKER_CHANNELS = ['/contractMarket/tickerV2', '/contract/instrument'];

export type KucoinPublicRole = 'detail' | 'snapshot';

export class KucoinPublicWebsocket extends BaseWebSocket<KucoinExchange> {
  messageHandlers: MessageHandlers = {};
  token: string | null = null;
  socketEndpoint: string | null = null;
  private role: KucoinPublicRole;
  private shardIndex: number;
  private shardCount: number;

  constructor(parent: KucoinExchange, opts?: { role?: KucoinPublicRole; shardIndex?: number; shardCount?: number }) {
    super(parent);
    this.role = opts?.role ?? 'detail';
    this.shardIndex = opts?.shardIndex ?? 0;
    this.shardCount = opts?.shardCount ?? 1;
  }

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
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
        this.retryFailedConnect(err?.response?.data?.msg || err?.message || 'Failed to connect to WebSocket');
      }
    }
  };

  reconnect = async () => {
    if (this.isDisposed) return;
    const previousSocket = this.ws;
    previousSocket?.removeEventListener?.('open', this.onOpen);
    previousSocket?.removeEventListener?.('message', this.onMessage);
    previousSocket?.removeEventListener?.('close', this.onClose);
    this.ws = undefined;
    previousSocket?.close?.();
    this.token = null;
    this.socketEndpoint = null;
    await this.connectAndSubscribe();
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
    if (added.length === 0 || !this.ws || this.isDisposed || !this.isConnected) return;
    // Only snapshot sockets carry the bulk feed; subscribe the newly-added
    // symbols that fall in THIS socket's interleaved shard. The detail socket
    // adds nothing here (it is purely on-demand).
    if (this.role !== 'snapshot') return;
    const owned = new Set(this.snapshotShardSymbols());
    const newSymbols = added
      .map((m) => m.id || this.parent.toApiSymbol(m.symbol))
      .filter((s): s is string => Boolean(s) && owned.has(s));
    if (newSymbols.length) this.sendChannelOp('subscribe', [SNAPSHOT_CHANNEL], newSymbols);
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      // KuCoin requires ping to be a JSON object
      this.send(JSON.stringify({ id: Date.now(), type: 'ping' }));
    }
  };

  subscribe = () => {
    if (this.role === 'snapshot') {
      // Bulk snapshot (last / 24h% / funding / volume) for this socket's shard
      // of all markets — gives the screener live data for every symbol.
      this.sendChannelOp('subscribe', [SNAPSHOT_CHANNEL], this.snapshotShardSymbols());
    }
    // The 'detail' socket subscribes nothing up front: orderbook, candles and
    // the viewed symbol's bid/ask + mark/index are added on demand (listen*).
  };

  // Interleaved (index % shardCount) shard of all market api-symbols owned by
  // this snapshot socket — balances load across sockets regardless of order.
  private snapshotShardSymbols = (): string[] => {
    const ids = this.parent.store.markets
      .map((m) => m.id || this.parent.toApiSymbol(m.symbol))
      .filter((s): s is string => Boolean(s));
    return ids.filter((_, i) => i % this.shardCount === this.shardIndex);
  };

  // Subscribe/unsubscribe the viewed symbol's detail ticker channels (bid/ask +
  // mark/index). Called from listenOrderBook on the 'detail' socket. snapshot
  // (last/24h%/funding/volume) for the symbol already arrives on a snapshot
  // socket, so it is not duplicated here.
  subscribeActiveTicker = (apiSymbol: string) => {
    if (apiSymbol && !this.isDisposed) this.sendChannelOp('subscribe', ACTIVE_DETAIL_TICKER_CHANNELS, [apiSymbol]);
  };

  unsubscribeActiveTicker = (apiSymbol: string) => {
    if (apiSymbol && !this.isDisposed) this.sendChannelOp('unsubscribe', ACTIVE_DETAIL_TICKER_CHANNELS, [apiSymbol]);
  };

  // Batched (≤100 symbols/frame, KuCoin's per-frame cap) sub/unsub across channels.
  private sendChannelOp = (type: 'subscribe' | 'unsubscribe', channels: string[], symbols: string[]) => {
    for (let i = 0; i < symbols.length; i += 100) {
      const chunk = symbols.slice(i, i + 100).join(',');
      if (!chunk) continue;
      for (const channel of channels) {
        this.send(
          JSON.stringify({
            id: Date.now(),
            type,
            topic: `${channel}:${chunk}`,
            privateChannel: false,
            response: false,
          }),
        );
      }
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

      // Ticker family: tickerV2 (best bid/ask), snapshot.24h (last / 24h% /
      // funding / volume), instrument (mark / index). Disjoint fields → route
      // each to its own partial-update handler. tickerV2 must be matched before
      // any '/contractMarket/ticker' prefix check.
      if (json.topic && json.topic.startsWith('/contractMarket/tickerV2')) {
        this.recordMessageHandlerStats('ticker');
        this.handleTickerV2(json);
        return;
      }
      if (json.topic && json.topic.startsWith('/contractMarket/snapshot')) {
        this.recordMessageHandlerStats('snapshot');
        this.handleSnapshot(json);
        return;
      }
      if (json.topic && json.topic.startsWith('/contract/instrument')) {
        this.recordMessageHandlerStats('instrument');
        this.handleInstrument(json);
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

  // KuCoin splits ticker data across THREE channels with disjoint fields, so
  // each handler must update ONLY the fields its channel actually carries —
  // mapping absent fields (the old single handler did) zeroes good data.
  private tickerForApiSymbol = (apiSymbol?: string): Writable<Ticker> | undefined => {
    if (!apiSymbol) return undefined;
    return this.parent.store.tickersBySymbol[this.parent.toInternalSymbol(apiSymbol)];
  };

  // /contractMarket/tickerV2 — best bid/ask only.
  handleTickerV2 = (json: Record<string, any>) => {
    const data = json.data;
    const ticker = this.tickerForApiSymbol(data?.symbol);
    if (!ticker) return;
    this.parent.store.updateTicker(ticker, {
      bid: parseFloat(data.bestBidPrice || ticker.bid || '0'),
      ask: parseFloat(data.bestAskPrice || ticker.ask || '0'),
    });
  };

  // /contractMarket/snapshot (subject snapshot.24h) — last price, 24h change,
  // funding rate, base + quote volume.
  handleSnapshot = (json: Record<string, any>) => {
    const data = json.data;
    const ticker = this.tickerForApiSymbol(data?.symbol);
    if (!ticker) return;
    const update: Partial<Writable<Ticker>> = {};
    if (data.lastPrice != null) update.last = parseFloat(data.lastPrice);
    if (data.priceChgPct != null) update.percentage = parseFloat(data.priceChgPct) * 100;
    if (data.fundingRate != null) update.fundingRate = parseFloat(data.fundingRate);
    if (data.volume != null) update.volume = parseFloat(data.volume);
    if (data.turnover != null) update.quoteVolume = parseFloat(data.turnover);
    if (Object.keys(update).length) this.parent.store.updateTicker(ticker, update);
  };

  // /contract/instrument (subject mark.index.price) — mark + index price. The
  // payload has no symbol field, so resolve it from the topic suffix.
  handleInstrument = (json: Record<string, any>) => {
    const data = json.data;
    if (!data) return;
    const ticker = this.tickerForApiSymbol((json.topic || '').split(':')[1]);
    if (!ticker) return;
    const update: Partial<Writable<Ticker>> = {};
    if (data.markPrice != null) update.mark = parseFloat(data.markPrice);
    if (data.indexPrice != null) update.index = parseFloat(data.indexPrice);
    if (Object.keys(update).length) this.parent.store.updateTicker(ticker, update);
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
        for (let i = 0; i < replay.length; i++) {
          if (!snapshotLoaded) {
            // A gap retains its first delta; keep the rest for the newer snapshot.
            bufferedUpdates = bufferedUpdates.concat(replay.slice(i));
            break;
          }
          applyIncrement(replay[i]);
        }

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
          // The viewed symbol may sit outside any snapshot shard's bid/ask scope
          // (snapshot has no bid/ask) and has no mark/index feed otherwise, so
          // pull its live bid/ask + mark/index here on the detail socket.
          this.subscribeActiveTicker(apiSymbol);
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
        this.unsubscribeActiveTicker(apiSymbol);
      }
    };
  };
}

// Build the snapshot-socket pool, sharded so each connection stays under
// KuCoin's 400-topic-per-session cap. One socket for ≤380 markets, two for
// ≤760, etc. The 'detail' socket is created separately by the exchange.
export const createKucoinSnapshotWebsockets = (
  parent: KucoinExchange,
  symbolCount: number,
): KucoinPublicWebsocket[] => {
  const shardCount = Math.max(1, Math.ceil(symbolCount / MAX_SNAPSHOT_SYMBOLS_PER_CONNECTION));
  return Array.from(
    { length: shardCount },
    (_, shardIndex) => new KucoinPublicWebsocket(parent, { role: 'snapshot', shardIndex, shardCount }),
  );
};
