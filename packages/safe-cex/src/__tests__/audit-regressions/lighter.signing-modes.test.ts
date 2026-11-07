import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from '../reference/lighter/lighter.exchange';
import { createLighterSignerAdapter } from '../reference/lighter/lighter.signer';
import { ENDPOINTS } from '../../exchanges/lighter/lighter.types';
import { DefaultStore } from '../../store/store.base';

const mocks = vi.hoisted(() => ({
  get: vi.fn(),
  send: vi.fn(),
  constructSigner: vi.fn(),
  auth: vi.fn(),
  order: vi.fn(),
  loop: vi.fn(),
}));

vi.mock('@tealstreet/poseidon-crypto', () => ({
  initWasm: vi.fn(async () => undefined),
  isWasmReady: vi.fn(() => true),
  getCryptoApi: vi.fn(() => ({})),
}));
vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {},
  OrderApi: class {},
  CandlestickApi: class {},
  TransactionClient: class {
    constructor(options: unknown) {
      mocks.constructSigner(options);
    }
    getAuthToken = mocks.auth;
    getCreateOrderTransaction = mocks.order;
    resetNonce = vi.fn();
  },
}));
vi.mock('../reference/lighter/lighter.api', () => ({
  createLighterSDKClient: () => ({ get: mocks.get, sendRawTx: mocks.send }),
}));
vi.mock('../../utils/loop', () => ({ loop: mocks.loop }));
vi.mock('../reference/lighter/lighter.ws-public', () => ({
  LighterPublicWebsocket: class {
    connectAndSubscribe = vi.fn();
    dispose = vi.fn();
  },
}));
vi.mock('../reference/lighter/lighter.ws-private', () => ({
  LighterPrivateWebsocket: class {
    connectAndSubscribe = vi.fn();
    dispose = vi.fn();
  },
}));

const exchanges: LighterExchange[] = [];
const setup = (name: ExchangeName, overrides: Partial<ExchangeOptions> = {}) => {
  const remoteSigner = {
    sign: vi.fn(),
    isAvailable: vi.fn(() => false),
    onAvailabilityChange: vi.fn(),
  };
  const payloadSigner = vi.fn();
  const options: ExchangeOptions = {
    key: 'public-key',
    secret: '',
    applicationId: '123',
    apiKeyIndex: '0',
    remoteSigner,
    payloadSigner,
    ...overrides,
  };
  const exchange = new LighterExchange(name, options, new DefaultStore());
  exchanges.push(exchange);
  (exchange as any).accountIndex = 123n;
  vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([]);
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { exchange, remoteSigner, payloadSigner };
};
const expectNoDelegation = (
  remoteSigner: ReturnType<typeof setup>['remoteSigner'],
  payloadSigner: ReturnType<typeof vi.fn>,
) => {
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(remoteSigner.isAvailable).not.toHaveBeenCalled();
  expect(remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
};

beforeEach(() => {
  mocks.get.mockResolvedValue({
    code: 200,
    accounts: [{ account_index: 123, total_asset_value: '100', available_balance: '80', positions: [] }],
  });
  mocks.auth.mockResolvedValue('local-token');
  mocks.order.mockResolvedValue({ tx_type: 14, tx_info: 'local-signed-order' });
  mocks.send.mockResolvedValue('local-hash');
});
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

describe.each(['lighter', 'lighter-robinhood'] as const)('%s TS signing-mode contract', (name) => {
  it('reads and polls account snapshots with remoteSigner alone, without delegated auth or transactions', async () => {
    const { exchange, remoteSigner, payloadSigner } = setup(name);
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(exchange.privateWebsocket.connectAndSubscribe).not.toHaveBeenCalled();
    expect(exchange.store.loaded).toMatchObject({ balance: true, positions: true, orders: true });
    expect(exchange.store.balance).toMatchObject({ total: 100, free: 80, used: 20 });
    expect(mocks.get).toHaveBeenCalledTimes(1);
    expect(mocks.get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT, { params: { by: 'index', value: '123' } });
    expect(mocks.loop).toHaveBeenCalledTimes(1);

    exchange.invalidateAccountSnapshotCache();
    await mocks.loop.mock.calls[0][0]();
    expect(mocks.get).toHaveBeenCalledTimes(2);
    expect(mocks.get.mock.calls.every(([endpoint]) => endpoint === ENDPOINTS.ACCOUNT)).toBe(true);
    expect(await exchange.fetchOrders()).toEqual([]);
    expect(await exchange.getPrivateWsAuthToken()).toBeUndefined();
    await expect((exchange as any).ensureSignerAdapter()).rejects.toThrow('Lighter private key is missing');
    expect(mocks.constructSigner).not.toHaveBeenCalled();
    expect(mocks.auth).not.toHaveBeenCalled();
    expect(mocks.send).not.toHaveBeenCalled();
    expectNoDelegation(remoteSigner, payloadSigner);
  });

  it('uses the local SDK signer when remote and payload callbacks accompany a secret', async () => {
    const { exchange, remoteSigner, payloadSigner } = setup(name, { secret: 'local-private-key' });
    await exchange.start();
    expect(exchange.privateWebsocket.connectAndSubscribe).toHaveBeenCalledOnce();
    expect(mocks.constructSigner).toHaveBeenCalledWith(
      expect.objectContaining({
        privateKey: 'local-private-key',
        accountIndex: 123n,
        apiKeyIndex: 0,
        chainId: name === 'lighter' ? 304 : 466324,
      }),
    );
    expect(await exchange.getPrivateWsAuthToken()).toBe('local-token');
    const request = {
      orderId: 1n,
      marketId: 1,
      isBuy: true,
      price: 100n,
      amount: 1n,
      postOnly: false,
      reduceOnly: false,
      orderType: 'limit' as const,
    };
    expect(await exchange.signerAdapter!.createOrder(request)).toBe('local-hash');
    expect(mocks.order).toHaveBeenCalledWith(request);
    expect(mocks.send).toHaveBeenCalledWith({ tx_type: 14, tx_info: 'local-signed-order' });
    expectNoDelegation(remoteSigner, payloadSigner);
  });

  it('starts public with payloadSigner alone despite the inherited canSign getter', async () => {
    const { exchange, remoteSigner, payloadSigner } = setup(name, { remoteSigner: undefined });
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(exchange.store.loaded).toMatchObject({ balance: true, positions: true, orders: true });
    expect(exchange.privateWebsocket.connectAndSubscribe).not.toHaveBeenCalled();
    expect(mocks.get).not.toHaveBeenCalled();
    expect(mocks.loop).not.toHaveBeenCalled();
    expect(mocks.constructSigner).not.toHaveBeenCalled();
    expectNoDelegation(remoteSigner, payloadSigner);
  });

  it.each([{ publicDataOnly: true, secret: 'local-private-key' }, { apiKeyIndex: '' }])(
    'keeps explicit public mode or absent API key index public: %j',
    async (overrides) => {
      const { exchange, remoteSigner, payloadSigner } = setup(name, overrides);
      await exchange.start();
      expect(exchange.privateWebsocket.connectAndSubscribe).not.toHaveBeenCalled();
      expect(mocks.get).not.toHaveBeenCalled();
      expect(mocks.loop).not.toHaveBeenCalled();
      expect(mocks.constructSigner).not.toHaveBeenCalled();
      expectNoDelegation(remoteSigner, payloadSigner);
    },
  );

  it('rejects legacy signer providers while retaining startup account reads', async () => {
    const { exchange, remoteSigner, payloadSigner } = setup(name, {
      secret: 'local-private-key',
      extra: { lighterSignerProvider: 'legacy' } as ExchangeOptions['extra'],
    });
    await expect(
      createLighterSignerAdapter({
        exchangeName: name,
        options: exchange.options,
        apiClient: {} as any,
        accountIndex: 123n,
        apiKeyIndex: 0,
      }),
    ).rejects.toThrow('Unsupported Lighter signer provider "legacy"');
    await exchange.start();
    expect(exchange.privateWebsocket.connectAndSubscribe).not.toHaveBeenCalled();
    expect(exchange.store.loaded.balance).toBe(true);
    expect(mocks.loop).toHaveBeenCalledOnce();
    expect(mocks.constructSigner).not.toHaveBeenCalled();
    expect(mocks.send).not.toHaveBeenCalled();
    expectNoDelegation(remoteSigner, payloadSigner);
  });
});
