import * as crypto from 'crypto';

import { ethers } from 'ethers';
import { describe, expect, it } from 'vitest';

import type { ExchangeOptions } from '../../../types';
import {
  getAsterDEXFapiPath,
  hasAsterDEXV3AuthContext,
  isAsterDEXAddress,
  isAsterDEXV3PrivateKey,
  normalizeAsterDEXRealizedPnlDeltas,
  resolveAsterDEXApiVersion,
  resolveAsterDEXV3AuthContext,
} from '../asterdex.types';

const buildV1QueryString = (params: Record<string, string | number>): string => {
  return Object.entries(params)
    .filter(([_, value]) => value !== undefined && value !== null && value !== '')
    .map(([key, value]) => `${key}=${String(value)}`)
    .join('&');
};

const buildV3Payload = (params: Record<string, string | number>): Record<string, string> => {
  const payload: Record<string, string> = {};
  for (const key of Object.keys(params).sort()) {
    payload[key] = String(params[key]);
  }
  return payload;
};

const signV3 = (
  params: Record<string, string | number>,
  userAddress: string,
  signerAddress: string,
  signerPrivateKey: string,
  nonce: string,
): { payloadJson: string; payloadHash: string; ethSignedHash: string; signature: string } => {
  const payload = buildV3Payload(params);
  const payloadJson = JSON.stringify(payload);
  const encodedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
    ['string', 'address', 'address', 'uint256'],
    [payloadJson, userAddress, signerAddress, BigInt(nonce)],
  );
  const payloadHash = ethers.keccak256(encodedPayload);
  const ethSignedHash = ethers.hashMessage(ethers.getBytes(payloadHash));
  const normalizedPrivateKey = signerPrivateKey.startsWith('0x') ? signerPrivateKey : `0x${signerPrivateKey}`;
  const signerKey = new ethers.SigningKey(normalizedPrivateKey);
  const signature = ethers.Signature.from(signerKey.sign(ethSignedHash)).serialized;
  return { payloadJson, payloadHash, ethSignedHash, signature };
};

const createOptions = (overrides: Partial<ExchangeOptions> = {}): ExchangeOptions => {
  return {
    key: 'api-key',
    secret: 'api-secret',
    ...overrides,
  };
};

describe('AsterDEX API uplift behavior', () => {
  describe('version/auth resolution', () => {
    it('keeps V1 default for classic key/secret accounts', () => {
      const options = createOptions();
      expect(resolveAsterDEXApiVersion(options)).toBe('v1');
    });

    it('enables V3 when wallet auth context is fully configured', () => {
      const options = createOptions({
        auth: {
          userAddress: '0x1234567890123456789012345678901234567890',
          signerAddress: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
          privateKey: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        },
      });
      expect(hasAsterDEXV3AuthContext(options)).toBe(true);
      expect(resolveAsterDEXApiVersion(options)).toBe('v3');
    });

    it('does not auto-switch to V3 for non-private-key secrets', () => {
      const options = createOptions({
        secret: 'regular-api-secret',
        auth: {
          userAddress: '0x1234567890123456789012345678901234567890',
          signerAddress: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        },
      });
      expect(hasAsterDEXV3AuthContext(options)).toBe(false);
      expect(resolveAsterDEXApiVersion(options)).toBe('v1');
    });

    it('enables V3 from applicationId + apiKey + private-key secret fallback fields', () => {
      const options = createOptions({
        key: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        applicationId: '0x1234567890123456789012345678901234567890',
      });
      const auth = resolveAsterDEXV3AuthContext(options);

      expect(auth.userAddress).toBe('0x1234567890123456789012345678901234567890');
      expect(auth.signerAddress).toBe('0xabcdefabcdefabcdefabcdefabcdefabcdefabcd');
      expect(hasAsterDEXV3AuthContext(options)).toBe(true);
      expect(resolveAsterDEXApiVersion(options)).toBe('v3');
    });

    it('does not auto-switch to V3 when addresses are malformed', () => {
      const options = createOptions({
        key: 'not-an-address',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        applicationId: '0x1234567890123456789012345678901234567890',
      });

      expect(hasAsterDEXV3AuthContext(options)).toBe(false);
      expect(resolveAsterDEXApiVersion(options)).toBe('v1');
    });

    it('supports explicit API version override in options.extra', () => {
      const forcedV3 = createOptions({ extra: { asterdexApiVersion: 'v3' } });
      const forcedV1 = createOptions({ extra: { asterdexApiVersion: 'v1' } });
      expect(resolveAsterDEXApiVersion(forcedV3)).toBe('v3');
      expect(resolveAsterDEXApiVersion(forcedV1)).toBe('v1');
    });
  });

  describe('path/version helpers', () => {
    it('builds versioned fapi paths deterministically', () => {
      expect(getAsterDEXFapiPath('order', 'v1')).toBe('/fapi/v1/order');
      expect(getAsterDEXFapiPath('/order', 'v3')).toBe('/fapi/v3/order');
      expect(getAsterDEXFapiPath('account', 'v4')).toBe('/fapi/v4/account');
    });
  });

  describe('private key validation', () => {
    it('accepts 64-byte hex private keys with and without 0x', () => {
      expect(isAsterDEXV3PrivateKey('0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef')).toBe(
        true,
      );
      expect(
        isAsterDEXV3PrivateKey('0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'),
      ).toBe(true);
    });

    it('rejects malformed private keys', () => {
      expect(isAsterDEXV3PrivateKey('regular-api-secret')).toBe(false);
      expect(isAsterDEXV3PrivateKey('0x1234')).toBe(false);
      expect(isAsterDEXV3PrivateKey(undefined)).toBe(false);
    });
  });

  describe('address validation', () => {
    it('accepts 20-byte hex addresses with 0x prefix', () => {
      expect(isAsterDEXAddress('0x1234567890123456789012345678901234567890')).toBe(true);
      expect(isAsterDEXAddress('0xabcdefabcdefabcdefabcdefabcdefabcdefabcd')).toBe(true);
    });

    it('rejects malformed addresses', () => {
      expect(isAsterDEXAddress('1234567890123456789012345678901234567890')).toBe(false);
      expect(isAsterDEXAddress('0x1234')).toBe(false);
      expect(isAsterDEXAddress(undefined)).toBe(false);
    });
  });

  describe('signature fixtures', () => {
    it('matches expected V1 HMAC signature for deterministic payload', () => {
      const params = {
        symbol: 'BTCUSDT',
        side: 'BUY',
        type: 'LIMIT',
        quantity: 0.001,
        price: 40000,
        recvWindow: 20000,
        timestamp: 1710000000000,
      };
      const query = buildV1QueryString(params);
      const signature = crypto.createHmac('sha256', 'test-secret').update(query).digest('hex');

      expect(query).toBe(
        'symbol=BTCUSDT&side=BUY&type=LIMIT&quantity=0.001&price=40000&recvWindow=20000&timestamp=1710000000000',
      );
      expect(signature).toBe('9d927243240134ef612dae111dbbb6bd1d8629e5a338294191c5d2132998da35');
    });

    it('matches expected V3 keccak+ecdsa signature for deterministic payload', () => {
      const params = {
        symbol: 'BTCUSDT',
        side: 'BUY',
        type: 'LIMIT',
        quantity: 0.001,
        price: 40000,
        recvWindow: 20000,
        timestamp: 1710000000000,
      };
      const { payloadJson, payloadHash, ethSignedHash, signature } = signV3(
        params,
        '0x1234567890123456789012345678901234567890',
        '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        '1710000000000000',
      );

      expect(payloadJson).toBe(
        '{"price":"40000","quantity":"0.001","recvWindow":"20000","side":"BUY","symbol":"BTCUSDT","timestamp":"1710000000000","type":"LIMIT"}',
      );
      expect(payloadHash).toBe('0xe355b2e2c1a967d413d73d82196cc20e5165ee2332f8e60b99c90a49fed1a65d');
      expect(ethSignedHash).toBe('0x3f83003b55c3585b97cdbbb25f0b90059cbc1de9346d7797bee167bf93fc605c');
      expect(signature).toBe(
        '0x9cf61685767ddefb04d93694b8dbb3e150a8a6f9a45572274fc0b308823eda89432a059f359a3f7f159381a50906b55e68ac5c5bfe0249d3afcb200b4e54714a1c',
      );
    });
  });

  describe('auth context extraction', () => {
    it('prefers auth.privateKey over secret when both exist', () => {
      const options = createOptions({
        secret: 'regular-secret',
        auth: {
          userAddress: '0x1234567890123456789012345678901234567890',
          signerAddress: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
          privateKey: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        },
      });

      const auth = resolveAsterDEXV3AuthContext(options);
      expect(auth.signerPrivateKey).toBe('0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef');
    });

    it('falls back to applicationId and key when auth addresses are absent', () => {
      const options = createOptions({
        key: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        applicationId: '0x1234567890123456789012345678901234567890',
      });

      const auth = resolveAsterDEXV3AuthContext(options);
      expect(auth.userAddress).toBe('0x1234567890123456789012345678901234567890');
      expect(auth.signerAddress).toBe('0xabcdefabcdefabcdefabcdefabcdefabcdefabcd');
      expect(auth.signerPrivateKey).toBe('0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef');
    });
  });

  describe('historical realized PnL normalization', () => {
    it('converts cumulative realized PnL into per-trade deltas', () => {
      const normalized = normalizeAsterDEXRealizedPnlDeltas([
        {
          symbol: 'BTCUSDT',
          id: 300,
          orderId: 3,
          price: '100',
          qty: '1',
          realizedPnl: '-69',
          side: 'SELL',
          positionSide: 'BOTH',
          time: 300,
        },
        {
          symbol: 'BTCUSDT',
          id: 200,
          orderId: 2,
          price: '100',
          qty: '1',
          realizedPnl: '-69',
          side: 'SELL',
          positionSide: 'BOTH',
          time: 200,
        },
        {
          symbol: 'BTCUSDT',
          id: 100,
          orderId: 1,
          price: '100',
          qty: '1',
          realizedPnl: '-50',
          side: 'BUY',
          positionSide: 'BOTH',
          time: 100,
        },
      ]);

      expect(normalized).toEqual([0, -19, 0]);
    });

    it('tracks cumulative baselines independently per position side', () => {
      const normalized = normalizeAsterDEXRealizedPnlDeltas([
        {
          symbol: 'BTCUSDT',
          id: 4,
          orderId: 4,
          price: '100',
          qty: '1',
          realizedPnl: '12',
          side: 'BUY',
          positionSide: 'SHORT',
          time: 400,
        },
        {
          symbol: 'BTCUSDT',
          id: 3,
          orderId: 3,
          price: '100',
          qty: '1',
          realizedPnl: '7',
          side: 'SELL',
          positionSide: 'LONG',
          time: 300,
        },
        {
          symbol: 'BTCUSDT',
          id: 2,
          orderId: 2,
          price: '100',
          qty: '1',
          realizedPnl: '10',
          side: 'BUY',
          positionSide: 'SHORT',
          time: 200,
        },
        {
          symbol: 'BTCUSDT',
          id: 1,
          orderId: 1,
          price: '100',
          qty: '1',
          realizedPnl: '5',
          side: 'SELL',
          positionSide: 'LONG',
          time: 100,
        },
      ]);

      expect(normalized).toEqual([2, 2, 0, 0]);
    });
  });
});
