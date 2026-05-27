import { LighterApiClient, TransactionClient } from '@tealstreet/lighter-sdk';
import { getCryptoApi, initWasm, isWasmReady } from '@tealstreet/poseidon-crypto';
import {
  LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE,
  TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET,
  TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET,
} from './lighter.builder-fee';
import { BASE_URL, RECV_WINDOW } from './lighter.types';
import { isLighterL1Address, parseLighterPositiveIntegerString } from './lighter.utils';

const LIGHTER_CHAIN_ID = {
  livenet: 304,
  testnet: 300,
} as const;

const DEFAULT_APPROVAL_WINDOW_MS = 365 * 24 * 60 * 60 * 1000;
const MAX_UINT64 = (1n << 64n) - 1n;

type LighterAccountSnapshot = {
  accountIndex: bigint;
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
  testnet: boolean;
};

export type LighterBuilderApprovalCheckResult = {
  accountIndex: bigint;
  approved: boolean;
  approvalExpiryMs?: number;
  approvalStatus: 'approved' | 'missing' | 'expired' | 'insufficient_fee_cap';
  integratorAccountIndex: bigint;
  l1Address?: string;
  maxPerpsMakerFee?: number;
  maxPerpsTakerFee?: number;
  maxSpotMakerFee?: number;
  maxSpotTakerFee?: number;
};

export const DEFAULT_LIGHTER_APPROVAL_FEE_VALUE = LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE;

export type LighterBuilderApprovalResult = {
  message: string;
  success: boolean;
  txHash?: string;
};

const getBaseUrl = (testnet: boolean, overrideBaseUrl?: string): string => {
  if (overrideBaseUrl && overrideBaseUrl.trim().length > 0) {
    return overrideBaseUrl.trim().replace(/\/$/, '');
  }
  return testnet ? BASE_URL.testnet : BASE_URL.livenet;
};

const shouldAttachLighterAuthHeaders = (url: string): boolean => {
  if (!url) {
    return true;
  }
  return !(url.includes('/public/') || url.includes('/markets') || url.includes('/tickers'));
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

const getDefaultIntegratorAccountIndex = (testnet: boolean): bigint => {
  const raw = testnet
    ? TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET
    : TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET;
  const parsed = parseLighterPositiveIntegerString(raw);
  if (!parsed) {
    throw new Error('Lighter integrator account index is not configured for this network');
  }
  return BigInt(parsed);
};

const createApprovalClient = (opts: {
  accountIndexHeader: string;
  baseUrl?: string;
  testnet: boolean;
}): LighterApiClient => {
  const client = new LighterApiClient({
    baseUrl: getBaseUrl(opts.testnet, opts.baseUrl),
    timeout: RECV_WINDOW,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  client.addRequestInterceptor((requestConfig) => {
    const url = String(requestConfig.url ?? '');
    if (shouldAttachLighterAuthHeaders(url)) {
      requestConfig.headers = {
        ...requestConfig.headers,
        'X-Timestamp': Date.now().toString(),
        'X-Account-Index': opts.accountIndexHeader,
      };
    }
    return requestConfig;
  });

  return client;
};

const resolveAccountIndex = async (params: {
  accountIdentifier: string;
  baseUrl?: string;
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
    baseUrl: getBaseUrl(params.testnet, params.baseUrl),
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
  testnet: boolean;
}): Promise<LighterAccountSnapshot> => {
  const client = createApprovalClient({
    accountIndexHeader: params.accountIndex.toString(),
    baseUrl: params.baseUrl,
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
  testnet: boolean;
}): Promise<bigint> => {
  const client = createApprovalClient({
    accountIndexHeader: params.accountIndex.toString(),
    baseUrl: params.baseUrl,
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
  integratorAccountIndex?: bigint;
  requiredFeeValue?: number;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalCheckResult> => {
  const testnet = Boolean(params.testnet);
  const accountIndex = await resolveAccountIndex({
    accountIdentifier: params.accountIdentifier,
    baseUrl: params.baseUrl,
    testnet,
  });
  const integratorAccountIndex = params.integratorAccountIndex ?? getDefaultIntegratorAccountIndex(testnet);
  const snapshot = await fetchAccountSnapshot({
    accountIndex,
    baseUrl: params.baseUrl,
    testnet,
  });

  const integrator = snapshot.approvedIntegrators.find((item) => item.accountIndex === integratorAccountIndex);
  const now = Date.now();
  const requiredFeeValue = params.requiredFeeValue ?? DEFAULT_LIGHTER_APPROVAL_FEE_VALUE;
  const hasSufficientFeeCap = Boolean(
    integrator &&
      integrator.maxPerpsTakerFee >= requiredFeeValue &&
      integrator.maxPerpsMakerFee >= requiredFeeValue &&
      integrator.maxSpotTakerFee >= requiredFeeValue &&
      integrator.maxSpotMakerFee >= requiredFeeValue,
  );
  const approvalStatus = !integrator
    ? 'missing'
    : integrator.approvalExpiry <= now
      ? 'expired'
      : !hasSufficientFeeCap
        ? 'insufficient_fee_cap'
        : 'approved';
  const approved = approvalStatus === 'approved';

  return {
    accountIndex,
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
  approvalWindowMs?: number;
  baseUrl?: string;
  integratorAccountIndex?: bigint;
  maxFeeValue?: number;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalContext> => {
  const testnet = Boolean(params.testnet);
  const accountIndex = await resolveAccountIndex({
    accountIdentifier: params.accountIdentifier,
    baseUrl: params.baseUrl,
    testnet,
  });
  const integratorAccountIndex = params.integratorAccountIndex ?? getDefaultIntegratorAccountIndex(testnet);
  const maxFeeValue = params.maxFeeValue ?? DEFAULT_LIGHTER_APPROVAL_FEE_VALUE;
  const approvalWindowMs = params.approvalWindowMs ?? DEFAULT_APPROVAL_WINDOW_MS;
  const approvalExpiryMs = Date.now() + approvalWindowMs;
  const nonce = await getNextNonce({
    accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    baseUrl: params.baseUrl,
    testnet,
  });
  const snapshot = await fetchAccountSnapshot({
    accountIndex,
    baseUrl: params.baseUrl,
    testnet,
  });

  return {
    accountIndex,
    apiKeyIndex: params.apiKeyIndex,
    approvalExpiryMs,
    integratorAccountIndex,
    l1Address: snapshot.l1Address,
    maxPerpsMakerFee: maxFeeValue,
    maxPerpsTakerFee: maxFeeValue,
    maxSpotMakerFee: maxFeeValue,
    maxSpotTakerFee: maxFeeValue,
    nonce,
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
    accountIndexHeader: params.accountIndex.toString(),
    baseUrl: params.baseUrl,
    testnet,
  });

  if (!isWasmReady()) {
    await initWasm();
  }

  const txClient = new TransactionClient({
    httpClient: apiClient,
    crypto: getCryptoApi(),
    chainId: testnet ? LIGHTER_CHAIN_ID.testnet : LIGHTER_CHAIN_ID.livenet,
    privateKey: params.apiPrivateKey,
    accountIndex: params.accountIndex,
    apiKeyIndex: params.apiKeyIndex,
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
  integratorAccountIndex?: bigint;
  maxFeeValue?: number;
  testnet?: boolean;
}): Promise<LighterBuilderApprovalResult> => {
  const context = await prepareLighterBuilderApprovalContext({
    accountIdentifier: params.accountIdentifier,
    apiKeyIndex: params.apiKeyIndex,
    approvalWindowMs: params.approvalWindowMs,
    baseUrl: params.baseUrl,
    integratorAccountIndex: params.integratorAccountIndex,
    maxFeeValue: params.maxFeeValue,
    testnet: params.testnet,
  });

  const chainId = context.testnet ? LIGHTER_CHAIN_ID.testnet : LIGHTER_CHAIN_ID.livenet;
  const signatureResult = await signLighterApproveIntegratorWithBrowserWallet({
    accountIndex: context.accountIndex,
    apiKeyIndex: context.apiKeyIndex,
    approvalExpiryMs: context.approvalExpiryMs,
    chainId,
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
