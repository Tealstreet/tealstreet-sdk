import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from './reference/lighter/lighter.exchange';
import { DefaultStore } from '../store/store.base';
import type { ExchangeOptions } from '../types';

const { initWasmMock, isWasmReadyMock, getCryptoApiMock, createLighterSignerAdapterMock } = vi.hoisted(() => ({
  initWasmMock: vi.fn(),
  isWasmReadyMock: vi.fn(() => true),
  getCryptoApiMock: vi.fn(() => ({})),
  createLighterSignerAdapterMock: vi.fn(),
}));

vi.mock('@tealstreet/poseidon-crypto', () => ({
  initWasm: initWasmMock,
  isWasmReady: isWasmReadyMock,
  getCryptoApi: getCryptoApiMock,
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {},
  OrderApi: class {},
  CandlestickApi: class {},
}));

vi.mock('./reference/lighter/lighter.api', () => ({
  createLighterSDKClient: vi.fn(() => ({
    get: vi.fn(),
    sendRawTx: vi.fn(),
  })),
}));

vi.mock('./reference/lighter/lighter.signer', () => ({
  createLighterSignerAdapter: createLighterSignerAdapterMock,
}));

vi.mock('./reference/lighter/lighter.ws-public', () => ({
  LighterPublicWebsocket: class {
    connectAndSubscribe = vi.fn();
    listenOHLCV = vi.fn();
    listenOrderBook = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock('./reference/lighter/lighter.ws-private', () => ({
  LighterPrivateWebsocket: class {
    connectAndSubscribe = vi.fn();
    throttleFetchOrders = vi.fn();
    dispose = vi.fn();
  },
}));

describe('LighterExchange signer retry gate', () => {
  const createExchange = (): LighterExchange => {
    const options = {
      key: 'lighter-key',
      secret: 'lighter-secret',
      apiKeyIndex: '1',
      applicationId: '123',
      testnet: false,
    } as ExchangeOptions;
    const exchange = new LighterExchange('lighter', options, new DefaultStore());
    (exchange as any).ensureAccountIndex = vi.fn().mockResolvedValue(123n);
    return exchange;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    initWasmMock.mockResolvedValue(undefined);
    isWasmReadyMock.mockReturnValue(true);
    getCryptoApiMock.mockReturnValue({});
  });

  it('short-circuits repeated signer init retries while WASM runtime is unavailable', async () => {
    const exchange = createExchange();
    isWasmReadyMock.mockReturnValue(false);
    initWasmMock.mockRejectedValue(new Error('wasm not initialized'));

    await expect((exchange as any).ensureSignerAdapter()).rejects.toThrow('Lighter signer initialization failed');
    expect(initWasmMock).toHaveBeenCalledTimes(1);

    await expect((exchange as any).ensureSignerAdapter()).rejects.toThrow(
      'Lighter signer unavailable; execution is disabled',
    );
    // Second attempt must not re-run wasm init while runtime still unavailable.
    expect(initWasmMock).toHaveBeenCalledTimes(1);
    expect(createLighterSignerAdapterMock).not.toHaveBeenCalled();
  });

  it('allows signer initialization again once WASM runtime is ready', async () => {
    const exchange = createExchange();
    isWasmReadyMock.mockReturnValue(false);
    initWasmMock.mockRejectedValueOnce(new Error('wasm not initialized'));

    await expect((exchange as any).ensureSignerAdapter()).rejects.toThrow('Lighter signer initialization failed');

    const mockSignerAdapter = {
      initialize: vi.fn(),
      createOrder: vi.fn(),
      cancelOrder: vi.fn(),
      updateLeverage: vi.fn(),
      getAuthToken: vi.fn(),
    };
    isWasmReadyMock.mockReturnValue(true);
    initWasmMock.mockResolvedValue(undefined);
    getCryptoApiMock.mockReturnValue({});
    createLighterSignerAdapterMock.mockResolvedValue(mockSignerAdapter);

    await expect((exchange as any).ensureSignerAdapter()).resolves.toBe(mockSignerAdapter);
    expect(createLighterSignerAdapterMock).toHaveBeenCalledTimes(1);
  });
});
