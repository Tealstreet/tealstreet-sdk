import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  buildLighterApproveIntegratorL1Message,
  checkLighterBuilderApproved,
  createLighterL1SignatureHelperUrl,
  prepareLighterBuilderApprovalContext,
} from '../exchanges/lighter/lighter.builder-approval';

const { lighterApiGetMock, lighterApiGetNextNonceMock, lighterApiInstances } = vi.hoisted(() => ({
  lighterApiGetMock: vi.fn(),
  lighterApiGetNextNonceMock: vi.fn(),
  lighterApiInstances: [] as Array<{ addRequestInterceptor: ReturnType<typeof vi.fn> }>,
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {
    addRequestInterceptor = vi.fn();
    get = lighterApiGetMock;
    getNextNonce = lighterApiGetNextNonceMock;

    constructor() {
      lighterApiInstances.push(this);
    }
  },
  TransactionClient: class {},
}));

vi.mock('@tealstreet/poseidon-crypto', () => ({
  getCryptoApi: vi.fn(),
  initWasm: vi.fn(),
  isWasmReady: vi.fn(() => true),
}));

describe('lighter.builder-approval utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lighterApiInstances.length = 0;
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
    expect(lighterApiGetNextNonceMock).toHaveBeenCalledWith(718895n, 2);
    expect(lighterApiInstances).toHaveLength(2);
    for (const instance of lighterApiInstances) {
      expect(instance.addRequestInterceptor).not.toHaveBeenCalled();
    }
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
});
