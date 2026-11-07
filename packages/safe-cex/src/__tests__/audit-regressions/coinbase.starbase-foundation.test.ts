import { describe, expect, it, vi } from 'vitest';

import {
  COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS,
  COINBASE_STARBASE_ENDPOINTS,
  CoinbaseStarbaseAuthSession,
  CoinbaseStarbaseHeartbeatController,
  CoinbaseStarbaseJsonRpcClient,
  CoinbaseStarbaseProtocolError,
  CoinbaseStarbaseRpcError,
  createCoinbaseStarbaseHttpTransport,
} from '../reference/coinbase/coinbase.starbase';
import {
  STARBASE_AUTH_REQUEST,
  STARBASE_AUTH_RESPONSE,
  STARBASE_ERROR_RESPONSE,
  STARBASE_OK_RESPONSE,
  STARBASE_PUBLIC_TEST_RESPONSE,
  STARBASE_SET_HEARTBEAT_REQUEST,
} from './coinbase.starbase.fixtures';

const INVALID_STARBASE_METHODS = [
  '',
  '/public/test',
  'public//test',
  'public/test/extra',
  'public/../private/test',
  'public/test?x=1',
  'public/test#fragment',
  'PUBLIC/test',
  'public/Test',
] as const;

describe('Coinbase Starbase inert protocol foundation', () => {
  it('pins the official HTTP and unified WebSocket endpoints without changing active routing', () => {
    expect(COINBASE_STARBASE_ENDPOINTS).toEqual({
      http: 'https://drb.coinbase.com/api/v2',
      websocket: 'wss://drb.coinbase.com/ws/api/v2',
    });
  });

  it('builds official JSON-RPC requests with monotonic request IDs', () => {
    const client = new CoinbaseStarbaseJsonRpcClient(vi.fn(), { kind: 'http' });

    expect(client.buildRequest('public/auth', STARBASE_AUTH_REQUEST.params)).toEqual(STARBASE_AUTH_REQUEST);
    expect(client.buildRequest('public/test', {})).toEqual({
      jsonrpc: '2.0',
      id: 2,
      method: 'public/test',
      params: {},
    });
  });

  it('accepts only one exact public/private method path segment during request construction', () => {
    const client = new CoinbaseStarbaseJsonRpcClient(vi.fn(), { kind: 'http' });
    expect(client.buildRequest('private/get_account_summary', {})).toMatchObject({
      method: 'private/get_account_summary',
    });

    for (const method of INVALID_STARBASE_METHODS) {
      expect(() => client.buildRequest(method, {})).toThrow('Invalid Starbase JSON-RPC method');
    }
  });

  it('routes HTTP JSON-RPC through the exact method path and adds bearer auth only when supplied', async () => {
    const send = vi.fn().mockResolvedValue(STARBASE_AUTH_RESPONSE);
    const transport = createCoinbaseStarbaseHttpTransport(send);
    const client = new CoinbaseStarbaseJsonRpcClient(transport, { kind: 'http' });

    await client.request('public/auth', STARBASE_AUTH_REQUEST.params, {
      authorization: 'Bearer cached-access-token',
    });

    expect(send).toHaveBeenCalledWith({
      body: STARBASE_AUTH_REQUEST,
      headers: {
        Authorization: 'Bearer cached-access-token',
        'Content-Type': 'application/json',
      },
      method: 'POST',
      url: 'https://drb.coinbase.com/api/v2/public/auth',
    });
  });

  it('normalizes a trailing endpoint slash before appending the method path', async () => {
    const send = vi.fn().mockResolvedValue(STARBASE_PUBLIC_TEST_RESPONSE);
    const transport = createCoinbaseStarbaseHttpTransport(send);

    await transport(
      { jsonrpc: '2.0', id: 1, method: 'public/test', params: {} },
      { endpoint: 'https://drb.coinbase.com/api/v2/', kind: 'http' },
    );

    expect(send.mock.calls[0]?.[0].url).toBe('https://drb.coinbase.com/api/v2/public/test');
  });

  it('rejects an invalid method before the HTTP sender can construct a URL', async () => {
    const send = vi.fn();
    const transport = createCoinbaseStarbaseHttpTransport(send);

    for (const method of INVALID_STARBASE_METHODS) {
      await expect(
        transport(
          { jsonrpc: '2.0', id: 1, method, params: {} },
          { endpoint: 'https://drb.coinbase.com/api/v2', kind: 'http' },
        ),
      ).rejects.toThrow('Invalid Starbase JSON-RPC method');
    }
    expect(send).not.toHaveBeenCalled();
  });

  it('accepts additive gateway metadata while returning the typed result', async () => {
    const client = new CoinbaseStarbaseJsonRpcClient(
      vi.fn().mockResolvedValue({ ...STARBASE_PUBLIC_TEST_RESPONSE, id: 1 }),
      { kind: 'http' },
    );

    await expect(client.request('public/test', {})).resolves.toEqual({ version: '1.2.26' });
  });

  it('rejects a mismatched response ID instead of resolving the wrong request', async () => {
    const client = new CoinbaseStarbaseJsonRpcClient(
      vi.fn().mockResolvedValue({ ...STARBASE_AUTH_RESPONSE, id: 999 }),
      { kind: 'http' },
    );

    await expect(client.request('public/auth', STARBASE_AUTH_REQUEST.params)).rejects.toThrow(
      CoinbaseStarbaseProtocolError,
    );
  });

  it('maps the gateway error object even when the upstream omits an error request ID', async () => {
    const client = new CoinbaseStarbaseJsonRpcClient(vi.fn().mockResolvedValue(STARBASE_ERROR_RESPONSE), {
      kind: 'http',
    });

    const request = client.request('public/not_a_method', {});
    await expect(request).rejects.toMatchObject({
      code: -32601,
      data: 'Method not found',
      message: 'Method not found',
      method: 'public/not_a_method',
      name: 'CoinbaseStarbaseRpcError',
      requestId: undefined,
    } satisfies Partial<CoinbaseStarbaseRpcError>);
  });

  it('exchanges a fresh CDP JWT and caches HTTP auth only until the documented refresh policy', async () => {
    let now = 1_800_000_000_000;
    const responses = [
      STARBASE_AUTH_RESPONSE,
      {
        ...STARBASE_AUTH_RESPONSE,
        id: 2,
        result: { ...STARBASE_AUTH_RESPONSE.result, access_token: 'refreshed-access-token' },
      },
    ];
    const transport = vi.fn().mockImplementation(async () => responses.shift());
    const client = new CoinbaseStarbaseJsonRpcClient(transport, { kind: 'http' });
    const createCdpJwt = vi.fn().mockResolvedValueOnce('jwt-1').mockResolvedValueOnce('jwt-2');
    const session = new CoinbaseStarbaseAuthSession(client, createCdpJwt, { clock: () => now });

    await expect(session.getHttpAuthorization()).resolves.toBe('Bearer official-schema-access-token-fixture');
    now += COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS.http - 1;
    await expect(session.getHttpAuthorization()).resolves.toBe('Bearer official-schema-access-token-fixture');
    now += 1;
    await expect(session.getHttpAuthorization()).resolves.toBe('Bearer refreshed-access-token');

    expect(createCdpJwt).toHaveBeenCalledTimes(2);
    expect(transport.mock.calls.map(([request]) => request.params)).toEqual([
      { grant_type: 'coinbase_cdp', token: 'jwt-1' },
      { grant_type: 'coinbase_cdp', token: 'jwt-2' },
    ]);
  });

  it('refreshes earlier when the gateway reports a shorter token lifetime', async () => {
    let now = 1000;
    const transport = vi
      .fn()
      .mockResolvedValueOnce({ ...STARBASE_AUTH_RESPONSE, result: { ...STARBASE_AUTH_RESPONSE.result, expires_in: 1 } })
      .mockResolvedValueOnce({ ...STARBASE_AUTH_RESPONSE, id: 2 });
    const client = new CoinbaseStarbaseJsonRpcClient(transport, { kind: 'http' });
    const session = new CoinbaseStarbaseAuthSession(client, vi.fn().mockResolvedValue('fresh-jwt'), {
      clock: () => now,
    });

    await session.authenticate();
    now += 1000;
    await session.authenticate();

    expect(transport).toHaveBeenCalledTimes(2);
  });

  it('configures the documented WebSocket heartbeat and answers classified test requests', async () => {
    const transport = vi
      .fn()
      .mockResolvedValueOnce(STARBASE_OK_RESPONSE)
      .mockResolvedValueOnce(STARBASE_PUBLIC_TEST_RESPONSE);
    const client = new CoinbaseStarbaseJsonRpcClient(transport, { kind: 'websocket' });
    const heartbeat = new CoinbaseStarbaseHeartbeatController(client, 30);

    await heartbeat.enable();
    await expect(heartbeat.handleSignal('heartbeat')).resolves.toBeUndefined();
    await expect(heartbeat.handleSignal('test_request')).resolves.toEqual({ version: '1.2.26' });

    expect(transport.mock.calls).toEqual([
      [STARBASE_SET_HEARTBEAT_REQUEST, { endpoint: COINBASE_STARBASE_ENDPOINTS.websocket, kind: 'websocket' }],
      [
        { jsonrpc: '2.0', id: 2, method: 'public/test', params: {} },
        { endpoint: COINBASE_STARBASE_ENDPOINTS.websocket, kind: 'websocket' },
      ],
    ]);
  });

  it('rejects heartbeat intervals below Coinbase minimum and HTTP heartbeat use', () => {
    const websocket = new CoinbaseStarbaseJsonRpcClient(vi.fn(), { kind: 'websocket' });
    const http = new CoinbaseStarbaseJsonRpcClient(vi.fn(), { kind: 'http' });

    expect(() => new CoinbaseStarbaseHeartbeatController(websocket, 9)).toThrow(CoinbaseStarbaseProtocolError);
    expect(() => new CoinbaseStarbaseHeartbeatController(http, 30)).toThrow(CoinbaseStarbaseProtocolError);
  });
});
