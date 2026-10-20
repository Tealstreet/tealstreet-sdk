import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../types';

import { AsyncLocalStorage } from 'node:async_hooks';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { setContextualProxyTokenProvider } from '../accessToken';
import { createEngineTransport } from './engine';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';

vi.mock('../index', () => ({ createExchange: vi.fn() }));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => true }));

type ProxyTokenProvider = () => string;

// Unsigned JWTs with a future expiry: the provider only checks expiry.
const jwt = (subject: string) =>
  ['{"alg":"none"}', JSON.stringify({ sub: subject, exp: 4_102_444_800 })]
    .map((part) => Buffer.from(part).toString('base64url'))
    .concat('sig')
    .join('.');
const TOKEN_A = jwt('a');
const TOKEN_B = jwt('b');

// exchange-server binds each socket's proxy token to an AsyncLocalStorage run.
const context = new AsyncLocalStorage<ProxyTokenProvider>();
setContextualProxyTokenProvider(() => context.getStore() ?? null);
afterEach(() => vi.useRealTimers());

const account = (token: string) => {
  const raw = {
    webSocketHeaders: true,
    now: () => 0,
    request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
    connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
    setTimeout: vi.fn<Transport['setTimeout']>(),
    clearTimeout: vi.fn<Transport['clearTimeout']>(),
  };
  const options: ExchangeOptions = {
    key: 'venue-key',
    secret: 'venue-secret',
    transport: tealstreetTransport,
    tealstreetProxy: true,
    tealstreetTunnel: false,
  };
  // The account is created inside its own socket's context, as exchange-server does.
  const transport = context.run(() => token, () => createEngineTransport('coinbase', options, raw));
  const send = () =>
    transport.request({
      method: 'GET',
      url: 'https://api.coinbase.com/api/v3/brokerage/orders/historical/batch',
      headers: { authorization: 'Bearer venue-jwt' },
    });
  const lastProxyAuthorization = () => raw.request.mock.lastCall![0].headers.Authorization;
  return { send, lastProxyAuthorization };
};

describe('engine proxy token context across interleaved accounts', () => {
  it('keeps each account on its own token from timers, foreign contexts and no context', async () => {
    const a = account(TOKEN_A);
    const b = account(TOKEN_B);

    // Engine work fired outside any account context (a shared timer or socket).
    await a.send();
    expect(a.lastProxyAuthorization()).toBe(`Bearer ${TOKEN_A}`);
    await b.send();
    expect(b.lastProxyAuthorization()).toBe(`Bearer ${TOKEN_B}`);

    // Engine work fired from the other account's context (a shared public feed
    // update ingested by A while B's socket handler is running).
    await context.run(() => TOKEN_B, () => a.send());
    expect(a.lastProxyAuthorization()).toBe(`Bearer ${TOKEN_A}`);
    await context.run(() => TOKEN_A, () => b.send());
    expect(b.lastProxyAuthorization()).toBe(`Bearer ${TOKEN_B}`);

    // Interleaved, concurrent requests from timers scheduled in the wrong contexts.
    vi.useFakeTimers();
    const fired: Array<Promise<unknown>> = [];
    context.run(() => TOKEN_B, () => setTimeout(() => fired.push(a.send()), 1));
    context.run(() => TOKEN_A, () => setTimeout(() => fired.push(b.send()), 1));
    await vi.advanceTimersByTimeAsync(1);
    await Promise.all(fired);
    expect(a.lastProxyAuthorization()).toBe(`Bearer ${TOKEN_A}`);
    expect(b.lastProxyAuthorization()).toBe(`Bearer ${TOKEN_B}`);
  });

  it('reads the account provider at request time, so a refreshed token is used', async () => {
    let token = jwt('1');
    const raw = {
      webSocketHeaders: true,
      now: () => 0,
      request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
      connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
      setTimeout: vi.fn<Transport['setTimeout']>(),
      clearTimeout: vi.fn<Transport['clearTimeout']>(),
    };
    const transport = context.run(
      () => token,
      () =>
        createEngineTransport(
          'coinbase',
          { key: 'k', secret: 's', transport: tealstreetTransport, tealstreetProxy: true, tealstreetTunnel: false },
          raw,
        ),
    );
    token = jwt('2');
    await transport.request({ method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/accounts', headers: {} });
    expect(raw.request.mock.lastCall![0].headers.Authorization).toBe(`Bearer ${jwt('2')}`);
  });
});
