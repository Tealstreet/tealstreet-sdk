import type { InternalAxiosRequestConfig } from 'axios';

import { AxiosError } from 'axios';
import createHmac from 'create-hmac';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAPI, isBitrueSignatureRejection } from './reference/bitrue/bitrue.api';
import { virtualClock } from '../utils/virtualClock';

const exchange = { name: 'bitrue', emitter: { emit: () => undefined } } as any;
const keys = { key: 'api-key', secret: 'api-secret' } as any;
const rejected = { code: -1022, msg: 'Signature for this request is not valid.' };
const ok = { orderId: 1 };

/** First attempt rejected (thrown, or resolved the way the HOW tunnel does), then ok. */
const venue = (resolveRejection: boolean) => {
  const sent: InternalAxiosRequestConfig[] = [];
  const adapter = async (config: InternalAxiosRequestConfig) => {
    sent.push(config);
    if (sent.length > 1) return { data: ok, status: 200, statusText: 'OK', headers: {}, config };
    const response = { data: rejected, status: 400, statusText: 'Bad Request', headers: {}, config };
    if (resolveRejection) return response;
    throw new AxiosError('Bad Request', AxiosError.ERR_BAD_REQUEST, config, null, response as any);
  };
  return { sent, adapter };
};

describe('bitrue signature rejection', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('recognises -1022 whether the code is a number or a string', () => {
    expect(isBitrueSignatureRejection(rejected)).toBe(true);
    expect(isBitrueSignatureRejection({ code: '-1022' })).toBe(true);
    expect(isBitrueSignatureRejection({ code: -2010, msg: 'insufficient balance' })).toBe(false);
  });

  it.each([
    ['rejected by axios', false],
    ['resolved over the tunnel', true],
  ])('re-signs and resends a write once when %s', async (_, resolveRejection) => {
    let now = 1_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => (now += 1_000));
    const xhr = createAPI(keys, exchange);
    const { sent, adapter } = venue(resolveRejection);
    xhr.defaults.adapter = adapter as any;

    const response = await xhr.post('/fapi/v2/order', { symbol: 'BTCUSDT' });

    expect(response.data).toEqual(ok);
    expect(sent).toHaveLength(2);
    expect(sent[1].data).toBe(sent[0].data);
    expect(sent[1].headers['X-CH-TS']).not.toBe(sent[0].headers['X-CH-TS']);
    for (const config of sent) {
      const expected = createHmac('sha256', 'api-secret')
        .update(`${config.headers['X-CH-TS']}POST/fapi/v2/order${config.data}`)
        .digest('hex');
      expect(config.headers['X-CH-SIGN']).toBe(expected);
    }
  });
});
