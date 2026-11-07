import { inflate } from 'pako';
import { Candle, OHLCVOptions, OrderBook } from '../../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { MessageHandlers } from '../../../exchanges/types';
import { BitunixExchange } from './bitunix.exchange';
import { resolveTransportRouting } from '../../../transport/registry';
import { BASE_WS_URL } from './bitunix.types';
import { INTERVAL } from './bitunix.ws-public';

export class BitunixFuturesWebsocket extends BaseWebSocket<BitunixExchange> {
  topics: { [key: string]: number } = {};
  messageHandlers: MessageHandlers = {};
  pingLoopTimeoutId?: NodeJS.Timeout;
  private messageCount = 0;
  private useFallbackUrl = this.parent.options.tealstreetWsRoute;
  private lastMessageCount = 0;
  private connectionAttempts = 0;
  private readonly MAX_ATTEMPTS = 2;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // Only switch to fallback URL if we've tried the normal connection multiple times without success
      if (this.connectionAttempts >= this.MAX_ATTEMPTS && this.lastMessageCount === 0 && this.messageCount === 0) {
        this.useFallbackUrl = true;
        this.connectionAttempts = 0; // Reset attempts when switching to fallback
      } else if (!this.useFallbackUrl) {
        this.connectionAttempts++;
      }

      this.lastMessageCount = this.messageCount;
      this.messageCount = 0; // Reset message counter on new connection

      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const fallbackUrl = resolveTransportRouting(this.parent.name, this.parent.options).wsBusiness.fallback;
      const url = this.useFallbackUrl && fallbackUrl ? fallbackUrl : BASE_WS_URL.frontendfutures[key];

      if (this.useFallbackUrl && fallbackUrl) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bitunix proxy (futures)');
        } catch (_e) {}
      }

      this.ws = this.createWebsocket(url);

      const handleClose = () => {
        clearTimeout(this.pingLoopTimeoutId);
        this.onClose();
      };

      const handleError = () => {
        if (this.ws && this.ws.readyState !== WebSocket.CLOSED) {
          try {
            this.ws.close();
          } catch (_e) {}
        }
        handleClose();
      };

      this.ws.addEventListener('open', this.onOpen);
      // we need to cast to any to avoid type error, onMessage is asynchoronous due to readBlob
      this.ws.addEventListener('message', this.onMessage as any);
      this.ws.addEventListener('close', handleClose);
      this.ws.addEventListener('error', handleError);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.resetOrderbook();

      this.send(JSON.stringify({ event: 'sub', params: { channel: 'system_notice' } }));
      this.send(JSON.stringify({ event: 'sub', params: { channel: 'reconnected' } }));
      this.send(JSON.stringify({ event: 'sub', params: { channel: 'onconnected' } }));
      // this.send(JSON.stringify({ event: 'sub', params: { channel: 'market_btcusdt' } }));
      // this.send(JSON.stringify({ event: 'sub', params: { channel: 'mark_price_btcusdt' } }));
      //this.send(JSON.stringify({ event: 'sub', params: { channel: 'market_btcusdt_depthop_0.1' } }));

      for (const [topic, count] of Object.entries(this.topics)) {
        if (count > 0) {
          this.send(topic);
        }
      }

      if (this.pingLoopTimeoutId) {
        clearTimeout(this.pingLoopTimeoutId);
      }

      const doPingLoop = () => {
        if (!this.isDisposed) {
          this.ping();
          this.pingLoopTimeoutId = setTimeout(doPingLoop, 15_000);
        }
      };
      doPingLoop();
    }
  };

  onMessage = async ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.messageCount++; // Increment message counter

      let strData = data;
      if (data instanceof Blob) {
        try {
          strData = new TextDecoder().decode(inflate(new Uint8Array(await data.arrayBuffer())));
        } catch (e) {
          console.error('bitunix: error reading blob', e);
        }
        if (this.isDisposed) return;
      } else {
        //        console.log('data is not blob');
      }
      const json = jsonParse(strData);
      // if (json?.channel) this.recordMessageHandlerStats(json?.channel);
      if (this.messageHandlers[json?.channel]) {
        this.messageHandlers[json?.channel].forEach((handler) => handler(json));
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ ping: virtualClock.getCurrentTimeMs() }));
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const handler = `market_${opts.symbol.toLowerCase()}_klineop_${INTERVAL[opts.interval]}`;
    const arg = {
      event: 'sub',
      params: { channel: handler },
    };
    const topic = JSON.stringify(arg);

    const parser = (json: any) => {
      const ticker = this.store.tickers.find((ticker) => ticker.symbol === opts.symbol);
      if (ticker) {
        this.store.updateTicker(ticker, {
          last: parseFloat(json.data.close),
        });
      }
      callback({
        timestamp: json.data.time,
        open: parseFloat(json.data.open),
        high: parseFloat(json.data.high),
        low: parseFloat(json.data.low),
        close: parseFloat(json.data.close),
        volume: parseFloat(json.data.volume),
      });
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;

        this.send(JSON.stringify(arg));
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
        this.cleanupMessageHandlerStats(handler);
        // this.send(
        //   JSON.stringify({
        //     op: 'unsubscribe',
        //     args: [arg],
        //   }),
        // );
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    //this.send(JSON.stringify({ event: 'sub', params: { channel: 'market_btcusdt_depthop_0.1' } }));
    const market = this.parent.store.markets.find((market) => market.symbol === symbol);
    const handler = `market_${symbol.toLowerCase()}_depthop_${market?.precision.price}`;
    const arg = {
      event: 'sub',
      params: { channel: handler },
    };
    let timeoutId: NodeJS.Timeout | null = null;

    const topic = JSON.stringify(arg);
    const orderBook: OrderBook = { bids: [], asks: [] };

    const parser = (rawData: any) => {
      const data = rawData?.data;

      orderBook.bids = data.bids.map(([price, volume]: [string, string]) => ({
        price: parseFloat(price),
        amount: parseFloat(volume),
        total: 0,
      }));

      orderBook.asks = data.asks.map(([price, volume]: [string, string]) => ({
        price: parseFloat(price),
        amount: parseFloat(volume),
        total: 0,
      }));

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

        this.send(JSON.stringify(arg));
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

      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        //const payload = { method: 'UNSUBSCRIBE', params: [topic], id: 3 };
        this.cleanupMessageHandlerStats(handler);
        //this.send(JSON.stringify(payload));
      }
    };
  };

}
