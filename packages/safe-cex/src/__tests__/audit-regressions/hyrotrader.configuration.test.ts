import { afterEach, describe, expect, it, vi } from 'vitest';

const load = async (rest = '', publicWs = '', privateWs = '') => {
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL', rest);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL', publicWs);
  vi.stubEnv('NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL', privateWs);
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
    const { HYROTRADER_SIMULATOR_WS_URL } = await load(
      'https://simulator.test',
      'wss://public.test/feed/',
      'wss://private.test/account/',
    );
    expect(HYROTRADER_SIMULATOR_WS_URL.public.linear.livenet).toBe('wss://public.test/feed');
    expect(HYROTRADER_SIMULATOR_WS_URL.private.livenet).toBe('wss://private.test/account');
  });
  it('fails visibly without an origin instead of falling through to Bybit', async () => {
    await load();
    const { HyrotraderExchange } = await import('../../exchanges/bybit/hyrotrader.exchange');
    const { DefaultStore } = await import('../../store/store.base');
    expect(() => new HyrotraderExchange('hyrotrader', { key: '', secret: '' }, new DefaultStore())).toThrow(
      'REST URL is not configured',
    );
  });
});
