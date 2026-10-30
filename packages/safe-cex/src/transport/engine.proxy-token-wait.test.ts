import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { setProxyToken, setTokenRefreshFunction } from '../accessToken';
import { createEngineTransport } from './engine';
import { PROXY_TOKEN_WAIT_MS, tealstreetTransport } from './tealstreet/tealstreetTransport';

vi.mock('../index', () => ({ createExchange: vi.fn() }));

/**
 * The browser SharedWorker and the CLI answer '' while their proxy token warms.
 * A proxied request sent then carries no Authorization header, and the edge's
 * plain-text 401 reached the engine as Bybit refusing the key.
 */

const jwt = (subject: string) =>
  ['{"alg":"none"}', JSON.stringify({ sub: subject, exp: 4_102_444_800 })]
    .map((part) => Buffer.from(part).toString('base64url'))
    .concat('sig')
    .join('.');
const TOKEN = jwt('user');
const ACCOUNT_INFO = 'https://api.bybit.com/v5/account/info';
const text = (body: string) => new TextEncoder().encode(body);

afterEach(() => {
  vi.useRealTimers();
  setTokenRefreshFunction(null);
  setProxyToken('');
});

const proxied = (respond: () => { status: number; body: Uint8Array } = () => ({ status: 200, body: text('{}') })) => {
  const raw = {
    now: () => 0,
    request: vi.fn<Transport['request']>(async () => respond()),
    connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
    setTimeout: vi.fn<Transport['setTimeout']>(),
    clearTimeout: vi.fn<Transport['clearTimeout']>(),
  };
  const reports: string[] = [];
  const transport = createEngineTransport(
    'bybit',
    { key: 'k', secret: 's', transport: tealstreetTransport, tealstreetProxy: true, tealstreetTunnel: false },
    raw,
    undefined,
    (message) => reports.push(message),
  );
  const send = () => transport.request({ method: 'GET', url: ACCOUNT_INFO, headers: {}, timeoutMs: 60_000 });
  return { raw, reports, send };
};

describe('engine proxy authorization', () => {
  it('waits for a warming token and sends the request once, authorized', async () => {
    vi.useFakeTimers();
    let lookups = 0;
    setTokenRefreshFunction(async () => (++lookups > 3 ? TOKEN : ''));
    const { raw, reports, send } = proxied();

    const response = send();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(raw.request).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1_000);
    await expect(response).resolves.toMatchObject({ status: 200 });

    expect(raw.request).toHaveBeenCalledTimes(1);
    const [sent] = raw.request.mock.calls[0];
    expect(sent.url).toBe(`https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/${ACCOUNT_INFO}`);
    expect(sent.headers.Authorization).toBe(`Bearer ${TOKEN}`);
    expect(reports).toEqual([]);
  });

  it('never sends without a token: after the bounded wait it fails, naming the missing proxy authorization', async () => {
    vi.useFakeTimers();
    setTokenRefreshFunction(async () => '');
    const { raw, reports, send } = proxied();

    const first = send();
    const failed = expect(first).rejects.toThrow(
      `[bybit] Tealstreet proxy authorization missing: no proxy token within ${PROXY_TOKEN_WAIT_MS / 1000}s, so the private request was not sent`,
    );
    await vi.advanceTimersByTimeAsync(PROXY_TOKEN_WAIT_MS + 1_000);
    await failed;
    const second = send();
    const alsoFailed = expect(second).rejects.toThrow('Tealstreet proxy authorization missing');
    await vi.advanceTimersByTimeAsync(PROXY_TOKEN_WAIT_MS + 1_000);
    await alsoFailed;

    expect(raw.request).not.toHaveBeenCalled();
    // One visible error per outage, not one per request.
    expect(reports).toEqual([expect.stringContaining('[bybit] Tealstreet proxy authorization missing')]);
  });

  it('fails the edge refusing our token as a transport error and refreshes the token', async () => {
    const refresh = vi.fn(async () => TOKEN);
    setTokenRefreshFunction(refresh);
    let status = 401;
    const { reports, send } = proxied(() =>
      status === 401 ? { status, body: text('Authorization header is missing') } : { status, body: text('{}') },
    );

    await expect(send()).rejects.toThrow(
      '[bybit] Tealstreet proxy refused our proxy authorization (HTTP 401: Authorization header is missing); refreshing the proxy token and retrying',
    );
    await expect(send()).rejects.toThrow('Tealstreet proxy refused our proxy authorization');
    expect(reports).toHaveLength(1);
    // The refusal asked for a fresh token beyond the per-request lookups.
    expect(refresh.mock.calls.length).toBeGreaterThan(2);

    status = 200;
    await expect(send()).resolves.toMatchObject({ status: 200 });
    status = 401;
    await expect(send()).rejects.toThrow('Tealstreet proxy refused our proxy authorization');
    expect(reports).toHaveLength(2);
  });

  it("passes the venue's own 401 through to the engine with its body", async () => {
    setTokenRefreshFunction(async () => TOKEN);
    const venueRefusal = '{"retCode":10003,"retMsg":"API key is invalid."}';
    const { reports, send } = proxied(() => ({ status: 401, body: text(venueRefusal) }));

    const response = await send();
    expect(response.status).toBe(401);
    expect(new TextDecoder().decode(response.body as Uint8Array)).toBe(venueRefusal);
    expect(reports).toEqual([]);
  });
});
