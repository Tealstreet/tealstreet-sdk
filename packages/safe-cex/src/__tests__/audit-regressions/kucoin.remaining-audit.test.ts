import type { Axios } from 'axios';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
import { ENDPOINTS } from '../reference/kucoin/kucoin.types';
import { DefaultStore } from '../../store/store.base';

const createExchange = () =>
  new KucoinExchange(
    'kucoin',
    { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false },
    new DefaultStore(),
  );

const tokenResponse = (token: string) => ({
  data: {
    code: '200000',
    data: { token, instanceServers: [{ endpoint: 'wss://ws-api-futures.kucoin.com/' }] },
  },
});

const socket = () => ({
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
  close: vi.fn(),
});

describe('KuCoin remaining audit regressions', () => {
  afterEach(() => vi.useRealTimers());

  it('debounces account-frame snapshot reconciliation and clears it on dispose', async () => {
    vi.useFakeTimers();
    const exchange = createExchange();
    const fetchSnapshot = vi.spyOn(exchange, 'fetchAndSetPrivateSnapshot').mockResolvedValue(undefined);
    const websocket = exchange.privateWebsocket;

    for (const topic of ['/contract/positionAll', '/contractAccount/wallet', '/contract/crossLeverage']) {
      websocket.onMessage({ data: JSON.stringify({ topic, data: {} }) } as MessageEvent);
    }

    await vi.advanceTimersByTimeAsync(499);
    expect(fetchSnapshot).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(fetchSnapshot).toHaveBeenCalledOnce();

    websocket.onMessage({ data: JSON.stringify({ topic: '/contract/marginMode', data: {} }) } as MessageEvent);
    websocket.dispose();
    await vi.advanceTimersByTimeAsync(500);
    expect(fetchSnapshot).toHaveBeenCalledOnce();
  });

  it.each([
    ['private', ENDPOINTS.GET_PRIVATE_WS_TOKEN],
    ['public', ENDPOINTS.GET_PUBLIC_WS_TOKEN],
  ] as const)('fetches a fresh bullet token for an explicit %s reconnect', async (kind, endpoint) => {
    const exchange = createExchange();
    const firstSocket = socket();
    const secondSocket = socket();
    const post = vi.fn().mockResolvedValueOnce(tokenResponse('first-token')).mockResolvedValueOnce(tokenResponse('second-token'));
    exchange.xhr = { post } as unknown as Axios;
    const websocket = kind === 'private' ? exchange.privateWebsocket : exchange.publicWebsocket;
    const createWebsocket = vi
      .spyOn(websocket, 'createWebsocket')
      .mockReturnValueOnce(firstSocket as any)
      .mockReturnValueOnce(secondSocket as any);

    await websocket.connectAndSubscribe();
    await websocket.reconnect();

    expect(post).toHaveBeenCalledTimes(2);
    expect(post).toHaveBeenNthCalledWith(1, endpoint);
    expect(post).toHaveBeenNthCalledWith(2, endpoint);
    expect(firstSocket.close).toHaveBeenCalledOnce();
    expect(createWebsocket.mock.calls[0][0]).toContain('token=first-token');
    expect(createWebsocket.mock.calls[1][0]).toContain('token=second-token');
  });
});
