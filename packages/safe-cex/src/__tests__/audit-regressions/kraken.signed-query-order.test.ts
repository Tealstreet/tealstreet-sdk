import type { ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { createAPI } from '../reference/kraken/kraken.api';

/**
 * The signed query and the SENT query must be the same string.
 *
 * Kraken signs `qs.stringify(params, { sort })` — alphabetical — while axios serializes
 * `config.params` in INSERTION order. A `paramsSerializer` did not reliably take effect
 * through this stack (neither the bare-function nor the `{serialize}` object form), so a
 * request whose signature covered `count=…&since=…` went out carrying `since=…&count=…`.
 *
 * Kraken answers that with **HTTP 200** and
 * `{"status":"unauthorized","reason":"You are not authorized to access this endpoint."}`,
 * which reads as a dead credential rather than a serialization bug. It authorized only when
 * a caller happened to declare its params alphabetically — so `history/v3/account-log`
 * returned 500 rows from a probe and `unauthorized` from the income pager with a
 * byte-identical signature, key and path.
 *
 * The interceptor now writes the signed string onto the URL and clears `params`, so the two
 * cannot diverge. This test drives the REAL interceptor through a capture adapter and
 * asserts the outgoing query is sorted whatever order the caller used.
 */

const options = {
  key: 'k'.repeat(56),
  secret: Buffer.from('s'.repeat(64)).toString('base64'),
} as unknown as ExchangeOptions;

const captureQuery = async (params: Record<string, unknown>) => {
  const exchange: any = { name: 'kraken', on: () => {}, emitter: { emit: () => {} } };
  const xhr = createAPI(options, exchange, 'history');

  let sentUrl = '';
  xhr.defaults.adapter = async (config: any) => {
    sentUrl = String(config.url ?? '');
    return { data: { accountUid: 'x', logs: [] }, status: 200, statusText: 'OK', headers: {}, config };
  };

  await xhr.get('/history/v3/account-log', { params });
  return sentUrl;
};

const captureBatchBody = async (body: Record<string, unknown>) => {
  const exchange: any = { name: 'kraken', on: () => {}, emitter: { emit: () => {} } };
  const xhr = createAPI(options, exchange);

  let sentBody = '';
  xhr.defaults.adapter = async (config: any) => {
    sentBody = String(config.data ?? '');
    return { data: { result: 'success', batchStatus: [] }, status: 200, statusText: 'OK', headers: {}, config };
  };

  await xhr.post('/v3/batchorder', body);
  return sentBody;
};

describe('kraken signed query', () => {
  it('sends the SORTED query even when the caller declares params out of order', async () => {
    const sent = await captureQuery({ since: 1784095822483, count: 500 });
    expect(sent).toContain('count=500&since=1784095822483');
  });

  it('produces the same query for either declaration order', async () => {
    const a = await captureQuery({ since: 1784095822483, count: 500 });
    const b = await captureQuery({ count: 500, since: 1784095822483 });
    expect(a).toBe(b);
  });

  it('leaves no `params` for axios to re-serialize behind the signature', async () => {
    // If both survived, axios would append its own serialization on top of the signed
    // query and the request would carry each key twice.
    const sent = await captureQuery({ since: 1, count: 2 });
    expect(sent.match(/count=/g)).toHaveLength(1);
    expect(sent.match(/since=/g)).toHaveLength(1);
  });

  it('sends the same encoded batch json string that the request signs', async () => {
    const sent = await captureBatchBody({
      batchOrder: [{ order: 'cancel', order_id: 'e35d61dd-8a30-4d5f-a574-b5593ef0c050' }],
    });

    expect(sent).toBe(
      'json=%7B%22batchOrder%22%3A%5B%7B%22order%22%3A%22cancel%22%2C%22order_id%22%3A%22e35d61dd-8a30-4d5f-a574-b5593ef0c050%22%7D%5D%7D',
    );
  });
});
