import type { ExchangeName, ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { assertEngineAccountRouting, resolveEngineRoutingPlan } from '../../transport/engine';
import { tealstreetTransport } from '../../transport/tealstreet/tealstreetTransport';

const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions =>
  ({
    key: 'organizations/test/apiKeys/trade',
    secret: 'unused-test-secret',
    secondary_key: 'organizations/test/apiKeys/read',
    secondary_secret: 'unused-read-secret',
    transport: tealstreetTransport,
    ...extra,
  }) as ExchangeOptions;

describe('Coinbase saved read-only key on the Rust engine', () => {
  it.each<ExchangeName>(['coinbase', 'coinbasespot'])('%s accepts a secondary read key', (name) => {
    expect(() => assertEngineAccountRouting(options(), name)).not.toThrow();
  });

  it.each<ExchangeName>(['coinbase', 'coinbasespot'])(
    '%s read route keeps the proxied Coinbase authorization alias',
    (name) => {
      const plan = resolveEngineRoutingPlan(name, options({ tealstreetProxy: true }), 'https://api.coinbase.com');
      expect(plan.readHttp).toBeDefined();
      expect(plan.readHeader).toBe('Authorization');
      expect(plan.readHttp?.headerAliases).toEqual({ Authorization: 'Coinbase-Authorization' });
      expect(JSON.stringify(plan)).not.toContain('unused-read-secret');
    },
  );
});
