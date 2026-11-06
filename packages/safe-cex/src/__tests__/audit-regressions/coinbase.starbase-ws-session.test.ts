import type { CoinbaseStarbaseJsonRpcRequest } from '../reference/coinbase/coinbase.starbase';
import type { CoinbaseStarbaseCancelOnDisconnectPolicy } from '../reference/coinbase/coinbase.starbase-ws-session';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import {
  COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS,
  CoinbaseStarbaseAuthSession,
  CoinbaseStarbaseHeartbeatController,
  CoinbaseStarbaseJsonRpcClient,
  CoinbaseStarbaseProtocolError,
} from '../reference/coinbase/coinbase.starbase';
import { CoinbaseStarbaseWebsocketSessionController } from '../reference/coinbase/coinbase.starbase-ws-session';

const AUTH_RESULT = {
  access_token: 'starbase-ws-access-token',
  expires_in: 3600,
  scope: 'wallet:user:read wallet:buys:create',
  token_type: 'bearer' as const,
};

const createHarness = (
  policy?: CoinbaseStarbaseCancelOnDisconnectPolicy,
  readback: { enabled: boolean; scope: 'account' | 'connection' } = {
    enabled: policy?.enabled ?? false,
    scope: policy?.scope ?? 'connection',
  },
) => {
  let now = 1_800_000_000_000;
  const requests: CoinbaseStarbaseJsonRpcRequest<Record<string, unknown>>[] = [];
  const transport = vi.fn(async (request: CoinbaseStarbaseJsonRpcRequest<Record<string, unknown>>) => {
    requests.push(request);
    const result = (() => {
      switch (request.method) {
        case 'public/auth':
          return AUTH_RESULT;
        case 'public/set_heartbeat':
        case 'private/disable_cancel_on_disconnect':
        case 'private/enable_cancel_on_disconnect':
          return 'ok';
        case 'private/get_cancel_on_disconnect':
          return readback;
        case 'public/test':
          return { version: '1.2.26' };
        default:
          throw new Error(`Unexpected method: ${request.method}`);
      }
    })();
    return { id: request.id, jsonrpc: '2.0', result };
  });
  const client = new CoinbaseStarbaseJsonRpcClient(transport, { kind: 'websocket' });
  const createCdpJwt = vi.fn().mockResolvedValue('fresh-cdp-jwt');
  const authSession = new CoinbaseStarbaseAuthSession(client, createCdpJwt, { clock: () => now });
  const heartbeat = new CoinbaseStarbaseHeartbeatController(client, 30);
  const replaySubscriptions = vi.fn().mockResolvedValue(undefined);
  const controller = new CoinbaseStarbaseWebsocketSessionController({
    authSession,
    cancelOnDisconnect: policy,
    client,
    heartbeat,
    replaySubscriptions,
  });

  return {
    advanceTime: (milliseconds: number) => {
      now += milliseconds;
    },
    controller,
    createCdpJwt,
    replaySubscriptions,
    requests,
  };
};

describe('Coinbase Starbase inert WebSocket session lifecycle', () => {
  it('authenticates, starts heartbeat, proves CoD policy, and only then replays subscriptions', async () => {
    const harness = createHarness({ enabled: true, scope: 'connection' });

    await harness.controller.activate();

    expect(harness.controller.state).toBe('ready');
    expect(harness.requests.map(({ method, params }) => ({ method, params }))).toEqual([
      {
        method: 'public/auth',
        params: { grant_type: 'coinbase_cdp', token: 'fresh-cdp-jwt' },
      },
      { method: 'public/set_heartbeat', params: { interval: 30 } },
      { method: 'private/enable_cancel_on_disconnect', params: { scope: 'connection' } },
      { method: 'private/get_cancel_on_disconnect', params: { scope: 'connection' } },
    ]);
    expect(harness.replaySubscriptions).toHaveBeenCalledTimes(1);

    // Calling activate again on the same ready connection is deliberately inert.
    await harness.controller.activate();
    expect(harness.createCdpJwt).toHaveBeenCalledTimes(1);
    expect(harness.replaySubscriptions).toHaveBeenCalledTimes(1);
  });

  it('refreshes WebSocket auth by policy and answers only an already-classified test request', async () => {
    const harness = createHarness(undefined);
    await harness.controller.activate();

    harness.advanceTime(COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS.websocket - 1);
    await harness.controller.maintainAuthentication();
    expect(harness.createCdpJwt).toHaveBeenCalledTimes(1);

    harness.advanceTime(1);
    await harness.controller.maintainAuthentication();
    expect(harness.createCdpJwt).toHaveBeenCalledTimes(2);

    await expect(harness.controller.handleClassifiedHeartbeat('heartbeat')).resolves.toBeUndefined();
    await expect(harness.controller.handleClassifiedHeartbeat('test_request')).resolves.toEqual({
      version: '1.2.26',
    });
    expect(harness.requests.map(({ method }) => method)).toEqual([
      'public/auth',
      'public/set_heartbeat',
      'public/auth',
      'public/test',
    ]);
  });

  it('supports an explicit disable policy and verifies its readback before readiness', async () => {
    const harness = createHarness({ enabled: false, scope: 'account' }, { enabled: false, scope: 'account' });

    await harness.controller.activate();

    expect(harness.requests.map(({ method }) => method)).toEqual([
      'public/auth',
      'public/set_heartbeat',
      'private/disable_cancel_on_disconnect',
      'private/get_cancel_on_disconnect',
    ]);
    expect(harness.controller.state).toBe('ready');
  });

  it('fails closed and skips subscription replay when CoD readback disagrees', async () => {
    const harness = createHarness({ enabled: true, scope: 'connection' }, { enabled: false, scope: 'connection' });

    await expect(harness.controller.activate()).rejects.toThrow('readback did not match policy');

    expect(harness.controller.state).toBe('failed');
    expect(harness.replaySubscriptions).not.toHaveBeenCalled();
    await expect(harness.controller.handleClassifiedHeartbeat('test_request')).rejects.toThrow('must be ready');
  });

  it('requires an external credit-aware backoff decision after a classified rate-limit disconnect', async () => {
    const harness = createHarness(undefined);
    await harness.controller.activate();

    harness.controller.recordDisconnect('rate-limit');
    expect(harness.controller.state).toBe('rate-limit-backoff');
    await expect(harness.controller.activate()).rejects.toThrow('backoff must be acknowledged');

    harness.controller.acknowledgeRateLimitBackoff();
    await harness.controller.activate();

    expect(harness.controller.state).toBe('ready');
    expect(harness.createCdpJwt).toHaveBeenCalledTimes(2);
    expect(harness.replaySubscriptions).toHaveBeenCalledTimes(2);
  });

  it('rejects non-WebSocket construction and remains absent from active Coinbase routing', () => {
    const httpClient = new CoinbaseStarbaseJsonRpcClient(vi.fn(), { kind: 'http' });
    const authSession = new CoinbaseStarbaseAuthSession(httpClient, vi.fn());
    expect(
      () =>
        new CoinbaseStarbaseWebsocketSessionController({
          authSession,
          client: httpClient,
          heartbeat: {} as CoinbaseStarbaseHeartbeatController,
          replaySubscriptions: vi.fn(),
        }),
    ).toThrow(CoinbaseStarbaseProtocolError);

    for (const file of [
      '../../__tests__/reference/coinbase/coinbase.api.ts',
      '../../__tests__/reference/coinbase/coinbase.exchange.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-private.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-public.ts',
      '../../createExchangeFromAccount.ts',
      '../../exchangeMetadata.ts',
      '../../transport/tealstreet/routing/routingTable.ts',
    ]) {
      const source = readFileSync(resolve(__dirname, '../../exchanges/coinbase', file), 'utf8');
      expect(source).not.toContain('coinbase.starbase-ws-session');
    }
  });
});
