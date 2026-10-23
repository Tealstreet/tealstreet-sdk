import { describe, expect, it, vi } from 'vitest';

import { LighterExchange } from '../../exchanges/lighter/lighter.exchange';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
} from '../../exchanges/lighter/lighter.types';
import { DefaultStore } from '../../store/store.base';
import { LogSeverity } from '../../types';

const createExchange = () =>
  new LighterExchange(
    'lighter',
    { key: 'test-key', secret: 'test-secret', apiKeyIndex: '1', publicDataOnly: true },
    new DefaultStore(),
  );

describe('audit regression: Lighter observability and endpoint inventory', () => {
  it('surfaces channel-less public websocket errors with code and message', () => {
    const exchange = createExchange();
    // A frame without hydrated prices also starts the REST ticker fallback.
    vi.spyOn(exchange.lighterApiClient, 'get').mockResolvedValue({ market_stats: {} });
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
    const recordStats = vi.spyOn(exchange.publicWebsocket as any, 'recordMessageHandlerStats');

    exchange.publicWebsocket.onMessage({
      data: JSON.stringify({ error: { code: 30005, message: 'Invalid Channel' } }),
    } as MessageEvent);

    expect(recordStats).toHaveBeenCalledWith('error');
    expect(log).toHaveBeenCalledWith('[Lighter] Public WS error 30005: Invalid Channel', LogSeverity.Warning);
    exchange.dispose();
  });

  it('tracks real transaction writes and only exercised periodic reads', () => {
    expect(GRAFANA_ENDPOINTS_EVERY).toEqual([ENDPOINTS.SEND_TX, ENDPOINTS.SEND_TX_BATCH]);
    expect(GRAFANA_ENDPOINTS_PERIODIC).toEqual([
      ENDPOINTS.ACCOUNT,
      ENDPOINTS.ACCOUNT_LIMITS,
      ENDPOINTS.OPEN_ORDERS,
    ]);
  });

  it('does not retain the dead fictitious route constants', () => {
    expect(ENDPOINTS.CANDLES).toBe('/api/v1/candles');
    expect(ENDPOINTS).not.toHaveProperty('TICKERS');
    expect(ENDPOINTS).not.toHaveProperty('ORDERBOOK');
    expect(ENDPOINTS).not.toHaveProperty('CREATE_ORDER');
    expect(ENDPOINTS).not.toHaveProperty('POSITIONS');
    expect(ENDPOINTS).not.toHaveProperty('TRANSACTIONS');
  });
});
