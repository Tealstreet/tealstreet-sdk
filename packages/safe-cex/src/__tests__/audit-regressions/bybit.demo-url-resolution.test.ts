import { describe, expect, it, vi } from 'vitest';

import { BASE_URL, DEMO_URL } from '../../exchanges/bybit/bybit.types';
import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitDemoExchange } from '../reference/bybit/bybitdemo.exchange';

const createStore = () =>
  ({
    subscribe: vi.fn(),
    markets: [],
    marketsBySymbol: {},
    tickersBySymbol: {},
    orders: [],
    positions: [],
  }) as any;

describe('Bybit demo URL resolution', () => {
  it('builds demo REST clients against the demo origin during parent construction', () => {
    const exchange = new BybitDemoExchange('bybitdemo', { key: 'key', secret: 'secret' }, createStore());

    expect(exchange.isDemo).toBe(true);
    expect(exchange.xhr.defaults.baseURL).toBe(DEMO_URL.livenet);
    expect(exchange.unlimitedXHR.defaults.baseURL).toBe(DEMO_URL.livenet);
  });

  it('keeps regular Bybit REST clients on the live origin', () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore());

    expect(exchange.xhr.defaults.baseURL).toBe(BASE_URL.livenet);
    expect(exchange.unlimitedXHR.defaults.baseURL).toBe(BASE_URL.livenet);
  });
});
