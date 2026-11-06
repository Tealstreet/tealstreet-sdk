import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { ENDPOINTS } from '../exchanges/bybit/bybit.types';
import { DefaultStore } from '../store/store.base';
import { LogSeverity } from '../types';

/**
 * Bybit answers a rejected key inside an HTTP 200 envelope, so the private probe
 * resolves either way and the readiness line used to print regardless.
 */
const REJECTION = 'API key is invalid.';

const createExchange = ({ externalPublicData }: { externalPublicData: boolean }) => {
  const store = new DefaultStore();
  const exchange = new BybitExchange(
    'bybit',
    { key: 'k', secret: 's', testnet: false, tuning: { consoleLogging: false } } as any,
    store,
  );
  vi.spyOn(exchange as any, 'fetchFees').mockResolvedValue([]);
  vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(externalPublicData);
  vi.spyOn(exchange as any, 'fetchAndCacheApiKeyInfo').mockResolvedValue(undefined);
  vi.spyOn(exchange as any, 'fetchMarkets').mockResolvedValue([]);
  vi.spyOn(exchange as any, 'fetchTickersWithoutParse').mockResolvedValue([]);
  vi.spyOn(exchange as any, 'parseTickers').mockReturnValue([]);
  vi.spyOn(exchange as any, 'tick').mockResolvedValue(undefined);
  vi.spyOn(exchange as any, 'fetchOrdersPositionsAndBalance').mockResolvedValue(undefined);
  (exchange as any).publicWebsocket = { connectAndSubscribe: vi.fn(), dispose: vi.fn() };
  (exchange as any).privateWebsocket = { connectAndSubscribe: vi.fn(), dispose: vi.fn() };

  const logged: Array<{ message: string; severity?: LogSeverity }> = [];
  vi.spyOn(exchange, 'log').mockImplementation((message: any, severity?: any) => {
    logged.push({ message: String(message), severity });
  });
  return { exchange, logged };
};

/** The real margin probe, answering exactly as the venue does for retCode 10003. */
const mockMarginProbe = (exchange: BybitExchange, retMsg: string) => {
  const get = vi.fn(async (url: string) => {
    if (url === ENDPOINTS.ACCOUNT_MARGIN) {
      return { data: { retCode: retMsg === 'OK' ? 0 : 10003, retMsg, result: {} } };
    }
    return { data: { retMsg: 'OK', result: { list: [] } } };
  });
  (exchange as any).xhr = { get };
  return get;
};

describe('Bybit startup readiness log', () => {
  it('handles a rejected key that answers before the markets do', async () => {
    const { exchange, logged } = createExchange({ externalPublicData: false });
    mockMarginProbe(exchange, REJECTION);
    vi.spyOn(exchange as any, 'fetchMarkets').mockImplementation(() => new Promise((r) => setTimeout(() => r([]), 50)));
    const unhandled = vi.fn();
    process.on('unhandledRejection', unhandled);

    await exchange.start();
    await new Promise((r) => setImmediate(r));

    process.off('unhandledRejection', unhandled);
    expect(unhandled).not.toHaveBeenCalled();
    expect(logged.some((entry) => entry.message.includes(REJECTION))).toBe(true);
    exchange.dispose();
  });

  it('says ready to trade when the venue accepted the key', async () => {
    const { exchange, logged } = createExchange({ externalPublicData: false });
    mockMarginProbe(exchange, 'OK');

    await exchange.start();

    expect(logged.some((entry) => entry.message === 'Ready to trade on Bybit')).toBe(true);
    exchange.dispose();
  });

  /**
   * `externalPublicData: false` is the load-bearing half. It is the branch that
   * awaits the SIGNED markets/tickers calls, whose success calls
   * `recordAuthSuccess()` and zeroes `authFailureStreak` after the probe already
   * failed — so a gate reading that streak passes with the stub and lies in
   * production. The gate must read the probe's own result.
   */
  it.each([true, false])(
    'does not claim ready to trade after a rejected key (externalPublicData=%s)',
    async (externalPublicData) => {
      const { exchange, logged } = createExchange({ externalPublicData });
      mockMarginProbe(exchange, REJECTION);

      await exchange.start();

      expect(logged.some((entry) => entry.message === 'Ready to trade on Bybit')).toBe(false);
      const rejection = logged.find((entry) => entry.message.includes('private API was rejected'));
      expect(rejection?.severity).toBe(LogSeverity.Warning);
      // The venue's own reason, so the line never guesses at the cause.
      expect(rejection?.message).toContain(REJECTION);
      expect(rejection?.message).toContain('trading is unavailable');
      exchange.dispose();
    },
  );

  it('still starts the account, so public market data survives a rejected key', async () => {
    const { exchange } = createExchange({ externalPublicData: false });
    mockMarginProbe(exchange, REJECTION);

    await expect(exchange.start()).resolves.not.toThrow();

    expect((exchange as any).publicWebsocket.connectAndSubscribe).toHaveBeenCalled();
    exchange.dispose();
  });
});
