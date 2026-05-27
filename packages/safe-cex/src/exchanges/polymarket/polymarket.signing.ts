/**
 * Polymarket EIP-712 Signing
 *
 * This module handles order signing for Polymarket's CLOB API.
 * Polymarket uses EIP-712 typed data signatures for order authentication.
 *
 * Reference: https://docs.polymarket.com/
 */

import { ethers } from 'ethers';

import { EIP712_DOMAIN, EIP712_ORDER_TYPES, POLYMARKET_CONFIG, SignatureType } from './polymarket.constants';
import type { PolymarketCredentials, PolymarketSignedOrder } from './polymarket.types';

/**
 * Order side numeric values for EIP-712 signing
 */
export const ORDER_SIDE_NUMERIC = {
  BUY: 0,
  SELL: 1,
} as const;

/**
 * Create an ethers Wallet from a private key
 */
export function createWallet(privateKey: string): ethers.Wallet {
  return new ethers.Wallet(privateKey);
}

/**
 * Generate a random salt for order uniqueness
 */
export function generateSalt(): string {
  const randomBytes = ethers.randomBytes(32);
  return ethers.toBigInt(randomBytes).toString();
}

/**
 * Get current nonce from the exchange (simplified - actual implementation may need API call)
 */
export function generateNonce(): string {
  return '0';
}

/**
 * Calculate order expiration timestamp
 * Default: 1 week from now
 */
export function calculateExpiration(daysFromNow: number = 7): string {
  const now = Math.floor(Date.now() / 1000);
  const expiration = now + daysFromNow * 24 * 60 * 60;
  return expiration.toString();
}

/**
 * Convert price and size to maker/taker amounts for order signing
 *
 * For BUY orders:
 * - makerAmount = size * price (USDC to spend)
 * - takerAmount = size (shares to receive)
 *
 * For SELL orders:
 * - makerAmount = size (shares to sell)
 * - takerAmount = size * price (USDC to receive)
 *
 * Amounts are in base units (6 decimals for USDC, integer for shares)
 */
export function calculateOrderAmounts(
  price: number,
  size: number,
  side: 'BUY' | 'SELL',
): { makerAmount: string; takerAmount: string } {
  // USDC has 6 decimals
  const USDC_DECIMALS = 6;
  const USDC_MULTIPLIER = 10 ** USDC_DECIMALS;

  // Shares are whole numbers (actually 10^6 precision internally)
  const SHARE_DECIMALS = 6;
  const SHARE_MULTIPLIER = 10 ** SHARE_DECIMALS;

  if (side === 'BUY') {
    // Buying shares: spend USDC, receive shares
    const usdcAmount = Math.floor(price * size * USDC_MULTIPLIER);
    const shareAmount = Math.floor(size * SHARE_MULTIPLIER);
    return {
      makerAmount: usdcAmount.toString(),
      takerAmount: shareAmount.toString(),
    };
  } else {
    // Selling shares: spend shares, receive USDC
    const shareAmount = Math.floor(size * SHARE_MULTIPLIER);
    const usdcAmount = Math.floor(price * size * USDC_MULTIPLIER);
    return {
      makerAmount: shareAmount.toString(),
      takerAmount: usdcAmount.toString(),
    };
  }
}

/**
 * Build the unsigned order structure for EIP-712 signing
 */
export function buildOrderData(params: {
  tokenId: string;
  price: number;
  size: number;
  side: 'BUY' | 'SELL';
  maker: string;
  signer: string;
  taker?: string;
  expiration?: string;
  nonce?: string;
  feeRateBps?: string;
  signatureType?: SignatureType;
}): Omit<PolymarketSignedOrder, 'signature'> {
  const { makerAmount, takerAmount } = calculateOrderAmounts(params.price, params.size, params.side);

  // Default taker is the Polymarket operator (exchange contract handles this)
  const taker = params.taker || '0x0000000000000000000000000000000000000000';

  return {
    salt: generateSalt(),
    maker: params.maker,
    signer: params.signer,
    taker,
    tokenId: params.tokenId,
    makerAmount,
    takerAmount,
    expiration: params.expiration || calculateExpiration(),
    nonce: params.nonce || generateNonce(),
    feeRateBps: params.feeRateBps || POLYMARKET_CONFIG.FEES.DEFAULT_FEE_BPS,
    side: ORDER_SIDE_NUMERIC[params.side],
    signatureType: params.signatureType ?? SignatureType.EOA,
  };
}

/**
 * Sign an order using EIP-712 typed data
 */
export async function signOrder(
  wallet: ethers.Wallet,
  orderData: Omit<PolymarketSignedOrder, 'signature'>,
): Promise<PolymarketSignedOrder> {
  // Create the EIP-712 typed data message
  const message = {
    salt: orderData.salt,
    maker: orderData.maker,
    signer: orderData.signer,
    taker: orderData.taker,
    tokenId: orderData.tokenId,
    makerAmount: orderData.makerAmount,
    takerAmount: orderData.takerAmount,
    expiration: orderData.expiration,
    nonce: orderData.nonce,
    feeRateBps: orderData.feeRateBps,
    side: orderData.side,
    signatureType: orderData.signatureType,
  };

  // Sign using EIP-712
  const signature = await wallet.signTypedData(EIP712_DOMAIN, EIP712_ORDER_TYPES, message);

  return {
    ...orderData,
    signature,
  };
}

/**
 * Create and sign an order in one step
 */
export async function createSignedOrder(
  wallet: ethers.Wallet,
  params: {
    tokenId: string;
    price: number;
    size: number;
    side: 'BUY' | 'SELL';
    taker?: string;
    expiration?: string;
    nonce?: string;
    feeRateBps?: string;
    signatureType?: SignatureType;
  },
): Promise<PolymarketSignedOrder> {
  const maker = await wallet.getAddress();
  const signer = maker; // For EOA wallets, maker and signer are the same

  const orderData = buildOrderData({
    ...params,
    maker,
    signer,
  });

  return signOrder(wallet, orderData);
}

/**
 * Generate HMAC signature for L2 API authentication
 *
 * Polymarket L2 API uses HMAC-SHA256 for request signing.
 * Headers required:
 * - POLY_ADDRESS: API key
 * - POLY_SIGNATURE: HMAC signature
 * - POLY_TIMESTAMP: Request timestamp
 * - POLY_NONCE: Request nonce
 */
export function generateHmacSignature(
  secret: string,
  timestamp: string,
  method: string,
  path: string,
  body: string = '',
): string {
  const message = `${timestamp}${method.toUpperCase()}${path}${body}`;
  const key = Buffer.from(secret, 'base64');
  const hmac = ethers.computeHmac('sha256', key, ethers.toUtf8Bytes(message));
  return Buffer.from(ethers.getBytes(hmac)).toString('base64');
}

/**
 * Build L2 authentication headers for API requests
 */
export function buildL2Headers(
  credentials: PolymarketCredentials,
  method: string,
  path: string,
  body: string = '',
): Record<string, string> {
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const nonce = Date.now().toString();
  const signature = generateHmacSignature(credentials.secret, timestamp, method, path, body);

  return {
    'POLY_ADDRESS': credentials.apiKey,
    'POLY_SIGNATURE': signature,
    'POLY_TIMESTAMP': timestamp,
    'POLY_NONCE': nonce,
    'POLY_PASSPHRASE': credentials.passphrase,
  };
}

/**
 * Sign the message for API key derivation
 *
 * To derive API credentials, the user must sign a specific message
 * with their wallet to prove ownership.
 */
export async function signApiKeyDerivationMessage(
  wallet: ethers.Wallet,
  nonce: number,
): Promise<{ address: string; signature: string; timestamp: number }> {
  const address = await wallet.getAddress();
  const timestamp = Math.floor(Date.now() / 1000);

  // The message to sign for API key derivation
  const message = `I am signing this message for Polymarket API key derivation. Nonce: ${nonce}. Timestamp: ${timestamp}`;

  const signature = await wallet.signMessage(message);

  return {
    address,
    signature,
    timestamp,
  };
}

/**
 * Verify an order signature (for testing purposes)
 */
export function verifyOrderSignature(
  order: PolymarketSignedOrder,
  expectedSigner: string,
): boolean {
  try {
    const message = {
      salt: order.salt,
      maker: order.maker,
      signer: order.signer,
      taker: order.taker,
      tokenId: order.tokenId,
      makerAmount: order.makerAmount,
      takerAmount: order.takerAmount,
      expiration: order.expiration,
      nonce: order.nonce,
      feeRateBps: order.feeRateBps,
      side: order.side,
      signatureType: order.signatureType,
    };

    const recoveredAddress = ethers.verifyTypedData(
      EIP712_DOMAIN,
      EIP712_ORDER_TYPES,
      message,
      order.signature,
    );

    return recoveredAddress.toLowerCase() === expectedSigner.toLowerCase();
  } catch {
    return false;
  }
}

/**
 * Cancel order signature
 * Creates a signature for canceling orders
 */
export async function signCancelOrder(
  wallet: ethers.Wallet,
  orderIds: string[],
): Promise<{ signature: string; timestamp: number }> {
  const timestamp = Math.floor(Date.now() / 1000);
  const message = `Cancel orders: ${orderIds.join(',')}. Timestamp: ${timestamp}`;
  const signature = await wallet.signMessage(message);

  return { signature, timestamp };
}
