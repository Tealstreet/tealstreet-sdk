import { beforeEach, describe, expect, it, vi } from 'vitest';

const testState = vi.hoisted(() => {
  const constructorCalls: Array<{ name: string; options: Record<string, unknown> }> = [];

  class MockExchange {
    constructor(name: string, options: Record<string, unknown>) {
      constructorCalls.push({ name, options });
    }
  }

  return { constructorCalls, MockExchange };
});

vi.mock('./exchangeMetadata', () => ({
  getExchangeMetadata: vi.fn((exchangeName: string) => ({
    exchangeName,
    requiresCors: exchangeName === 'coinbase',
    enforceWsTunnel: false,
  })),
}));

vi.mock('./exchanges/registry', () => ({
  exchangeRegistry: {
    bybit: () => Promise.resolve(testState.MockExchange),
    coinbase: () => Promise.resolve(testState.MockExchange),
  },
  getRegisteredExchangeNames: vi.fn(() => ['bybit', 'coinbase']),
}));

vi.mock('./utils/virtualClock', () => ({
  virtualClock: {
    start: vi.fn(),
  },
}));

import { createPublicExchange } from './index';

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
