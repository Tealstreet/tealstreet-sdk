/**
 * Hyperliquid Agent Wallet Utilities
 *
 * This module provides functions for creating and managing agent (API) wallets
 * on Hyperliquid. Agent wallets are limited-permission wallets that can only
 * trade on behalf of the main wallet - they cannot withdraw or transfer funds.
 *
 * @see https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api
 */

import { ethers } from 'ethers';

import { HYPERLIQUID_BUILDER_DOMAIN, HYPERLIQUID_BUILDER_DOMAIN_TESTNET } from './hyperliquid.builderFee';
import { BASE_URLS, Signature } from './hyperliquid.types';

// Window.ethereum is declared globally in src/env.d.ts (EthereumProvider)

export interface AgentWalletCreationResult {
  success: boolean;
  message: string;
  agentAddress?: string;
  agentPrivateKey?: string;
}

/**
 * Generate a unique agent wallet name with random suffix
 * Format: "Tealstreet XXXX" (15 chars, within 16 char limit)
 */
export function generateAgentWalletName(): string {
  const chars = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let suffix = '';
  for (let i = 0; i < 4; i++) {
    suffix += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return `Tealstreet ${suffix}`;
}

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
 */
async function switchToCorrectNetwork(testnet: boolean): Promise<{ success: boolean; error?: string }> {
  if (typeof window === 'undefined' || !window.ethereum) {
    return { success: false, error: 'No Ethereum wallet detected' };
  }

  const targetChain = testnet ? ARBITRUM_SEPOLIA_CHAIN : ARBITRUM_ONE_CHAIN;
  const targetChainId = targetChain.chainId;

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

  if (await trySwitch()) {
    return { success: true };
  }

  if (await tryAddChain()) {
    if (await trySwitch()) {
      return { success: true };
    }
  }

  return {
    success: false,
    error: `Please manually switch to ${targetChain.chainName} in your wallet before proceeding`,
  };
}

// EIP-712 types for ApproveAgent
export const APPROVE_AGENT_TYPES = {
  'HyperliquidTransaction:ApproveAgent': [
    { name: 'hyperliquidChain', type: 'string' },
    { name: 'agentAddress', type: 'address' },
    { name: 'agentName', type: 'string' },
    { name: 'nonce', type: 'uint64' },
  ],
};

/**
 * Sign the agent approval message using an ethers Signer
 */
export async function signAgentApproval(
  signer: ethers.Signer,
  agentAddress: string,
  agentName: string,
  testnet: boolean = false,
  nonce?: number,
): Promise<{ signature: Signature; nonce: number }> {
  const actualNonce = nonce ?? Date.now();
  const domain = testnet ? HYPERLIQUID_BUILDER_DOMAIN_TESTNET : HYPERLIQUID_BUILDER_DOMAIN;
  const hyperliquidChain = testnet ? 'Testnet' : 'Mainnet';

  const message = {
    hyperliquidChain,
    agentAddress,
    agentName,
    nonce: actualNonce,
  };

  const rawSignature = await (signer as ethers.JsonRpcSigner).signTypedData(domain, APPROVE_AGENT_TYPES, message);
  const { r, s, v } = ethers.Signature.from(rawSignature);

  return {
    signature: { r, s, v },
    nonce: actualNonce,
  };
}

/**
 * Submit the agent approval to Hyperliquid
 */
export async function submitAgentApproval(
  agentAddress: string,
  agentName: string,
  nonce: number,
  signature: Signature,
  testnet: boolean = false,
): Promise<AgentWalletCreationResult> {
  const baseUrl = testnet ? BASE_URLS.TESTNET : BASE_URLS.PRODUCTION;
  const hyperliquidChain = testnet ? 'Testnet' : 'Mainnet';
  const signatureChainId = testnet ? '0x66eee' : '0xa4b1';

  try {
    const response = await fetch(`${baseUrl}/exchange`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        action: {
          type: 'approveAgent',
          hyperliquidChain,
          signatureChainId,
          agentAddress,
          agentName,
          nonce,
        },
        signature,
        nonce,
      }),
    });

    const result = (await response.json()) as Record<string, any>;

    if (result.status === 'err') {
      return {
        success: false,
        message: result.response || 'Failed to approve agent wallet',
      };
    }

    return {
      success: true,
      message: 'Agent wallet approved successfully',
      agentAddress,
    };
  } catch (error) {
    return {
      success: false,
      message: error instanceof Error ? error.message : 'Failed to submit agent approval',
    };
  }
}

/**
 * Create a new agent (API) wallet for Hyperliquid trading
 *
 * This function:
 * 1. Generates a new random wallet locally
 * 2. Connects to the user's browser wallet
 * 3. Signs the agent approval with the main wallet
 * 4. Submits the approval to Hyperliquid
 * 5. Returns the new agent wallet's address and private key
 *
 * The agent wallet can only trade - it cannot withdraw or transfer funds.
 *
 * @param expectedMainAddress - The expected main wallet address (for verification)
 * @param testnet - Whether to use testnet
 * @param agentName - Name for the agent wallet (auto-generated if not provided) - max 16 characters
 * @returns The result including agent address and private key if successful
 */
export async function createAgentWalletWithBrowserWallet(
  expectedMainAddress: string,
  testnet: boolean = false,
  agentName?: string,
): Promise<AgentWalletCreationResult> {
  if (typeof window === 'undefined' || !window.ethereum) {
    return {
      success: false,
      message: 'No Ethereum wallet detected. Please install MetaMask or another wallet.',
    };
  }

  const walletName = agentName || generateAgentWalletName();

  try {
    // 1. Generate a new random wallet for the agent
    const agentWallet = ethers.Wallet.createRandom();
    const agentAddress = agentWallet.address;
    const agentPrivateKey = agentWallet.privateKey;

    // 2. Switch to the correct network
    const networkResult = await switchToCorrectNetwork(testnet);
    if (!networkResult.success) {
      return {
        success: false,
        message: networkResult.error || 'Failed to switch to the correct network',
      };
    }

    // 3. Connect to browser wallet and get signer
    const provider = new ethers.BrowserProvider(window!.ethereum!);
    await provider.send('eth_requestAccounts', []);
    const signer = await provider.getSigner();
    const mainAddress = await signer.getAddress();

    // 4. Verify address matches
    if (mainAddress.toLowerCase() !== expectedMainAddress.toLowerCase()) {
      return {
        success: false,
        message: `Wrong wallet connected. Expected ${expectedMainAddress.slice(0, 8)}...${expectedMainAddress.slice(-6)}, got ${mainAddress.slice(0, 8)}...${mainAddress.slice(-6)}. Please connect the correct wallet.`,
      };
    }

    // 5. Sign the agent approval
    const { signature, nonce } = await signAgentApproval(signer, agentAddress, walletName, testnet);

    // 6. Submit to Hyperliquid
    const result = await submitAgentApproval(agentAddress, walletName, nonce, signature, testnet);

    if (result.success) {
      return {
        success: true,
        message: 'Agent wallet created successfully',
        agentAddress,
        agentPrivateKey,
      };
    }

    return result;
  } catch (error) {
    // Handle user rejection
    if (error instanceof Error && error.message.includes('user rejected')) {
      return {
        success: false,
        message: 'Agent wallet creation was rejected by user',
      };
    }

    return {
      success: false,
      message: error instanceof Error ? error.message : 'Failed to create agent wallet',
    };
  }
}

/**
 * Check if the browser has an Ethereum wallet available
 */
export function hasEthereumWallet(): boolean {
  return typeof window !== 'undefined' && !!window.ethereum;
}

/**
 * Request wallet connection and return the connected address
 */
export async function connectBrowserWallet(): Promise<{ success: boolean; address?: string; error?: string }> {
  if (!hasEthereumWallet()) {
    return {
      success: false,
      error: 'No Ethereum wallet detected. Please install MetaMask or another wallet.',
    };
  }

  try {
    const provider = new ethers.BrowserProvider(window!.ethereum!);
    await provider.send('eth_requestAccounts', []);
    const signer = await provider.getSigner();
    const address = await signer.getAddress();

    return {
      success: true,
      address,
    };
  } catch (error) {
    if (error instanceof Error && error.message.includes('user rejected')) {
      return {
        success: false,
        error: 'Wallet connection was rejected',
      };
    }

    return {
      success: false,
      error: error instanceof Error ? error.message : 'Failed to connect wallet',
    };
  }
}
