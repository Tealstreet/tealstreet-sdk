import type { AsterBuilderFeeConfig } from './asterdex.builder-fee';

import { ethers } from 'ethers';

import { getTransport } from '../../transport/registry';
import {
  buildAsterDEXCodeQueryString,
  buildAsterDEXV3Nonce,
  generateAsterDEXCodeMessageSignature,
  getAsterDEXAsterChain,
} from './asterdex.api';
import { getAsterBuilderApprovalFeeRate, resolveAsterBuilderFeeConfig } from './asterdex.builder-fee';
import { ASTERDEX_ENDPOINTS, isAsterDEXAddress, isAsterDEXV3PrivateKey } from './asterdex.types';

export type AsterBuilderApprovalStatus = 'approved' | 'missing' | 'insufficient_fee_cap' | 'unverified';

export type AsterBuilderApprovalCheckResult = {
  approved: boolean;
  approvalStatus: AsterBuilderApprovalStatus;
  builderAddress: string;
  error?: string;
  maxFeeRate?: string;
};

export type AsterBuilderApprovalResult = {
  message: string;
  success: boolean;
};

type AsterBuilderRow = {
  builderAddress?: unknown;
  builder?: unknown;
  maxFeeRate?: unknown;
};

const ASTER_CODE_ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';
const ASTER_EVM_SIGNATURE_CHAIN_ID = 56;
const ASTER_EVM_SIGNATURE_CHAIN_ID_HEX = '0x38';
const ASTER_EVM_SIGNATURE_CHAIN_PARAMS = {
  chainId: ASTER_EVM_SIGNATURE_CHAIN_ID_HEX,
  chainName: 'BNB Smart Chain',
  nativeCurrency: {
    name: 'BNB',
    symbol: 'BNB',
    decimals: 18,
  },
  rpcUrls: ['https://bsc-dataseed.binance.org'],
  blockExplorerUrls: ['https://bscscan.com'],
};

const getBaseUrl = (testnet?: boolean, baseUrl?: string): string => {
  if (baseUrl && baseUrl.trim()) {
    return baseUrl.trim().replace(/\/$/, '');
  }
  return testnet ? ASTERDEX_ENDPOINTS.REST.TESTNET : ASTERDEX_ENDPOINTS.REST.PRODUCTION;
};

const buildAsterApprovalRequest = async (
  targetUrl: string,
  baseUrl?: string,
): Promise<{ headers: Record<string, string>; url: string }> => {
  if (baseUrl) {
    return { headers: {}, url: targetUrl };
  }

  // No transport proxy, or no credentials for it yet: go to the venue directly.
  const proxy = getTransport({}).privateProxy('asterdex');
  const headers = proxy ? await proxy.resolveHeaders() : {};
  if (!proxy || Object.keys(headers).length === 0) {
    return { headers: {}, url: targetUrl };
  }

  return { headers, url: `${proxy.url}/${targetUrl}` };
};

const normalizeAddress = (value: string, fieldName: string): string => {
  if (!isAsterDEXAddress(value)) {
    throw new Error(`${fieldName} must be an EVM address`);
  }
  const address = ethers.getAddress(value);
  if (address === ASTER_CODE_ZERO_ADDRESS) {
    throw new Error(`${fieldName} cannot be the zero address`);
  }
  return address;
};

const extractBuilderRows = (payload: unknown): AsterBuilderRow[] => {
  if (Array.isArray(payload)) {
    return payload as AsterBuilderRow[];
  }
  const data = (payload as { data?: unknown } | undefined)?.data;
  return Array.isArray(data) ? (data as AsterBuilderRow[]) : [];
};

const parseFeeRate = (value: unknown): number | undefined => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const getProviderErrorCode = (error: unknown): number | undefined =>
  typeof error === 'object' && error !== null && 'code' in error && typeof error.code === 'number'
    ? error.code
    : undefined;

const getProviderErrorMessage = (error: unknown, fallback: string): string => {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (typeof error === 'string' && error.trim()) {
    return error;
  }
  if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  return fallback;
};

const switchToAsterSignatureChain = async (): Promise<AsterBuilderApprovalResult> => {
  if (typeof window === 'undefined' || !window.ethereum) {
    return {
      success: false,
      message: 'No browser wallet detected. Please install MetaMask or another wallet.',
    };
  }

  try {
    await window.ethereum.request({
      method: 'wallet_switchEthereumChain',
      params: [{ chainId: ASTER_EVM_SIGNATURE_CHAIN_ID_HEX }],
    });
    return { success: true, message: 'Wallet network ready' };
  } catch (error) {
    const code = getProviderErrorCode(error);
    if (code === 4902) {
      try {
        await window.ethereum.request({
          method: 'wallet_addEthereumChain',
          params: [ASTER_EVM_SIGNATURE_CHAIN_PARAMS],
        });
        return { success: true, message: 'Wallet network ready' };
      } catch (addError) {
        return {
          success: false,
          message: getProviderErrorMessage(addError, 'Failed to add BNB Smart Chain to wallet'),
        };
      }
    }

    if (code === 4001) {
      return {
        success: false,
        message: 'Aster builder approval was rejected by user',
      };
    }

    if (code === -32601 || code === 4200) {
      return { success: true, message: 'Wallet network switch unsupported; continuing to signature request' };
    }

    return {
      success: false,
      message: getProviderErrorMessage(error, 'Failed to switch wallet to BNB Smart Chain'),
    };
  }
};

export const buildAsterApproveBuilderTypedData = (params: {
  builderAddress: string;
  builderName?: string;
  maxFeeRate: string;
  nonce: string;
  testnet?: boolean;
  userAddress: string;
}) => ({
  domain: {
    name: 'AsterSignTransaction',
    version: '1',
    chainId: ASTER_EVM_SIGNATURE_CHAIN_ID,
    verifyingContract: ASTER_CODE_ZERO_ADDRESS,
  },
  types: {
    EIP712Domain: [
      { name: 'name', type: 'string' },
      { name: 'version', type: 'string' },
      { name: 'chainId', type: 'uint256' },
      { name: 'verifyingContract', type: 'address' },
    ],
    ApproveBuilder: [
      { name: 'Builder', type: 'string' },
      { name: 'MaxFeeRate', type: 'string' },
      { name: 'BuilderName', type: 'string' },
      { name: 'AsterChain', type: 'string' },
      { name: 'User', type: 'string' },
      { name: 'Nonce', type: 'uint256' },
    ],
  },
  primaryType: 'ApproveBuilder' as const,
  message: {
    Builder: params.builderAddress,
    MaxFeeRate: params.maxFeeRate,
    BuilderName: params.builderName ?? '',
    AsterChain: getAsterDEXAsterChain(params.testnet),
    User: params.userAddress,
    Nonce: params.nonce,
  },
});

export const getAsterBuilderApprovalConfig = (options: Parameters<typeof resolveAsterBuilderFeeConfig>[0]) => {
  const config = resolveAsterBuilderFeeConfig(options);
  if (!config) {
    return undefined;
  }

  return {
    ...config,
    maxFeeRate: getAsterBuilderApprovalFeeRate(config),
  };
};

export const checkAsterBuilderApproved = async (params: {
  apiKey?: string;
  baseUrl?: string;
  builderConfig?: AsterBuilderFeeConfig;
  builderAddress?: string;
  requiredFeeRate?: string;
  signerAddress: string;
  signerPrivateKey: string;
  testnet?: boolean;
  userAddress: string;
}): Promise<AsterBuilderApprovalCheckResult> => {
  let builderAddress: string;
  let userAddress: string;
  let signerAddress: string;
  try {
    builderAddress = normalizeAddress(
      params.builderAddress ?? params.builderConfig?.builderAddress ?? '',
      'Builder address',
    );
    userAddress = normalizeAddress(params.userAddress, 'Aster user address');
    signerAddress = normalizeAddress(params.signerAddress, 'Aster signer address');
  } catch (error) {
    return {
      approved: false,
      approvalStatus: 'unverified',
      builderAddress: params.builderAddress ?? params.builderConfig?.builderAddress ?? '',
      error: error instanceof Error ? error.message : 'Invalid Aster builder approval inputs',
    };
  }

  if (!isAsterDEXV3PrivateKey(params.signerPrivateKey)) {
    return {
      approved: false,
      approvalStatus: 'unverified',
      builderAddress,
      error: 'Aster builder approval check requires a valid agent private key',
    };
  }

  const nonce = buildAsterDEXV3Nonce(`${userAddress}:${signerAddress}:builder-check`);
  const queryString = buildAsterDEXCodeQueryString({
    asterChain: getAsterDEXAsterChain(params.testnet),
    user: userAddress,
    signer: signerAddress,
    nonce,
  });
  const signature = generateAsterDEXCodeMessageSignature(queryString, params.signerPrivateKey, params.testnet);
  const targetUrl = `${getBaseUrl(params.testnet, params.baseUrl)}/fapi/v3/builder?${queryString}&signature=${signature}`;

  try {
    const request = await buildAsterApprovalRequest(targetUrl, params.baseUrl);
    const response = await fetch(request.url, {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        'X-MBX-APIKEY': params.apiKey || signerAddress,
        ...request.headers,
      },
    });
    if (!response.ok) {
      return {
        approved: false,
        approvalStatus: 'unverified',
        builderAddress,
        error: `Aster builder approval check failed with HTTP ${response.status}`,
      };
    }

    const payload = await response.json();
    const rows = extractBuilderRows(payload);
    const row = rows.find((item) => {
      const rawAddress = item.builderAddress ?? item.builder;
      if (typeof rawAddress !== 'string') {
        return false;
      }
      try {
        return ethers.getAddress(rawAddress) === builderAddress;
      } catch {
        return false;
      }
    });

    if (!row) {
      return {
        approved: false,
        approvalStatus: 'missing',
        builderAddress,
      };
    }

    const approvedRate = parseFeeRate(row.maxFeeRate);
    const requiredRate = parseFeeRate(
      params.requiredFeeRate ?? (params.builderConfig ? getAsterBuilderApprovalFeeRate(params.builderConfig) : '0'),
    );
    const hasSufficientFeeCap =
      approvedRate !== undefined && requiredRate !== undefined && approvedRate + Number.EPSILON >= requiredRate;

    return {
      approved: hasSufficientFeeCap,
      approvalStatus: hasSufficientFeeCap ? 'approved' : 'insufficient_fee_cap',
      builderAddress,
      maxFeeRate: row.maxFeeRate === undefined ? undefined : String(row.maxFeeRate),
    };
  } catch (error) {
    return {
      approved: false,
      approvalStatus: 'unverified',
      builderAddress,
      error: error instanceof Error ? error.message : 'Failed to check Aster builder approval',
    };
  }
};

export const approveAsterBuilderFeeWithBrowserWallet = async (params: {
  baseUrl?: string;
  builderAddress: string;
  builderName?: string;
  maxFeeRate: string;
  testnet?: boolean;
  userAddress: string;
}): Promise<AsterBuilderApprovalResult> => {
  if (typeof window === 'undefined' || !window.ethereum) {
    return {
      success: false,
      message: 'No browser wallet detected. Please install MetaMask or another wallet.',
    };
  }

  let builderAddress: string;
  let userAddress: string;
  try {
    builderAddress = normalizeAddress(params.builderAddress, 'Builder address');
    userAddress = normalizeAddress(params.userAddress, 'Aster user address');
  } catch (error) {
    return {
      success: false,
      message: error instanceof Error ? error.message : 'Invalid Aster builder approval inputs',
    };
  }

  const nonce = buildAsterDEXV3Nonce(`${userAddress}:approve-builder`);
  const typedData = buildAsterApproveBuilderTypedData({
    builderAddress,
    builderName: params.builderName,
    maxFeeRate: params.maxFeeRate,
    nonce,
    testnet: params.testnet,
    userAddress,
  });

  try {
    const networkResult = await switchToAsterSignatureChain();
    if (!networkResult.success) {
      return networkResult;
    }

    const accounts = (await window.ethereum.request({ method: 'eth_requestAccounts' })) as string[];
    const signerAddress = String(accounts?.[0] ?? '');
    if (!signerAddress) {
      return {
        success: false,
        message: 'No wallet account returned by provider',
      };
    }

    if (ethers.getAddress(signerAddress) !== userAddress) {
      return {
        success: false,
        message: `Wrong wallet connected. Expected ${userAddress}, got ${signerAddress}`,
      };
    }

    const signature = (await window.ethereum.request({
      method: 'eth_signTypedData_v4',
      params: [signerAddress, JSON.stringify(typedData)],
    })) as string;

    if (typeof signature !== 'string' || !signature.startsWith('0x')) {
      return {
        success: false,
        message: 'Wallet returned an invalid signature payload',
      };
    }

    const body = new URLSearchParams({
      builder: builderAddress,
      maxFeeRate: params.maxFeeRate,
      builderName: params.builderName ?? '',
      asterChain: getAsterDEXAsterChain(params.testnet),
      user: userAddress,
      nonce,
      signature,
      signatureChainId: String(ASTER_EVM_SIGNATURE_CHAIN_ID),
    });
    const request = await buildAsterApprovalRequest(
      `${getBaseUrl(params.testnet, params.baseUrl)}/fapi/v3/approveBuilder`,
      params.baseUrl,
    );
    const response = await fetch(request.url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        ...request.headers,
      },
      body,
    });
    const payload = await response.json().catch(() => undefined);
    if (!response.ok || (payload as { code?: number } | undefined)?.code !== 200) {
      const message =
        (payload as { msg?: string; error?: string } | undefined)?.msg ||
        (payload as { msg?: string; error?: string } | undefined)?.error ||
        `Aster builder approval failed with HTTP ${response.status}`;
      return {
        success: false,
        message,
      };
    }

    return {
      success: true,
      message: 'Aster builder approval submitted successfully',
    };
  } catch (error) {
    return {
      success: false,
      message: getProviderErrorMessage(error, 'Failed to submit Aster builder approval'),
    };
  }
};
