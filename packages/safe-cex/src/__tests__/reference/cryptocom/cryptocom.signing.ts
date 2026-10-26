import createHmac from 'create-hmac';

import { virtualClock } from '../../../utils/virtualClock';

const MAX_PARAM_STRING_LEVEL = 3;

const paramsToString = (value: unknown, level = 0): string => {
  if (level >= MAX_PARAM_STRING_LEVEL) return String(value);
  if (value === null) return 'null';

  if (Array.isArray(value)) {
    return value.map((item) => paramsToString(item, level + 1)).join('');
  }

  if (typeof value === 'object' && value !== undefined) {
    return Object.keys(value)
      .sort()
      .map((key) => `${key}${paramsToString((value as Record<string, unknown>)[key], level + 1)}`)
      .join('');
  }

  return String(value);
};

/**
 * Generate HMAC-SHA256 signature for Crypto.com API requests
 *
 * Signature generation process:
 * 1. Sort parameters alphabetically by key
 * 2. Concatenate key-value pairs: key1value1key2value2
 * 3. Build signature payload: method + id + api_key + params + nonce
 * 4. Compute HMAC-SHA256 using API secret
 * 5. Return as hexadecimal string
 */
export function generateSignature(
  method: string,
  id: number,
  apiKey: string,
  apiSecret: string,
  params: Record<string, unknown>,
  nonce: number,
): string {
  const paramString = paramsToString(params);

  // Build the signature payload
  const signPayload = `${method}${id}${apiKey}${paramString}${nonce}`;

  // Compute HMAC-SHA256
  const signature = createHmac('sha256', apiSecret).update(signPayload).digest('hex');

  return signature;
}

/**
 * Generate authenticated request payload for Crypto.com API
 */
export function generateAuthenticatedRequest(
  method: string,
  params: Record<string, unknown>,
  apiKey: string,
  apiSecret: string,
  nonce?: number,
): {
  id: number;
  method: string;
  api_key: string;
  params: Record<string, unknown>;
  sig: string;
  nonce: number;
} {
  const id = virtualClock.getCurrentTimeMs();
  const requestNonce = nonce ?? virtualClock.getCurrentTimeMs();

  const signature = generateSignature(method, id, apiKey, apiSecret, params, requestNonce);

  return {
    id,
    method,
    api_key: apiKey,
    params,
    sig: signature,
    nonce: requestNonce,
  };
}

/**
 * Generate WebSocket authentication message
 */
export function generateWsAuthMessage(
  apiKey: string,
  apiSecret: string,
): {
  id: number;
  method: string;
  api_key: string;
  sig: string;
  nonce: number;
} {
  const id = virtualClock.getCurrentTimeMs();
  const nonce = virtualClock.getCurrentTimeMs();

  // For auth method, params is empty
  const signature = generateSignature('public/auth', id, apiKey, apiSecret, {}, nonce);

  return {
    id,
    method: 'public/auth',
    api_key: apiKey,
    sig: signature,
    nonce,
  };
}
