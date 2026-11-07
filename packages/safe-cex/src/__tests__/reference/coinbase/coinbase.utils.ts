import { ORDER_EVENT } from './coinbase.types';
import { OrderStatus, OrderType } from '@tealstreet/safe-cex/src/types';
import { hashes, sign as ed25519Sign } from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2.js';

import { uuid } from '../../../utils/uuid';
import { virtualClock } from '../../../utils/virtualClock';

const COINBASE_USDC_ALIAS_EXCEPTIONS = new Set(['USDT-USDC', 'EURC-USDC']);
const COINBASE_KEY_NAME_PATTERN = /^organizations\/[^/]+\/apiKeys\/[^/]+$/;
const COINBASE_PRIVATE_KEY_PATTERN = /-----BEGIN (?:EC )?PRIVATE KEY-----[\s\S]+-----END (?:EC )?PRIVATE KEY-----/;
const COINBASE_ED25519_PREFIX = /^ed25519:/i;

hashes.sha512 = (message) => Uint8Array.from(sha512(message));

type CoinbaseCredentialJson = {
  name?: unknown;
  privateKey?: unknown;
  private_key?: unknown;
  keySecret?: unknown;
  key_secret?: unknown;
  secret?: unknown;
};

export const normalizeCoinbaseUsdEquivalentProductId = (productId: string): string => {
  if (productId.endsWith('-USDC') && !COINBASE_USDC_ALIAS_EXCEPTIONS.has(productId)) {
    return `${productId.slice(0, -5)}-USD`;
  }
  return productId;
};

export const getCoinbaseUsdEquivalentAliasProductId = (productId: string): string | undefined => {
  const normalized = normalizeCoinbaseUsdEquivalentProductId(productId);
  if (normalized !== productId) return normalized;

  if (productId.endsWith('-USD')) {
    return `${productId.slice(0, -4)}-USDC`;
  }

  return undefined;
};

export const resolveCoinbaseUsdEquivalentMarket = <T>(
  productId: string,
  marketsById: Record<string, T>,
): T | undefined => {
  const exact = marketsById[productId];
  const aliasProductId = getCoinbaseUsdEquivalentAliasProductId(productId);
  const alias = aliasProductId ? marketsById[aliasProductId] : undefined;

  // Coinbase often reports ordinary USDC-book activity on the equivalent USD
  // product. Prefer the user-facing USDC market when both are loaded so exact
  // symbol filters in the UI still match the selected market.
  if (productId.endsWith('-USD') && alias) return alias;

  return exact ?? alias;
};

const getCoinbaseJsonSecret = (value: CoinbaseCredentialJson | undefined): string | undefined => {
  if (typeof value?.privateKey === 'string') return value.privateKey;
  if (typeof value?.private_key === 'string') return value.private_key;
  if (typeof value?.keySecret === 'string') return value.keySecret;
  if (typeof value?.key_secret === 'string') return value.key_secret;
  if (typeof value?.secret === 'string') return value.secret;
  return undefined;
};

const parseCoinbaseCredentialJson = (value: string): CoinbaseCredentialJson | undefined => {
  const trimmed = value.trim();
  if (!trimmed.startsWith('{')) return undefined;

  try {
    const parsed = JSON.parse(trimmed);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? (parsed as CoinbaseCredentialJson) : undefined;
  } catch {
    return undefined;
  }
};

const base64ToBytes = (value: string): Uint8Array | undefined => {
  const normalized = value.replace(COINBASE_ED25519_PREFIX, '').replace(/\s/g, '').replace(/-/g, '+').replace(/_/g, '/');
  if (!normalized || !/^[A-Za-z0-9+/=]+$/.test(normalized)) return undefined;

  try {
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=');
    return Uint8Array.from(atob(padded), (char) => char.charCodeAt(0));
  } catch {
    return undefined;
  }
};

export const normalizeCoinbaseApiCredentials = (
  key: string,
  secret: string,
): { algorithm: 'ES256' | 'EdDSA'; ed25519Seed?: Uint8Array; keyName: string; keySecret: string } => {
  const keyJson = parseCoinbaseCredentialJson(key);
  const secretJson = parseCoinbaseCredentialJson(secret);
  const keyName = (typeof keyJson?.name === 'string' ? keyJson.name : key).trim();
  const keySecret = (getCoinbaseJsonSecret(secretJson) ?? getCoinbaseJsonSecret(keyJson) ?? secret).replace(/\\n/g, '\n').trim();

  if (!COINBASE_KEY_NAME_PATTERN.test(keyName)) {
    throw new Error(
      'Coinbase API key must be the Advanced Trade CDP key name, like organizations/{org_id}/apiKeys/{key_id}.',
    );
  }

  if (COINBASE_PRIVATE_KEY_PATTERN.test(keySecret)) {
    return { algorithm: 'ES256', keyName, keySecret };
  }

  const ed25519Secret = base64ToBytes(keySecret);
  if (ed25519Secret && (ed25519Secret.length === 32 || ed25519Secret.length === 64)) {
    return { algorithm: 'EdDSA', ed25519Seed: ed25519Secret.slice(0, 32), keyName, keySecret };
  }

  throw new Error(
    'Coinbase API secret must be the Advanced Trade private key: either an ES256 privateKey PEM or an Ed25519 base64 key secret from Coinbase.',
  );
};

// Cache imported CryptoKey to avoid re-importing on every request
let cachedCryptoKey: CryptoKey | null = null;
let cachedKeySecret = '';

function base64UrlEncode(data: Uint8Array | ArrayBuffer): string {
  const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : data;
  let b64 = '';
  for (let i = 0; i < bytes.length; i++) {
    b64 += String.fromCharCode(bytes[i]);
  }
  return btoa(b64).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64UrlEncodeString(str: string): string {
  return base64UrlEncode(new TextEncoder().encode(str));
}

/**
 * Wrap data in an ASN.1 TLV (Tag-Length-Value) structure.
 */
function asn1Wrap(tag: number, data: Uint8Array): Uint8Array {
  const len = data.length;
  let header: Uint8Array;
  if (len < 128) {
    header = new Uint8Array([tag, len]);
  } else if (len < 256) {
    header = new Uint8Array([tag, 0x81, len]);
  } else {
    header = new Uint8Array([tag, 0x82, (len >> 8) & 0xff, len & 0xff]);
  }
  const result = new Uint8Array(header.length + len);
  result.set(header, 0);
  result.set(data, header.length);
  return result;
}

/**
 * Convert PEM private key to PKCS8 DER format for Web Crypto API.
 * Handles both SEC1 (-----BEGIN EC PRIVATE KEY-----) and PKCS8 (-----BEGIN PRIVATE KEY-----) formats.
 * Coinbase provides SEC1 format, but Web Crypto requires PKCS8.
 */
function pemToPkcs8Der(pem: string): ArrayBuffer {
  const isPkcs8 = pem.includes('-----BEGIN PRIVATE KEY-----');

  const b64 = pem
    .replace(/-----BEGIN (EC )?PRIVATE KEY-----/, '')
    .replace(/-----END (EC )?PRIVATE KEY-----/, '')
    .replace(/\s/g, '');
  const derBytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

  if (isPkcs8) {
    return derBytes.buffer as ArrayBuffer;
  }

  // SEC1 → PKCS8: wrap SEC1 DER in PKCS8 PrivateKeyInfo structure
  // SEQUENCE { INTEGER 0, SEQUENCE { OID ecPublicKey, OID secp256r1 }, OCTET STRING { SEC1 } }
  const version = new Uint8Array([0x02, 0x01, 0x00]);
  const algId = new Uint8Array([
    0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03,
    0x01, 0x07,
  ]);
  const octetString = asn1Wrap(0x04, derBytes);

  const inner = new Uint8Array(version.length + algId.length + octetString.length);
  inner.set(version, 0);
  inner.set(algId, version.length);
  inner.set(octetString, version.length + algId.length);

  return asn1Wrap(0x30, inner).buffer as ArrayBuffer;
}

async function getSigningKey(secret: string): Promise<CryptoKey> {
  if (cachedCryptoKey && cachedKeySecret === secret) {
    return cachedCryptoKey;
  }

  const pkcs8Der = pemToPkcs8Der(secret);
  cachedCryptoKey = (await crypto.subtle.importKey('pkcs8', pkcs8Der, { name: 'ECDSA', namedCurve: 'P-256' }, false, [
    'sign',
  ])) as CryptoKey;
  cachedKeySecret = secret;
  return cachedCryptoKey;
}

export const sign = async (key: string, secret: string, uri?: string): Promise<string> => {
  const { algorithm, ed25519Seed, keyName: key_name, keySecret: key_secret } = normalizeCoinbaseApiCredentials(key, secret);

  const now = virtualClock.getCurrentTimeMs();

  const header = {
    alg: algorithm,
    typ: 'JWT',
    kid: key_name,
    nonce: uuid(),
  };

  const payload =
    algorithm === 'EdDSA'
      ? {
          iss: 'cdp',
          aud: ['cdp_service'],
          nbf: Math.floor(now / 1000),
          exp: Math.floor(now / 1000) + 120,
          sub: key_name,
          ...(uri ? { uri } : {}),
        }
      : {
          iss: 'coinbase-cloud',
          nbf: Math.floor(now / 1000),
          exp: Math.floor(now / 1000) + 120,
          sub: key_name,
          ...(uri ? { uri } : {}),
        };

  const encodedHeader = base64UrlEncodeString(JSON.stringify(header));
  const encodedPayload = base64UrlEncodeString(JSON.stringify(payload));
  const signingInput = `${encodedHeader}.${encodedPayload}`;

  const message = new TextEncoder().encode(signingInput);
  const signature =
    algorithm === 'EdDSA'
      ? ed25519Sign(message, ed25519Seed!)
      : await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, (await getSigningKey(key_secret)) as any, message);

  return `${signingInput}.${base64UrlEncode(signature)}`;
};

export const formatOrderState = (rawState: ORDER_EVENT['orders'][0]['status']): OrderStatus => {
  switch (rawState) {
    case 'PENDING':
      return OrderStatus.Open;
    case 'OPEN':
      return OrderStatus.Open;
    case 'FILLED':
      return OrderStatus.Closed;
    case 'CANCELLED':
      return OrderStatus.Canceled;
    case 'EXPIRED':
      return OrderStatus.Canceled;
    case 'FAILED':
      return OrderStatus.Canceled;
    default:
      console.error('Unknown order state', rawState);
      return OrderStatus.Open;
  }
};

export const formatOrderType = (rawType: ORDER_EVENT['orders'][0]['order_type']): OrderType => {
  switch (rawType) {
    case 'Market':
      return OrderType.Market;
    case 'Limit':
      return OrderType.Limit;
    case 'Stop Limit':
      return OrderType.StopLoss;
    default:
      console.error('Unknown order type', rawType);
      return OrderType.Limit;
  }
};

/**
 * Coinbase CFM quantities arrive in contracts. Expiring products must have a
 * positive finite base-unit multiplier before they can enter the store.
 */
export const normalizeCfmContractSize = (value: unknown): number | undefined => {
  const contractSize = typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : Number.NaN;
  return Number.isFinite(contractSize) && contractSize > 0 ? contractSize : undefined;
};
