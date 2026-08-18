import { describe, expect, it, vi } from 'vitest';

import { createCliDataTools } from './dataTools';

const WALLET = '0x1234567890abcdef1234567890abcdef12345678';

const okRows = <T>(rows: T[]) => ({ ok: true as const, rows });

const jsonOf = (result: { content: { text: string }[]; isError?: boolean }) => {
  expect(result.isError).not.toBe(true);
  return JSON.parse(result.content[0].text);
};

describe('CLI MCP data tools', () => {
  it('exposes the hosted public and Hyperliquid read surfaces without module write tools', () => {
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });

    expect(tools.map((tool) => tool.name)).toEqual([
      'get_mcp_context_dsl_reference',
      'list_mcp_context_planes',
      'plan_mcp_context_query',
      'list_market_data_sources',
      'get_public_tickers',
      'get_public_ticker',
      'get_top_movers',
      'get_top_volume',
      'get_top_funding',
      'get_exchange_proxy_candles',
      'get_recent_news',
      'get_safe_cex_markets',
      'get_safe_cex_market',
      'get_polymarket_events',
      'search_polymarket_events',
      'get_polymarket_midpoints',
      'get_polymarket_order_book',
      'get_polymarket_price_history',
      'get_hyperliquid_leaderboard',
      'get_hyperliquid_wallet_positions',
      'get_hyperliquid_wallet_fills',
      'get_hyperliquid_account_value_curve',
    ]);
    expect(tools.map((tool) => tool.scope)).toEqual([
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'market:read',
      'analytics:read',
      'onchain:read',
      'onchain:read',
      'onchain:read',
    ]);
  });

  it('returns the shared context DSL reference in the CLI runtime', async () => {
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_mcp_context_dsl_reference')!;

    const body = jsonOf(await tool.handler({}));

    expect(body).toMatchObject({
      data: {
        version: 'mcp-context/v1',
        plannerTool: 'plan_mcp_context_query',
        callbacks: { targetModes: ['event'] },
      },
      meta: { source: 'mcp-context:reference', backend: 'third-party' },
    });
    expect(body.data.planes).toEqual(
      expect.arrayContaining([expect.objectContaining({ requiredScope: 'onchain:read' })]),
    );
  });

  it('plans refreshable context queries in the CLI runtime without executing them', async () => {
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'plan_mcp_context_query')!;

    const body = jsonOf(
      await tool.handler({
        query: `
          plane curve = hyperliquid.account_value_curve(wallet: "${WALLET}", timeframe: "1W")
          plane tweets = news.recent(source: "twitter", symbol: "BTC")
          render timeline(x: "curve.pointsByTimeframe", markers: "tweets.items", refreshMs: 60000)
        `,
      }),
    );

    expect(body.data).toMatchObject({
      requiredScopes: ['onchain:read', 'market:read'],
      dashboardWidget: {
        kind: 'context',
        queryMeta: {
          toolName: 'plan_mcp_context_query',
          refresh: { intervalMs: 60_000, mode: 'poll' },
        },
      },
    });
  });

  it('normalizes public ticker snapshots through the shared handler', async () => {
    const fetchJson = vi.fn(async () => ({
      tickers: {
        ETHUSDT: { l: 3000, q: 10 },
        BTCUSDT: { l: 100000, q: 20 },
      },
      timestamp: 1_800_000_000_000,
    }));
    const tools = createCliDataTools({
      fetchJson,
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_public_tickers')!;

    const body = jsonOf(await tool.handler({ exchange: 'BINANCE', limit: 1 }));

    expect(fetchJson).toHaveBeenCalledWith('/api/tickers?exchange=binance');
    expect(body.data.tickers).toEqual([expect.objectContaining({ symbol: 'BTCUSDT', last: 100000 })]);
  });

  it('reads exchange proxy candles through the shared handler', async () => {
    const fetchExchangeProxyJson = vi.fn(async () => ({
      exchange: 'binance',
      symbol: 'BTCUSDT',
      interval: '1m',
      candles: [{ timestamp: 1_800_000_000_000, open: 100, high: 101, low: 99, close: 100.5, volume: 12 }],
    }));
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson,
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_exchange_proxy_candles')!;

    const body = jsonOf(
      await tool.handler({
        exchange: 'BINANCE',
        symbol: 'btcusdt',
        interval: '1m',
        from: 1_800_000_000_000,
        to: 1_800_000_060_000,
        limit: 1,
      }),
    );

    expect(fetchExchangeProxyJson).toHaveBeenCalledWith(
      '/api/market-data/candles?exchange=binance&symbol=BTCUSDT&interval=1m&from=1800000000000&to=1800000060000&limit=1',
    );
    expect(body.data.candles).toEqual([
      { timestamp: 1_800_000_000_000, open: 100, high: 101, low: 99, close: 100.5, volume: 12 },
    ]);
    expect(body.meta).toMatchObject({ source: 'exchange-proxy:candles', backend: 'exchange-proxy' });
  });

  it('reads safe-cex market metadata through exchange-services', async () => {
    const fetchExchangeProxyJson = vi.fn(async () => ({
      exchange: 'bybit',
      refreshedAt: 1_800_000_000_000,
      marketCount: 1,
      markets: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
          info: { raw: 'not returned' },
        },
      ],
    }));
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson,
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_safe_cex_markets')!;

    const body = jsonOf(await tool.handler({ exchange: 'BYBIT', query: 'btc', limit: 1 }));

    expect(fetchExchangeProxyJson).toHaveBeenCalledWith(
      '/api/market-data/markets?exchange=bybit&limit=10000&query=BTC',
    );
    expect(body.data.markets).toEqual([expect.objectContaining({ symbol: 'BTCUSDT', base: 'BTC' })]);
    expect(body.meta).toMatchObject({ source: 'safe-cex:markets', backend: 'exchange-proxy' });
    expect(body.meta.freshness).toMatchObject({ live: true, cached: false });
    expect(JSON.stringify(body)).not.toContain('not returned');
  });

  it('reads a single safe-cex market through the exact exchange-services route', async () => {
    const fetchExchangeProxyJson = vi.fn(async () => ({
      exchange: 'okx',
      refreshedAt: 1_800_000_000_000,
      marketCount: 1,
      market: {
        id: 'BTC-USDT-SWAP',
        symbol: 'BTC-USDT-SWAP',
        base: 'BTC',
        quote: 'USDT',
        active: true,
      },
    }));
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson,
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_safe_cex_market')!;

    const body = jsonOf(await tool.handler({ exchange: 'okx', symbol: 'btc-usdt-swap' }));

    expect(fetchExchangeProxyJson).toHaveBeenCalledWith('/api/market-data/market?exchange=okx&symbol=BTC-USDT-SWAP');
    expect(body.data.market).toMatchObject({ id: 'BTC-USDT-SWAP', symbol: 'BTC-USDT-SWAP' });
  });

  it('returns market null instead of a tool error for unknown exact safe-cex symbols', async () => {
    const fetchExchangeProxyJson = vi.fn(async () => ({
      error: 'Unknown symbol BAD on okx',
      code: 'market-not-found',
    }));
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson,
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_safe_cex_market')!;

    const body = jsonOf(await tool.handler({ exchange: 'okx', symbol: 'BAD' }));

    expect(body.data).toMatchObject({ exchange: 'okx', symbol: 'BAD', market: null });
  });

  it('returns Hyperliquid analytics rows through the shared handler', async () => {
    const queryCalls: Array<[string, unknown[] | undefined]> = [];
    const queryHyperliquidAnalytics = async <T extends Record<string, unknown>>(sql: string, params?: unknown[]) => {
      queryCalls.push([sql, params]);
      return okRows<T>([
        {
          dt: '2026-08-17T01:00:00.000Z',
          ethaddress: WALLET,
          accountvalue: '1000',
          displayname: 'desk',
          day_pnl: '10',
          day_roi: '0.01',
          week_pnl: '20',
          week_roi: '0.02',
          month_pnl: '30',
          month_roi: '0.03',
          alltime_pnl: '40',
          alltime_roi: '0.04',
        } as unknown as T,
      ]);
    };
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics,
    });
    const tool = tools.find((entry) => entry.name === 'get_hyperliquid_leaderboard')!;

    const body = jsonOf(await tool.handler({ sortBy: 'week_pnl', limit: 5 }));

    expect(queryCalls).toEqual([[expect.stringContaining('leaderboard_current'), [5]]]);
    expect(body.data.traders[0]).toMatchObject({ wallet: WALLET, displayName: 'desk', weekPnl: 20 });
  });

  it('surfaces unavailable data as an MCP tool error', async () => {
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(async () => ({ ok: false as const, reason: 'not-configured' as const })),
    });
    const tool = tools.find((entry) => entry.name === 'get_hyperliquid_wallet_positions')!;

    const result = await tool.handler({ wallet: WALLET });

    expect(result.isError).toBe(true);
    expect(result.content[0].text).toContain('Hyperliquid analytics database is not configured');
  });

  it('returns Polymarket public DEX data through the shared handler', async () => {
    const fetchPolymarketJson = vi.fn(async () => ({
      body: [
        {
          id: 'event-1',
          title: 'Election',
          slug: 'election',
          markets: [{ id: 'market-1', question: 'Yes?', conditionId: 'condition-1', clobTokenIds: ['123'] }],
        },
      ],
      receivedAt: '2026-08-17T00:00:00.000Z',
    }));
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson: vi.fn(),
      fetchPolymarketJson,
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_polymarket_events')!;

    const body = jsonOf(await tool.handler({ limit: 1 }));

    expect(fetchPolymarketJson).toHaveBeenCalledWith(
      '/v1/events?limit=1&offset=0&order=volume24hr&ascending=false&active=true&closed=false',
    );
    expect(body.data.events[0]).toMatchObject({
      id: 'event-1',
      markets: [expect.objectContaining({ id: 'market-1' })],
    });
    expect(body.data.readAt).toBe('2026-08-17T00:00:00.000Z');
  });

  it('reads public recent news through the news gateway allowlist tool', async () => {
    const fetchNewsGatewayJson = vi.fn(async () => ({
      feedSource: 'tealnews',
      mode: 'public',
      generatedAt: '2026-08-18T00:00:00.000Z',
      returned: 1,
      items: [
        {
          id: 'news-1',
          ts: 1_800_000_000_000,
          custom: false,
          tags: {
            source: 'twitter',
            author: { handle: 'tealstreet' },
            text: 'BTC moving',
            symbols: ['BINANCE:BTCUSDT'],
          },
          raw: { vendor: 'hidden' },
        },
      ],
    }));
    const tools = createCliDataTools({
      fetchJson: vi.fn(),
      fetchExchangeProxyJson: vi.fn(),
      fetchNewsGatewayJson,
      fetchPolymarketJson: vi.fn(),
      queryHyperliquidAnalytics: vi.fn(),
    });
    const tool = tools.find((entry) => entry.name === 'get_recent_news')!;

    const body = jsonOf(await tool.handler({ limit: 1, source: 'twitter', symbol: 'btcusdt' }));

    expect(fetchNewsGatewayJson).toHaveBeenCalledWith('/news/recent?limit=1&source=twitter&symbol=BTCUSDT', {
      authorizationToken: undefined,
    });
    expect(body.data).toMatchObject({ feedSource: 'tealnews', mode: 'public', returned: 1 });
    expect(JSON.stringify(body)).not.toContain('vendor');
  });
});
