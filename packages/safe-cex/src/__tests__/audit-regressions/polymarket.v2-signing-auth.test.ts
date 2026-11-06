import { afterEach, describe, expect, it, vi } from 'vitest';
import { ethers } from 'ethers';

import {
  EIP712_DOMAIN,
  EIP712_ORDER_TYPES,
  POLYMARKET_CONFIG,
  SignatureType,
} from '../reference/polymarket/polymarket.constants';
import {
  buildL2Headers,
  createSignedOrder,
  generateHmacSignature,
  signApiKeyDerivationMessage,
} from '../reference/polymarket/polymarket.signing';

const PRIVATE_KEY = `0x${'11'.repeat(32)}`;
const ZERO_BYTES32 = `0x${'00'.repeat(32)}`;

afterEach(() => vi.useRealTimers());

describe('Polymarket CLOB V2 signing and authentication', () => {
  it.each([
    [false, POLYMARKET_CONFIG.CONTRACTS.EXCHANGE_V2],
    [true, POLYMARKET_CONFIG.CONTRACTS.NEG_RISK_EXCHANGE_V2],
  ])('signs the V2 order struct against the correct neg-risk contract', async (negRisk, contract) => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-15T10:00:00.000Z'));
    const wallet = new ethers.Wallet(PRIVATE_KEY);

    const order = await createSignedOrder(wallet, {
      tokenId: '123456789',
      price: 0.42,
      size: 10,
      side: 'BUY',
      tickSize: '0.01',
      negRisk,
    });

    expect(order).toMatchObject({
      maker: wallet.address,
      signer: wallet.address,
      tokenId: '123456789',
      makerAmount: '4200000',
      takerAmount: '10000000',
      side: 'BUY',
      signatureType: SignatureType.EOA,
      timestamp: Date.now().toString(),
      expiration: '0',
      metadata: ZERO_BYTES32,
      builder: ZERO_BYTES32,
    });
    expect(order).not.toHaveProperty('nonce');
    expect(order).not.toHaveProperty('feeRateBps');
    expect(order).not.toHaveProperty('taker');

    const recovered = ethers.verifyTypedData(
      { ...EIP712_DOMAIN, verifyingContract: contract },
      EIP712_ORDER_TYPES,
      {
        salt: order.salt,
        maker: order.maker,
        signer: order.signer,
        tokenId: order.tokenId,
        makerAmount: order.makerAmount,
        takerAmount: order.takerAmount,
        side: 0,
        signatureType: order.signatureType,
        timestamp: order.timestamp,
        metadata: order.metadata,
        builder: order.builder,
      },
      order.signature,
    );
    expect(recovered).toBe(wallet.address);
  });

  it('uses the official POLY_1271 wrapped-signature path for deposit wallets', async () => {
    const wallet = new ethers.Wallet(PRIVATE_KEY);
    const depositWallet = '0x2222222222222222222222222222222222222222';

    const order = await createSignedOrder(wallet, {
      tokenId: '123456789',
      price: 0.42,
      size: 10,
      side: 'SELL',
      tickSize: '0.01',
      negRisk: false,
      signatureType: SignatureType.POLY_1271,
      funderAddress: depositWallet,
    });

    expect(order).toMatchObject({
      maker: depositWallet,
      signer: depositWallet,
      signatureType: SignatureType.POLY_1271,
    });
    expect(order.signature.length).toBeGreaterThan(132);
  });

  it('signs the documented ClobAuth typed data with the server timestamp', async () => {
    const wallet = new ethers.Wallet(PRIVATE_KEY);
    const timestamp = 1_789_000_000;
    const result = await signApiKeyDerivationMessage(wallet, 0, timestamp);

    const recovered = ethers.verifyTypedData(
      { name: 'ClobAuthDomain', version: '1', chainId: 137 },
      {
        ClobAuth: [
          { name: 'address', type: 'address' },
          { name: 'timestamp', type: 'string' },
          { name: 'nonce', type: 'uint256' },
          { name: 'message', type: 'string' },
        ],
      },
      {
        address: wallet.address,
        timestamp: timestamp.toString(),
        nonce: 0,
        message: 'This message attests that I control the given wallet',
      },
      result.signature,
    );

    expect(result).toMatchObject({ address: wallet.address, timestamp });
    expect(recovered).toBe(wallet.address);
  });

  it('matches the official URL-safe-base64 L2 fixture and five-header contract', () => {
    const body = '{"a":1}';
    expect(generateHmacSignature('c2VjcmV0', '1700000000', 'POST', '/order', body)).toBe(
      'RG08Lr_BXWneCX42aD1tpDtvA9AAzyktUouVy2A61gk=',
    );
    expect(generateHmacSignature('c2VjcmV0MQ', '1700000000', 'POST', '/order', body)).toBe(
      'CjbD92QPecdPjgXGx1TYxfmyRqDIS3Krl4Q-ndPyopY=',
    );
    expect(
      buildL2Headers(
        { apiKey: 'api-key', secret: 'c2VjcmV0', passphrase: 'pass' },
        '0x1111111111111111111111111111111111111111',
        'POST',
        '/order',
        body,
        1_700_000_000,
      ),
    ).toEqual({
      POLY_ADDRESS: '0x1111111111111111111111111111111111111111',
      POLY_SIGNATURE: 'RG08Lr_BXWneCX42aD1tpDtvA9AAzyktUouVy2A61gk=',
      POLY_TIMESTAMP: '1700000000',
      POLY_API_KEY: 'api-key',
      POLY_PASSPHRASE: 'pass',
    });
  });
});
