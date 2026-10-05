import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { getAllowedApiPathsForTargets } from '@tealstreet/script-api-core';
import { CORE_READ_BINDINGS } from '@tealstreet/script-api-core/src/runtime/readBindings';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { CLI_READ_TOOLS, readToolShape, renderReadResult } from './readTools';

describe('CLI_READ_TOOLS', () => {
  it('names only real SDK read methods', () => {
    for (const method of Object.values(CLI_READ_TOOLS)) {
      expect(CORE_READ_BINDINGS[method]).toBeDefined();
    }
  });

  /** Snake_case is in users' saved prompts and in the published CLI reference.
   *  The manifest supplies the definition; the shell owns the naming. */
  it('keeps the established snake_case names', () => {
    expect(Object.keys(CLI_READ_TOOLS)).toEqual(
      expect.arrayContaining(['get_positions', 'get_orders', 'get_balance', 'get_ticker']),
    );
    for (const name of Object.keys(CLI_READ_TOOLS)) expect(name).toMatch(/^[a-z][a-z_]*$/);
  });

  /**
   * These are excluded on purpose and the reasons differ. The session-derived
   * three would answer confidently and wrongly from a second process with no
   * REPL session; the account-less two cannot be gated on `ensureConnected`.
   * A tool that always lies is worse than a missing tool.
   */
  it('excludes the reads a second process cannot answer honestly', () => {
    const exposed = new Set(Object.values(CLI_READ_TOOLS));
    for (const method of ['getActiveAccount', 'getActiveSymbol', 'getActiveExchange'] as const) {
      expect(exposed.has(method), method).toBe(false);
    }
    for (const method of ['getActiveAccounts', 'getCombinedBalance'] as const) {
      expect(exposed.has(method), method).toBe(false);
    }
  });

  it('leaves candles to the existing proxy tool rather than shipping two', () => {
    expect(Object.values(CLI_READ_TOOLS)).not.toContain('getCandles');
  });

  /**
   * Exposing a read the `cli` target does not provide would register a tool
   * whose adapter method is missing — it would return `undefined`, which
   * `renderReadResult` reports as "may still be loading". A tool that lies
   * politely is the worst of the three outcomes.
   */
  it('exposes only reads the cli target actually provides', () => {
    const allowed = getAllowedApiPathsForTargets(['core', 'cli']);
    for (const method of Object.values(CLI_READ_TOOLS)) {
      expect(allowed.has(CORE_READ_BINDINGS[method].path), method).toBe(true);
    }
  });
});

describe('readToolShape', () => {
  it('converts every exposed tool without a hand-written schema', () => {
    for (const method of Object.values(CLI_READ_TOOLS)) {
      const shape = readToolShape(method);
      expect(Object.keys(shape)).toEqual(Object.keys(CORE_READ_BINDINGS[method].params.properties));
    }
  });

  /** The manifest marks nullable fields REQUIRED because OpenAI strict mode
   *  needs that. An MCP client should be able to omit one instead of sending
   *  an explicit null. */
  it('lets a client omit a nullable argument entirely', () => {
    const shape = readToolShape('getPositions');
    const parsed = z.object(shape).safeParse({ account: 'main' });
    expect(parsed.success).toBe(true);
  });

  it('still requires a non-nullable argument', () => {
    const shape = readToolShape('getTicker');
    expect(z.object(shape).safeParse({ account: 'main' }).success).toBe(false);
    expect(z.object(shape).safeParse({ account: 'main', symbol: 'BTCUSDT' }).success).toBe(true);
  });

  /** Named for what it actually checks: `depth` is a nullable NUMBER, and no
   *  tool this shell exposes carries an enum. */
  it('keeps a nullable numeric argument typed', () => {
    const shape = readToolShape('getOrderbook');
    expect(z.object(shape).safeParse({ symbol: 'BTCUSDT', account: 'main', depth: 5 }).success).toBe(true);
    expect(z.object(shape).safeParse({ symbol: 'BTCUSDT', account: 'main', depth: 'five' }).success).toBe(false);
  });
});

describe('renderReadResult', () => {
  /** Stores hydrate asynchronously after connect, so an empty result right
   *  after a cold connect is ambiguous — "none" would be a wrong answer. */
  it('distinguishes empty from still-loading', () => {
    expect(renderReadResult('getPositions', [])).toMatch(/may still be loading/);
    expect(renderReadResult('getBalance', undefined)).toMatch(/may still be loading/);
    expect(renderReadResult('getBalance', null)).toMatch(/may still be loading/);
  });

  it('renders a real result as JSON', () => {
    expect(renderReadResult('getBalance', { total: 100 })).toBe(JSON.stringify({ total: 100 }, null, 2));
  });

  /**
   * `CoreAdapter.getPositions` does NOT filter zero-size positions — it returns
   * the store's list as-is — and the hand-written tool this replaced did. A
   * flat position is not a position, and `buildOemsContext` on the web filters
   * the same way, so this is the shared expectation rather than a CLI quirk.
   */
  it('drops zero-size positions, which the adapter does not', () => {
    const rendered = renderReadResult('getPositions', [
      { symbol: 'BTCUSDT', contracts: 0 },
      { symbol: 'ETHUSDT', contracts: 2 },
    ]);
    expect(rendered).toContain('ETHUSDT');
    expect(rendered).not.toContain('BTCUSDT');
  });

  it('says still-loading when every position is flat', () => {
    expect(renderReadResult('getPositions', [{ symbol: 'BTCUSDT', contracts: 0 }])).toMatch(/may still be loading/);
  });

  /** `Position`, `Ticker`, `Market` and `Order` all carry `info?: any` — the
   *  raw venue payload. The old prose picked named fields and never saw it; a
   *  JSON dump would ship megabytes down a stdio transport. */
  it('strips the raw venue payload rather than dumping it', () => {
    const rendered = renderReadResult('getTicker', { symbol: 'BTCUSDT', last: 62_000, info: { blob: 'z'.repeat(5_000) } });
    expect(rendered).toContain('62000');
    expect(rendered).not.toContain('zzzz');
  });

  /** A client told "60 of 800" can narrow; one told only "truncated" reasons
   *  over the tail as if it were the set. */
  it('bounds a huge result and says how much of it is shown', () => {
    const markets = Array.from({ length: 800 }, (_, i) => ({ symbol: `S${i}` }));
    const rendered = renderReadResult('getMarkets', markets);
    expect(rendered).toContain('of 800');
    expect(rendered).toMatch(/showing \d+ of 800/);
    expect(rendered.length).toBeLessThan(40_000);
  });

  /**
   * "Narrow the request to see the rest" used to be appended to EVERY read,
   * including `get_markets`, which takes an account and nothing else. Told
   * that, a client re-issues the same call and gets the same subset.
   */
  it('advises narrowing only where the read has an argument that would', () => {
    const rows = (n: number) => Array.from({ length: n }, (_, i) => ({ symbol: `S${i}`, contracts: 1 }));
    expect(renderReadResult('getMarkets', rows(800))).toContain('no way to page past this');
    expect(renderReadResult('getPositions', rows(800))).toContain('naming a symbol');
    expect(renderReadResult('getOrderbook', {
      bids: Array.from({ length: 500 }, (_, i) => [i, 1]),
      asks: Array.from({ length: 500 }, (_, i) => [i, 1]),
    })).toContain('nearest the touch');
  });

  /**
   * The counts come from the bounding, not from a second derivation here — the
   * two shells that render truncation had drifted into describing it their own
   * way. The zero-size filter runs FIRST, so the total is what a reader can
   * actually ask for, not what the adapter returned.
   */
  it('counts what survives the zero-size filter, not what the adapter returned', () => {
    const positions = [
      ...Array.from({ length: 61 }, (_, i) => ({ symbol: `S${i}`, contracts: 1 })),
      ...Array.from({ length: 40 }, (_, i) => ({ symbol: `Z${i}`, contracts: 0 })),
    ];
    expect(renderReadResult('getPositions', positions)).toContain('showing 60 of 61');
  });
});

/**
 * The generated shape has to survive the MCP SDK's own zod → JSON Schema
 * conversion, which is what the CLIENT actually sees. Asserting the shape in
 * isolation would pass while clients were shown the wrong parameter list.
 */
describe('what an MCP client is actually shown', () => {
  const listTools = async () => {
    const server = new McpServer({ name: 'test', version: '0' });
    for (const [name, method] of Object.entries(CLI_READ_TOOLS)) {
      server.tool(name, CORE_READ_BINDINGS[method].summary, readToolShape(method), async () => ({
        content: [{ type: 'text' as const, text: 'ok' }],
      }));
    }
    const handlers = (server.server as unknown as { _requestHandlers: Map<string, (r: unknown, e: unknown) => unknown> })
      ._requestHandlers;
    const listed = (await handlers.get('tools/list')!({ method: 'tools/list', params: {} }, {})) as {
      tools: { name: string; inputSchema: { properties?: Record<string, unknown>; required?: string[] } }[];
    };
    return new Map(listed.tools.map((tool) => [tool.name, tool.inputSchema]));
  };

  it('registers every tool without the SDK rejecting a generated schema', async () => {
    const schemas = await listTools();
    expect([...schemas.keys()].sort()).toEqual(Object.keys(CLI_READ_TOOLS).sort());
  });

  it('shows a nullable argument as OPTIONAL, not as a required null', async () => {
    const schemas = await listTools();
    expect(schemas.get('get_positions')?.required).toEqual(['account']);
    expect(Object.keys(schemas.get('get_positions')?.properties ?? {})).toEqual(['account', 'symbol']);
    expect(schemas.get('get_orderbook')?.required).toEqual(['symbol', 'account']);
  });

  /**
   * `.nullish()` on an already-nullable union makes the SDK emit a NESTED
   * `anyOf` with the description buried in the inner arm, where most clients
   * will not show it. This asserts the description survives at the top level.
   */
  it('keeps a nullable argument’s description where a client will show it', async () => {
    const schemas = await listTools();
    const symbol = schemas.get('get_positions')?.properties?.symbol as {
      description?: string;
      anyOf?: { anyOf?: unknown[] }[];
    };
    expect(symbol.description).toContain('every symbol held');
    expect(symbol.anyOf?.some((arm) => Array.isArray(arm.anyOf))).toBeFalsy();
  });

  it('keeps a genuinely required argument required', async () => {
    const schemas = await listTools();
    expect(schemas.get('get_ticker')?.required?.sort()).toEqual(['account', 'symbol']);
    expect(schemas.get('get_tickers')?.required?.sort()).toEqual(['account', 'symbols']);
  });
});

