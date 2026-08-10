import { describe, expect, it } from 'vitest';

import { createAPI } from '../../exchanges/blofin/blofin.api';
import { BlofinExchange } from '../../exchanges/blofin/blofin.exchange';
import { DefaultStore } from '../../store/store.base';

const tickInterval = (exchange: BlofinExchange) => (exchange as any).getTickInterval() as number;

const buildExchange = () =>
  new BlofinExchange('blofin' as any, { key: 'k', secret: 's', passphrase: 'p' } as any, new DefaultStore());

const runResponseInterceptor = (exchange: BlofinExchange, response: any) => {
  const xhr = createAPI({ key: 'k', secret: 's', passphrase: 'p' } as any, exchange);
  const handlers = (xhr.interceptors.response as any).handlers.filter(Boolean);
  const fulfilled = handlers[handlers.length - 1].fulfilled;
  return fulfilled(response);
};

const signed = (code: string) => ({
  config: { headers: { 'ACCESS-SIGN': 'sig' } },
  data: { code, msg: code === '0' ? 'success' : 'error' },
});

const unsigned = (code: string) => ({ config: { headers: {} }, data: { code } });

describe('audit regression: Blofin auth failure backoff', () => {
  it('backs the tick interval off while the key is rejected', () => {
    // A dead key used to be polled at the full 10s tick forever; the e2e bound
    // caught it as 132 HTTP requests against a limit of 100.
    const exchange = buildExchange();
    const base = tickInterval(exchange);

    runResponseInterceptor(exchange, signed('152401'));
    const afterOne = tickInterval(exchange);

    runResponseInterceptor(exchange, signed('152401'));
    const afterTwo = tickInterval(exchange);

    expect(afterOne).toBeGreaterThan(base);
    expect(afterTwo).toBeGreaterThan(afterOne);
  });

  it('treats an IP whitelist rejection as an auth failure too', () => {
    const exchange = buildExchange();
    const base = tickInterval(exchange);

    runResponseInterceptor(exchange, signed('152406'));

    expect(tickInterval(exchange)).toBeGreaterThan(base);
  });

  it('caps the backoff rather than growing without bound', () => {
    const exchange = buildExchange();

    for (let i = 0; i < 40; i++) runResponseInterceptor(exchange, signed('152401'));

    expect(tickInterval(exchange)).toBeLessThanOrEqual(60_000);
  });

  it('clears the streak only on a signed success', () => {
    const exchange = buildExchange();
    runResponseInterceptor(exchange, signed('152401'));
    const backedOff = tickInterval(exchange);

    // A public endpoint returns code 0 with no credentials involved — it must
    // not be read as proof the key works.
    runResponseInterceptor(exchange, unsigned('0'));
    expect(tickInterval(exchange)).toBe(backedOff);

    runResponseInterceptor(exchange, signed('0'));
    expect(tickInterval(exchange)).toBe(10_000);
  });

  it('ignores unrelated API errors', () => {
    // 400 Bad Request from set-position-mode is a real rejection, but it is not
    // a credential problem and must not slow the tick loop.
    const exchange = buildExchange();

    runResponseInterceptor(exchange, signed('400'));

    expect(tickInterval(exchange)).toBe(10_000);
  });
});
