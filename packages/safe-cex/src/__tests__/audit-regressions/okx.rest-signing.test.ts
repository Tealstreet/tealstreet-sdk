import type { ExchangeOptions } from '../../types';

import createHmac from 'create-hmac';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI } from '../reference/okx/okx.api';
import { ENDPOINTS } from '../../exchanges/okx/okx.types';
import { virtualClock } from '../../utils/virtualClock';

const options = {
  key: 'key',
  secret: 'secret',
  passphrase: 'passphrase',
  testnet: false,
} as ExchangeOptions;

const exchange = {
  name: 'okx',
  region: 'okx',
  grafanaEndpointsEvery: [],
  grafanaEndpointsPeriodic: [],
  emitter: { emit: () => undefined },
  store: { recordRequestOutcome: () => undefined },
} as any;

const timestamp = '2026-09-15T20:57:15.000Z';
const amendBody = '{"instId":"BTC-USDT-SWAP","ordId":"order-1","newPx":"60000"}';

const getHeader = (headers: any, name: string) => {
  return typeof headers?.get === 'function' ? headers.get(name) : headers?.[name];
};

const expectedSignature = (body: string) =>
  createHmac('sha256', options.secret!)
    .update(`${timestamp}POST${ENDPOINTS.UPDATE_ORDER}${body}`)
    .digest('base64');

const captureAmendRequest = async (body: Record<string, string> | string) => {
  const xhr = createAPI(options, exchange);
  const adapter = vi.fn(async (config) => ({
    data: { code: '0', data: [] },
    status: 200,
    statusText: 'OK',
    headers: {},
    config,
  }));
  xhr.defaults.adapter = adapter as any;

  vi.spyOn(virtualClock, 'getCurrentTime').mockReturnValue({ toISOString: () => timestamp } as any);

  await xhr.post(ENDPOINTS.UPDATE_ORDER, body);

  return adapter.mock.calls[0][0] as any;
};

describe('audit regression: OKX REST signing', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('signs the exact JSON body sent to amend-order', async () => {
    const request = await captureAmendRequest(JSON.parse(amendBody));

    expect(request.data).toBe(amendBody);
    expect(getHeader(request.headers, 'Content-Type')).toBe('application/json');
    expect(getHeader(request.headers, 'OK-ACCESS-SIGN')).toBe(expectedSignature(amendBody));
  });

  it('does not double-stringify a serialized amend body before signing', async () => {
    const request = await captureAmendRequest(amendBody);

    expect(request.data).toBe(amendBody);
    expect(request.data).not.toBe(JSON.stringify(amendBody));
    expect(getHeader(request.headers, 'OK-ACCESS-SIGN')).toBe(expectedSignature(amendBody));
  });

  it('surfaces row-level batch failure messages before generic top-level code errors', async () => {
    const xhr = createAPI(options, exchange);
    xhr.defaults.adapter = vi.fn(async (config) => ({
      data: {
        code: '1',
        data: [{ ordId: '', sCode: '51000', sMsg: 'Order does not exist.' }],
        msg: '',
      },
      status: 200,
      statusText: 'OK',
      headers: {},
      config,
    })) as any;

    await expect(xhr.post(ENDPOINTS.UPDATE_ORDER, JSON.parse(amendBody))).rejects.toThrow(
      'Order does not exist. (code 51000)',
    );
  });
});
