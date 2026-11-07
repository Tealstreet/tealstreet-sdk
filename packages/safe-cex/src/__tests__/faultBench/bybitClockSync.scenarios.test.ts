import type { Market } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { parseBybitServerTimestamp } from '../reference/bybit/bybit.api';
import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';
import { createFakeVenue } from './fakeVenue';

// Browser (worker) by default; a server shares one clock across every user's exchange.
const runtime = vi.hoisted(() => ({ node: false }));
vi.mock('../../utils/runtime', () => ({ isNodeOrBunRuntime: () => runtime.node }));
import { createFaultAdapter } from './faultAdapter';

// Reported 2026-10-09: "invalid request, please check your server timestamp or
// recv_window param" toasted on a client clock 2.1s ahead of Bybit's. The resync
// before the one retry zeroed the offset in a web worker, so the retry failed too.

const MARKET: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  info: {},
};

const LOCAL_NOW = 1_791_518_968_627;
const SERVER_NOW = 1_791_518_966_503;
const skewMessage = (requestTimestamp: number) =>
  `invalid request, please check your server timestamp or recv_window param: req_timestamp[${requestTimestamp}],server_timestamp[${SERVER_NOW}],recv_window[20000]`;

const exchanges: BybitExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  runtime.node = false;
  virtualClock.syncFromServerTime(Date.now());
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('Bybit clock skew', () => {
  it('reads the server clock out of the rejection', () => {
    expect(parseBybitServerTimestamp(skewMessage(LOCAL_NOW))).toBe(SERVER_NOW);
    expect(parseBybitServerTimestamp('Insufficient balance')).toBeUndefined();
  });

  it('leaves the shared clock alone on a server', async () => {
    runtime.node = true;
    vi.useFakeTimers({ toFake: ['Date'], now: LOCAL_NOW });
    virtualClock.syncFromServerTime(LOCAL_NOW);
    const syncFromServerTime = vi.spyOn(virtualClock, 'syncFromServerTime');
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new BybitExchange('bybit' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange);
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const venue = createFakeVenue({
      'get /v5/position/list': () => ({ status: 200, data: { retCode: 10002, retMsg: skewMessage(LOCAL_NOW) } }),
    });
    createFaultAdapter(venue).install((exchange as any).xhr);

    await (exchange as any).xhr.get('/v5/position/list', { params: { category: 'linear' } });
    expect(syncFromServerTime).not.toHaveBeenCalled();
  });

  it('resyncs from the venue clock and the one retry succeeds, with nothing shown', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: LOCAL_NOW });
    virtualClock.syncFromServerTime(LOCAL_NOW);
    const store = new DefaultStore();
    store.update({ markets: [MARKET] });
    const exchange = new BybitExchange('bybit' as any, { key: 'k', secret: 's' } as any, store);
    exchanges.push(exchange);
    const errors: string[] = [];
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation((error) => {
      errors.push(String(error));
    });

    let calls = 0;
    const venue = createFakeVenue({
      'get /v5/position/list': ({ config }) => {
        calls += 1;
        const requestTimestamp = Number(config.headers?.['X-BAPI-TIMESTAMP']);
        // Bybit refuses a request stamped more than 1s ahead of its own clock.
        if (requestTimestamp > SERVER_NOW + 1_000) {
          return { status: 200, data: { retCode: 10002, retMsg: skewMessage(requestTimestamp) } };
        }
        return { status: 200, data: { retCode: 0, retMsg: 'OK', result: { list: [] } } };
      },
    });
    createFaultAdapter(venue).install((exchange as any).xhr);

    const response = await (exchange as any).xhr.get('/v5/position/list', { params: { category: 'linear' } });

    expect(response.data.retCode).toBe(0);
    expect(calls).toBe(2);
    expect(Math.abs(Number(venue.received[1].config.headers?.['X-BAPI-TIMESTAMP']) - SERVER_NOW)).toBeLessThan(1_000);
    expect(errors).toEqual([]);
  });
});
