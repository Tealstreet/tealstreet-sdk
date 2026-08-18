import { z, type ZodRawShape } from 'zod';

import {
  getHyperliquidAccountValueCurveData,
  getHyperliquidLeaderboardData,
  getHyperliquidWalletFillsData,
  getHyperliquidWalletPositionsData,
  getExchangeProxyCandlesData,
  getPolymarketEventsData,
  getPolymarketMidpointsData,
  getPolymarketOrderBookData,
  getPolymarketPriceHistoryData,
  getPublicTickerData,
  getPublicTickersData,
  getSafeCexMarketData,
  getSafeCexMarketsData,
  getTopFundingData,
  getTopMoversData,
  getTopVolumeData,
  searchPolymarketEventsData,
  listMarketDataSourcesData,
  type HyperliquidAnalyticsQuery,
  type ExchangeProxyJsonFetch,
  type McpDataHandlerResult,
  type McpDataScope,
  type PolymarketJsonFetch,
  type PublicdataJsonFetch,
  type SafeCexMarketMetadataReadRequest,
  type SafeCexMarketMetadataSnapshot,
} from '@tealstreet/mcp-data';
import { queryHyperliquidAnalytics } from './hyperliquidAnalyticsDb';
import { exchangeProxyJson } from './exchangeProxy';
import { polymarketJson } from './polymarket';
import { publicdataJson } from './publicdata';

export type ToolResult = { content: { type: 'text'; text: string }[]; isError?: boolean };

export interface CliDataToolDefinition<S extends ZodRawShape = ZodRawShape> {
  name: string;
  description: string;
  scope: McpDataScope;
  shape: S;
  handler: (args: z.infer<z.ZodObject<S>>) => Promise<ToolResult>;
}

interface CliDataToolDeps {
  fetchJson: PublicdataJsonFetch;
  fetchExchangeProxyJson: ExchangeProxyJsonFetch;
  fetchPolymarketJson: PolymarketJsonFetch;
  queryHyperliquidAnalytics: HyperliquidAnalyticsQuery;
}

const text = (value: string): ToolResult => ({ content: [{ type: 'text', text: value }] });
const failure = (value: string): ToolResult => ({ content: [{ type: 'text', text: value }], isError: true });
const jsonText = (value: unknown): ToolResult => text(JSON.stringify(value, null, 2));

const renderDataResult = (result: McpDataHandlerResult): ToolResult =>
  result.ok ? jsonText(result.value) : failure(result.message);

const runDataTool = async (source: string, run: () => Promise<ToolResult>): Promise<ToolResult> => {
  try {
    return await run();
  } catch (error) {
    console.error(`[mcp] ${source} data tool failed`, { error });
    return failure(`${source} data is unavailable right now. This is a data source error.`);
  }
};

const countArg = z.number().optional().describe('Maximum rows to return. Defaults to 20; max 100.');

const asNumber = (value: unknown, fallback: number): number =>
  typeof value === 'number' && Number.isFinite(value) ? value : fallback;

const asString = (value: unknown, fallback: string): string => (typeof value === 'string' ? value : fallback);

const safeCexSnapshotFromBody = (
  exchange: string,
  body: Record<string, unknown>,
  markets: unknown[],
): SafeCexMarketMetadataSnapshot => ({
  exchange: asString(body.exchange, exchange),
  bucket: 'exchange-services-live',
  refreshedAt: asNumber(body.refreshedAt, Date.now()),
  marketCount: asNumber(body.marketCount, markets.length),
  contentHash: `exchange-services:${exchange}:${asNumber(body.refreshedAt, 0)}:${markets.length}`,
  markets,
  live: true,
  cached: false,
});

const readSafeCexMarketSnapshot = async (
  exchange: string,
  fetchExchangeProxyJson: ExchangeProxyJsonFetch,
  request: SafeCexMarketMetadataReadRequest = {},
): Promise<SafeCexMarketMetadataSnapshot | null> => {
  const exchangeParam = encodeURIComponent(exchange);
  const body = request.symbol
    ? await fetchExchangeProxyJson(
        `/api/market-data/market?exchange=${exchangeParam}&symbol=${encodeURIComponent(request.symbol)}`,
      )
    : await fetchExchangeProxyJson(
        `/api/market-data/markets?exchange=${exchangeParam}&limit=10000${
          request.query ? `&query=${encodeURIComponent(request.query)}` : ''
        }`,
      );
  if (request.symbol && body.code === 'market-not-found') return safeCexSnapshotFromBody(exchange, body, []);
  if (typeof body.error === 'string') throw new Error(body.error);

  if (request.symbol) {
    const market = body.market === null ? null : body.market;
    if (market !== null && (typeof market !== 'object' || Array.isArray(market))) {
      throw new Error('exchange-services market metadata response is malformed');
    }
    return safeCexSnapshotFromBody(exchange, body, market ? [market] : []);
  }

  if (!Array.isArray(body.markets)) throw new Error('exchange-services market metadata response is malformed');
  return safeCexSnapshotFromBody(exchange, body, body.markets);
};

export const createCliDataTools = ({
  fetchJson,
  fetchExchangeProxyJson,
  fetchPolymarketJson,
  queryHyperliquidAnalytics: query,
}: CliDataToolDeps): readonly CliDataToolDefinition[] => [
  {
    name: 'list_market_data_sources',
    description:
      'List public market-data sources available through Tealstreet MCP, including live publicdata exchange health.',
    scope: 'market:read',
    shape: {},
    handler: async () =>
      runDataTool('Public market', async () =>
        renderDataResult(await listMarketDataSourcesData(fetchJson, 'cli-stdio')),
      ),
  },
  {
    name: 'get_public_tickers',
    description: 'Get a bounded public ticker snapshot for one publicdata exchange.',
    scope: 'market:read',
    shape: {
      exchange: z.string().describe('publicdata exchange name, for example binance, bybit, hyperliquid.'),
      limit: z.number().optional().describe('Maximum tickers to return. Defaults to 100; max 500.'),
    },
    handler: async (args) =>
      runDataTool('Public market', async () => renderDataResult(await getPublicTickersData(args, fetchJson))),
  },
  {
    name: 'get_public_ticker',
    description: 'Look up one public symbol on one publicdata exchange.',
    scope: 'market:read',
    shape: {
      exchange: z.string().describe('publicdata exchange name, for example binance, bybit, hyperliquid.'),
      symbol: z.string().describe('Native or common symbol, for example BTCUSDT or BTC.'),
    },
    handler: async (args) =>
      runDataTool('Public market', async () => renderDataResult(await getPublicTickerData(args, fetchJson))),
  },
  {
    name: 'get_top_movers',
    description: 'Get public top movers by 24h percentage change across publicdata exchanges.',
    scope: 'market:read',
    shape: {
      count: countArg,
      direction: z.enum(['up', 'down', 'both']).optional().describe('Mover direction. Defaults to both.'),
    },
    handler: async (args) =>
      runDataTool('Public market', async () => renderDataResult(await getTopMoversData(args, fetchJson))),
  },
  {
    name: 'get_top_volume',
    description: 'Get public top tickers by 24h quote volume across publicdata exchanges.',
    scope: 'market:read',
    shape: { count: countArg },
    handler: async (args) =>
      runDataTool('Public market', async () => renderDataResult(await getTopVolumeData(args, fetchJson))),
  },
  {
    name: 'get_top_funding',
    description: 'Get public top tickers by absolute funding rate across publicdata exchanges.',
    scope: 'market:read',
    shape: { count: countArg },
    handler: async (args) =>
      runDataTool('Public market', async () => renderDataResult(await getTopFundingData(args, fetchJson))),
  },
  {
    name: 'get_exchange_proxy_candles',
    description: 'Read public historical candles through the exchange.tealstreet.io candle service.',
    scope: 'market:read',
    shape: {
      exchange: z.string().describe('safe-cex exchange id, for example binance, bybit, okx.'),
      symbol: z.string().describe('Native market symbol, for example BTCUSDT.'),
      interval: z
        .enum(['1m', '3m', '5m', '15m', '30m', '1h', '2h', '4h', '6h', '8h', '12h', '1d', '3d', '1w', '1M'])
        .describe('Candle interval.'),
      from: z.number().describe('Inclusive start time in epoch milliseconds.'),
      to: z.number().describe('Inclusive end time in epoch milliseconds.'),
      limit: z.number().optional().describe('Maximum candles to return. Defaults to 1000; max 5000.'),
    },
    handler: async (args) =>
      runDataTool('Exchange proxy', async () =>
        renderDataResult(await getExchangeProxyCandlesData(args, fetchExchangeProxyJson)),
      ),
  },
  {
    name: 'get_safe_cex_markets',
    description: 'Read bounded safe-cex public market metadata through the exchange.tealstreet.io candle service.',
    scope: 'market:read',
    shape: {
      exchange: z.string().describe('safe-cex exchange id, for example bybit, binance, okx.'),
      query: z.string().optional().describe('Optional substring filter across id, symbol, base, quote.'),
      limit: z.number().optional().describe('Maximum markets to return. Defaults to 100; max 1000.'),
    },
    handler: async (args) =>
      runDataTool('safe-cex market metadata', async () =>
        renderDataResult(
          await getSafeCexMarketsData(args, (exchange, request) =>
            readSafeCexMarketSnapshot(exchange, fetchExchangeProxyJson, request),
          ),
        ),
      ),
  },
  {
    name: 'get_safe_cex_market',
    description: 'Look up one safe-cex public market metadata row through the exchange.tealstreet.io candle service.',
    scope: 'market:read',
    shape: {
      exchange: z.string().describe('safe-cex exchange id, for example bybit, binance, okx.'),
      symbol: z.string().describe('Market symbol or venue id, for example BTCUSDT.'),
    },
    handler: async (args) =>
      runDataTool('safe-cex market metadata', async () =>
        renderDataResult(
          await getSafeCexMarketData(args, (exchange, request) =>
            readSafeCexMarketSnapshot(exchange, fetchExchangeProxyJson, request),
          ),
        ),
      ),
  },
  {
    name: 'get_polymarket_events',
    description: 'Read public Polymarket prediction-market events and compact market rows.',
    scope: 'market:read',
    shape: {
      limit: z.number().optional().describe('Maximum events to return. Defaults to 20; max 100.'),
      offset: z.number().optional().describe('Offset for event pagination. Defaults to 0.'),
      order: z.enum(['volume24hr', 'volume', 'liquidity', 'endDate', 'startDate']).optional(),
      ascending: z.boolean().optional(),
      active: z.boolean().optional(),
      closed: z.boolean().optional(),
      tagSlug: z.string().optional().describe('Optional Polymarket tag slug.'),
    },
    handler: async (args) =>
      runDataTool('Polymarket DEX', async () =>
        renderDataResult(await getPolymarketEventsData(args, fetchPolymarketJson)),
      ),
  },
  {
    name: 'search_polymarket_events',
    description: 'Search public Polymarket events and tags.',
    scope: 'market:read',
    shape: { query: z.string().describe('Search query.') },
    handler: async (args) =>
      runDataTool('Polymarket DEX', async () =>
        renderDataResult(await searchPolymarketEventsData(args, fetchPolymarketJson)),
      ),
  },
  {
    name: 'get_polymarket_midpoints',
    description: 'Read public Polymarket CLOB midpoint prices for token ids.',
    scope: 'market:read',
    shape: {
      tokenIds: z.union([z.array(z.string()), z.string()]).describe('Token ids as an array or comma-separated string.'),
    },
    handler: async (args) =>
      runDataTool('Polymarket DEX', async () =>
        renderDataResult(await getPolymarketMidpointsData(args, fetchPolymarketJson)),
      ),
  },
  {
    name: 'get_polymarket_order_book',
    description: 'Read a public Polymarket CLOB order book for one token id.',
    scope: 'market:read',
    shape: {
      tokenId: z.string().describe('Polymarket CLOB token id.'),
      depth: z.number().optional().describe('Maximum price levels per side. Defaults to 20; max 100.'),
    },
    handler: async (args) =>
      runDataTool('Polymarket DEX', async () =>
        renderDataResult(await getPolymarketOrderBookData(args, fetchPolymarketJson)),
      ),
  },
  {
    name: 'get_polymarket_price_history',
    description: 'Read public Polymarket CLOB price-history points for one token id.',
    scope: 'market:read',
    shape: {
      tokenId: z.string().describe('Polymarket CLOB token id.'),
      interval: z.enum(['1h', '6h', '1d', '1w', 'max']).optional().describe('Defaults to 1d.'),
    },
    handler: async (args) =>
      runDataTool('Polymarket DEX', async () =>
        renderDataResult(await getPolymarketPriceHistoryData(args, fetchPolymarketJson)),
      ),
  },
  {
    name: 'get_hyperliquid_leaderboard',
    description: 'Read the crawled Hyperliquid trader leaderboard from Tealstreet analytics.',
    scope: 'analytics:read',
    shape: {
      limit: z.number().optional().describe('Maximum traders to return. Defaults to 20; max 100.'),
      sortBy: z
        .enum(['day_pnl', 'week_pnl', 'month_pnl', 'alltime_pnl', 'accountvalue'])
        .optional()
        .describe('Leaderboard sort column. Defaults to day_pnl.'),
      sortOrder: z.enum(['asc', 'desc']).optional().describe('Sort order. Defaults to desc.'),
    },
    handler: async (args) =>
      runDataTool('Hyperliquid analytics', async () =>
        renderDataResult(await getHyperliquidLeaderboardData(args, query)),
      ),
  },
  {
    name: 'get_hyperliquid_wallet_positions',
    description: 'Read the latest crawled Hyperliquid clearinghouse positions for a public wallet.',
    scope: 'wallet:read',
    shape: { wallet: z.string().describe('0x Hyperliquid wallet address.') },
    handler: async (args) =>
      runDataTool('Hyperliquid analytics', async () =>
        renderDataResult(await getHyperliquidWalletPositionsData(args, query)),
      ),
  },
  {
    name: 'get_hyperliquid_wallet_fills',
    description: 'Read recent crawled Hyperliquid fills for a public wallet.',
    scope: 'wallet:read',
    shape: {
      wallet: z.string().describe('0x Hyperliquid wallet address.'),
      coin: z.string().optional().describe('Optional Hyperliquid coin filter, for example BTC.'),
      limit: z.number().optional().describe('Maximum fills to return. Defaults to 100; max 500.'),
    },
    handler: async (args) =>
      runDataTool('Hyperliquid analytics', async () =>
        renderDataResult(await getHyperliquidWalletFillsData(args, query)),
      ),
  },
  {
    name: 'get_hyperliquid_account_value_curve',
    description: 'Read crawled Hyperliquid account-value curve points for one public wallet.',
    scope: 'wallet:read',
    shape: {
      wallet: z.string().describe('0x Hyperliquid wallet address.'),
      timeframe: z.enum(['24H', '1W', '1M', 'all']).optional().describe('Defaults to all.'),
      limit: z.number().optional().describe('Maximum points to return. Defaults to 2000; max 5000.'),
    },
    handler: async (args) =>
      runDataTool('Hyperliquid analytics', async () =>
        renderDataResult(await getHyperliquidAccountValueCurveData(args, query)),
      ),
  },
];

export const cliDataTools = createCliDataTools({
  fetchJson: publicdataJson,
  fetchExchangeProxyJson: exchangeProxyJson,
  fetchPolymarketJson: polymarketJson,
  queryHyperliquidAnalytics,
});
