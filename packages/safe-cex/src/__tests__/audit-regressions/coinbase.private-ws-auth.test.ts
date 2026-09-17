import type { ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../../exchanges/coinbase/coinbase.exchange';
import { CoinbasePrivateWebsocket } from '../../exchanges/coinbase/coinbase.ws-private';
import { BASE_WS_URL } from '../../exchanges/coinbase/coinbase.types';
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
    const { websocket } = createSubject({ secret: '', remoteSigner });
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);

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
});
