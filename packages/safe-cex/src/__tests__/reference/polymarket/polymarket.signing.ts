/**
 * Polymarket CLOB V2 signing helpers.
 *
 * Order construction delegates to the official V2 SDK so the domain,
 * negative-risk contract selection, rounding, and POLY_1271 wrapper stay in
 * lock-step with Polymarket's production contract.
 */

import {
  isV2Order,
  OrderBuilder,
  Side,
  SignatureTypeV2,
  TickSize,
} from '@polymarket/clob-client-v2';
import { ethers } from 'ethers';

import { POLYMARKET_CONFIG, SignatureType } from './polymarket.constants';
import type { PolymarketCredentials, PolymarketSignedOrder } from './polymarket.types';

const CLOB_AUTH_MESSAGE = 'This message attests that I control the given wallet';

const CLOB_AUTH_TYPES: Record<string, Array<{ name: string; type: string }>> = {
  ClobAuth: [
    { name: 'address', type: 'address' },
    { name: 'timestamp', type: 'string' },
    { name: 'nonce', type: 'uint256' },
    { name: 'message', type: 'string' },
  ],
};

/** Create an ethers wallet from a private key. */
export function createWallet(privateKey: string): ethers.Wallet {
  return new ethers.Wallet(privateKey);
}

/**
 * Create and sign a CLOB V2 order with Polymarket's official order builder.
 */
export async function createSignedOrder(
  wallet: ethers.Wallet,
  params: {
    tokenId: string;
    price: number;
    size: number;
    side: 'BUY' | 'SELL';
    tickSize: TickSize;
    negRisk: boolean;
    signatureType?: SignatureType;
    funderAddress?: string;
  },
): Promise<PolymarketSignedOrder> {
  // clob-client-v2 accepts ethers v5's _signTypedData contract. Adapt the
  // equivalent ethers v6 Wallet method without exposing the private key.
  const signer = {
    getAddress: () => wallet.getAddress(),
    _signTypedData: (
      domain: Record<string, unknown>,
      types: Record<string, Array<{ name: string; type: string }>>,
      value: Record<string, unknown>,
    ) => wallet.signTypedData(domain, types, value),
  };

  const builder = new OrderBuilder(
    signer,
    POLYMARKET_CONFIG.CHAIN_ID,
    (params.signatureType ?? SignatureType.EOA) as unknown as SignatureTypeV2,
    params.funderAddress,
  );
  const signed = await builder.buildOrder(
    {
      tokenID: params.tokenId,
      price: params.price,
      size: params.size,
      side: params.side === 'BUY' ? Side.BUY : Side.SELL,
    },
    { tickSize: params.tickSize, negRisk: params.negRisk },
    2,
  );

  if (!isV2Order(signed)) {
    throw new Error('Polymarket official client returned a non-V2 order');
  }

  return signed as unknown as PolymarketSignedOrder;
}

/** Generate the canonical URL-safe-base64 L2 HMAC digest. */
export function generateHmacSignature(
  secret: string,
  timestamp: string,
  method: string,
  path: string,
  body?: string,
): string {
  let message = `${timestamp}${method.toUpperCase()}${path}`;
  if (body !== undefined) message += body;

  const normalized = secret.replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized + '='.repeat((4 - (normalized.length % 4)) % 4);
  const key = ethers.decodeBase64(padded);
  const hmac = ethers.computeHmac('sha256', key, ethers.toUtf8Bytes(message));
  return ethers.encodeBase64(ethers.getBytes(hmac)).replace(/\+/g, '-').replace(/\//g, '_');
}

/** Build the five canonical L2 authentication headers. */
export function buildL2Headers(
  credentials: PolymarketCredentials,
  walletAddress: string,
  method: string,
  path: string,
  body?: string,
  timestamp = Math.floor(Date.now() / 1000),
): Record<string, string> {
  const timestampString = timestamp.toString();
  const signature = generateHmacSignature(credentials.secret, timestampString, method, path, body);

  return {
    'POLY_ADDRESS': walletAddress,
    'POLY_SIGNATURE': signature,
    'POLY_TIMESTAMP': timestampString,
    'POLY_API_KEY': credentials.apiKey,
    'POLY_PASSPHRASE': credentials.passphrase,
  };
}

/** Build the documented ClobAuth EIP-712 L1 signature. */
export async function signApiKeyDerivationMessage(
  wallet: ethers.Wallet,
  nonce: number,
  timestamp: number,
): Promise<{ address: string; signature: string; timestamp: number }> {
  const address = await wallet.getAddress();
  const signature = await wallet.signTypedData(
    {
      name: 'ClobAuthDomain',
      version: '1',
      chainId: POLYMARKET_CONFIG.CHAIN_ID,
    },
    CLOB_AUTH_TYPES,
    {
      address,
      timestamp: timestamp.toString(),
      nonce,
      message: CLOB_AUTH_MESSAGE,
    },
  );

  return { address, signature, timestamp };
}
