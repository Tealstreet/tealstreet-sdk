import type { ExchangeName } from '../../types';
import type { LighterAccountTier } from './lighter.account-tier';

import { LighterApiClient, TransactionClient } from '@tealstreet/lighter-sdk';
import { getCryptoApi, initWasm, isWasmReady } from '@tealstreet/poseidon-crypto';
import {
  extractLighterAccountTierFromPayload,
  hasNonZeroLighterIntegratorFee,
  isLighterAccountTierEligibleForPartnerAttribution,
  isLighterPartnerAttributionTierRejection,
  LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE,
} from './lighter.account-tier';
import {
  LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
  LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
  TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET,
  TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD,
  TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET,
} from './lighter.builder-fee';
import {
  getLighterApiBaseUrl,
  isLighterRobinhoodExchange,
  RECV_WINDOW,
  resolveLighterDeploymentProfile,
} from './lighter.types';
import { isLighterL1Address, parseLighterPositiveIntegerString } from './lighter.utils';

const DEFAULT_APPROVAL_WINDOW_MS = 365 * 24 * 60 * 60 * 1000;
const MAX_UINT64 = (1n << 64n) - 1n;

type LighterAccountSnapshot = {
  accountIndex: bigint;
  accountTier?: LighterAccountTier;
  l1Address?: string;
  approvedIntegrators: Array<{
    accountIndex: bigint;
    approvalExpiry: number;
    maxPerpsTakerFee: number;
    maxPerpsMakerFee: number;
    maxSpotTakerFee: number;
    maxSpotMakerFee: number;
  }>;
};

export type LighterBuilderApprovalContext = {
  accountIndex: bigint;
  apiKeyIndex: number;
  approvalExpiryMs: number;
  integratorAccountIndex: bigint;
  l1Address?: string;
  maxPerpsMakerFee: number;
  maxPerpsTakerFee: number;
  maxSpotMakerFee: number;
  maxSpotTakerFee: number;
  nonce: bigint;
  signerChainId: number;
  testnet: boolean;
};

export type LighterBuilderApprovalCheckResult = {
  accountIndex: bigint;
  accountTier?: LighterAccountTier;
  approved: boolean;
  approvalExpiryMs?: number;
  approvalStatus: 'approved' | 'missing' | 'expired' | 'insufficient_fee_cap' | 'account_tier_ineligible';
  integratorAccountIndex: bigint;
  l1Address?: string;
  maxPerpsMakerFee?: number;
  maxPerpsTakerFee?: number;
  maxSpotMakerFee?: number;
  maxSpotTakerFee?: number;
};

export type LighterBuilderApprovalFeeCaps = {
  maxPerpsMakerFee: number;
  maxPerpsTakerFee: number;
  maxSpotMakerFee: number;
  maxSpotTakerFee: number;
};

export const DEFAULT_LIGHTER_APPROVAL_FEE_CAPS: LighterBuilderApprovalFeeCaps = {
  maxPerpsMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
  maxPerpsTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
  maxSpotMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
  maxSpotTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
};

export const DEFAULT_LIGHTER_APPROVAL_FEE_VALUE = LIGHTER_INTEGRATOR_TAKER_FEE_VALUE;

export type LighterBuilderApprovalResult = {
  message: string;
  success: boolean;
  txHash?: string;
};

const getBaseUrl = (params: { exchangeName?: ExchangeName; overrideBaseUrl?: string; testnet: boolean }): string => {
  const overrideBaseUrl = params.overrideBaseUrl;
  if (overrideBaseUrl && overrideBaseUrl.trim().length > 0) {
    return overrideBaseUrl.trim().replace(/\/$/, '');
  }
  return getLighterApiBaseUrl({
    exchangeName: params.exchangeName,
    testnet: params.testnet,
  });
};

const toBigInt = (value: unknown): bigint | undefined => {
  if (value === null || value === undefined) {
    return undefined;
  }
  try {
    return BigInt(String(value));
  } catch {
    return undefined;
  }
};

const toNumber = (value: unknown): number | undefined => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const toHex10 = (value: bigint | number, fieldName: string): string => {
  const normalized = typeof value === 'number' ? BigInt(value) : value;
  if (normalized < 0n) {
    throw new Error(`${fieldName} must be non-negative`);
  }
  if (normalized > MAX_UINT64) {
    throw new Error(`${fieldName} exceeds uint64 range`);
  }
  return `0x${normalized.toString(16).padStart(16, '0')}`;
};

const resolveApprovalFeeCaps = (params: {
  feeCaps?: Partial<LighterBuilderApprovalFeeCaps>;
  feeValue?: number;
}): LighterBuilderApprovalFeeCaps => {
  const baseCaps =
    typeof params.feeValue === 'number' && Number.isFinite(params.feeValue)
      ? {
          maxPerpsMakerFee: params.feeValue,
          maxPerpsTakerFee: params.feeValue,
          maxSpotMakerFee: params.feeValue,
          maxSpotTakerFee: params.feeValue,
        }
      : DEFAULT_LIGHTER_APPROVAL_FEE_CAPS;

  return {
    ...baseCaps,
    ...params.feeCaps,
  };
};

const approvalFeeCapsHaveNonZeroFees = (feeCaps: LighterBuilderApprovalFeeCaps): boolean =>
  hasNonZeroLighterIntegratorFee({
    integratorMakerFee: feeCaps.maxPerpsMakerFee,
    integratorTakerFee: feeCaps.maxPerpsTakerFee,
  }) ||
  hasNonZeroLighterIntegratorFee({
    integratorMakerFee: feeCaps.maxSpotMakerFee,
    integratorTakerFee: feeCaps.maxSpotTakerFee,
  });

const getDefaultIntegratorAccountIndex = (params: { exchangeName?: ExchangeName; testnet: boolean }): bigint => {
  const raw = isLighterRobinhoodExchange(params.exchangeName)
    ? TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD
    : params.testnet
      ? TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET
      : TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET;
  const parsed = parseLighterPositiveIntegerString(raw);
  if (!parsed) {
    throw new Error('Lighter integrator account index is not configured for this network');
  }
  return BigInt(parsed);
};

const createApprovalClient = (opts: { baseUrl?: string; exchangeName?: ExchangeName; testnet: boolean }): LighterApiClient =>
  new LighterApiClient({
    baseUrl: getBaseUrl({
      exchangeName: opts.exchangeName,
      overrideBaseUrl: opts.baseUrl,
      testnet: opts.testnet,
    }),
    timeout: RECV_WINDOW,
    headers: {
      'Content-Type': 'application/json',
    },
  });

const ensureWasmReady = async (): Promise<void> => {
  if (!isWasmReady()) {
    await initWasm();
  }
};

const createApprovalTransactionClient = (params: {
  accountIndex: bigint;
  apiClient: LighterApiClient;
  apiKeyIndex: number;
  apiPrivateKey: string;
  exchangeName?: ExchangeName;
  testnet: boolean;
}): TransactionClient =>
  new TransactionClient({
    httpClient: params.apiClient,
    crypto: getCryptoApi(),
    chainId: resolveLighterDeploymentProfile({
      exchangeName: params.exchangeName,
      testnet: params.testnet,
    }).signerChainId,
    privateKey: params.apiPrivateKey,
    accountIndex: params.accountIndex,
    apiKeyIndex: params.apiKeyIndex,
  });

const fetchAccountLimitsForApproval = async (params: {
  accountIndex: bigint;
  apiKeyIndex: number;
  apiPrivateKey: string;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  testnet: boolean;
}): Promise<Record<string, unknown> | undefined> => {
  await ensureWasmReady();
  const apiClient = createApprovalClient({
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet: params.testnet,
  });
  const txClient = createApprovalTransactionClient({
    accountIndex: params.accountIndex,
    apiClient,
    apiKeyIndex: params.apiKeyIndex,
    apiPrivateKey: params.apiPrivateKey,
    exchangeName: params.exchangeName,
    testnet: params.testnet,
  });
  const deadline = BigInt(Math.floor(Date.now() / 1000) + 60);
  const authToken = await txClient.getAuthToken(deadline);

  return apiClient.get<Record<string, unknown>>('/api/v1/accountLimits', {
    params: {
      account_index: params.accountIndex.toString(),
    },
    headers: {
      authorization: authToken,
    },
  });
};

const assertPartnerAttributionTierEligibleForApproval = async (params: {
  accountIndex: bigint;
  apiKeyIndex?: number;
  apiPrivateKey?: string;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  feeCaps: LighterBuilderApprovalFeeCaps;
  testnet: boolean;
}): Promise<void> => {
  if (!approvalFeeCapsHaveNonZeroFees(params.feeCaps) || params.apiKeyIndex === undefined || !params.apiPrivateKey) {
    return;
  }

  try {
    const accountLimits = await fetchAccountLimitsForApproval({
      accountIndex: params.accountIndex,
      apiKeyIndex: params.apiKeyIndex,
      apiPrivateKey: params.apiPrivateKey,
      baseUrl: params.baseUrl,
      exchangeName: params.exchangeName,
      testnet: params.testnet,
    });
    const tier = extractLighterAccountTierFromPayload(accountLimits);
    if (tier && !isLighterAccountTierEligibleForPartnerAttribution(tier)) {
      throw new Error(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE);
    }
  } catch (error) {
    if (
      error instanceof Error &&
      error.message === LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE
    ) {
      throw error;
    }
    // Account tier lookup is best-effort here; sendTx still normalizes code 21520.
  }
};

const resolveAccountIndex = async (params: {
  accountIdentifier: string;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  testnet: boolean;
}): Promise<bigint> => {
  const trimmed = params.accountIdentifier.trim();
  const numericCandidate = parseLighterPositiveIntegerString(trimmed);
  if (numericCandidate) {
    return BigInt(numericCandidate);
  }

  if (!isLighterL1Address(trimmed)) {
    throw new Error('Lighter account identifier must be a numeric account index or an L1 wallet address');
  }

  // l1_address account-index lookup does not require account headers.
  const lookupClient = new LighterApiClient({
    baseUrl: getBaseUrl({
      exchangeName: params.exchangeName,
      overrideBaseUrl: params.baseUrl,
      testnet: params.testnet,
    }),
    timeout: RECV_WINDOW,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  const payload = await lookupClient.get<Record<string, unknown>>('/api/v1/accountsByL1Address', {
    params: {
      l1_address: trimmed,
    },
  });

  const accounts = [
    ...(Array.isArray(payload?.sub_accounts) ? payload.sub_accounts : []),
    ...(Array.isArray(payload?.accounts) ? payload.accounts : []),
    ...(Array.isArray((payload as any)?.data?.sub_accounts) ? (payload as any).data.sub_accounts : []),
    ...(Array.isArray((payload as any)?.data?.accounts) ? (payload as any).data.accounts : []),
  ] as Array<Record<string, unknown>>;

  const first = accounts[0];
  const resolved = toBigInt(first?.account_index ?? first?.index);
  if (resolved === undefined) {
    throw new Error('Failed to resolve Lighter account index from L1 address');
  }
  return resolved;
};

const fetchAccountSnapshot = async (params: {
  accountIndex: bigint;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  testnet: boolean;
}): Promise<LighterAccountSnapshot> => {
  const client = createApprovalClient({
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet: params.testnet,
  });

  const payload = await client.get<Record<string, unknown>>('/api/v1/account', {
    params: {
      by: 'index',
      value: params.accountIndex.toString(),
    },
  });

  const account = Array.isArray((payload as any)?.accounts) ? (payload as any).accounts[0] : undefined;
  if (!account || typeof account !== 'object') {
    throw new Error('Lighter account lookup returned no account');
  }

  const approvedIntegratorsRaw = Array.isArray((account as any).approved_integrators)
    ? ((account as any).approved_integrators as Array<Record<string, unknown>>)
    : [];

  return {
    accountIndex: params.accountIndex,
    accountTier: extractLighterAccountTierFromPayload(account),
    l1Address: typeof (account as any).l1_address === 'string' ? (account as any).l1_address : undefined,
    approvedIntegrators: approvedIntegratorsRaw
      .map((item) => ({
        accountIndex: toBigInt(item.account_index ?? item.index) ?? -1n,
        approvalExpiry: toNumber(item.approval_expiry) ?? 0,
        maxPerpsTakerFee: toNumber(item.max_perps_taker_fee) ?? 0,
        maxPerpsMakerFee: toNumber(item.max_perps_maker_fee) ?? 0,
        maxSpotTakerFee: toNumber(item.max_spot_taker_fee) ?? 0,
        maxSpotMakerFee: toNumber(item.max_spot_maker_fee) ?? 0,
      }))
      .filter((item) => item.accountIndex >= 0n),
  };
};

const getNextNonce = async (params: {
  accountIndex: bigint;
  apiKeyIndex: number;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  testnet: boolean;
}): Promise<bigint> => {
  const client = createApprovalClient({
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet: params.testnet,
  });
  return client.getNextNonce(params.accountIndex, params.apiKeyIndex);
};

export const buildLighterApproveIntegratorL1Message = (params: {
  nonce: bigint;
  accountIndex: bigint;
  apiKeyIndex: number;
  integratorAccountIndex: bigint;
  maxPerpsTakerFee: number;
  maxPerpsMakerFee: number;
  maxSpotTakerFee: number;
  maxSpotMakerFee: number;
  approvalExpiryMs: bigint;
  chainId: number;
}): string => {
  return (
    'Approve Integrator\n\n' +
    `nonce: ${toHex10(params.nonce, 'nonce')}\n` +
    `account index: ${toHex10(params.accountIndex, 'accountIndex')}\n` +
    `api key index: ${toHex10(params.apiKeyIndex, 'apiKeyIndex')}\n` +
    `integrator account index: ${toHex10(params.integratorAccountIndex, 'integratorAccountIndex')}\n` +
    `max perps taker fee: ${toHex10(params.maxPerpsTakerFee, 'maxPerpsTakerFee')}\n` +
    `max perps maker fee: ${toHex10(params.maxPerpsMakerFee, 'maxPerpsMakerFee')}\n` +
    `max spot taker fee: ${toHex10(params.maxSpotTakerFee, 'maxSpotTakerFee')}\n` +
    `max spot maker fee: ${toHex10(params.maxSpotMakerFee, 'maxSpotMakerFee')}\n` +
    `approval expiry: ${toHex10(params.approvalExpiryMs, 'approvalExpiryMs')}\n` +
    `chainId: ${toHex10(params.chainId, 'chainId')}\n` +
    'Only sign this message for a trusted client!'
  );
};

export const createLighterL1SignatureHelperUrl = (params: {
  accountIndex: bigint;
  apiKeyIndex: number;
  approvalExpiryMs: number;
  chainId: number;
  expectedWalletAddress?: string;
  integratorAccountIndex: bigint;
  maxPerpsMakerFee: number;
  maxPerpsTakerFee: number;
  maxSpotMakerFee: number;
  maxSpotTakerFee: number;
  nonce: bigint;
  origin: string;
  testnet: boolean;
}): string => {
  const query = new URLSearchParams({
    nonce: params.nonce.toString(),
    accountIndex: params.accountIndex.toString(),
    apiKeyIndex: String(params.apiKeyIndex),
    integratorAccountIndex: params.integratorAccountIndex.toString(),
    maxPerpsTakerFee: String(params.maxPerpsTakerFee),
    maxPerpsMakerFee: String(params.maxPerpsMakerFee),
    maxSpotTakerFee: String(params.maxSpotTakerFee),
    maxSpotMakerFee: String(params.maxSpotMakerFee),
    approvalExpiry: String(params.approvalExpiryMs),
    chainId: String(params.chainId),
    testnet: String(params.testnet),
    ...(params.expectedWalletAddress ? { wallet: params.expectedWalletAddress } : {}),
  });
  return `${params.origin}/trade/lighter-l1sig?${query.toString()}`;
};

export const checkLighterBuilderApproved = async (params: {
  accountIdentifier: string;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  integratorAccountIndex?: bigint;
  requiredFeeCaps?: Partial<LighterBuilderApprovalFeeCaps>;
  requiredFeeValue?: number;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalCheckResult> => {
  const testnet = Boolean(params.testnet);
  const accountIndex = await resolveAccountIndex({
    accountIdentifier: params.accountIdentifier,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet,
  });
  const integratorAccountIndex =
    params.integratorAccountIndex ?? getDefaultIntegratorAccountIndex({ exchangeName: params.exchangeName, testnet });
  const snapshot = await fetchAccountSnapshot({
    accountIndex,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet,
  });

  const integrator = snapshot.approvedIntegrators.find((item) => item.accountIndex === integratorAccountIndex);
  const now = Date.now();
  const requiredFeeCaps = resolveApprovalFeeCaps({
    feeCaps: params.requiredFeeCaps,
    feeValue: params.requiredFeeValue,
  });
  const requiresEligibleTier = approvalFeeCapsHaveNonZeroFees(requiredFeeCaps);
  const hasSufficientFeeCap = Boolean(
    integrator &&
    integrator.maxPerpsTakerFee >= requiredFeeCaps.maxPerpsTakerFee &&
    integrator.maxPerpsMakerFee >= requiredFeeCaps.maxPerpsMakerFee &&
    integrator.maxSpotTakerFee >= requiredFeeCaps.maxSpotTakerFee &&
    integrator.maxSpotMakerFee >= requiredFeeCaps.maxSpotMakerFee,
  );
  const hasIneligibleTier =
    requiresEligibleTier &&
    snapshot.accountTier &&
    !isLighterAccountTierEligibleForPartnerAttribution(snapshot.accountTier);
  const approvalStatus = hasIneligibleTier
    ? 'account_tier_ineligible'
    : !integrator
    ? 'missing'
    : integrator.approvalExpiry <= now
      ? 'expired'
      : !hasSufficientFeeCap
        ? 'insufficient_fee_cap'
        : 'approved';
  const approved = approvalStatus === 'approved';

  return {
    accountIndex,
    accountTier: snapshot.accountTier,
    approved,
    approvalStatus,
    approvalExpiryMs: integrator?.approvalExpiry,
    integratorAccountIndex,
    l1Address: snapshot.l1Address,
    maxPerpsMakerFee: integrator?.maxPerpsMakerFee,
    maxPerpsTakerFee: integrator?.maxPerpsTakerFee,
    maxSpotMakerFee: integrator?.maxSpotMakerFee,
    maxSpotTakerFee: integrator?.maxSpotTakerFee,
  };
};

export const prepareLighterBuilderApprovalContext = async (params: {
  accountIdentifier: string;
  apiKeyIndex: number;
  apiPrivateKey?: string;
  approvalWindowMs?: number;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  integratorAccountIndex?: bigint;
  maxFeeCaps?: Partial<LighterBuilderApprovalFeeCaps>;
  maxFeeValue?: number;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalContext> => {
  const testnet = Boolean(params.testnet);
  const accountIndex = await resolveAccountIndex({
    accountIdentifier: params.accountIdentifier,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet,
  });
  const integratorAccountIndex =
    params.integratorAccountIndex ?? getDefaultIntegratorAccountIndex({ exchangeName: params.exchangeName, testnet });
  const maxFeeCaps = resolveApprovalFeeCaps({
    feeCaps: params.maxFeeCaps,
    feeValue: params.maxFeeValue,
  });
  await assertPartnerAttributionTierEligibleForApproval({
    accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    apiPrivateKey: params.apiPrivateKey,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    feeCaps: maxFeeCaps,
    testnet,
  });
  const approvalWindowMs = params.approvalWindowMs ?? DEFAULT_APPROVAL_WINDOW_MS;
  const approvalExpiryMs = Date.now() + approvalWindowMs;
  const nonce = await getNextNonce({
    accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet,
  });
  const snapshot = await fetchAccountSnapshot({
    accountIndex,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet,
  });
  const signerChainId = resolveLighterDeploymentProfile({
    exchangeName: params.exchangeName,
    testnet,
  }).signerChainId;

  return {
    accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    approvalExpiryMs,
    integratorAccountIndex,
    l1Address: snapshot.l1Address,
    maxPerpsMakerFee: maxFeeCaps.maxPerpsMakerFee,
    maxPerpsTakerFee: maxFeeCaps.maxPerpsTakerFee,
    maxSpotMakerFee: maxFeeCaps.maxSpotMakerFee,
    maxSpotTakerFee: maxFeeCaps.maxSpotTakerFee,
    nonce,
    signerChainId,
    testnet,
  };
};

export const signLighterApproveIntegratorWithBrowserWallet = async (params: {
  accountIndex: bigint;
  apiKeyIndex: number;
  approvalExpiryMs: number;
  chainId: number;
  expectedWalletAddress?: string;
  integratorAccountIndex: bigint;
  maxPerpsMakerFee: number;
  maxPerpsTakerFee: number;
  maxSpotMakerFee: number;
  maxSpotTakerFee: number;
  nonce: bigint;
}): Promise<{
  l1Signature?: string;
  message: string;
  signerAddress?: string;
  success: boolean;
  messageText?: string;
}> => {
  if (typeof window === 'undefined' || !window.ethereum) {
    return {
      success: false,
      message: 'No browser wallet detected. Please install MetaMask or another wallet.',
    };
  }

  const message = buildLighterApproveIntegratorL1Message({
    nonce: params.nonce,
    accountIndex: params.accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    integratorAccountIndex: params.integratorAccountIndex,
    maxPerpsTakerFee: params.maxPerpsTakerFee,
    maxPerpsMakerFee: params.maxPerpsMakerFee,
    maxSpotTakerFee: params.maxSpotTakerFee,
    maxSpotMakerFee: params.maxSpotMakerFee,
    approvalExpiryMs: BigInt(params.approvalExpiryMs),
    chainId: params.chainId,
  });

  try {
    const accounts = (await window.ethereum.request({ method: 'eth_requestAccounts' })) as string[];
    const signerAddress = String(accounts?.[0] ?? '');
    if (!signerAddress) {
      return {
        success: false,
        message: 'No wallet account returned by provider',
      };
    }

    if (params.expectedWalletAddress && signerAddress.toLowerCase() !== params.expectedWalletAddress.toLowerCase()) {
      return {
        success: false,
        message: `Wrong wallet connected. Expected ${params.expectedWalletAddress}, got ${signerAddress}`,
      };
    }

    let l1Signature = (await window.ethereum.request({
      method: 'personal_sign',
      params: [message, signerAddress],
    })) as string;

    if (typeof l1Signature !== 'string' || !l1Signature.startsWith('0x')) {
      l1Signature = (await window.ethereum.request({
        method: 'personal_sign',
        params: [signerAddress, message],
      })) as string;
    }

    if (typeof l1Signature !== 'string' || !l1Signature.startsWith('0x')) {
      return {
        success: false,
        message: 'Wallet returned an invalid signature payload',
      };
    }

    return {
      success: true,
      l1Signature,
      signerAddress,
      message,
      messageText: 'L1 signature captured successfully',
    };
  } catch (error) {
    const messageText = error instanceof Error ? error.message : 'L1 signature request failed';
    return {
      success: false,
      message: messageText,
    };
  }
};

export const submitLighterBuilderApprovalWithL1Signature = async (params: {
  accountIndex: bigint;
  apiKeyIndex: number;
  apiPrivateKey: string;
  approvalExpiryMs: number;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  integratorAccountIndex: bigint;
  l1Signature: string;
  maxPerpsMakerFee: number;
  maxPerpsTakerFee: number;
  maxSpotMakerFee: number;
  maxSpotTakerFee: number;
  nonce: bigint;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalResult> => {
  const testnet = Boolean(params.testnet);
  const apiClient = createApprovalClient({
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    testnet,
  });

  await ensureWasmReady();
  await assertPartnerAttributionTierEligibleForApproval({
    accountIndex: params.accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    apiPrivateKey: params.apiPrivateKey,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    feeCaps: {
      maxPerpsMakerFee: params.maxPerpsMakerFee,
      maxPerpsTakerFee: params.maxPerpsTakerFee,
      maxSpotMakerFee: params.maxSpotMakerFee,
      maxSpotTakerFee: params.maxSpotTakerFee,
    },
    testnet,
  });

  const txClient = createApprovalTransactionClient({
    accountIndex: params.accountIndex,
    apiClient,
    apiKeyIndex: params.apiKeyIndex,
    apiPrivateKey: params.apiPrivateKey,
    exchangeName: params.exchangeName,
    testnet,
  });

  try {
    const txInfo = await txClient.getApproveIntegratorTransaction({
      integratorAccountIndex: params.integratorAccountIndex,
      maxPerpsTakerFee: params.maxPerpsTakerFee,
      maxPerpsMakerFee: params.maxPerpsMakerFee,
      maxSpotTakerFee: params.maxSpotTakerFee,
      maxSpotMakerFee: params.maxSpotMakerFee,
      approvalExpiry: BigInt(params.approvalExpiryMs),
      nonce: params.nonce,
      l1Signature: params.l1Signature,
    });
    const txHash = await apiClient.sendRawTx(txInfo);
    return {
      success: true,
      message: 'Lighter builder approval submitted successfully',
      txHash,
    };
  } catch (error) {
    if (
      isLighterPartnerAttributionTierRejection({
        code: (error as any)?.code ?? (error as any)?.response?.data?.code ?? (error as any)?.response?.data?.error?.code,
        message: error instanceof Error ? error.message : String(error),
      })
    ) {
      return {
        success: false,
        message: LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE,
      };
    }
    return {
      success: false,
      message: error instanceof Error ? error.message : 'Failed to submit Lighter builder approval',
    };
  }
};

export const approveLighterBuilderFeeWithBrowserWallet = async (params: {
  accountIdentifier: string;
  apiKeyIndex: number;
  apiPrivateKey: string;
  approvalWindowMs?: number;
  baseUrl?: string;
  exchangeName?: ExchangeName;
  integratorAccountIndex?: bigint;
  maxFeeCaps?: Partial<LighterBuilderApprovalFeeCaps>;
  maxFeeValue?: number;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalResult> => {
  const context = await prepareLighterBuilderApprovalContext({
    accountIdentifier: params.accountIdentifier,
    apiKeyIndex: params.apiKeyIndex,
    apiPrivateKey: params.apiPrivateKey,
    approvalWindowMs: params.approvalWindowMs,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    integratorAccountIndex: params.integratorAccountIndex,
    maxFeeCaps: params.maxFeeCaps,
    maxFeeValue: params.maxFeeValue,
    testnet: params.testnet,
  });

  const signatureResult = await signLighterApproveIntegratorWithBrowserWallet({
    accountIndex: context.accountIndex,
    apiKeyIndex: context.apiKeyIndex,
    approvalExpiryMs: context.approvalExpiryMs,
    chainId: context.signerChainId,
    expectedWalletAddress: context.l1Address,
    integratorAccountIndex: context.integratorAccountIndex,
    maxPerpsMakerFee: context.maxPerpsMakerFee,
    maxPerpsTakerFee: context.maxPerpsTakerFee,
    maxSpotMakerFee: context.maxSpotMakerFee,
    maxSpotTakerFee: context.maxSpotTakerFee,
    nonce: context.nonce,
  });

  if (!signatureResult.success || !signatureResult.l1Signature) {
    return {
      success: false,
      message: signatureResult.message || 'Failed to collect L1 signature',
    };
  }

  return submitLighterBuilderApprovalWithL1Signature({
    accountIndex: context.accountIndex,
    apiKeyIndex: context.apiKeyIndex,
    apiPrivateKey: params.apiPrivateKey,
    approvalExpiryMs: context.approvalExpiryMs,
    baseUrl: params.baseUrl,
    exchangeName: params.exchangeName,
    integratorAccountIndex: context.integratorAccountIndex,
    l1Signature: signatureResult.l1Signature,
    maxPerpsMakerFee: context.maxPerpsMakerFee,
    maxPerpsTakerFee: context.maxPerpsTakerFee,
    maxSpotMakerFee: context.maxSpotMakerFee,
    maxSpotTakerFee: context.maxSpotTakerFee,
    nonce: context.nonce,
    testnet: context.testnet,
  });
};
