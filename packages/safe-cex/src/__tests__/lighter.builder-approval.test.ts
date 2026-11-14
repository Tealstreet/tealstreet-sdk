import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE } from '../exchanges/lighter/lighter.account-tier';
import {
  buildLighterApproveIntegratorL1Message,
  checkLighterBuilderApproved,
  createLighterL1SignatureHelperUrl,
  prepareLighterBuilderApprovalContext,
  signLighterApproveIntegratorWithBrowserWallet,
} from '../exchanges/lighter/lighter.builder-approval';

const { lighterApiGetMock, lighterApiGetNextNonceMock, lighterApiInstances, txGetAuthTokenMock } = vi.hoisted(() => ({
  lighterApiGetMock: vi.fn(),
  lighterApiGetNextNonceMock: vi.fn(),
  lighterApiInstances: [] as Array<{
    addRequestInterceptor: ReturnType<typeof vi.fn>;
    config?: Record<string, unknown>;
  }>,
  txGetAuthTokenMock: vi.fn(),
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {
    config?: Record<string, unknown>;
    addRequestInterceptor = vi.fn();
    get = lighterApiGetMock;
    getNextNonce = lighterApiGetNextNonceMock;

    constructor(config?: Record<string, unknown>) {
      this.config = config;
      lighterApiInstances.push(this);
    }
  },
  TransactionClient: class {
    getAuthToken = txGetAuthTokenMock;
  },
}));

vi.mock('@tealstreet/poseidon-crypto', () => ({
  getCryptoApi: vi.fn(),
  initWasm: vi.fn(),
  isWasmReady: vi.fn(() => true),
}));

describe('lighter.builder-approval utilities', () => {
  afterEach(() => vi.unstubAllGlobals());
  beforeEach(() => {
    vi.clearAllMocks();
    lighterApiInstances.length = 0;
    txGetAuthTokenMock.mockResolvedValue('auth-token');
  });

  it('builds the expected L1 message template', () => {
    const message = buildLighterApproveIntegratorL1Message({
      nonce: 919n,
      accountIndex: 718895n,
      apiKeyIndex: 2,
      integratorAccountIndex: 158385n,
      maxPerpsTakerFee: 250,
      maxPerpsMakerFee: 0,
      maxSpotTakerFee: 250,
      maxSpotMakerFee: 0,
      approvalExpiryMs: 1777588355553n,
      chainId: 304,
    });

    expect(message).toContain('Approve Integrator');
    expect(message).toContain('nonce: 0x0000000000000397');
    expect(message).toContain('account index: 0x00000000000af82f');
    expect(message).toContain('integrator account index: 0x0000000000026ab1');
    expect(message).toContain('max perps taker fee: 0x00000000000000fa');
    expect(message).toContain('max perps maker fee: 0x0000000000000000');
    expect(message).toContain('chainId: 0x0000000000000130');
    expect(message).toContain('Only sign this message for a trusted client!');
  });

  it('creates helper URL with all required query params', () => {
    const url = createLighterL1SignatureHelperUrl({
      origin: 'https://preview.tealstreet.io',
      accountIndex: 718895n,
      apiKeyIndex: 2,
      approvalExpiryMs: 1777588355553,
      chainId: 304,
      expectedWalletAddress: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
      integratorAccountIndex: 158385n,
      maxPerpsMakerFee: 0,
      maxPerpsTakerFee: 250,
      maxSpotMakerFee: 0,
      maxSpotTakerFee: 250,
      nonce: 919n,
      testnet: false,
    });

    expect(url).toContain('/trade/lighter-l1sig?');
    expect(url).toContain('nonce=919');
    expect(url).toContain('accountIndex=718895');
    expect(url).toContain('apiKeyIndex=2');
    expect(url).toContain('integratorAccountIndex=158385');
    expect(url).toContain('approvalExpiry=1777588355553');
    expect(url).toContain('chainId=304');
  });

  it('prepares approval context without unsupported Lighter REST auth headers', async () => {
    lighterApiGetNextNonceMock.mockResolvedValueOnce(919n);
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
          approved_integrators: [],
        },
      ],
    });

    const context = await prepareLighterBuilderApprovalContext({
      accountIdentifier: '718895',
      apiKeyIndex: 2,
    });

    expect(context.nonce).toBe(919n);
    expect(context.signerChainId).toBe(304);
    expect(lighterApiGetNextNonceMock).toHaveBeenCalledWith(718895n, 2);
    expect(lighterApiInstances).toHaveLength(2);
    for (const instance of lighterApiInstances) {
      expect(instance.addRequestInterceptor).not.toHaveBeenCalled();
    }
  });

  it('prepares Robinhood approval context with the Robinhood signer chain id', async () => {
    lighterApiGetNextNonceMock.mockResolvedValueOnce(920n);
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
          approved_integrators: [],
        },
      ],
    });

    const context = await prepareLighterBuilderApprovalContext({
      accountIdentifier: '718895',
      apiKeyIndex: 2,
      exchangeName: 'lighter-robinhood',
      integratorAccountIndex: 123456n,
    });

    expect(context.nonce).toBe(920n);
    expect(context.signerChainId).toBe(466324);
    expect(context.integratorAccountIndex).toBe(123456n);
    expect(lighterApiInstances[0]?.config?.baseUrl).toContain('https://api.rh.lighter.xyz');
  });

  it('uses the Robinhood default integrator account index for approval context', async () => {
    lighterApiGetNextNonceMock.mockResolvedValueOnce(921n);
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0xEB0F03ce558333846772efc2C0372f092E60B982',
          approved_integrators: [],
        },
      ],
    });

    const context = await prepareLighterBuilderApprovalContext({
      accountIdentifier: '21972',
      apiKeyIndex: 2,
      exchangeName: 'lighter-robinhood',
    });

    expect(context.nonce).toBe(921n);
    expect(context.signerChainId).toBe(466324);
    expect(context.integratorAccountIndex).toBe(47296n);
    expect(context.l1Address).toBe('0xEB0F03ce558333846772efc2C0372f092E60B982');
    expect(lighterApiInstances[0]?.config?.baseUrl).toContain('https://api.rh.lighter.xyz');
  });

  it('treats an unexpired approval below the default taker fee cap as not approved', async () => {
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
          approved_integrators: [
            {
              account_index: 158385,
              approval_expiry: Date.now() + 60_000,
              max_perps_taker_fee: 200,
              max_perps_maker_fee: 200,
              max_spot_taker_fee: 200,
              max_spot_maker_fee: 200,
            },
          ],
        },
      ],
    });

    const result = await checkLighterBuilderApproved({
      accountIdentifier: '718895',
    });

    expect(result.approved).toBe(false);
    expect(result.approvalStatus).toBe('insufficient_fee_cap');
    expect(result.maxPerpsTakerFee).toBe(200);
  });

  it('reports known Standard accounts as ineligible for non-zero integrator approval', async () => {
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          user_tier_name: 'Standard',
          l1_address: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
          approved_integrators: [
            {
              account_index: 158385,
              approval_expiry: Date.now() + 60_000,
              max_perps_taker_fee: 250,
              max_perps_maker_fee: 0,
              max_spot_taker_fee: 250,
              max_spot_maker_fee: 0,
            },
          ],
        },
      ],
    });

    const result = await checkLighterBuilderApproved({
      accountIdentifier: '718895',
    });

    expect(result.approved).toBe(false);
    expect(result.accountTier).toBe('standard');
    expect(result.approvalStatus).toBe('account_tier_ineligible');
  });

  it('blocks approval preparation for Standard accounts when account limits are available', async () => {
    lighterApiGetMock.mockResolvedValueOnce({
      code: 200,
      data: {
        user_tier_name: 'Standard',
      },
    });

    await expect(
      prepareLighterBuilderApprovalContext({
        accountIdentifier: '718895',
        apiKeyIndex: 2,
        apiPrivateKey: 'lighter-private-key',
      }),
    ).rejects.toThrow(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE);

    expect(lighterApiGetNextNonceMock).not.toHaveBeenCalled();
  });

  it('approves when maker caps are zero and taker caps cover the default fee value', async () => {
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
          approved_integrators: [
            {
              account_index: 158385,
              approval_expiry: Date.now() + 60_000,
              max_perps_taker_fee: 250,
              max_perps_maker_fee: 0,
              max_spot_taker_fee: 250,
              max_spot_maker_fee: 0,
            },
          ],
        },
      ],
    });

    const result = await checkLighterBuilderApproved({
      accountIdentifier: '718895',
    });

    expect(result.approved).toBe(true);
    expect(result.approvalStatus).toBe('approved');
  });

  it('checks Robinhood approval status against the Robinhood default integrator', async () => {
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0xEB0F03ce558333846772efc2C0372f092E60B982',
          approved_integrators: [
            {
              account_index: 47296,
              approval_expiry: Date.now() + 60_000,
              max_perps_taker_fee: 250,
              max_perps_maker_fee: 0,
              max_spot_taker_fee: 250,
              max_spot_maker_fee: 0,
            },
          ],
        },
      ],
    });

    const result = await checkLighterBuilderApproved({
      accountIdentifier: '21972',
      exchangeName: 'lighter-robinhood',
    });

    expect(result.approved).toBe(true);
    expect(result.approvalStatus).toBe('approved');
    expect(result.integratorAccountIndex).toBe(47296n);
    expect(lighterApiInstances[0]?.config?.baseUrl).toContain('https://api.rh.lighter.xyz');
  });

  it('preserves flat requiredFeeValue compatibility for callers that pass it explicitly', async () => {
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: '0x22b270B95599989A3D2A54BdcCbb75e21e772630',
          approved_integrators: [
            {
              account_index: 158385,
              approval_expiry: Date.now() + 60_000,
              max_perps_taker_fee: 300,
              max_perps_maker_fee: 300,
              max_spot_taker_fee: 300,
              max_spot_maker_fee: 300,
            },
          ],
        },
      ],
    });

    const result = await checkLighterBuilderApproved({
      accountIdentifier: '718895',
      requiredFeeValue: 300,
    });

    expect(result.approved).toBe(true);
    expect(result.approvalStatus).toBe('approved');
  });

  it.each([null, 1, 'row', []])(
    'rejects a malformed approval row (%j) rather than reporting an approval result',
    async (row) => {
      lighterApiGetMock.mockResolvedValueOnce({ accounts: [{ approved_integrators: [row] }] });
      await expect(checkLighterBuilderApproved({ accountIdentifier: '718895' })).rejects.toThrow();
    },
  );

  it.each([null, {}, { accounts: [] }, { accounts: [null] }, { accounts: ['account'] }, { accounts: [[]] }])(
    'retains the failed account-lookup path for malformed snapshots (%j)',
    async (payload) => {
      lighterApiGetMock.mockResolvedValueOnce(payload);
      await expect(checkLighterBuilderApproved({ accountIdentifier: '718895' })).rejects.toThrow(
        'Lighter account lookup returned no account',
      );
    },
  );

  it('preserves approval index fallback, numeric conversions, zero defaults and fee units', async () => {
    const expiry = Date.now() + 60_000;
    lighterApiGetMock.mockResolvedValueOnce({
      accounts: [
        {
          l1_address: 'fixture-wallet',
          approved_integrators: [
            { account_index: '-1', max_perps_taker_fee: 999 },
            {
              index: '158385',
              approval_expiry: String(expiry),
              max_perps_taker_fee: '250',
              max_spot_taker_fee: 250,
              max_perps_maker_fee: null,
              max_spot_maker_fee: 'not-a-number',
            },
          ],
        },
      ],
    });
    const result = await checkLighterBuilderApproved({ accountIdentifier: '718895' });
    expect(result).toMatchObject({
      approved: true,
      approvalStatus: 'approved',
      l1Address: 'fixture-wallet',
      approvalExpiryMs: expiry,
      maxPerpsTakerFee: 250,
      maxSpotTakerFee: 250,
      maxPerpsMakerFee: 0,
      maxSpotMakerFee: 0,
    });
  });

  it.each([undefined, null, {}, 'approvals'])(
    'keeps a missing/non-array approval list (%j) as missing',
    async (approvedIntegrators) => {
      lighterApiGetMock.mockResolvedValueOnce({ accounts: [{ approved_integrators: approvedIntegrators }] });
      expect(await checkLighterBuilderApproved({ accountIdentifier: '718895' })).toMatchObject({
        approved: false,
        approvalStatus: 'missing',
      });
    },
  );

  const walletParams = {
    accountIndex: 718895n,
    apiKeyIndex: 2,
    approvalExpiryMs: 1777588355553,
    chainId: 304,
    expectedWalletAddress: '0xWallet',
    integratorAccountIndex: 158385n,
    maxPerpsMakerFee: 0,
    maxPerpsTakerFee: 250,
    maxSpotMakerFee: 0,
    maxSpotTakerFee: 250,
    nonce: 919n,
  };

  it.each([[], null, '0xWallet', [123], [{}]])(
    'uses the no-account failure for malformed wallet results (%j)',
    async (accounts) => {
      const request = vi.fn().mockResolvedValueOnce(accounts);
      vi.stubGlobal('window', { ethereum: { request } });
      const result = await signLighterApproveIntegratorWithBrowserWallet(walletParams);
      expect(result).toEqual({ success: false, message: 'No wallet account returned by provider' });
      expect(request.mock.calls).toEqual([[{ method: 'eth_requestAccounts' }]]);
    },
  );

  it('keeps the first account and refuses the wrong expected wallet without requesting a signature', async () => {
    const request = vi.fn().mockResolvedValueOnce(['0xOther', '0xWallet']);
    vi.stubGlobal('window', { ethereum: { request } });
    expect(await signLighterApproveIntegratorWithBrowserWallet(walletParams)).toEqual({
      success: false,
      message: 'Wrong wallet connected. Expected 0xWallet, got 0xOther',
    });
    expect(request).toHaveBeenCalledTimes(1);
  });

  it('requests the unchanged approval message before the wallet address', async () => {
    const request = vi.fn().mockResolvedValueOnce(['0xWALLET']).mockResolvedValueOnce('0xsignature');
    vi.stubGlobal('window', { ethereum: { request } });
    const result = await signLighterApproveIntegratorWithBrowserWallet(walletParams);
    const message = buildLighterApproveIntegratorL1Message({
      ...walletParams,
      approvalExpiryMs: BigInt(walletParams.approvalExpiryMs),
    });
    expect(result).toMatchObject({ success: true, l1Signature: '0xsignature', signerAddress: '0xWALLET', message });
    expect(request.mock.calls).toEqual([
      [{ method: 'eth_requestAccounts' }],
      [{ method: 'personal_sign', params: [message, '0xWALLET'] }],
    ]);
  });

  it.each([123, 'invalid', null])(
    'retains address-before-message fallback for an invalid first signature (%j)',
    async (first) => {
      const request = vi
        .fn()
        .mockResolvedValueOnce(['0xWallet'])
        .mockResolvedValueOnce(first)
        .mockResolvedValueOnce('0xfallback');
      vi.stubGlobal('window', { ethereum: { request } });
      const result = await signLighterApproveIntegratorWithBrowserWallet(walletParams);
      const message = buildLighterApproveIntegratorL1Message({
        ...walletParams,
        approvalExpiryMs: BigInt(walletParams.approvalExpiryMs),
      });
      expect(result).toMatchObject({ success: true, l1Signature: '0xfallback' });
      expect(request.mock.calls).toEqual([
        [{ method: 'eth_requestAccounts' }],
        [{ method: 'personal_sign', params: [message, '0xWallet'] }],
        [{ method: 'personal_sign', params: ['0xWallet', message] }],
      ]);
    },
  );

  it('keeps invalid-signature and rejected-request failures without retrying a rejected wallet call', async () => {
    const request = vi.fn().mockResolvedValueOnce(['0xWallet']).mockResolvedValueOnce(123).mockResolvedValueOnce({});
    vi.stubGlobal('window', { ethereum: { request } });
    expect(await signLighterApproveIntegratorWithBrowserWallet(walletParams)).toEqual({
      success: false,
      message: 'Wallet returned an invalid signature payload',
    });
    request.mockReset().mockResolvedValueOnce(['0xWallet']).mockRejectedValueOnce(new Error('wallet rejected'));
    expect(await signLighterApproveIntegratorWithBrowserWallet(walletParams)).toEqual({
      success: false,
      message: 'wallet rejected',
    });
    expect(request).toHaveBeenCalledTimes(2);
  });
});
