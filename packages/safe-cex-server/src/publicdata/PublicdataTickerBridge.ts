import type { ExchangeName } from '../../../safe-cex/src/types';

import {
  DEFAULT_PUBLICDATA_TICKER_CAPABILITIES,
  buildTickerSubscribeMessage,
  filterPublicdataTickerExchanges,
} from './capabilities';
import { applyTickerFrame, buildPublicdataMarketIndex } from './tickerState';
import type {
  PublicdataAllowedExchange,
  PublicdataMarketIndex,
  PublicdataTickerBridgeOptions,
  PublicdataTickerBridgeStartResult,
  PublicdataTickerFrame,
  PublicdataTickerState,
  PublicdataWebSocketLike,
} from './types';

const isObject = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null;

export const parsePublicdataTickerFrame = (raw: string): PublicdataTickerFrame | null => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }

  if (!isObject(parsed)) return null;
  if (parsed.channel !== 'tickers') return null;
  if (parsed.type !== 'snapshot' && parsed.type !== 'delta') return null;
  if (!isObject(parsed.data)) return null;

  return parsed as PublicdataTickerFrame;
};

export class PublicdataTickerBridge {
  private ws: PublicdataWebSocketLike | null = null;
  private unbind: Array<() => void> = [];
  private allowedExchanges: readonly PublicdataAllowedExchange[] = [];
  private exchangeByPublicdataExchange = new Map<string, ExchangeName>();
  private marketIndexes = new Map<ExchangeName, PublicdataMarketIndex>();
  private state: PublicdataTickerState = new Map();

  constructor(private readonly options: PublicdataTickerBridgeOptions) {}

  getState(): PublicdataTickerState {
    return this.state;
  }

  getAllowedExchanges(): readonly PublicdataAllowedExchange[] {
    return this.allowedExchanges;
  }

  async start(): Promise<PublicdataTickerBridgeStartResult> {
    const now = this.options.now?.() ?? Date.now();
    const allowedExchanges = filterPublicdataTickerExchanges({
      requestedExchanges: this.options.exchanges,
      exchangeHealth: this.options.exchangeHealth,
      now,
      maxStalenessMs: this.options.maxStalenessMs,
      capabilities: this.options.capabilities ?? DEFAULT_PUBLICDATA_TICKER_CAPABILITIES,
    });

    const subscribeMessage = buildTickerSubscribeMessage(allowedExchanges, {
      rate: this.options.rate,
      includeIndex: this.options.includeIndex,
      includeTps30s: this.options.includeTps30s,
      includeTps60s: this.options.includeTps60s,
      includeOiAgg: this.options.includeOiAgg,
    });

    if (!subscribeMessage) {
      this.allowedExchanges = [];
      this.exchangeByPublicdataExchange = new Map();
      this.marketIndexes = new Map();
      return { started: false, reason: 'empty-filter', exchanges: [] };
    }

    this.allowedExchanges = allowedExchanges;
    this.exchangeByPublicdataExchange = new Map(
      allowedExchanges.map((item) => [item.publicdataExchange, item.exchange]),
    );
    this.marketIndexes = await this.loadMarketIndexes(allowedExchanges);

    const headers = this.options.origin ? { Origin: this.options.origin } : undefined;
    const ws = this.options.webSocketFactory(this.options.wsUrl, headers ? { headers } : undefined);
    this.ws = ws;
    this.unbind = [
      ws.onOpen(() => {
        ws.send(JSON.stringify(subscribeMessage));
      }),
      ws.onMessage((data) => {
        const frame = parsePublicdataTickerFrame(data);
        if (!frame) return;
        this.state = applyTickerFrame({
          state: this.state,
          frame,
          exchangeByPublicdataExchange: this.exchangeByPublicdataExchange,
          marketIndexes: this.marketIndexes,
        });
      }),
      ws.onClose(() => {
        this.ws = null;
      }),
      ws.onError(() => undefined),
    ];

    return { started: true, exchanges: allowedExchanges };
  }

  dispose(): void {
    for (const off of this.unbind) {
      off();
    }
    this.unbind = [];

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  private async loadMarketIndexes(
    allowedExchanges: readonly PublicdataAllowedExchange[],
  ): Promise<Map<ExchangeName, PublicdataMarketIndex>> {
    const indexes = new Map<ExchangeName, PublicdataMarketIndex>();
    await Promise.all(
      allowedExchanges.map(async ({ exchange }) => {
        const markets = await this.options.marketSource.getMarkets(exchange);
        indexes.set(exchange, buildPublicdataMarketIndex(markets));
      }),
    );
    return indexes;
  }
}
