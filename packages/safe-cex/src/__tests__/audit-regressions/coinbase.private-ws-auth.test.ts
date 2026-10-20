import type { ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../reference/coinbase/coinbase.exchange';
import { CoinbasePrivateWebsocket } from '../reference/coinbase/coinbase.ws-private';
import { BASE_WS_URL } from '../reference/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';

const exchanges: CoinbaseExchange[] = [];

const createSubject = (options: Partial<ExchangeOptions> = {}) => {
  const exchange = new CoinbaseExchange(
    'coinbase' as any,
    {
      key: 'api-key',
      secret: 'api-secret',
      tealstreetProxy: false,
      tealstreetTunnel: false,
      ...options,
    } as ExchangeOptions,
    new DefaultStore(),
  );
  exchanges.push(exchange);
  return { exchange, websocket: new CoinbasePrivateWebsocket(exchange) };
};

afterEach(() => {
  vi.restoreAllMocks();
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
});

describe('Coinbase private WebSocket auth', () => {
  it('uses Coinbase user-order-data WebSocket endpoints for direct private connections', () => {
    expect(BASE_WS_URL.private).toEqual({
      livenet: 'wss://advanced-trade-ws-user.coinbase.com',
      testnet: 'wss://advanced-trade-ws-user.coinbase.com',
    });

    const { websocket } = createSubject();
    const addEventListener = vi.fn();
    const createWebsocket = vi.spyOn(websocket, 'createWebsocket').mockReturnValue({ addEventListener } as any);

    websocket.connectAndSubscribe();

    expect(createWebsocket).toHaveBeenCalledWith(BASE_WS_URL.private.livenet);
    expect(addEventListener).toHaveBeenCalledWith('open', websocket.onOpen);
  });

  it('delegates per-subscription JWT generation to the remote signer in hosted mode', async () => {
    const remoteSigner = {
      accountName: 'hosted-account',
      sign: vi.fn(async ({ id }) => ({
        id,
        wsAuthSignature: id.endsWith('.user') ? 'remote-user-jwt' : 'remote-heartbeats-jwt',
      })),
      isAvailable: () => true,
      onAvailabilityChange: () => () => undefined,
    } satisfies RemoteSigner;
    const { exchange, websocket } = createSubject({ secret: '', remoteSigner });
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);
    const fetchRestSnapshot = vi.spyOn(exchange, 'fetchAndSetPrivateSnapshot').mockResolvedValue(undefined);

    await websocket.onOpen();

    expect(remoteSigner.sign).toHaveBeenCalledTimes(2);
    expect(remoteSigner.sign).toHaveBeenNthCalledWith(1, {
      id: 'coinbase-ws-auth.user',
      type: 'ws-auth',
      exchangeName: 'coinbase',
      accountName: 'hosted-account',
      wsAuth: {
        toSign: 'coinbase-websocket-jwt.user',
        outputFormat: 'coinbase-jwt',
        extraData: { channel: 'user' },
      },
    });
    expect(remoteSigner.sign).toHaveBeenNthCalledWith(2, {
      id: 'coinbase-ws-auth.heartbeats',
      type: 'ws-auth',
      exchangeName: 'coinbase',
      accountName: 'hosted-account',
      wsAuth: {
        toSign: 'coinbase-websocket-jwt.heartbeats',
        outputFormat: 'coinbase-jwt',
        extraData: { channel: 'heartbeats' },
      },
    });
    expect(send.mock.calls.map(([frame]) => JSON.parse(String(frame)))).toEqual([
      { type: 'subscribe', channel: 'user', jwt: 'remote-user-jwt' },
      { type: 'subscribe', channel: 'heartbeats', jwt: 'remote-heartbeats-jwt' },
    ]);
    expect(fetchRestSnapshot).not.toHaveBeenCalled();
  });

  it('refuses to open a private socket when neither a local secret nor remote signer is available', () => {
    const { exchange, websocket } = createSubject({ secret: '' });
    const createWebsocket = vi.spyOn(websocket, 'createWebsocket');
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    websocket.connectAndSubscribe();

    expect(createWebsocket).not.toHaveBeenCalled();
    expect(emitAndLogError.mock.calls[0]?.[0]).toBe(
      'Coinbase private WebSocket requires an API key and signing secret',
    );
  });

  it('surfaces private WebSocket error frames', () => {
    const { exchange, websocket } = createSubject();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    websocket.onMessage({ data: JSON.stringify({ type: 'error', message: 'product_ids required' }) } as MessageEvent);

    expect(emitAndLogError.mock.calls[0]?.[0]).toBe('product_ids required');
  });

  it('marks private WebSocket auth failures as degraded instead of emitting an error toast', () => {
    const { exchange, websocket } = createSubject();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

    websocket.onMessage({ data: JSON.stringify({ type: 'error', message: 'authentication failure' }) } as MessageEvent);

    expect(emitAndLogError).not.toHaveBeenCalled();
    expect(exchange.store.socketHealth?.privateDegradedReason).toContain('Coinbase private WS authentication failed');
    expect(exchange.store.socketHealth?.privateDegradedReason).toContain('authentication failure');
  });

  it('closes but does not force-close when subscribe-time signing fails, so backoff can reconnect', async () => {
    // Regression: a transient remote-signer failure must not set `forceClosed=true`,
    // which would permanently skip reconnect in `base.ws.ts` until an explicit
    // `forceOpen()` call. See `coinbase.ws-private.ts` onOpen catch block.
    const remoteSigner = {
      accountName: 'hosted-account',
      sign: vi.fn(async () => ({ id: 'x', error: 'remote signer offline' })),
      isAvailable: () => true,
      onAvailabilityChange: () => () => undefined,
    } satisfies RemoteSigner;
    const { exchange, websocket } = createSubject({ secret: '', remoteSigner });

    const close = vi.fn();
    (websocket as any).ws = { close };
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    const forceClose = vi.spyOn(websocket, 'forceClose');

    await websocket.onOpen();

    expect(close).toHaveBeenCalledTimes(1);
    expect(forceClose).not.toHaveBeenCalled();
    expect((websocket as any).forceClosed).toBeFalsy();
    expect(emitAndLogError.mock.calls[0]?.[0]).toContain('remote signer offline');
  });
});
