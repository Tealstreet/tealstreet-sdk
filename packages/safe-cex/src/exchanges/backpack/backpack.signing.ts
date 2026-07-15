import * as ed25519 from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2.js';

import { virtualClock } from '../../utils/virtualClock';

const sha512ForEd25519: NonNullable<typeof ed25519.hashes.sha512> = (message) =>
  Uint8Array.from(sha512(message));

// Set the crypto hash function for noble/ed25519 v3
ed25519.hashes.sha512 = sha512ForEd25519;

/**
 * Generate ED25519 signature for Backpack API requests
 * @param instruction - The instruction type (e.g., "orderExecute", "balanceQuery")
 * @param params - Request parameters (will be sorted alphabetically)
 * @param timestamp - Unix timestamp in milliseconds
 * @param window - Validity window in milliseconds (default: 5000, max: 60000)
 * @param privateKeyBase64 - Base64-encoded ED25519 private key
 * @returns Base64-encoded signature
 */
export async function signRequest(
  instruction: string,
  params: Record<string, any>,
  timestamp: number,
  window: number,
  privateKeyBase64: string,
): Promise<string> {
  // 1. Sort parameters alphabetically and convert to query string
  const sortedKeys = Object.keys(params).sort();
  const queryParts: string[] = [];

  for (const key of sortedKeys) {
    const value = params[key];
    if (value !== null && value !== undefined) {
      queryParts.push(`${key}=${encodeURIComponent(String(value))}`);
    }
  }

  const queryString = queryParts.join('&');

  // 2. Build the signature payload: instruction=<instruction>&<params>&timestamp=<timestamp>&window=<window>
  const signaturePayload = `instruction=${instruction}${queryString ? '&' + queryString : ''}&timestamp=${timestamp}&window=${window}`;

  // 3. Convert to bytes
  const messageBytes = new TextEncoder().encode(signaturePayload);

  // 4. Decode private key from base64
  const privateKeyBytes = Uint8Array.from(Buffer.from(privateKeyBase64, 'base64'));

  // 5. Sign the message with ED25519
  const signatureBytes = await ed25519.signAsync(messageBytes, privateKeyBytes);

  // 6. Encode signature to base64
  return Buffer.from(signatureBytes).toString('base64');
}

/**
 * Generate authentication headers for Backpack API requests
 * @param instruction - The instruction type
 * @param params - Request parameters
 * @param apiKey - Base64-encoded public key
 * @param secret - Base64-encoded private key
 * @param window - Validity window in milliseconds (default: 5000)
 * @returns Headers object with authentication
 */
export async function generateAuthHeaders(
  instruction: string,
  params: Record<string, any>,
  apiKey: string,
  secret: string,
  window: number = 5000,
): Promise<Record<string, string>> {
  const timestamp = virtualClock.getCurrentTimeMs();
  const signature = await signRequest(instruction, params, timestamp, window, secret);

  return {
    'X-API-Key': apiKey,
    'X-Signature': signature,
    'X-Timestamp': timestamp.toString(),
    'X-Window': window.toString(),
    'Content-Type': 'application/json',
  };
}

/**
 * Generate WebSocket authentication signature
 * @param apiKey - Base64-encoded public key
 * @param secret - Base64-encoded private key
 * @param window - Validity window in milliseconds (default: 5000)
 * @returns Array with [key, signature, timestamp, window] as strings
 */
export async function generateWsAuthSignature(
  apiKey: string,
  secret: string,
  window: number = 5000,
): Promise<[string, string, string, string]> {
  const timestamp = virtualClock.getCurrentTimeMs();

  // For WebSocket subscription, signature payload is just: instruction=subscribe&timestamp=...&window=...
  const signature = await signRequest('subscribe', {}, timestamp, window, secret);

  // Return as array of strings per API spec
  return [apiKey, signature, String(timestamp), String(window)];
}

/**
 * Verify ED25519 signature (for testing purposes)
 * @param instruction - The instruction type
 * @param params - Request parameters
 * @param timestamp - Unix timestamp in milliseconds
 * @param window - Validity window in milliseconds
 * @param signature - Base64-encoded signature
 * @param publicKeyBase64 - Base64-encoded public key
 * @returns True if signature is valid
 */
export async function verifySignature(
  instruction: string,
  params: Record<string, any>,
  timestamp: number,
  window: number,
  signature: string,
  publicKeyBase64: string,
): Promise<boolean> {
  try {
    // Reconstruct the signature payload
    const sortedKeys = Object.keys(params).sort();
    const queryParts: string[] = [];

    for (const key of sortedKeys) {
      const value = params[key];
      if (value !== null && value !== undefined) {
        queryParts.push(`${key}=${encodeURIComponent(String(value))}`);
      }
    }

    const queryString = queryParts.join('&');
    const signaturePayload = `instruction=${instruction}${queryString ? '&' + queryString : ''}&timestamp=${timestamp}&window=${window}`;

    const messageBytes = new TextEncoder().encode(signaturePayload);
    const publicKeyBytes = Uint8Array.from(Buffer.from(publicKeyBase64, 'base64'));
    const signatureBytes = Uint8Array.from(Buffer.from(signature, 'base64'));

    return await ed25519.verifyAsync(signatureBytes, messageBytes, publicKeyBytes);
  } catch {
    return false;
  }
}
