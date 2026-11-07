import type { ExchangeOptions } from '../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from './reference/lighter/lighter.exchange';
import { ENDPOINTS } from '../exchanges/lighter/lighter.types';
import { DefaultStore } from '../store/store.base';

const { lighterApiGetMock, lighterApiSendRawTxMock } = vi.hoisted(() => ({
  lighterApiGetMock: vi.fn(),
  lighterApiSendRawTxMock: vi.fn(),
}));

vi.mock('@tealstreet/poseidon-crypto', () => ({
  initWasm: vi.fn(),
  isWasmReady: vi.fn(() => true),
  getCryptoApi: vi.fn(() => ({})),
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {},
  OrderApi: class {},
  CandlestickApi: class {},
}));

vi.mock('./reference/lighter/lighter.api', () => ({
  createLighterSDKClient: vi.fn(() => ({
    get: lighterApiGetMock,
    sendRawTx: lighterApiSendRawTxMock,
  })),
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

describe('LighterExchange account lookup fallbacks', () => {
  const createExchange = (overrides: Partial<ExchangeOptions> = {}): LighterExchange => {
    const options = {
      key: 'lighter-key',
      secret: 'lighter-secret',
      apiKeyIndex: '1',
      applicationId: '123',
      testnet: false,
      ...overrides,
    } as ExchangeOptions;

    return new LighterExchange('lighter', options, new DefaultStore());
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('resolves account index from accountsByL1Address payload', async () => {
    const exchange = createExchange();
    const l1Address = '0x22b270B95599989A3D2A54BdcCbb75e21e772630';
    lighterApiGetMock.mockResolvedValue({
      data: {
        sub_accounts: [{ account_index: '718895', referrer: 'tealstreet' }],
      },
    });

    const result = await (exchange as any).fetchAccountFromL1AddressLookup(l1Address);

    expect(lighterApiGetMock).toHaveBeenCalledWith(ENDPOINTS.ACCOUNTS_BY_L1_ADDRESS, {
      params: { l1_address: l1Address },
    });
    expect(result).toEqual({
      accountIndex: 718895n,
      referrer: 'tealstreet',
    });
  });

  it('throws API message when accountsByL1Address cannot resolve an account', async () => {
    const exchange = createExchange();
    lighterApiGetMock.mockResolvedValue({
      message: 'not found',
    });

    await expect(
      (exchange as any).fetchAccountFromL1AddressLookup('0x22b270B95599989A3D2A54BdcCbb75e21e772630'),
    ).rejects.toThrow('not found');
  });

  it('falls back to accountsByL1Address when account lookup endpoint fails', async () => {
    const l1Address = '0x22b270B95599989A3D2A54BdcCbb75e21e772630';
    const exchange = createExchange({
      applicationId: l1Address,
      key: 'not-an-address',
    });
    const fetchParsedAccountLookupSpy = vi
      .spyOn(exchange as any, 'fetchParsedAccountLookup')
      .mockRejectedValue(new Error('invalid param'));
    const fetchAccountFromL1AddressLookupSpy = vi
      .spyOn(exchange as any, 'fetchAccountFromL1AddressLookup')
      .mockResolvedValue({ accountIndex: 718895n, referrer: 'tealstreet' });

    const result = await (exchange as any).fetchAccountInfo();

    expect(fetchParsedAccountLookupSpy).toHaveBeenCalledWith({
      by: 'l1_address',
      value: l1Address,
    });
    expect(fetchAccountFromL1AddressLookupSpy).toHaveBeenCalledWith(l1Address);
    expect(result).toEqual({
      accountIndex: 718895n,
      referrer: 'tealstreet',
    });
  });
});
