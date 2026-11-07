import { describe, expect, it, vi } from 'vitest';

import { BybitInverseExchange } from './reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from './reference/bybit/bybitspot.exchange';
import { DefaultStore } from '../store/store.base';

/**
 * A rejected key answers the margin probe before the markets load. The probe used to get
 * its handler only after those awaits, so Node saw an unhandled rejection and exited.
 */
const REJECTION = 'API key is invalid.';

const variants = [
  ['bybitinverse', BybitInverseExchange, 'publicInverseWebsocket'],
  ['bybitspot', BybitSpotExchange, 'publicWebsocket'],
] as const;

describe.each(variants)('%s startup with a rejected key', (name, Exchange, publicSocket) => {
  it('raises no unhandled rejection when the probe fails before the markets load', async () => {
    const exchange = new Exchange(
      name,
      { key: 'k', secret: 's', testnet: false, tuning: { consoleLogging: false } } as any,
      new DefaultStore(),
    );
    for (const method of ['fetchFees', 'fetchAndCacheApiKeyInfo', 'tick', 'fetchOrdersPositionsAndBalance']) {
      if (typeof (exchange as any)[method] === 'function') vi.spyOn(exchange as any, method).mockResolvedValue([]);
    }
    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'fetchMarkets').mockImplementation(
      () => new Promise((resolve) => setTimeout(() => resolve([]), 50)),
    );
    vi.spyOn(exchange as any, 'fetchTickersWithoutParse').mockResolvedValue([]);
    vi.spyOn(exchange as any, 'parseTickers').mockReturnValue([]);
    (exchange as any)[publicSocket] = { connectAndSubscribe: vi.fn(), dispose: vi.fn() };
    (exchange as any).privateWebsocket = { connectAndSubscribe: vi.fn(), dispose: vi.fn() };
    (exchange as any).xhr = {
      get: vi.fn(async (url: string) =>
        url.includes('/v5/account/info')
          ? { data: { retCode: 10003, retMsg: REJECTION, result: {} } }
          : { data: { retMsg: 'OK', result: { list: [] } } },
      ),
    };
    const logged: string[] = [];
    vi.spyOn(exchange, 'log').mockImplementation((message: any) => {
      logged.push(String(message));
    });
    const unhandled = vi.fn();
    process.on('unhandledRejection', unhandled);

    await exchange.start();
    await new Promise((resolve) => setImmediate(resolve));

    process.off('unhandledRejection', unhandled);
    expect(unhandled).not.toHaveBeenCalled();
    expect(logged.some((message) => message.includes(REJECTION))).toBe(true);
    exchange.dispose();
  });
});
