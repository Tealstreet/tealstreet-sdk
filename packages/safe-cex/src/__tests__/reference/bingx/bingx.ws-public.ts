import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { BingxExchange } from './bingx.exchange';

import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { v } from '../../../utils/get-key';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { readBlob } from '../../../utils/readBlob';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL } from './bingx.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

const parsePositiveFiniteNumber = (value: unknown): number | undefined => {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
};

export class BingxPublicWebsocket extends BaseWebSocket<BingxExchange> {
  messageHandlers: MessageHandlers = {};

  topics: { [key: string]: number } = {};

  private getDiagnosticSymbols = () => {
    const symbols = new Set<string>();
    for (const topic of Object.keys(this.messageHandlers)) {
      const match = topic.match(/^market\.(?:kline|depth)\.([^.]+)\./);
      if (match?.[1]) {
        symbols.add(match[1]);
      }
    }
    return symbols;
  };

  private formatMarketContractsDiagnostic = (json: any, symbols: Set<string>) => {
    const contracts = Array.isArray(json?.data?.contracts)
      ? json.data.contracts.filter((contract: Record<string, any>) => symbols.has(contract?.symbol))
      : [];
    return `[bingx public] ${JSON.stringify({
      ...json,
      data: {
        ...json?.data,
        contracts,
        filteredForSymbols: [...symbols],
      },
    })}`;
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
      const payload = {
        id: nanoid(),
        reqType: 'sub',
        dataType: 'market.contracts',
        //params: ['!ticker@arr', '!bookTicker', '!markPrice@arr@1s'],
      };

      this.sendWithLog(JSON.stringify(payload));
      this.subscribe();
      this.ping();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      try {
        this.pingAt = performance.now();
        this.sendWithLog('Ping');
        this.sendWithLog(`{"ping":"${nanoid()}","time":"${new Date(virtualClock.getCurrentTimeMs()).toISOString()}"}`);
      } catch {
        // Ignored
      }
    }
  };

  subscribe = () => {
    for (const topic of Object.keys(this.topics)) {
      this.sendWithLog(JSON.stringify({ reqType: 'sub', dataType: topic, id: nanoid() }));
    }
  };

  onMessage = async ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      let strData = data;
      if (data instanceof Blob) {
        strData = await readBlob(data);
        if (this.isDisposed) return;
      }
      const json = jsonParse(strData);
      const topic = json?.dataType;
      const diagnosticSymbols = topic === 'market.contracts' ? this.getDiagnosticSymbols() : undefined;
      this.emitWsDiagnosticReceive(strData, {
        allowChannel: () => {
          return topic === 'market.contracts' && diagnosticSymbols !== undefined && diagnosticSymbols.size > 0;
        },
        format: () => {
          if (topic === 'market.contracts' && diagnosticSymbols) {
            return this.formatMarketContractsDiagnostic(json, diagnosticSymbols);
          }
          return null;
        },
      });

      if (strData === 'Ping' || json?.ping) {
        this.sendWithLog('Pong');
        return;
      }

      if (strData === 'Pong' || json?.pong) {
        this.handlePongEvent();
        return;
      }

      if (json?.dataType === 'market.contracts') {
        this.recordMessageHandlerStats('ticker');
        const contracts = json?.data?.contracts || [];

        // O(1) hashmap lookup. market.contracts arrives at high frequency
        // with ~600 contracts; the prior tickers.find made it N*M per frame.
        const tickersById = this.parent.store.tickersById;
        for (const contract of contracts) {
          const ticker = tickersById[contract?.symbol];

          if (ticker) {
            const last = parsePositiveFiniteNumber(v(contract, 'tradePrice'));
            const market = this.parent.store.marketsBySymbol[ticker.symbol];
            const touchPrices = last && market ? this.parent.deriveTickerBidAsk(market, last) : undefined;

            this.store.updateTicker(ticker, {
              last: last ?? ticker.last,
              bid: touchPrices?.bid ?? ticker.bid,
              ask: touchPrices?.ask ?? ticker.ask,
              percentage: parseFloat(v(contract, 'changePercentage')),
              volume: parseFloat(v(contract, 'volumeF')),
              quoteVolume: parseFloat(v(contract, 'valueF')),
              mark: parseFloat(v(contract, 'fairPrice')),
              index: parseFloat(v(contract, 'indexPrice')),
              fundingRate: parseFloat(v(contract, 'fundingRate')) / 100,
              fundingTime: 0,
            });
          }
        }
      } else {
        const topic = json?.dataType;
        const callbacks = this.messageHandlers[topic];
        if (callbacks?.length) {
          this.recordMessageHandlerStats(topic);
          for (const callback of callbacks) {
            callback(Array.isArray(json) ? json : [json]);
          }
        }
      }
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const timeframes = {
      '1m': '1min',
      '3m': '3min',
      '5m': '5min',
      '15m': '15min',
      '30m': '30min',
      '1h': '1hour',
      '2h': '2hour',
      '3h': '3hour',
      '4h': '4hour',
      '6h': '6hour',
      '12h': '12hour',
      '1d': '1day',
      '1w': '1week',
      '1M': '1month',
      '3M': '3month',
      '1Y': '1year',
    };
    const topic = `market.kline.${this.parent.toBingxSymbol(opts.symbol)}.${timeframes[opts.interval]}.trade.utc+8`;
    const handler = topic;

    const parser = ([json]: Data) => {
      const row = json?.data?.klineInfosVo?.[0];
      callback({
        timestamp: row.time / 1000,
        open: parseFloat(row.open),
        high: parseFloat(row.high),
        low: parseFloat(row.low),
        close: parseFloat(row.close),
        volume: parseFloat(row.volume),
      });
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;

        const payload = { reqType: 'sub', dataType: topic, id: nanoid() };
        this.sendWithLog(JSON.stringify(payload));
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
        const payload = { reqType: 'unsub', dataType: topic, id: nanoid() };
        this.cleanupMessageHandlerStats(handler);
        this.sendWithLog(JSON.stringify(payload));
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    const topic = `market.depth.${this.parent.toBingxSymbol(symbol)}.step0.level100`;
    const handler = topic;
    const orderBook: OrderBook = { bids: [], asks: [] };

    const parser = ([rawData]: Data) => {
      const data = rawData?.data;

      orderBook.bids = data.bids.map(({ price, volume }: { price: string; volume: string }) => ({
        price: parseFloat(price),
        amount: parseFloat(volume),
        total: 0,
      }));

      orderBook.asks = data.asks.map(({ price, volume }: { price: string; volume: string }) => ({
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

        const payload = { reqType: 'sub', dataType: topic, id: nanoid() };
        this.sendWithLog(JSON.stringify(payload));
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
        const payload = { reqType: 'unsub', dataType: topic, id: nanoid() };
        this.cleanupMessageHandlerStats(handler);
        this.sendWithLog(JSON.stringify(payload));
      }
    };
  };
}
