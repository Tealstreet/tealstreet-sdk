import { beforeEach, describe, expect, it, vi } from 'vitest';

import './transport/registerTealstreetTransport';

import { buildPublicExchangeOptions, createPublicExchange } from './index';

const testState = vi.hoisted(() => {
  const constructorCalls: Array<{ name: string; options: Record<string, unknown> }> = [];

  class MockExchange {
    constructor(name: string, options: Record<string, unknown>) {
      constructorCalls.push({ name, options });
    }
  }

  return { constructorCalls, MockExchange };
});

// Keep the real `exchangeMetadata` record (routing table needs its keys, now reachable
// via the routing table); only override the getExchangeMetadata function this suite controls.
vi.mock('./exchangeMetadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./exchangeMetadata')>()),
  getExchangeMetadata: vi.fn((exchangeName: string) => ({
    exchangeName,
    requiresCors: exchangeName === 'coinbase',
    enforceWsTunnel: exchangeName === 'bitmex',
  })),
}));

vi.mock('./exchanges/registry', () => ({
  exchangeRegistry: {
    bybit: () => Promise.resolve(testState.MockExchange),
    bitmex: () => Promise.resolve(testState.MockExchange),
    coinbase: () => Promise.resolve(testState.MockExchange),
  },
  getRegisteredExchangeNames: vi.fn(() => ['bybit', 'bitmex', 'coinbase']),
}));

vi.mock('./utils/virtualClock', () => ({
  virtualClock: {
    start: vi.fn(),
    setServerTimeSource: vi.fn(),
  },
}));

describe('createPublicExchange', () => {
  beforeEach(() => {
    testState.constructorCalls.length = 0;
  });

  it('adds metadata-driven proxy override for public exchanges that need proxying', async () => {
    await createPublicExchange('coinbase');

    expect(testState.constructorCalls[0]?.options).toMatchObject({
      key: '',
      secret: '',
      publicDataOnly: true,
      overrideProxyUrl: 'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy',
    });
  });

  it('uses the same proxy override for public exchanges that enforce HoW', () => {
    expect(buildPublicExchangeOptions('bitmex')).toMatchObject({
      key: '',
      secret: '',
      publicDataOnly: true,
      overrideProxyUrl: 'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy',
    });
  });

  it('disables all proxy routing when requested', async () => {
    await createPublicExchange('coinbase', {
      forceAllProxiesOff: true,
      overrideProxyUrl: 'https://custom-proxy.example.com',
      tealstreetProxy: true,
      tealstreetTunnel: true,
      tealstreetWsRoute: true,
    });

    expect(testState.constructorCalls[0]?.options).toMatchObject({
      key: '',
      secret: '',
      publicDataOnly: true,
      overrideProxyUrl: undefined,
      tealstreetProxy: false,
      tealstreetTunnel: false,
      tealstreetWsRoute: false,
    });
  });
});
