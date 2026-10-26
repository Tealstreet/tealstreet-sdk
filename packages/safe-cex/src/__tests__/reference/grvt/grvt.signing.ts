import { ethers } from 'ethers';

/**
 * GRVT EIP-712 Signing Utilities
 *
 * GRVT requires EIP-712 signatures for all trading operations.
 * Orders are signed using an Ethereum private key and verified on-chain via zkSync.
 */

// GRVT EIP-712 Domain for testnet
const GRVT_TESTNET_DOMAIN = {
  name: 'Grvt',
  version: '0',
  chainId: '325', // zkSync testnet
};

// GRVT EIP-712 Domain for mainnet
const GRVT_MAINNET_DOMAIN = {
  name: 'Grvt',
  version: '0',
  chainId: '324', // zkSync mainnet
};

// EIP-712 types for GRVT Order
const ORDER_TYPES = {
  Order: [
    { name: 'subAccountId', type: 'string' },
    { name: 'isMarket', type: 'bool' },
    { name: 'timeInForce', type: 'string' },
    { name: 'postOnly', type: 'bool' },
    { name: 'reduceOnly', type: 'bool' },
    { name: 'legs', type: 'OrderLeg[]' },
    { name: 'nonce', type: 'uint64' },
    { name: 'expiration', type: 'uint64' },
  ],
  OrderLeg: [
    { name: 'instrument', type: 'string' },
    { name: 'size', type: 'string' },
    { name: 'limitPrice', type: 'string' },
    { name: 'isBuyingAsset', type: 'bool' },
  ],
};

export interface GrvtSignature {
  r: string;
  s: string;
  v: number;
  signer: string;
  nonce: number;
  expiration: string;
}

export interface OrderLeg {
  instrument: string;
  size: string;
  limitPrice?: string;
  isBuyingAsset: boolean;
}

export interface OrderPayload {
  subAccountId: string;
  isMarket: boolean;
  timeInForce: string;
  postOnly: boolean;
  reduceOnly: boolean;
  legs: OrderLeg[];
  nonce: number;
  expiration: string;
}

/**
 * Sign a GRVT order using EIP-712
 */
export async function signOrder(
  wallet: ethers.Wallet,
  payload: OrderPayload,
  isTestnet: boolean = true,
): Promise<GrvtSignature> {
  const domain = isTestnet ? GRVT_TESTNET_DOMAIN : GRVT_MAINNET_DOMAIN;

  // Sign using EIP-712
  const signature = await wallet.signTypedData(domain, ORDER_TYPES, payload);

  // Split signature into r, s, v components
  const sig = ethers.Signature.from(signature);

  return {
    r: sig.r,
    s: sig.s,
    v: sig.v,
    signer: wallet.address,
    nonce: payload.nonce,
    expiration: payload.expiration,
  };
}

/**
 * Generate a nonce for order signing
 * Uses current timestamp in milliseconds
 */
export function generateNonce(): number {
  return Date.now();
}

/**
 * Generate an expiration timestamp
 * Default: 1 hour from now, in nanoseconds
 */
export function generateExpiration(hoursFromNow: number = 1): string {
  const now = Date.now(); // milliseconds
  const expirationMs = now + hoursFromNow * 60 * 60 * 1000;
  // Convert to nanoseconds (multiply by 1,000,000)
  return (expirationMs * 1_000_000).toString();
}

/**
 * Validate Ethereum private key format
 */
export function validatePrivateKey(privateKey: string): boolean {
  try {
    // Try to create a wallet with the key
    new ethers.Wallet(privateKey);
    return true;
  } catch {
    return false;
  }
}
