import type { RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceSpotExchange } from '../reference/binance/binancespot.exchange';
import {
  BINANCE_SPOT_HEARTBEAT_REQUEST_ID,
  BINANCE_SPOT_USER_DATA_REQUEST_ID,
  BinancePrivateWebsocket,
  buildBinanceSpotUserDataSignaturePayload,
} from '../reference/binance/binancespot.ws-private';
import { BASE_WS_URL, ENDPOINTS, USER_DATA_RECV_WINDOW } from '../../exchanges/binance/binancespot.types';
import { DefaultStore } from '../../store/store.base';
import { OrderStatus } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

const exchanges: BinanceSpotExchange[] = [];

const createSubject = (options: Record<string, unknown> = {}) => {
  const store = new DefaultStore();
  const exchange = new BinanceSpotExchange(
    'binancespot',
    { key: 'api-key', secret: 'api-secret', ...options },
    store,
  );
  exchanges.push(exchange);
  return { exchange, store, websocket: new BinancePrivateWebsocket(exchange) };
};

afterEach(() => {
  vi.restoreAllMocks();
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
});

describe('Binance Spot WebSocket API user-data subscription', () => {
  it('uses Binance WebSocket API endpoints and has no retired listen-key endpoint', async () => {
    expect(BASE_WS_URL).toEqual({
      public: {
        livenet: 'wss://stream.binance.com:9443/ws',
        testnet: 'wss://stream.testnet.binance.vision/ws',
      },
      private: {
        livenet: 'wss://ws-api.binance.com:443/ws-api/v3',
        testnet: 'wss://ws-api.testnet.binance.vision/ws-api/v3',
      },
    });
    expect(ENDPOINTS).not.toHaveProperty('LISTEN_KEY');

    for (const testnet of [false, true]) {
      const { websocket } = createSubject({ testnet });
      const addEventListener = vi.fn();
      const createWebsocket = vi
        .spyOn(websocket, 'createWebsocket')
        .mockReturnValue({ addEventListener } as any);

      await websocket.connectAndSubscribe();

      expect(createWebsocket).toHaveBeenCalledWith(
        testnet ? BASE_WS_URL.private.testnet : BASE_WS_URL.private.livenet,
      );
      expect(addEventListener).toHaveBeenCalledWith('open', websocket.onOpen);
    }
  });

  it('constructs and HMAC-signs the alphabetically ordered official params', async () => {
    const timestamp = 1_747_385_641_636;
    const apiKey = 'vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A';
    expect(buildBinanceSpotUserDataSignaturePayload(apiKey, timestamp)).toBe(
      `apiKey=${apiKey}&recvWindow=5000&timestamp=1747385641636`,
    );

    const { websocket } = createSubject({
      key: apiKey,
      secret: 'NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j',
    });
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(timestamp);
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);

    await websocket.onOpen();

    expect(JSON.parse(String(send.mock.calls[0][0]))).toEqual({
      id: BINANCE_SPOT_USER_DATA_REQUEST_ID,
      method: 'userDataStream.subscribe.signature',
      params: {
        apiKey,
        recvWindow: USER_DATA_RECV_WINDOW,
        timestamp,
        signature: '49601ffc5b0122c3c7ee3ea85ef7d6cd76256f5191a57c976418faa32e917653',
      },
    });
  });

  it('re-signs and re-subscribes on every connection open', async () => {
    const { websocket } = createSubject();
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValueOnce(1000).mockReturnValueOnce(2000);
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);

    await websocket.onOpen();
    await websocket.onOpen();

    const requests = send.mock.calls.map(([frame]) => JSON.parse(String(frame)));
    expect(requests).toHaveLength(2);
    expect(requests.map((request) => request.method)).toEqual([
      'userDataStream.subscribe.signature',
      'userDataStream.subscribe.signature',
    ]);
    expect(requests.map((request) => request.params.timestamp)).toEqual([1000, 2000]);
    expect(requests[0].params.signature).not.toBe(requests[1].params.signature);
  });

  it('delegates the exact subscription payload to a remote signer', async () => {
    const remoteSigner = {
      accountName: 'hosted-account',
      sign: vi.fn().mockResolvedValue({ id: BINANCE_SPOT_USER_DATA_REQUEST_ID, wsAuthSignature: 'remote-signature' }),
      isAvailable: () => true,
      onAvailabilityChange: () => () => undefined,
    } satisfies RemoteSigner;
    const { websocket } = createSubject({ secret: '', remoteSigner });
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1234);
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);

    await websocket.onOpen();

    expect(remoteSigner.sign).toHaveBeenCalledWith({
      id: BINANCE_SPOT_USER_DATA_REQUEST_ID,
      type: 'ws-auth',
      exchangeName: 'binancespot',
      accountName: 'hosted-account',
      wsAuth: {
        toSign: 'apiKey=api-key&recvWindow=5000&timestamp=1234',
        outputFormat: 'hex',
        extraData: { apiKey: 'api-key', recvWindow: 5000, timestamp: 1234 },
      },
    });
    expect(JSON.parse(String(send.mock.calls[0][0])).params.signature).toBe('remote-signature');
  });

  it('drops a stale remote-signature result after a newer reconnect attempt', async () => {
    let resolveFirst!: (value: { id: string; wsAuthSignature: string }) => void;
    const remoteSigner = {
      sign: vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<{ id: string; wsAuthSignature: string }>((resolve) => {
              resolveFirst = resolve;
            }),
        )
        .mockResolvedValueOnce({ id: BINANCE_SPOT_USER_DATA_REQUEST_ID, wsAuthSignature: 'new-signature' }),
      isAvailable: () => true,
      onAvailabilityChange: () => () => undefined,
    } satisfies RemoteSigner;
    const { websocket } = createSubject({ secret: '', remoteSigner });
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValueOnce(1000).mockReturnValueOnce(2000);
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);

    const staleOpen = websocket.onOpen();
    await websocket.onOpen();
    resolveFirst({ id: BINANCE_SPOT_USER_DATA_REQUEST_ID, wsAuthSignature: 'stale-signature' });
    await staleOpen;

    expect(send).toHaveBeenCalledOnce();
    expect(JSON.parse(String(send.mock.calls[0][0])).params).toMatchObject({
      timestamp: 2000,
      signature: 'new-signature',
    });
  });

  it('unwraps official WebSocket API event envelopes before applying them', () => {
    const { store, websocket } = createSubject();

    websocket.onMessage({
      data: JSON.stringify({
        subscriptionId: 0,
        event: {
          e: 'executionReport',
          s: 'ETHBTC',
          c: 'wrapped-order',
          S: 'BUY',
          o: 'LIMIT',
          x: 'NEW',
          X: 'NEW',
          q: '1.00000000',
          p: '0.10264410',
          sp: '0.00000000',
          z: '0.00000000',
          l: '0.00000000',
        },
      }),
    } as MessageEvent);

    expect(store.orders).toMatchObject([{ id: 'wrapped-order', status: OrderStatus.Open, symbol: 'ETHBTC' }]);
  });

  it('fails closed when Binance rejects the subscription', () => {
    const { websocket } = createSubject();
    const forceClose = vi.spyOn(websocket, 'forceClose').mockImplementation(() => undefined);
    const emitError = vi.spyOn(websocket, 'emitAndLogError').mockImplementation(() => undefined as any);

    websocket.onMessage({
      data: JSON.stringify({
        id: BINANCE_SPOT_USER_DATA_REQUEST_ID,
        status: 401,
        error: { code: -2015, msg: 'Invalid API-key, IP, or permissions for action.' },
      }),
    } as MessageEvent);

    expect(emitError).toHaveBeenCalledWith(
      'Binance Spot user-data subscription failed: Invalid API-key, IP, or permissions for action.',
    );
    expect(forceClose).toHaveBeenCalledOnce();
  });

  it('uses session.status as heartbeat and reconnects if the subscription disappears', () => {
    const { websocket } = createSubject();
    const send = vi.spyOn(websocket, 'sendWithLog').mockImplementation(() => undefined);
    websocket.ping();
    expect(JSON.parse(String(send.mock.calls[0][0]))).toEqual({
      id: BINANCE_SPOT_HEARTBEAT_REQUEST_ID,
      method: 'session.status',
    });

    const reconnect = vi.spyOn(websocket, 'reconnect').mockResolvedValue(undefined);
    vi.spyOn(websocket, 'emitAndLogError').mockImplementation(() => undefined as any);
    websocket.onMessage({
      data: JSON.stringify({
        id: BINANCE_SPOT_HEARTBEAT_REQUEST_ID,
        status: 200,
        result: { userDataStream: false },
      }),
    } as MessageEvent);

    expect(reconnect).toHaveBeenCalledOnce();
  });
});
