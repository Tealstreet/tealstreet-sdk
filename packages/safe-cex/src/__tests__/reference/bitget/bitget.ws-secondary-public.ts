import type { Ticker, Writable } from '../../../types';
import { resolveTransportRouting } from '../../../transport/registry';
import type { BitgetExchange } from './bitget.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL } from '../../../exchanges/bitget/bitget.types';


type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: { [instId: string]: number };
};

export class BitgetSecondaryPublicWebsocket extends BaseWebSocket<BitgetExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};
  index: number;
  private messageCount = 0;
  private useFallbackUrl = false;
  private lastMessageCount = 0;
  private connectionAttempts = 0;
  private readonly MAX_ATTEMPTS = 2;

  private toFiniteNumber = (value: unknown): number | undefined => {
    if (typeof value === 'number') {
      return Number.isFinite(value) ? value : undefined;
    }
    if (typeof value === 'string') {
      if (value.trim() === '') return undefined;
      const parsed = Number.parseFloat(value);
      return Number.isFinite(parsed) ? parsed : undefined;
    }
    return undefined;
  };

  private buildTickerUpdate = (data: Record<string, any>): Partial<Writable<Ticker>> => {
    const update: Partial<Writable<Ticker>> = {};

    const bid = this.toFiniteNumber(data.bidPr ?? data.bidPrice ?? data.bid);
    if (bid !== undefined) update.bid = bid;

    const ask = this.toFiniteNumber(data.askPr ?? data.askPrice ?? data.ask);
    if (ask !== undefined) update.ask = ask;

    const last = this.toFiniteNumber(data.lastPr ?? data.lastPrice ?? data.last);
    if (last !== undefined) update.last = last;

    const mark = this.toFiniteNumber(data.markPrice ?? data.markPr ?? data.mark);
    if (mark !== undefined) update.mark = mark;

    const index = this.toFiniteNumber(data.indexPrice ?? data.indexPr ?? data.index);
    if (index !== undefined) update.index = index;

    const change = this.toFiniteNumber(data.change24h ?? data.changeUtc24h ?? data.change);
    if (change !== undefined) update.percentage = change * 100;

    const fundingRate = this.toFiniteNumber(data.fundingRate);
    if (fundingRate !== undefined) update.fundingRate = fundingRate;

    const fundingTime = this.toFiniteNumber(data.nextFundingTime ?? data.fundingTime);
    if (fundingTime !== undefined) update.fundingTime = fundingTime;

    const volume = this.toFiniteNumber(data.baseVolume ?? data.volume);
    if (volume !== undefined) update.volume = volume;

    const quoteVolume = this.toFiniteNumber(data.quoteVolume);
    if (quoteVolume !== undefined) update.quoteVolume = quoteVolume;

    return update;
  };

  constructor(parent: BitgetExchange, index: number) {
    super(parent);
    this.index = index;
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // Check if we need to switch to fallback URL
      if (this.lastMessageCount === 0 && this.messageCount === 0) {
        this.connectionAttempts++;
        if (this.connectionAttempts >= this.MAX_ATTEMPTS) {
          this.useFallbackUrl = true;
        }
      }
      this.lastMessageCount = this.messageCount;
      this.messageCount = 0; // Reset message counter on new connection

      this.topics.ticker = this.parent.store.markets
        .filter((_, i) => i % 3 === this.index)
        .reduce(
          (acc, m) => ({
            ...acc,
            [m.symbol]: 1,
          }),
          {},
        );

      // Only a transport that declares a fallback host has one to switch to.
      const fallbackUrl = resolveTransportRouting(this.parent.name, this.parent.options).wsPublic.fallback;
      const useFallbackUrl = this.useFallbackUrl && !!fallbackUrl;
      const url = useFallbackUrl ? fallbackUrl : BASE_WS_URL;
      if (useFallbackUrl) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bitget proxy (secondary public)');
        } catch (_e) {}
      }
      this.ws = this.createWebsocket(url);
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

  private onMarketsChanged = ({ added }: { added: Array<{ symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    // Only subscribe to markets matching this instance's index (i % 3 === this.index)
    const existingMarketCount = Object.keys(this.topics.ticker || {}).length;
    const newArgs = added
      .filter((_, i) => (existingMarketCount + i) % 3 === this.index)
      .map((m) => ({
        instType: this.parent.apiProductType.toUpperCase(),
        channel: 'ticker',
        instId: m.symbol,
      }));

    if (newArgs.length === 0) return;

    for (const arg of newArgs) {
      if (!this.topics.ticker) this.topics.ticker = {};
      this.topics.ticker[arg.instId] = 1;
    }

    this.send(JSON.stringify({ op: 'subscribe', args: newArgs }));
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send('ping');
    }
  };

  subscribe = () => {
    const args = Object.entries(this.topics).flatMap(([channel, symbols]) =>
      Object.keys(symbols).map((instId) => ({
        instType: this.parent.apiProductType.toUpperCase(),
        channel,
        instId,
      })),
    );

    this.send(JSON.stringify({ op: 'subscribe', args }));
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.messageCount++; // Increment message counter

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (data.includes('"channel":"ticker"')) {
        const json = jsonParse<Record<string, any>>(data);
        if (json?.arg?.channel === 'ticker' && json?.event !== 'subscribe') {
          this.recordMessageHandlerStats('ticker');
          this.handleTickerTopic(json);
        }
      }
    }
  };

  handleTickerTopic = (json: Record<string, any>) => {
    const dataList = Array.isArray(json?.data) ? json.data : [];
    for (const data of dataList) {
      const symbol = `${data?.instId || data?.symbol || ''}`;
      if (!symbol) continue;

      const ticker = this.parent.store.tickersBySymbol[symbol];
      if (!ticker) continue;

      const update = this.buildTickerUpdate(data);
      if (Object.keys(update).length === 0) continue;

      this.parent.store.updateTicker(ticker, update);
    }
  };
}
