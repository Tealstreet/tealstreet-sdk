import { ORDER_EVENT } from '@tealstreet/safe-cex/src/exchanges/coinbase/coinbase.types';
import { OrderStatus, OrderType } from '@tealstreet/safe-cex/src/types';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';

const COINBASE_USDC_ALIAS_EXCEPTIONS = new Set(['USDT-USDC', 'EURC-USDC']);

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
  const key_name = key;
  const key_secret = secret.replace(/\\n/g, '\n');

  const now = virtualClock.getCurrentTimeMs();

  const header = {
    alg: 'ES256',
    typ: 'JWT',
    kid: key_name,
    nonce: uuid(),
  };

  const payload = {
    iss: 'coinbase-cloud',
    nbf: Math.floor(now / 1000),
    exp: Math.floor(now / 1000) + 120,
    sub: key_name,
    ...(uri ? { uri } : {}),
  };

  const encodedHeader = base64UrlEncodeString(JSON.stringify(header));
  const encodedPayload = base64UrlEncodeString(JSON.stringify(payload));
  const signingInput = `${encodedHeader}.${encodedPayload}`;

  const signingKey = await getSigningKey(key_secret);
  const signature = await crypto.subtle.sign(
    { name: 'ECDSA', hash: 'SHA-256' },
    signingKey as any,
    new TextEncoder().encode(signingInput),
  );

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
