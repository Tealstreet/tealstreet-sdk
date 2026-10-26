import type { ExchangeOptions } from '../../types';

import createHmac from 'create-hmac';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI } from '../reference/woo/woo.api';
import { BROKER_ID, ENDPOINTS } from '../reference/woo/woo.types';

const makeExchange = () =>
  ({
    name: 'woo',
    emitter: { emit: vi.fn() },
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
  }) as any;

describe('WOO legacy V1 request signing', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('sends V1 mutation params as a signed form body instead of an empty POST body', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-20T15:01:06.956Z'));

    const adapter = vi.fn().mockResolvedValue({
      data: { success: true, order_id: 123 },
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI({ key: 'key', secret: 'secret' } as ExchangeOptions, makeExchange());
    api.defaults.adapter = adapter;

    await api.post(ENDPOINTS.PLACE_ORDER, {
      symbol: 'PERP_ETH_USDT',
      order_type: 'POST_ONLY',
      order_price: 1800,
      order_quantity: 0.01,
      reduce_only: false,
      side: 'BUY',
    });

    const request = adapter.mock.calls[0][0];
    const expectedBody = [
      `broker_id=${BROKER_ID}`,
      'order_price=1800',
      'order_quantity=0.01',
      'order_type=POST_ONLY',
      'reduce_only=false',
      'side=BUY',
      'symbol=PERP_ETH_USDT',
    ].join('&');
    const timestamp = Date.parse('2026-07-20T15:01:06.956Z');
    const expectedSignature = createHmac('sha256', 'secret').update(`${expectedBody}|${timestamp}`).digest('hex');

    expect(request.url).toBe(ENDPOINTS.PLACE_ORDER);
    expect(request.params).toBeUndefined();
    expect(request.data).toBe(expectedBody);
    expect(request.headers.get('Content-Type')).toBe('application/x-www-form-urlencoded');
    expect(request.headers.get('x-api-timestamp')).toBe(String(timestamp));
    expect(request.headers.get('x-api-signature')).toBe(expectedSignature);
  });
});
