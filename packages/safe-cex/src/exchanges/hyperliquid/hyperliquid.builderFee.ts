/**
 * Hyperliquid Builder Fee Utilities
 *
 * This module provides functions for approving and checking builder fee status
 * on Hyperliquid. Builder fees allow Tealstreet to receive a portion of trading
 * fees when users execute trades through the platform.
 *
 * @see https://hyperliquid.gitbook.io/hyperliquid-docs/trading/builder-codes
 */

import { ethers } from 'ethers';

import {
  getBuilderAddress,
  TEALSTREET_BUILDER_ADDRESS_MAINNET,
  TEALSTREET_BUILDER_ADDRESS_TESTNET,
  TEALSTREET_BUILDER_FEE,
} from './hyperliquid.constants';
import { BASE_URLS, BuilderFeeApprovalResult, BuilderFeeCheckResult, Signature } from './hyperliquid.types';

// Re-export for public API
export { getBuilderAddress, TEALSTREET_BUILDER_ADDRESS_MAINNET, TEALSTREET_BUILDER_ADDRESS_TESTNET };

// Window.ethereum is declared globally in src/env.d.ts (EthereumProvider)

// Default fee rate: 1 basis point (0.01%)
export const DEFAULT_BUILDER_FEE_RATE = '0.01%';
export const DEFAULT_BUILDER_FEE_TENTHS_BPS = TEALSTREET_BUILDER_FEE;

// EIP-712 domain for Hyperliquid signing (mainnet)
export const HYPERLIQUID_BUILDER_DOMAIN = {
  name: 'HyperliquidSignTransaction',
  version: '1',
  chainId: 42161, // Arbitrum One mainnet (0xa4b1)
  verifyingContract: '0x0000000000000000000000000000000000000000' as const,
};

// EIP-712 domain for Hyperliquid signing (testnet)
export const HYPERLIQUID_BUILDER_DOMAIN_TESTNET = {
  name: 'HyperliquidSignTransaction',
  version: '1',
  chainId: 421614, // Arbitrum Sepolia testnet
  verifyingContract: '0x0000000000000000000000000000000000000000' as const,
};

// Network configurations for automatic switching
const ARBITRUM_ONE_CHAIN = {
  chainId: '0xa4b1', // 42161
  chainName: 'Arbitrum One',
  nativeCurrency: {
    name: 'Ether',
    symbol: 'ETH',
    decimals: 18,
  },
  rpcUrls: ['https://arb1.arbitrum.io/rpc'],
  blockExplorerUrls: ['https://arbiscan.io'],
};

const ARBITRUM_SEPOLIA_CHAIN = {
  chainId: '0x66eee', // 421614
  chainName: 'Arbitrum Sepolia',
  nativeCurrency: {
    name: 'Ether',
    symbol: 'ETH',
    decimals: 18,
  },
  rpcUrls: ['https://sepolia-rollup.arbitrum.io/rpc'],
  blockExplorerUrls: ['https://sepolia.arbiscan.io'],
};

/**
 * Switch the wallet to the correct network for Hyperliquid signing
 *
 * @param testnet - Whether to switch to testnet (Arbitrum Sepolia) or mainnet (Arbitrum One)
 * @returns True if the network was switched successfully, false otherwise
 */
async function switchToCorrectNetwork(testnet: boolean): Promise<{ success: boolean; error?: string }> {
  if (typeof window === 'undefined' || !window.ethereum) {
    return { success: false, error: 'No Ethereum wallet detected' };
  }

  const targetChain = testnet ? ARBITRUM_SEPOLIA_CHAIN : ARBITRUM_ONE_CHAIN;
  const targetChainId = targetChain.chainId;

  // Helper to try switching
  const trySwitch = async (): Promise<boolean> => {
    try {
      await window!.ethereum!.request({
        method: 'wallet_switchEthereumChain',
        params: [{ chainId: targetChainId }],
      });
      return true;
    } catch {
      return false;
    }
  };

  // Helper to try adding chain
  const tryAddChain = async (): Promise<boolean> => {
    try {
      await window!.ethereum!.request({
        method: 'wallet_addEthereumChain',
        params: [targetChain],
      });
      return true;
    } catch {
      return false;
    }
  };

  // First attempt: try switching directly
  if (await trySwitch()) {
    return { success: true };
  }

  // Second attempt: try adding the chain, then switch
  if (await tryAddChain()) {
    // After adding, try switching again
    if (await trySwitch()) {
      return { success: true };
    }
  }

  // If both failed, return helpful error message
  return {
    success: false,
    error: `Please manually switch to ${targetChain.chainName} in your wallet before approving`,
  };
}

// EIP-712 types for ApproveBuilderFee
export const APPROVE_BUILDER_FEE_TYPES = {
  'HyperliquidTransaction:ApproveBuilderFee': [
    { name: 'hyperliquidChain', type: 'string' },
    { name: 'maxFeeRate', type: 'string' },
    { name: 'builder', type: 'address' },
    { name: 'nonce', type: 'uint64' },
  ],
};

/**
 * Check if a user has approved builder fee for a specific builder
 *
 * @param userAddress - The user's main wallet address
 * @param builderAddress - The builder address to check (defaults to Tealstreet based on network)
 * @param testnet - Whether to use testnet API
 * @returns Whether the builder fee is approved and the max fee rate
 */
export async function checkBuilderFeeApproved(
  userAddress: string,
  builderAddress?: string,
  testnet: boolean = false,
): Promise<BuilderFeeCheckResult> {
  const baseUrl = testnet ? BASE_URLS.TESTNET : BASE_URLS.PRODUCTION;
  const builder = builderAddress ?? getBuilderAddress(testnet);

  try {
    const response = await fetch(`${baseUrl}/info`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        type: 'maxBuilderFee',
        user: userAddress.toLowerCase(),
        builder: builder.toLowerCase(),
      }),
    });

    if (!response.ok) {
      return { approved: false, error: `Hyperliquid returned ${response.status}` };
    }

    const maxFeeRateRaw = await response.text();
    const feeValueTenthsBps = parseFloat(maxFeeRateRaw);

    // An error page or an API shape change parses to NaN. Reporting that as
    // "not approved" is indistinguishable from a genuine denial, which blocks
    // already-approved users with no way to tell why.
    if (isNaN(feeValueTenthsBps)) {
      return { approved: false, error: 'Unexpected response from Hyperliquid' };
    }

    // Convert from tenths of basis points to percentage string
    // e.g., 10 tenths of bps = 1 bps = 0.01%
    const feePercentage = `${feeValueTenthsBps / 10 / 100}%`;

    return {
      approved: feeValueTenthsBps >= TEALSTREET_BUILDER_FEE,
      maxFeeRate: feePercentage,
      maxFeeRateTenthsBps: feeValueTenthsBps,
    };
  } catch (error) {
    console.error('Failed to check builder fee approval:', error);
    return {
      approved: false,
      error: error instanceof Error ? error.message : 'Failed to reach Hyperliquid',
    };
  }
}

/**
 * Sign the builder fee approval message using an ethers Signer
 *
 * This is used when the user connects their wallet via BrowserProvider.
 *
 * @param signer - The ethers signer from the connected wallet
 * @param builderAddress - The builder address to approve
 * @param maxFeeRate - The maximum fee rate to approve (e.g., "0.01%")
 * @param testnet - Whether to use testnet configuration
 * @param nonce - Optional nonce (defaults to current timestamp)
 * @returns The signature components (r, s, v) and nonce
 */
export async function signBuilderFeeApproval(
  signer: ethers.Signer,
  builderAddress: string,
  maxFeeRate: string = DEFAULT_BUILDER_FEE_RATE,
  testnet: boolean = false,
  nonce?: number,
): Promise<{ signature: Signature; nonce: number }> {
  const actualNonce = nonce ?? Date.now();
  const domain = testnet ? HYPERLIQUID_BUILDER_DOMAIN_TESTNET : HYPERLIQUID_BUILDER_DOMAIN;
  const hyperliquidChain = testnet ? 'Testnet' : 'Mainnet';

  const message = {
    hyperliquidChain,
    maxFeeRate,
    builder: builderAddress,
    nonce: actualNonce,
  };

  // Sign the typed data
  const rawSignature = await (signer as ethers.JsonRpcSigner).signTypedData(domain, APPROVE_BUILDER_FEE_TYPES, message);

  // Split the signature
  const { r, s, v } = ethers.Signature.from(rawSignature);

  return {
    signature: { r, s, v },
    nonce: actualNonce,
  };
}

/**
 * Submit the builder fee approval to Hyperliquid
 *
 * @param builderAddress - The builder address
 * @param maxFeeRate - The maximum fee rate
 * @param nonce - The nonce used in signing
 * @param signature - The signature components
 * @param testnet - Whether to use testnet API
 * @returns The result of the approval submission
 */
export async function submitBuilderFeeApproval(
  builderAddress: string,
  maxFeeRate: string,
  nonce: number,
  signature: Signature,
  testnet: boolean = false,
): Promise<BuilderFeeApprovalResult> {
  const baseUrl = testnet ? BASE_URLS.TESTNET : BASE_URLS.PRODUCTION;
  const hyperliquidChain = testnet ? 'Testnet' : 'Mainnet';
  const signatureChainId = testnet ? '0x66eee' : '0xa4b1'; // Arbitrum Sepolia or One

  try {
    const response = await fetch(`${baseUrl}/exchange`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        action: {
          type: 'approveBuilderFee',
          hyperliquidChain,
          signatureChainId,
          builder: builderAddress,
          maxFeeRate,
          nonce,
        },
        signature,
        nonce,
      }),
    });

    const result = (await response.json()) as { status?: string; response?: string };

    if (result.status === 'err') {
      return {
        success: false,
        message: result.response || 'Failed to approve builder fee',
      };
    }

    return {
      success: true,
      message: 'Builder fee approved successfully',
    };
  } catch (error) {
    return {
      success: false,
      message: error instanceof Error ? error.message : 'Failed to submit approval',
    };
  }
}

/**
 * Create BuilderInfo object for including in order actions
 *
 * @param testnet - Whether to use testnet builder address
 * @param builderAddress - The builder's wallet address (optional, defaults based on network)
 * @param feeInTenthsBps - Fee in tenths of basis points (10 = 1bp = 0.01%)
 * @returns BuilderInfo object to include in order action
 */
export function createBuilderInfo(
  testnet: boolean = false,
  builderAddress?: string,
  feeInTenthsBps: number = DEFAULT_BUILDER_FEE_TENTHS_BPS,
): { b: string; f: number } {
  const builder = builderAddress ?? getBuilderAddress(testnet);
  return {
    b: builder.toLowerCase(),
    f: feeInTenthsBps,
  };
}

/**
 * Full flow to approve builder fee (for use in browser with wallet connection)
 *
 * This function handles the complete approval process:
 * 1. Creates a BrowserProvider from window.ethereum
 * 2. Requests account access
 * 3. Verifies the connected address matches the expected address
 * 4. Signs the approval message
 * 5. Submits the approval to Hyperliquid
 *
 * @param expectedAddress - The expected wallet address (should match account's applicationId)
 * @param testnet - Whether to use testnet
 * @param builderAddress - The builder address to approve (defaults to Tealstreet based on network)
 * @param maxFeeRate - The maximum fee rate (defaults to 0.01%)
 * @returns The result of the approval process
 */
export async function approveBuilderFeeWithBrowserWallet(
  expectedAddress: string,
  testnet: boolean = false,
  builderAddress?: string,
  maxFeeRate: string = DEFAULT_BUILDER_FEE_RATE,
): Promise<BuilderFeeApprovalResult> {
  const builder = builderAddress ?? getBuilderAddress(testnet);
  // Check for window.ethereum (browser wallet)
  if (typeof window === 'undefined' || !window.ethereum) {
    return {
      success: false,
      message: 'No Ethereum wallet detected. Please install MetaMask or another wallet.',
    };
  }

  try {
    // Switch to the correct network before signing
    const networkResult = await switchToCorrectNetwork(testnet);
    if (!networkResult.success) {
      return {
        success: false,
        message: networkResult.error || 'Failed to switch to the correct network',
      };
    }

    // Create provider and get signer
    const provider = new ethers.BrowserProvider(window!.ethereum!);
    await provider.send('eth_requestAccounts', []);
    const signer = await provider.getSigner();
    const address = await signer.getAddress();

    // Verify address matches
    if (address.toLowerCase() !== expectedAddress.toLowerCase()) {
      return {
        success: false,
        message: `Wrong wallet connected. Expected ${expectedAddress.slice(0, 8)}...${expectedAddress.slice(-6)}, got ${address.slice(0, 8)}...${address.slice(-6)}. Please connect the correct wallet.`,
      };
    }

    // Sign the approval
    const { signature, nonce } = await signBuilderFeeApproval(signer, builder, maxFeeRate, testnet);

    // Submit to Hyperliquid
    const result = await submitBuilderFeeApproval(builder, maxFeeRate, nonce, signature, testnet);

    // Send notification on success (fire and forget)
    if (result.success) {
      fetch('/api/logger/builder-fee', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ wallet: address, testnet }),
      }).catch(() => {
        // Silently ignore notification errors
      });
    }

    return result;
  } catch (error) {
    // Handle user rejection
    if (error instanceof Error && error.message.includes('user rejected')) {
      return {
        success: false,
        message: 'Approval was rejected by user',
      };
    }

    return {
      success: false,
      message: error instanceof Error ? error.message : 'Failed to approve builder fee',
    };
  }
}
