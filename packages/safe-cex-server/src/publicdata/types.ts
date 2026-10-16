import type { ExchangeName, Market, Ticker } from '../../../safe-cex/src/types';

export type PublicdataExchangeName =
  | 'apexomni'
  | 'asterdex'
  | 'binance'
  | 'binancespot'
  | 'binancepm'
  | 'bingx'
  | 'bitget'
  | 'bitrue'
  | 'bitunix'
  | 'blofin'
  | 'bybit'
  | 'bybitdemo'
  | 'bybitinverse'
  | 'bybitspot'
  | 'coinbase'
  | 'grvt'
  | 'hyperliquid'
  | 'hyperliquidspot'
  | 'kraken'
  | 'kucoin'
  | 'lighter'
  | 'okx'
  | 'woo';

export type PublicdataExchangeStatus = 'connecting' | 'connected' | 'error' | 'disconnected';

export type PublicdataExchangeHealth = {
  readonly exchange: string;
  readonly status: PublicdataExchangeStatus;
  readonly tickerCount: number;
  readonly lastUpdate: number;
  readonly error?: string | null;
};

export type PublicDataCapability = {
  readonly exchange: ExchangeName;
  readonly publicdataExchange: PublicdataExchangeName;
  readonly tickerSnapshot: boolean;
  readonly tickerDeltas: boolean;
  readonly tps: boolean;
  readonly usdAggregates: boolean;
  readonly safeCexMarketShape: false;
  readonly orderbook: false;
  readonly liveOhlcv: false;
  readonly notes?: string;
};

export type PublicdataAllowedExchange = {
  readonly exchange: ExchangeName;
  readonly publicdataExchange: PublicdataExchangeName;
  readonly capability: PublicDataCapability;
  readonly health: PublicdataExchangeHealth;
};

export type PublicdataTickerSubscribeMessage = {
  readonly op: 'subscribe';
  readonly channel: 'tickers';
  readonly rate: number;
  readonly exchanges: PublicdataExchangeName[];
  readonly includeIndex: boolean;
  readonly includeTps30s: boolean;
  readonly includeTps60s: boolean;
  readonly includeOiAgg: boolean;
};

export type PublicdataSubscribeOptions = {
  readonly rate?: number;
  readonly includeIndex?: boolean;
  readonly includeTps30s?: boolean;
  readonly includeTps60s?: boolean;
  readonly includeOiAgg?: boolean;
};

export type PublicdataWireTicker = {
  readonly b?: number;
  readonly a?: number;
  readonly l?: number;
  readonly m?: number;
  readonly i?: number;
  readonly p?: number;
  readonly o?: number;
  readonly f?: number;
  readonly t?: number;
  readonly v?: number;
  readonly q?: number;
  readonly n?: number;
  readonly n3?: number;
  readonly n6?: number;
  readonly ou?: number;
  readonly oa?: number;
  readonly va?: number;
};

export type PublicdataWireTickerMap = Record<string, PublicdataWireTicker>;

export type PublicdataTickerSnapshotFrame = {
  readonly type: 'snapshot';
  readonly channel: 'tickers';
  readonly data: Record<string, PublicdataWireTickerMap>;
  readonly timestamp?: number;
};

export type PublicdataTickerDeltaFrame = {
  readonly type: 'delta';
  readonly channel: 'tickers';
  readonly data: Record<string, PublicdataWireTickerMap>;
  readonly removed?: Record<string, string[]>;
  readonly timestamp?: number;
};

export type PublicdataTickerFrame = PublicdataTickerSnapshotFrame | PublicdataTickerDeltaFrame;

export type PublicdataTickerMetrics = {
  // safe-cex Ticker has no clean fields for publicdata TPS/USD aggregates.
  // Keep them sidecar-only so ticker conversion does not smuggle protocol
  // extensions through Ticker.info.
  readonly tradesPerSecond?: number;
  readonly tradesPerSecond30s?: number;
  readonly tradesPerSecond60s?: number;
  readonly openInterestUsd?: number;
  readonly openInterestUsdAggregate?: number;
  readonly volumeUsdAggregate?: number;
};

export type PublicdataTickerEntry = {
  readonly ticker: Ticker;
  readonly metrics: PublicdataTickerMetrics;
};

export type PublicdataTickerExchangeState = {
  readonly exchange: ExchangeName;
  readonly tickersByNativeSymbol: ReadonlyMap<string, PublicdataTickerEntry>;
  readonly skippedMissingMarkets: number;
  readonly removedSymbols: readonly string[];
  readonly lastSnapshotTime?: number;
  readonly lastDeltaTime?: number;
};

export type PublicdataTickerState = ReadonlyMap<ExchangeName, PublicdataTickerExchangeState>;

export type PublicdataMarketIndex = ReadonlyMap<string, Market>;

export interface PublicMarketMetadataSource {
  getMarkets(exchange: ExchangeName): Promise<readonly Market[]>;
  refreshMarkets?(exchange: ExchangeName): Promise<readonly Market[]>;
}

export interface PublicdataWebSocketLike {
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onOpen(listener: () => void): () => void;
  onMessage(listener: (data: string) => void): () => void;
  onClose(listener: (event?: unknown) => void): () => void;
  onError(listener: (error: unknown) => void): () => void;
}

export type PublicdataWebSocketFactory = (
  url: string,
  opts?: { headers?: Record<string, string> },
) => PublicdataWebSocketLike;

export type PublicdataTickerBridgeOptions = {
  readonly wsUrl: string;
  readonly origin?: string;
  readonly exchanges: readonly ExchangeName[];
  readonly exchangeHealth: readonly PublicdataExchangeHealth[];
  readonly maxStalenessMs: number;
  readonly now?: () => number;
  readonly rate?: number;
  readonly includeIndex?: boolean;
  readonly includeTps30s?: boolean;
  readonly includeTps60s?: boolean;
  readonly includeOiAgg?: boolean;
  readonly marketSource: PublicMarketMetadataSource;
  readonly webSocketFactory: PublicdataWebSocketFactory;
  readonly capabilities?: readonly PublicDataCapability[];
};

export type PublicdataTickerBridgeStartResult =
  | {
      readonly started: true;
      readonly exchanges: readonly PublicdataAllowedExchange[];
    }
  | {
      readonly started: false;
      readonly reason: 'empty-filter';
      readonly exchanges: readonly PublicdataAllowedExchange[];
    };
