import type { AxiosAdapter, InternalAxiosRequestConfig } from 'axios';

import { ethers } from 'ethers';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import '../../transport/registerTealstreetTransport';

import { setProxyToken } from '../../accessToken';
import {
  buildAsterDEXCodeQueryString,
  createAPI,
  generateAsterDEXCodeMessageSignature,
} from '../../exchanges/asterdex/asterdex.api';
import {
  approveAsterBuilderFeeWithBrowserWallet,
  checkAsterBuilderApproved,
} from '../../exchanges/asterdex/asterdex.builder-approval';
import {
  ASTER_BUILDER_FEE_RATE_CAP,
  DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER,
  DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER,
  getAsterBuilderApprovalFeeRate,
  resolveAsterBuilderFeeConfig,
} from '../../exchanges/asterdex/asterdex.builder-fee';
import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { asterDEXVirtualClock } from '../../exchanges/asterdex/asterdex.virtualClock';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';
import { PRIVATE_PROXY_JP } from '../../transport/tealstreet/transportResolution';

const builderAddress = '0xc2af13e1B1de3A015252A115309A0F9DEEDCFa0A';
const userAddress = '0x1111111111111111111111111111111111111111';
const signerPrivateKey = '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
const signerAddress = new ethers.Wallet(signerPrivateKey).address;

const createExchange = (opts?: {
  apiVersion?: 'v1' | 'v3';
  feeRateMaker?: string;
  feeRateTaker?: string;
  includeBuilderConfig?: boolean;
  marketMaxAmount?: number;
}) => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        symbol: 'BTCUSDT',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { max: opts?.marketMaxAmount ?? 100 } },
        info: { filters: [{ filterType: 'PRICE_FILTER', tickSize: '0.1' }] },
      } as any,
    ],
  });

  const exchange = new AsterDEXExchange(
    'asterdex',
    {
      key: signerAddress,
      secret: signerPrivateKey,
      applicationId: userAddress,
      extra: {
        asterdexApiVersion: opts?.apiVersion ?? 'v3',
        ...(opts?.includeBuilderConfig === false
          ? {}
          : {
              asterBuilderAddress: builderAddress,
              asterBuilderFeeRateMaker: opts?.feeRateMaker ?? '0.00002',
              asterBuilderFeeRateTaker: opts?.feeRateTaker ?? '0.00005',
            }),
      },
    },
    store,
  );
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return exchange;
};

describe('audit regression: Aster builder attribution', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    setProxyToken('');
    delete (window as any).ethereum;
  });

  it('resolves only valid non-zero builder attribution config', () => {
    expect(
      resolveAsterBuilderFeeConfig({
        key: 'key',
        secret: 'secret',
        extra: {
          asterBuilderAddress: builderAddress,
          asterBuilderFeeRateMaker: '0',
          asterBuilderFeeRateTaker: '0',
        },
      }),
    ).toBeUndefined();

    expect(
      resolveAsterBuilderFeeConfig({
        key: 'key',
        secret: 'secret',
        extra: {
          asterBuilderAddress: builderAddress,
        },
      }),
    ).toEqual({
      builderAddress,
      feeRateMaker: DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER,
      feeRateTaker: DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER,
    });

    const resolved = resolveAsterBuilderFeeConfig({
      key: 'key',
      secret: 'secret',
      extra: {
        asterBuilderAddress: builderAddress.toLowerCase(),
        asterBuilderFeeRateMaker: '0.0000100',
        asterBuilderFeeRateTaker: '0.00005',
      },
    });

    expect(resolved).toEqual({
      builderAddress,
      feeRateMaker: '0.00001',
      feeRateTaker: '0.00005',
    });
    expect(getAsterBuilderApprovalFeeRate(resolved!)).toBe('0.00005');
  });

  it.each([
    ['rate above cap', '0.0011'],
    ['NaN', 'NaN'],
    ['negative', '-0.0001'],
    ['scientific notation', '1e-5'],
  ])('rejects invalid fee rate input: %s', (_, feeRate) => {
    expect(() =>
      resolveAsterBuilderFeeConfig({
        key: 'key',
        secret: 'secret',
        extra: {
          asterBuilderAddress: builderAddress,
          asterBuilderFeeRateMaker: '0',
          asterBuilderFeeRateTaker: feeRate,
        },
      }),
    ).toThrow();
  });

  it('rejects invalid and zero builder addresses when attribution rates are enabled', () => {
    expect(() =>
      resolveAsterBuilderFeeConfig({
        key: 'key',
        secret: 'secret',
        extra: {
          asterBuilderAddress: 'not-an-address',
          asterBuilderFeeRateMaker: '0',
          asterBuilderFeeRateTaker: '0.00001',
        },
      }),
    ).toThrow('expected an EVM address');

    expect(() =>
      resolveAsterBuilderFeeConfig({
        key: 'key',
        secret: 'secret',
        extra: {
          asterBuilderAddress: '0x0000000000000000000000000000000000000000',
          asterBuilderFeeRateMaker: '0',
          asterBuilderFeeRateTaker: ASTER_BUILDER_FEE_RATE_CAP,
        },
      }),
    ).toThrow('zero address');
  });

  it('keeps V1 order params unattributed even when builder config exists', async () => {
    const exchange = createExchange({ apiVersion: 'v1' });
    const post = vi.fn().mockResolvedValue({ data: { orderId: 12345 } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 0.25,
    });

    expect(post).toHaveBeenCalledWith('/fapi/v1/order', expect.not.objectContaining({ builder: expect.anything() }));
    expect(post.mock.calls[0][1]).not.toHaveProperty('feeRate');
  });

  it('keeps V3 order params unattributed when builder rates are disabled', async () => {
    const exchange = createExchange({ feeRateMaker: '0', feeRateTaker: '0' });
    const post = vi.fn().mockResolvedValue({ data: { orderId: 12345 } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 0.25,
    });

    expect(post).toHaveBeenCalledWith('/fapi/v3/order', expect.not.objectContaining({ builder: expect.anything() }));
    expect(post.mock.calls[0][1]).not.toHaveProperty('feeRate');
  });

  it('skips V3 attribution when selected maker rate is zero', async () => {
    const exchange = createExchange({ feeRateMaker: '0', feeRateTaker: '0.00005' });
    const post = vi.fn().mockResolvedValue({ data: { orderId: 12345 } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.25,
      price: 50_000,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(post).toHaveBeenCalledWith('/fapi/v3/order', expect.not.objectContaining({ builder: expect.anything() }));
    expect(post.mock.calls[0][1]).not.toHaveProperty('feeRate');
  });

  it('selects taker fee for V3 market orders and maker fee for post-only limits', async () => {
    const exchange = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { orderId: 1 } })
      .mockResolvedValueOnce({ data: { orderId: 2 } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 0.25,
    });
    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 0.25,
      price: 50_000,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(post.mock.calls[0][1]).toMatchObject({ builder: builderAddress, feeRate: '0.00005' });
    expect(post.mock.calls[1][1]).toMatchObject({ builder: builderAddress, feeRate: '0.00002', timeInForce: 'GTX' });
  });

  it('propagates attribution to every split oversized child order', async () => {
    const exchange = createExchange({ marketMaxAmount: 1 });
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { orderId: 1 } })
      .mockResolvedValueOnce({ data: { orderId: 2 } })
      .mockResolvedValueOnce({ data: { orderId: 3 } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      amount: 2.25,
      price: 50_000,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(post).toHaveBeenCalledTimes(3);
    expect(post.mock.calls.map((call) => call[1])).toEqual([
      expect.objectContaining({ builder: builderAddress, feeRate: '0.00002', quantity: '1' }),
      expect.objectContaining({ builder: builderAddress, feeRate: '0.00002', quantity: '1' }),
      expect.objectContaining({ builder: builderAddress, feeRate: '0.00002', quantity: '0.25' }),
    ]);
  });

  it('propagates taker attribution to bracket siblings', async () => {
    const exchange = createExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { data: { orderId: 1 } } })
      .mockResolvedValueOnce({ data: { data: { orderId: 2 } } })
      .mockResolvedValueOnce({ data: { data: { orderId: 3 } } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Market,
      amount: 0.25,
      stopLoss: 49_000,
      takeProfit: 52_000,
    });

    expect(post).toHaveBeenCalledTimes(3);
    expect(post.mock.calls.map((call) => call[1])).toEqual([
      expect.objectContaining({ builder: builderAddress, feeRate: '0.00005', type: 'MARKET' }),
      expect.objectContaining({ builder: builderAddress, feeRate: '0.00005', type: 'STOP_MARKET' }),
      expect.objectContaining({ builder: builderAddress, feeRate: '0.00005', type: 'TAKE_PROFIT_MARKET' }),
    ]);
  });

  it('attributes closePosition stop orders without sending quantity', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { orderId: 12345 } });
    (exchange as any).secondaryAPI.post = post;

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Sell,
      type: OrderType.StopLoss,
      amount: 0,
      triggerPrice: 49_000,
    });

    expect(post.mock.calls[0][1]).toMatchObject({
      builder: builderAddress,
      closePosition: 'true',
      feeRate: '0.00005',
    });
    expect(post.mock.calls[0][1]).not.toHaveProperty('quantity');
  });

  it('signs the exact Aster Code query string that is sent on V3 order URLs', async () => {
    vi.spyOn(asterDEXVirtualClock, 'needsSync').mockReturnValue(false);
    vi.spyOn(asterDEXVirtualClock, 'getCurrentTimeMs').mockReturnValue(1_700_000_000_000);
    const adapter = vi.fn<AxiosAdapter>(async (config: InternalAxiosRequestConfig) => ({
      config,
      data: { data: { orderId: 12345 } },
      headers: {},
      status: 200,
      statusText: 'OK',
    }));
    const api = createAPI(
      { options: { remoteSigner: false }, log: vi.fn() } as any,
      {
        apiKey: signerAddress,
        apiSecret: signerPrivateKey,
        apiVersion: 'v3',
        testnet: true,
        v3UserAddress: userAddress,
        v3SignerAddress: signerAddress,
        v3SignerPrivateKey: signerPrivateKey,
      },
      { key: signerAddress, secret: signerPrivateKey } as any,
    );
    api.defaults.adapter = adapter;

    await api.post('/fapi/v3/order', {
      symbol: 'BTCUSDT',
      side: 'BUY',
      type: 'LIMIT',
      quantity: '0.001',
      price: '50000.1',
      builder: builderAddress,
      feeRate: '0.00001',
    });

    const url = adapter.mock.calls[0][0].url!;
    const queryString = url.split('?')[1].split('&signature=')[0];
    const signature = url.split('&signature=')[1];
    const expectedQueryString = buildAsterDEXCodeQueryString({
      symbol: 'BTCUSDT',
      side: 'BUY',
      type: 'LIMIT',
      quantity: '0.001',
      price: '50000.1',
      builder: builderAddress,
      feeRate: '0.00001',
      asterChain: 'Testnet',
      user: userAddress,
      signer: signerAddress,
      nonce: '1700000000000000',
    });

    expect(queryString).toBe(expectedQueryString);
    expect(queryString).not.toContain('recvWindow');
    expect(queryString).not.toContain('timestamp');
    expect(signature).toBe(generateAsterDEXCodeMessageSignature(queryString, signerPrivateKey, true));
    expect(
      ethers.verifyTypedData(
        {
          name: 'AsterSignTransaction',
          version: '1',
          chainId: 714,
          verifyingContract: '0x0000000000000000000000000000000000000000',
        },
        { Message: [{ name: 'msg', type: 'string' }] },
        { msg: queryString },
        signature,
      ),
    ).toBe(signerAddress);
  });

  it('keeps V3 order nonces strictly increasing for rapid attributed requests', async () => {
    vi.spyOn(asterDEXVirtualClock, 'needsSync').mockReturnValue(false);
    vi.spyOn(asterDEXVirtualClock, 'getCurrentTimeMs').mockReturnValue(1_700_000_000_000);
    const seenUrls: string[] = [];
    const adapter = vi.fn<AxiosAdapter>(async (config: InternalAxiosRequestConfig) => {
      seenUrls.push(config.url!);
      return {
        config,
        data: { data: { orderId: seenUrls.length } },
        headers: {},
        status: 200,
        statusText: 'OK',
      };
    });
    const api = createAPI(
      { options: { remoteSigner: false }, log: vi.fn() } as any,
      {
        apiKey: signerAddress,
        apiSecret: signerPrivateKey,
        apiVersion: 'v3',
        v3UserAddress: userAddress,
        v3SignerAddress: signerAddress,
        v3SignerPrivateKey: signerPrivateKey,
      },
      { key: signerAddress, secret: signerPrivateKey } as any,
    );
    api.defaults.adapter = adapter;

    await api.post('/fapi/v3/order', { symbol: 'BTCUSDT', side: 'BUY', type: 'MARKET', quantity: '0.001' });
    await api.post('/fapi/v3/order', { symbol: 'BTCUSDT', side: 'BUY', type: 'MARKET', quantity: '0.001' });

    const nonces = seenUrls.map((url) => new URLSearchParams(url.split('?')[1]).get('nonce'));
    expect(BigInt(nonces[1]!)).toBeGreaterThan(BigInt(nonces[0]!));
  });

  it('surfaces Aster missing-builder-config errors with actionable API-key guidance', async () => {
    const emitAndLogError = vi.fn();
    const api = createAPI(
      { options: { remoteSigner: false }, emitAndLogError, log: vi.fn() } as any,
      {},
      { key: '', secret: '' } as any,
    );
    api.defaults.adapter = vi.fn<AxiosAdapter>(async (config: InternalAxiosRequestConfig) => ({
      config,
      data: { code: -5025, msg: 'Cannot found builder config.' },
      headers: {},
      status: 200,
      statusText: 'OK',
    }));

    await expect(api.get('/fapi/v3/ping')).rejects.toThrow(
      'Aster builder attribution is not synced for this API key',
    );
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('Open your Aster API key settings'));
  });

  it('surfaces HTTP Aster missing-builder-config errors with actionable API-key guidance', async () => {
    const emitAndLogError = vi.fn();
    const api = createAPI(
      { options: { remoteSigner: false }, emitAndLogError, log: vi.fn() } as any,
      {},
      { key: '', secret: '' } as any,
    );
    api.defaults.adapter = vi.fn<AxiosAdapter>(async (config: InternalAxiosRequestConfig) =>
      Promise.reject(
        Object.assign(new Error('Request failed'), {
          config,
          response: {
            config,
            data: { code: -5025, msg: 'Cannot found builder config.' },
            headers: {},
            status: 400,
            statusText: 'Bad Request',
          },
        }),
      ),
    );

    await expect(api.get('/fapi/v3/ping')).rejects.toThrow(
      'Aster builder attribution is not synced for this API key',
    );
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('reconnect the account and try again'));
  });

  it('distinguishes approved, missing, insufficient cap, and unverified builder status', async () => {
    vi.spyOn(asterDEXVirtualClock, 'getCurrentTimeMs').mockReturnValue(1_700_000_000_000);
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [{ builderAddress, maxFeeRate: '0.00005' }],
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [],
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [{ builderAddress, maxFeeRate: '0.00001' }],
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 503,
        json: async () => ({ msg: 'temporarily unavailable' }),
      });
    vi.stubGlobal('fetch', fetchMock);

    const common = {
      apiKey: signerAddress,
      baseUrl: 'https://fapi.asterdex.com',
      builderAddress,
      requiredFeeRate: '0.00005',
      signerAddress,
      signerPrivateKey,
      userAddress,
    };

    await expect(checkAsterBuilderApproved(common)).resolves.toMatchObject({
      approved: true,
      approvalStatus: 'approved',
      maxFeeRate: '0.00005',
    });
    await expect(checkAsterBuilderApproved(common)).resolves.toMatchObject({
      approved: false,
      approvalStatus: 'missing',
    });
    await expect(checkAsterBuilderApproved(common)).resolves.toMatchObject({
      approved: false,
      approvalStatus: 'insufficient_fee_cap',
      maxFeeRate: '0.00001',
    });
    await expect(checkAsterBuilderApproved(common)).resolves.toMatchObject({
      approved: false,
      approvalStatus: 'unverified',
      error: 'Aster builder approval check failed with HTTP 503',
    });
  });

  it('routes Aster builder approval checks through the Tealstreet proxy when a proxy token is available', async () => {
    vi.spyOn(asterDEXVirtualClock, 'getCurrentTimeMs').mockReturnValue(1_700_000_000_000);
    const proxyToken = 'eyJhbGciOiJub25lIn0.eyJleHAiOjQxMDI0NDQ4MDB9.sig';
    setProxyToken(proxyToken);
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => [],
    });
    vi.stubGlobal('fetch', fetchMock);

    await checkAsterBuilderApproved({
      apiKey: signerAddress,
      builderAddress,
      requiredFeeRate: '0.00005',
      signerAddress,
      signerPrivateKey,
      userAddress,
    });

    const [url, init] = fetchMock.mock.calls[0] as [string, { headers: Record<string, string> }];
    expect(url).toMatch(new RegExp(`^${PRIVATE_PROXY_JP}/https://fapi\\.asterdex\\.com/fapi/v3/builder\\?`));
    expect(init.headers.Authorization).toBe(`Bearer ${proxyToken}`);
    expect(init.headers['X-MBX-APIKEY']).toBe(signerAddress);
  });

  it('signs Aster approveBuilder with EVM signatureChainId and submits through the proxy', async () => {
    vi.spyOn(asterDEXVirtualClock, 'getCurrentTimeMs').mockReturnValue(1_700_000_000_000);
    const proxyToken = 'eyJhbGciOiJub25lIn0.eyJleHAiOjQxMDI0NDQ4MDB9.sig';
    setProxyToken(proxyToken);

    let signedTypedData: any;
    const ethereumRequest = vi.fn(async ({ method, params }: { method: string; params?: unknown[] }) => {
      if (method === 'wallet_switchEthereumChain') {
        expect(params).toEqual([{ chainId: '0x38' }]);
        return null;
      }
      if (method === 'eth_requestAccounts') {
        return [userAddress];
      }
      if (method === 'eth_signTypedData_v4') {
        signedTypedData = JSON.parse(String(params?.[1]));
        return '0xsignature';
      }
      throw new Error(`Unexpected wallet method ${method}`);
    });
    (window as any).ethereum = { request: ethereumRequest };

    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ code: 200 }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await expect(
      approveAsterBuilderFeeWithBrowserWallet({
        builderAddress,
        maxFeeRate: '0.0001',
        userAddress,
      }),
    ).resolves.toMatchObject({ success: true });

    expect(signedTypedData).toMatchObject({
      domain: {
        chainId: 56,
        name: 'AsterSignTransaction',
        verifyingContract: '0x0000000000000000000000000000000000000000',
        version: '1',
      },
      message: {
        AsterChain: 'Mainnet',
        Builder: builderAddress,
        MaxFeeRate: '0.0001',
        User: userAddress,
      },
      primaryType: 'ApproveBuilder',
      types: {
        EIP712Domain: [
          { name: 'name', type: 'string' },
          { name: 'version', type: 'string' },
          { name: 'chainId', type: 'uint256' },
          { name: 'verifyingContract', type: 'address' },
        ],
      },
    });

    const [url, init] = fetchMock.mock.calls[0] as [string, { body: URLSearchParams; headers: Record<string, string> }];
    expect(url).toBe(`${PRIVATE_PROXY_JP}/https://fapi.asterdex.com/fapi/v3/approveBuilder`);
    expect(init.headers.Authorization).toBe(`Bearer ${proxyToken}`);
    expect(init.body.get('signatureChainId')).toBe('56');
    expect(init.body.get('signature')).toBe('0xsignature');
  });
});
