import type { CoreAdapter } from '../adapter/AdapterContext';

import { describe, expect, it } from 'vitest';

import { CORE_READ_BINDINGS } from './readBindings';
import { CORE_READ_API_PATHS } from './runtimeSurface';

/**
 * The manifest is only trustworthy if it covers the surface exactly. A missing
 * binding is a capability no shell can offer; an extra one is a tool that
 * calls a method the runtime does not have.
 */
describe('CORE_READ_BINDINGS covers the read surface', () => {
  it('binds exactly the paths in CORE_READ_API_PATHS', () => {
    const bound = Object.values(CORE_READ_BINDINGS)
      .map((binding) => binding.path)
      .sort();
    expect(bound).toEqual([...CORE_READ_API_PATHS].sort());
  });

  it('keys the record by the method each binding names', () => {
    for (const [key, binding] of Object.entries(CORE_READ_BINDINGS)) {
      expect(binding.method).toBe(key);
      expect(binding.path).toBe(`api.${key}`);
    }
  });
});

/**
 * OpenAI strict mode rejects a schema at REQUEST time, not review time. These
 * are the two rules it enforces, checked here so a bad binding fails in CI
 * rather than as a provider 400 in front of a user.
 */
describe('params are OpenAI strict-mode JSON Schema', () => {
  it.each(Object.values(CORE_READ_BINDINGS).map((b) => [b.method, b] as const))(
    '%s lists every property as required and forbids extras',
    (_method, binding) => {
      expect(binding.params.type).toBe('object');
      expect(binding.params.additionalProperties).toBe(false);
      expect([...binding.params.required].sort()).toEqual(Object.keys(binding.params.properties).sort());
    },
  );

  it('expresses optional arguments as a nullable type, never by omission', () => {
    const positions = CORE_READ_BINDINGS.getPositions.params;
    expect(positions.required).toContain('symbol');
    expect(positions.properties.symbol.type).toEqual(['string', 'null']);
  });
});

const recordingAdapter = () => {
  const calls: Array<{ method: string; args: readonly unknown[] }> = [];
  const spy =
    (method: string) =>
    (...args: readonly unknown[]) => {
      calls.push({ method, args });
      return undefined;
    };
  const adapter = new Proxy({} as CoreAdapter, {
    get: (_target, prop: string) => spy(prop),
  });
  return { adapter, calls };
};

/**
 * The flattening is the whole reason this file exists — a tool sends one flat
 * object, the adapter takes positional arguments. These assert the mapping for
 * the three the audit flagged as ambiguous.
 */
describe('invoke flattens a tool argument object onto the adapter signature', () => {
  it('maps getCandles symbol/timeframe positionally and the rest into opts', () => {
    const { adapter, calls } = recordingAdapter();
    CORE_READ_BINDINGS.getCandles.invoke(adapter, {
      symbol: 'BTCUSDT',
      timeframe: '4h',
      account: 'main',
      limit: 100,
      from: null,
      to: null,
    });
    expect(calls).toEqual([
      { method: 'getCandles', args: ['BTCUSDT', '4h', { account: 'main', limit: 100 }] },
    ]);
  });

  it('maps getOrderbook depth into opts and drops it when null', () => {
    const { adapter, calls } = recordingAdapter();
    CORE_READ_BINDINGS.getOrderbook.invoke(adapter, { symbol: 'ETHUSDT', account: 'main', depth: null });
    expect(calls).toEqual([{ method: 'getOrderbook', args: ['ETHUSDT', { account: 'main' }] }]);
  });

  it('passes getCombinedBalance an accounts ARRAY, not a single account', () => {
    const { adapter, calls } = recordingAdapter();
    CORE_READ_BINDINGS.getCombinedBalance.invoke(adapter, { accounts: ['a', 'b'] });
    expect(calls).toEqual([{ method: 'getCombinedBalance', args: [{ accounts: ['a', 'b'] }] }]);
  });
});

/**
 * Arguments arrive as untyped JSON from a model. A refusal the caller can read
 * beats a `TypeError` from inside the venue client.
 */
describe('invoke refuses malformed arguments', () => {
  it('requires account', () => {
    const { adapter } = recordingAdapter();
    expect(() => CORE_READ_BINDINGS.getBalance.invoke(adapter, {})).toThrow(/"account" is required/);
  });

  it('rejects a timeframe outside the enum', () => {
    const { adapter } = recordingAdapter();
    expect(() =>
      CORE_READ_BINDINGS.getCandles.invoke(adapter, { symbol: 'BTCUSDT', timeframe: '7m', account: 'main' }),
    ).toThrow(/"timeframe" must be one of/);
  });

  it('rejects a non-array symbols argument', () => {
    const { adapter } = recordingAdapter();
    expect(() => CORE_READ_BINDINGS.getTickers.invoke(adapter, { symbols: 'BTCUSDT', account: 'main' })).toThrow(
      /must be an array of strings/,
    );
  });
});
