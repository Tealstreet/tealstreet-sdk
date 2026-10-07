import type { ReadManifestEntry } from '@tealstreet/script-api-core/src/runtime/readBindings';
import type { OemsContext } from './context.types';

import { AGENT_READ_API_PATHS } from '@tealstreet/script-api-core/src/runtime/runtimeSurface';
import { describe, expect, it } from 'vitest';

import { OEMS_PLAN_SCHEMA } from './planSchema';
import {
  AGENT_READ_TOOL_NAMES,
  AGENT_READ_TOOLS,
  buildReadToolsBlock,
  MAX_READ_CALLS_PER_TURN,
  parseReadCalls,
  READ_CALL_SCHEMA,
  buildReadCallProperties,
} from './readTools';

const context = (over: Partial<OemsContext> = {}): OemsContext => ({
  generatedAt: Date.now(),
  activeAccount: 'binance-main',
  activeSymbol: 'BTCUSDT',
  activeAccounts: ['binance-main'],
  accounts: [
    { name: 'binance-main', exchange: 'binance', active: true, testnet: false, connected: true },
    { name: 'bybit-hedge', exchange: 'bybit', active: true, testnet: false, connected: true },
  ],
  positions: [],
  orders: [],
  balances: [],
  markets: [],
  fatFingerNotional: 0,
  ...over,
});

describe('the agent read slice', () => {
  /** EQUALITY, not narrowing — `surface.test.ts` asserts strict narrowing for
   *  `OEMS_READ_METHODS`, and reusing that helper here would be wrong. */
  it('covers exactly AGENT_READ_API_PATHS', () => {
    expect(AGENT_READ_TOOLS.map((tool) => tool.path).sort()).toEqual([...AGENT_READ_API_PATHS].sort());
  });

  it('offers the analysis reads the OEMS write slice deliberately excludes', () => {
    expect(AGENT_READ_TOOL_NAMES).toContain('getCandles');
    expect(AGENT_READ_TOOL_NAMES).toContain('getOrderbook');
  });
});

describe('READ_CALL_SCHEMA is OpenAI strict-mode valid', () => {
  it('lists every property as required and forbids extras', () => {
    expect(READ_CALL_SCHEMA.additionalProperties).toBe(false);
    expect([...READ_CALL_SCHEMA.required].sort()).toEqual(Object.keys(READ_CALL_SCHEMA.properties).sort());
  });

  it('makes every argument nullable, because a flat union fills one tool at a time', () => {
    for (const [name, field] of Object.entries(READ_CALL_SCHEMA.properties)) {
      if (name === 'tool') continue;
      const types = Array.isArray(field.type) ? field.type : [field.type];
      expect(types, name).toContain('null');
    }
  });

  it('preserves a per-field enum through the union', () => {
    expect(READ_CALL_SCHEMA.properties.timeframe.enum).toContain('4h');
  });

  /**
   * The enum has to widen with the type. Without `null` as a member, the only
   * legal values under strict mode are real timeframes — so a model calling
   * `getBalance`, which has no timeframe, is forced to invent one.
   */
  it('carries null in every nullable enum, not just in the type', () => {
    for (const [name, field] of Object.entries(READ_CALL_SCHEMA.properties)) {
      if (name === 'tool' || !field.enum) continue;
      const types = Array.isArray(field.type) ? field.type : [field.type];
      expect(types, name).toContain('null');
      expect(field.enum, name).toContain(null);
    }
  });

  it('unions fields across tools rather than taking the first tool that declares one', () => {
    // `symbols` exists only on getTickers and `depth` only on getOrderbook, so
    // their presence proves the union walked every tool.
    expect(Object.keys(READ_CALL_SCHEMA.properties)).toEqual(
      expect.arrayContaining(['tool', 'symbol', 'symbols', 'account', 'accounts', 'timeframe', 'depth', 'limit']),
    );
    expect(READ_CALL_SCHEMA.properties.symbols.type).toContain('array');
    expect(READ_CALL_SCHEMA.properties.symbol.type).toContain('string');
  });

  it('refuses to build a union when two tools give one field name different types', () => {
    // Synthetic tools: no real pair conflicts today, which is the point of the
    // guard — it has to be exercised deliberately or it is never reached.
    const field = (type: string) => ({
      type: 'object' as const,
      additionalProperties: false as const,
      required: ['x'],
      properties: { x: { type, description: 'the same name, two types' } },
    });
    const clash = [
      { ...AGENT_READ_TOOLS[0], method: 'a', params: field('string') },
      { ...AGENT_READ_TOOLS[0], method: 'b', params: field('number') },
    ] as unknown as readonly ReadManifestEntry[];
    expect(() => buildReadCallProperties(clash)).toThrow(/cannot express both/);
  });

  it('builds cleanly from the real manifest', () => {
    expect(() => buildReadCallProperties(AGENT_READ_TOOLS)).not.toThrow();
  });

  /** Tool names reach the provider as enum values, and the plan's own `kind`
   *  enum is snake_case; a collision would make one vocabulary two things. */
  it('uses provider-legal names that cannot collide with a plan step kind', () => {
    const kinds = new Set(OEMS_PLAN_SCHEMA.properties.steps.items.properties.kind.enum as readonly string[]);
    for (const name of AGENT_READ_TOOL_NAMES) {
      expect(name).toMatch(/^[a-zA-Z0-9_-]+$/);
      expect(kinds.has(name)).toBe(false);
    }
  });
});

describe('buildReadToolsBlock', () => {
  it('describes every tool it offers', () => {
    const block = buildReadToolsBlock();
    for (const name of AGENT_READ_TOOL_NAMES) expect(block).toContain(name);
  });

  it('marks nullable arguments optional and spells out an enum', () => {
    const block = buildReadToolsBlock();
    expect(block).toContain('symbol?:string');
    expect(block).toContain('4h');
  });
});

describe('parseReadCalls', () => {
  it('keeps only the fields the named tool declares', () => {
    const { calls } = parseReadCalls(
      [{ tool: 'getBalance', account: 'binance-main', symbol: 'BTCUSDT', depth: 5, timeframe: null }],
      context(),
    );
    expect(calls).toEqual([{ tool: 'getBalance', args: { account: 'binance-main' } }]);
  });

  /** The write path is backstopped by `resolvePlan` looking every address up in
   *  the context. A read has no such lookup, so this IS the backstop. */
  it('refuses an account that is not in the snapshot', () => {
    const { calls, rejections } = parseReadCalls([{ tool: 'getBalance', account: 'not-mine' }], context());
    expect(calls).toHaveLength(0);
    expect(rejections[0].reason).toContain('not-mine');
  });

  it('refuses a combined-balance request naming an account outside the snapshot', () => {
    const { calls, rejections } = parseReadCalls(
      [{ tool: 'getCombinedBalance', accounts: ['binance-main', 'not-mine'] }],
      context(),
    );
    expect(calls).toHaveLength(0);
    expect(rejections[0].reason).toContain('not-mine');
  });

  /** `find` returns undefined for "not found" AND for finding a literal
   *  undefined, so this shape used to slip past the allowlist entirely. */
  it.each([[[undefined]], [[null]], [[1]], [['binance-main', undefined]]])(
    'refuses a non-string entry in accounts: %j',
    (accounts) => {
      const { calls, rejections } = parseReadCalls([{ tool: 'getCombinedBalance', accounts }], context());
      expect(calls).toHaveLength(0);
      expect(rejections).toHaveLength(1);
    },
  );

  it('refuses an accounts argument that is not a list', () => {
    const { calls, rejections } = parseReadCalls([{ tool: 'getCombinedBalance', accounts: 'binance-main' }], context());
    expect(calls).toHaveLength(0);
    expect(rejections[0].reason).toContain('list of account names');
  });

  it('accepts a combined-balance request naming only snapshot accounts', () => {
    const { calls } = parseReadCalls(
      [{ tool: 'getCombinedBalance', accounts: ['binance-main', 'bybit-hedge'] }],
      context(),
    );
    expect(calls).toHaveLength(1);
  });

  it('refuses a tool this terminal does not offer', () => {
    const { calls, rejections } = parseReadCalls([{ tool: 'placeOrder', account: 'binance-main' }], context());
    expect(calls).toHaveLength(0);
    expect(rejections[0].reason).toContain('not a read');
  });

  it('caps the fan-out and says so', () => {
    const many = Array.from({ length: MAX_READ_CALLS_PER_TURN + 3 }, () => ({
      tool: 'getBalance',
      account: 'binance-main',
    }));
    const { calls, rejections } = parseReadCalls(many, context());
    expect(calls).toHaveLength(MAX_READ_CALLS_PER_TURN);
    expect(rejections.some((rejection) => rejection.reason.includes('3 more were not run'))).toBe(true);
  });

  /** The cap counts ACCEPTED calls. Slicing the input first meant a burst of
   *  malformed requests consumed the turn and a valid one was never examined. */
  it('does not let rejected calls consume the fan-out budget', () => {
    const raw = [
      ...Array.from({ length: MAX_READ_CALLS_PER_TURN }, () => ({ tool: 'nope' })),
      { tool: 'getBalance', account: 'binance-main' },
    ];
    const { calls } = parseReadCalls(raw, context());
    expect(calls).toEqual([{ tool: 'getBalance', args: { account: 'binance-main' } }]);
  });

  /** Same class as the `accounts` bug below, one field over: falling through as
   *  `undefined` would SKIP the allowlist and leave the value on `args`. */
  it.each([[123], [{}], [['binance-main']], [true]])('refuses a non-string account: %j', (account) => {
    const { calls, rejections } = parseReadCalls([{ tool: 'getBalance', account }], context());
    expect(calls).toHaveLength(0);
    expect(rejections[0].reason).toContain('must be an account name');
  });

  it('refuses a call missing an argument the tool declares non-nullable', () => {
    const { calls, rejections } = parseReadCalls([{ tool: 'getTicker', symbol: null, account: null }], context());
    expect(calls).toHaveLength(0);
    expect(rejections[0].reason).toContain('needs');
  });

  it('drops nulls so an optional argument reaches the binding as absent', () => {
    const { calls } = parseReadCalls(
      [{ tool: 'getCandles', symbol: 'BTCUSDT', timeframe: '4h', account: 'binance-main', from: null, to: null, limit: 100 }],
      context(),
    );
    expect(calls[0].args).toEqual({ symbol: 'BTCUSDT', timeframe: '4h', account: 'binance-main', limit: 100 });
  });

  it('is empty for a non-array', () => {
    expect(parseReadCalls('nope', context())).toEqual({ calls: [], rejections: [] });
  });
});

/**
 * `getCombinedBalance` declares `accounts` nullable and its binding reads null
 * as "sum every streaming account" — the right default for a script the user
 * wrote, the wrong one for a caller whose account reach is meant to BE the
 * snapshot. Omitted, it walked past the allowlist, which only ever runs on a
 * value that is present.
 */
describe('an omitted account list is not "no restriction"', () => {
  it('substitutes the snapshot accounts rather than passing null through', () => {
    const { calls } = parseReadCalls([{ tool: 'getCombinedBalance', accounts: null }], context());
    expect(calls).toHaveLength(1);
    expect(calls[0].args.accounts).toEqual(context().accounts.map((account) => account.name));
  });

  it('still refuses a list naming an account the snapshot does not have', () => {
    const { calls, rejections } = parseReadCalls(
      [{ tool: 'getCombinedBalance', accounts: ['someone-elses'] }],
      context(),
    );
    expect(calls).toEqual([]);
    expect(rejections[0].reason).toContain('someone-elses');
  });
});
