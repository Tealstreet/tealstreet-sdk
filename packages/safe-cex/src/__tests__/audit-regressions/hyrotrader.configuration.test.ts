import { afterEach, describe, expect, it, vi } from 'vitest';

// Transform the real constructor graph during setup; load() still re-evaluates
// it after each environment change rather than timing cold imports in a test.
import '../reference/bybit/hyrotrader.exchange';
import { DefaultStore } from '../../store/store.base';

// The account-options mapper never constructs an exchange. Avoid importing
// every venue through its unused factory dependency, and fail if it is called.
vi.mock('../../index', () => ({
  createExchange: () => {
    throw new Error('accountToExchangeOptions must not create an exchange');
  },
}));

const load = async (rest = '', publicWs = '', privateWs = '', prodRest = '', prodPublicWs = '', prodPrivateWs = '') => {
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL', rest);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL', publicWs);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL', privateWs);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_PROD_REST_URL', prodRest);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL', prodPublicWs);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL', prodPrivateWs);
  vi.resetModules();
  return import('../../exchanges/bybit/bybit.types');
};
afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

describe('HyroTrader build-time configuration', () => {
  it('derives secure socket URLs from the configured REST origin for both networks', async () => {
    const { HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL } = await load(' https://simulator.test/// ');
    expect(HYROTRADER_SIMULATOR_URL).toEqual({ livenet: 'https://simulator.test', testnet: 'https://simulator.test' });
    for (const network of ['livenet', 'testnet'] as const) {
      expect(HYROTRADER_SIMULATOR_WS_URL.public.linear[network]).toBe('wss://simulator.test/v5/public/linear');
      expect(HYROTRADER_SIMULATOR_WS_URL.private[network]).toBe('wss://simulator.test/v5/private');
    }
  });
  it('uses explicit public and private socket overrides when supplied', async () => {
    const { HYROTRADER_SIMULATOR_WS_URL, HYROTRADER_PROD_WS_URL } = await load(
      'https://simulator.test',
      'wss://public.test/feed/',
      'wss://private.test/account/',
      'https://prod.test',
      'wss://prod-public.test/feed/',
      'wss://prod-private.test/account/',
    );
    expect(HYROTRADER_SIMULATOR_WS_URL.public.linear.livenet).toBe('wss://public.test/feed');
    expect(HYROTRADER_SIMULATOR_WS_URL.private.livenet).toBe('wss://private.test/account');
    expect(HYROTRADER_PROD_WS_URL.public.linear.livenet).toBe('wss://prod-public.test/feed');
    expect(HYROTRADER_PROD_WS_URL.private.livenet).toBe('wss://prod-private.test/account');
  });

  it('keeps simulator and production origins separate', async () => {
    const { HYROTRADER_SIMULATOR_URL, HYROTRADER_PROD_URL, HYROTRADER_PROD_WS_URL } = await load(
      'https://simulator.test',
      '',
      '',
      'https://prod.test',
    );
    expect(HYROTRADER_SIMULATOR_URL.livenet).toBe('https://simulator.test');
    expect(HYROTRADER_PROD_URL.livenet).toBe('https://prod.test');
    expect(HYROTRADER_PROD_WS_URL.public.linear.livenet).toBe('wss://prod.test/v5/public/linear');
    expect(HYROTRADER_PROD_WS_URL.private.livenet).toBe('wss://prod.test/v5/private');
  });

  it('fails visibly without any Hyro origin instead of falling through to Bybit', async () => {
    await load();
    const { HyrotraderExchange } = await import('../reference/bybit/hyrotrader.exchange');
    expect(() => new HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore())).toThrow(
      'HyroTrader production REST URL is not configured. Set NEXT_PUBLIC_HYROTRADER_PROD_REST_URL, or NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL while v1 is simulator-backed.',
    );
  });

  it('marks bundled demo accounts as demo while using simulator config', async () => {
    await load('https://simulator.test');
    const { HyrotraderExchange } = await import('../reference/bybit/hyrotrader.exchange');
    const exchange = new HyrotraderExchange(
      'hyrotrader',
      { key: '', secret: '', extra: { hyrotraderMode: 'demo' } },
      new DefaultStore(),
    );
    try {
      expect(exchange.isDemo).toBe(true);
      expect(exchange.getBybitRestBaseUrl('livenet')).toBe('https://simulator.test');
    } finally {
      exchange.dispose();
    }
  });

  it('uses simulator config for ordinary v1 accounts when production origins are not live yet', async () => {
    await load('https://simulator.test');
    const { HyrotraderExchange } = await import('../reference/bybit/hyrotrader.exchange');
    const exchange = new HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore());
    try {
      expect(exchange.isDemo).toBe(false);
      expect(exchange.getBybitRestBaseUrl('livenet')).toBe('https://simulator.test');
      expect(exchange.getBybitPublicWsUrl('linear', 'livenet')).toBe('wss://simulator.test/v5/public/linear');
      expect(exchange.getBybitPrivateWsUrl('livenet')).toBe('wss://simulator.test/v5/private');
    } finally {
      exchange.dispose();
    }
  });

  it('marks unflagged accounts as production', async () => {
    await load('', '', '', 'https://prod.test');
    const { HyrotraderExchange } = await import('../reference/bybit/hyrotrader.exchange');
    const exchange = new HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore());
    try {
      expect(exchange.isDemo).toBe(false);
      expect(exchange.getBybitRestBaseUrl('livenet')).toBe('https://prod.test');
    } finally {
      exchange.dispose();
    }
  });

  it('reloads real constructors with fresh origins while existing instances retain their captured config', async () => {
    await load('https://first.test');
    const firstModule = await import('../reference/bybit/hyrotrader.exchange');
    const first = new firstModule.HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore());
    try {
      await load('', '', '', 'https://second.test');
      const secondModule = await import('../reference/bybit/hyrotrader.exchange');
      const second = new secondModule.HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore());
      try {
        expect(first.getBybitRestBaseUrl('livenet')).toBe('https://first.test');
        expect(first.getBybitPublicWsUrl('linear', 'livenet')).toBe('wss://first.test/v5/public/linear');
        expect(first.getBybitPrivateWsUrl('livenet')).toBe('wss://first.test/v5/private');
        expect(second.getBybitRestBaseUrl('livenet')).toBe('https://second.test');
        expect(second.getBybitPublicWsUrl('linear', 'livenet')).toBe('wss://second.test/v5/public/linear');
        expect(second.getBybitPrivateWsUrl('livenet')).toBe('wss://second.test/v5/private');
      } finally {
        second.dispose();
      }
    } finally {
      first.dispose();
    }
  });

  it('maps hyro demo account records into demo exchange options', async () => {
    const { accountToExchangeOptions } = await import('../../createExchangeFromAccount');
    expect(
      accountToExchangeOptions({
        exchange: 'hyrotrader',
        name: 'default-demo',
        apiKey: 'demo',
        apiSecret: 'secret',
        hyrotraderDemo: true,
      }).extra?.hyrotraderMode,
    ).toBe('demo');
    expect(
      accountToExchangeOptions({
        exchange: 'hyrotrader',
        name: 'prod',
        apiKey: 'prod',
        apiSecret: 'secret',
      }).extra?.hyrotraderMode,
    ).toBe('production');
  });

  it('does not define public production credential env config', async () => {
    const types = await load('', '', '', 'https://prod.test');
    expect(JSON.stringify(types)).not.toContain('NEXT_PUBLIC_HYROTRADER_PROD_API');
    expect(JSON.stringify(types)).not.toContain('HYROTRADER_PROD_API_SECRET');
  });

  it('keeps the local REST order-lifecycle fallback engaged for both demo and prod', async () => {
    // The base `BybitExchange` gates the local REST order-lifecycle fallback
    // on `isDemo || BYBIT_BRAZIL_SITE_ID`. This branch flipped hyrotrader's
    // `isDemo` to `false` for prod so journaling/grading classify prod live,
    // which would silently disable the fallback for prod accounts and let
    // place/cancel/amend REST responses sit un-reflected in the store until a
    // private-WS frame arrives. HyroTrader's private WS is simulator-backed
    // in both modes and its delivery reliability has not yet been
    // independently measured against Bybit mainnet (see the `probe-coverage`
    // close-out in `.cursor/plans/hyrotrader-audit_757b4e5d.plan.md`), so
    // `HyrotraderExchange` overrides the gate to run the fallback
    // unconditionally. Removing this override without first producing a
    // probe entry demonstrating mainnet-grade private-WS delivery is a
    // regression gate.
    await load('https://simulator.test', '', '', 'https://prod.test');
    const { HyrotraderExchange } = await import('../reference/bybit/hyrotrader.exchange');

    const demo = new HyrotraderExchange(
      'hyrotrader',
      { key: '', secret: '', extra: { hyrotraderMode: 'demo' } },
      new DefaultStore(),
    );
    const prod = new HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore());
    try {
      // The method is `protected` on the base — reach through the instance
      // for the audit assertion. Any caller shape would be acceptable; the
      // invariant is that both resolve to `true`.
      const demoFallback = (demo as unknown as { shouldUseLocalRestOrderLifecycleFallback: () => boolean })
        .shouldUseLocalRestOrderLifecycleFallback;
      const prodFallback = (prod as unknown as { shouldUseLocalRestOrderLifecycleFallback: () => boolean })
        .shouldUseLocalRestOrderLifecycleFallback;
      expect(demoFallback.call(demo)).toBe(true);
      expect(prodFallback.call(prod)).toBe(true);
    } finally {
      demo.dispose();
      prod.dispose();
    }
  });
});
