import createHmac from 'create-hmac';

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
  // Sort parameters alphabetically by key
  const sortedKeys = Object.keys(params).sort();

  // Concatenate key-value pairs
  const paramString = sortedKeys.map((key) => `${key}${params[key]}`).join('');

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
  const id = Date.now();
  const requestNonce = nonce || Date.now();

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
  const id = Date.now();
  const nonce = Date.now();

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
