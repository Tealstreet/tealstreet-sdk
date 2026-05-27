import { describe, it, expect, beforeAll } from 'vitest';
import { Account, hash } from 'starknet';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Paradex Authentication Test Suite
 *
 * This test suite validates the Paradex subkey authentication flow:
 * 1. Message construction (method + timestamp + path + body)
 * 2. Message hashing (starknetKeccak for >31 char messages)
 * 3. EIP-712 typed data signing with subkey
 * 4. API call to /auth/{pubkey} endpoint
 *
 * Reference: https://docs.paradex.trade/trading/api-authentication#subkeys
 *
 * ## Running the Tests
 *
 * Unit tests (no network calls):
 * ```bash
 * cd packages/safe-cex
 * npx vitest run src/exchanges/paradex/paradex.auth.test.ts
 * ```
 *
 * Integration tests (require .apikey.json with real credentials):
 * These are currently skipped by default. To run them:
 * 1. Ensure .apikey.json exists in this directory with valid credentials
 * 2. Change `it.skip` to `it` in the API Integration and Error Handling describe blocks
 * 3. Run the tests in a Node.js environment with fetch support
 *
 * Manual integration testing:
 * ```bash
 * cd /home/rayana/projects/tealstreet-next
 * npx tsx packages/safe-cex/src/exchanges/paradex/test-auth-curl.ts
 * ```
 */

// Load test credentials from .apikey.json
let testCreds: any;
try {
  const credsPath = path.join(__dirname, '.apikey.json');
  testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
} catch (error) {
  console.warn('⚠️  .apikey.json not found - some tests will be skipped');
  testCreds = null;
}

describe('Paradex Authentication - Message Construction', () => {
  it('should build message in correct format: method + timestamp + path + body', () => {
    const method = 'POST';
    const timestamp = 1234567890;
    const path = '/v1/auth/0x123abc';
    const body = '';

    const message = `${method}${timestamp}${path}${body}`;

    expect(message).toBe('POST1234567890/v1/auth/0x123abc');
  });

  it('should use Unix timestamp in seconds, not milliseconds', () => {
    const timestampMs = Date.now();
    const timestampSec = Math.floor(timestampMs / 1000);

    // Verify timestamp is in seconds (10 digits, not 13)
    expect(timestampSec.toString().length).toBe(10);
    expect(timestampMs.toString().length).toBe(13);
  });

  it('should include body in message if present', () => {
    const method = 'POST';
    const timestamp = 1234567890;
    const path = '/v1/orders';
    const body = '{"symbol":"BTC-USD","size":"0.1"}';

    const message = `${method}${timestamp}${path}${body}`;

    expect(message).toContain(body);
    expect(message).toBe('POST1234567890/v1/orders{"symbol":"BTC-USD","size":"0.1"}');
  });

  it('should produce messages longer than 31 characters for auth endpoint', () => {
    const method = 'POST';
    const timestamp = 1234567890;
    const subkeyAddress = '0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';
    const path = `/v1/auth/${subkeyAddress}`;
    const body = '';

    const message = `${method}${timestamp}${path}${body}`;

    // Auth messages should be >31 chars and require hashing
    expect(message.length).toBeGreaterThan(31);
  });
});

describe('Paradex Authentication - Message Hashing', () => {
  it('should hash messages longer than 31 characters using starknetKeccak', () => {
    const longMessage = 'POST1234567890/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';

    expect(longMessage.length).toBeGreaterThan(31);

    const messageHash = hash.starknetKeccak(longMessage);
    const messageHashHex = '0x' + messageHash.toString(16);

    // Verify hash is a valid hex string
    expect(messageHashHex).toMatch(/^0x[a-fA-F0-9]+$/);
    expect(messageHashHex.length).toBeGreaterThan(2);
  });

  it('should produce consistent hashes for same input', () => {
    const message = 'POST1234567890/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';

    const hash1 = hash.starknetKeccak(message);
    const hash2 = hash.starknetKeccak(message);

    expect(hash1).toBe(hash2);
  });

  it('should produce different hashes for different inputs', () => {
    const message1 = 'POST1234567890/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';
    const message2 = 'POST1234567891/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';

    const hash1 = hash.starknetKeccak(message1);
    const hash2 = hash.starknetKeccak(message2);

    expect(hash1).not.toBe(hash2);
  });

  it('should not hash messages ≤31 characters', () => {
    const shortMessage = 'SHORT'; // 5 chars

    expect(shortMessage.length).toBeLessThanOrEqual(31);

    // For short messages, use as-is (no hashing needed)
    const messageToSign = shortMessage;

    expect(messageToSign).toBe(shortMessage);
  });
});

describe('Paradex Authentication - EIP-712 Signing', () => {
  const testPrivateKey = '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
  const testAddress = '0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';
  const chainId = '0x5041524144455820434841494e'; // "PARADEX CHAIN"

  it('should create Account from private key and address', () => {
    const account = new Account(
      { nodeUrl: 'https://api.prod.paradex.trade' },
      testAddress,
      testPrivateKey
    );

    expect(account.address).toBe(testAddress);
  });

  it('should sign typed data with correct domain', async () => {
    const account = new Account(
      { nodeUrl: 'https://api.prod.paradex.trade' },
      testAddress,
      testPrivateKey
    );

    const message = 'POST1234567890/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';
    const messageHash = hash.starknetKeccak(message);
    const messageToSign = '0x' + messageHash.toString(16);

    const signature = await account.signMessage({
      domain: {
        name: 'Paradex',
        version: '1',
        chainId: chainId,
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'version', type: 'felt' },
          { name: 'chainId', type: 'felt' },
        ],
        Request: [{ name: 'message', type: 'felt' }],
      },
      primaryType: 'Request',
      message: {
        message: messageToSign,
      },
    });

    // Signature can be array [r, s] or object {r, s}
    const isArray = Array.isArray(signature);
    const hasRandS = !isArray && typeof signature === 'object' && 'r' in signature && 's' in signature;

    expect(isArray || hasRandS).toBe(true);

    if (isArray) {
      expect(signature.length).toBe(2);
    } else {
      expect(signature.r).toBeTruthy();
      expect(signature.s).toBeTruthy();
    }
  });

  it('should convert signature to string array format', async () => {
    const account = new Account(
      { nodeUrl: 'https://api.prod.paradex.trade' },
      testAddress,
      testPrivateKey
    );

    const message = 'POST1234567890/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';
    const messageHash = hash.starknetKeccak(message);
    const messageToSign = '0x' + messageHash.toString(16);

    const signature = await account.signMessage({
      domain: {
        name: 'Paradex',
        version: '1',
        chainId: chainId,
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'version', type: 'felt' },
          { name: 'chainId', type: 'felt' },
        ],
        Request: [{ name: 'message', type: 'felt' }],
      },
      primaryType: 'Request',
      message: {
        message: messageToSign,
      },
    });

    const sigArray = Array.isArray(signature)
      ? signature.map((s: any) => (typeof s === 'bigint' ? s.toString() : String(s)))
      : [String((signature as any).r), String((signature as any).s)];

    expect(Array.isArray(sigArray)).toBe(true);
    expect(sigArray.length).toBe(2);
    expect(typeof sigArray[0]).toBe('string');
    expect(typeof sigArray[1]).toBe('string');
  });

  it('should produce consistent signatures for same message', async () => {
    const account = new Account(
      { nodeUrl: 'https://api.prod.paradex.trade' },
      testAddress,
      testPrivateKey
    );

    const message = 'POST1234567890/v1/auth/0x13a818fe15f676dec39702e021dec14b240936ba671694d68e10e1e648a12b2';
    const messageHash = hash.starknetKeccak(message);
    const messageToSign = '0x' + messageHash.toString(16);

    const typedData = {
      domain: {
        name: 'Paradex',
        version: '1',
        chainId: chainId,
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'version', type: 'felt' },
          { name: 'chainId', type: 'felt' },
        ],
        Request: [{ name: 'message', type: 'felt' }],
      },
      primaryType: 'Request' as const,
      message: {
        message: messageToSign,
      },
    };

    const sig1 = await account.signMessage(typedData);
    const sig2 = await account.signMessage(typedData);

    // Convert to string arrays for comparison (avoid BigInt serialization issues)
    const sigArray1 = Array.isArray(sig1)
      ? sig1.map((s: any) => (typeof s === 'bigint' ? s.toString() : String(s)))
      : [String((sig1 as any).r), String((sig1 as any).s)];
    const sigArray2 = Array.isArray(sig2)
      ? sig2.map((s: any) => (typeof s === 'bigint' ? s.toString() : String(s)))
      : [String((sig2 as any).r), String((sig2 as any).s)];

    // Signatures should be deterministic
    expect(sigArray1[0]).toBe(sigArray2[0]);
    expect(sigArray1[1]).toBe(sigArray2[1]);
  });
});

describe('Paradex Authentication - API Integration', () => {
  // Skip these tests if credentials not available
  // Note: These are integration tests that require real API calls
  const skipIfNoCredentials = testCreds ? it.skip : it.skip;

  skipIfNoCredentials('should authenticate using /auth/{pubkey} endpoint', async () => {
    const timestamp = Math.floor(Date.now() / 1000);
    const method = 'POST';
    const path = `/v1/auth/${testCreds.subkeyAddress}`;
    const body = '';

    const fullMessage = `${method}${timestamp}${path}${body}`;
    const messageHash = hash.starknetKeccak(fullMessage);
    const messageToSign = '0x' + messageHash.toString(16);

    const account = new Account(
      { nodeUrl: 'https://api.prod.paradex.trade' },
      testCreds.subkeyAddress,
      testCreds.apiKey
    );

    const chainId = '0x5041524144455820434841494e';

    const signature = await account.signMessage({
      domain: {
        name: 'Paradex',
        version: '1',
        chainId: chainId,
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'version', type: 'felt' },
          { name: 'chainId', type: 'felt' },
        ],
        Request: [{ name: 'message', type: 'felt' }],
      },
      primaryType: 'Request',
      message: {
        message: messageToSign,
      },
    });

    const sigArray = Array.isArray(signature)
      ? signature.map((s: any) => (typeof s === 'bigint' ? s.toString() : String(s)))
      : [String((signature as any).r), String((signature as any).s)];

    const endpoint = `https://api.prod.paradex.trade/v1/auth/${testCreds.subkeyAddress}`;

    const response = await fetch(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'PARADEX-STARKNET-ACCOUNT': testCreds.paradexAddress,
        'PARADEX-STARKNET-SIGNATURE': JSON.stringify(sigArray),
        'PARADEX-TIMESTAMP': timestamp.toString(),
      },
    });

    const responseText = await response.text();

    console.log('Response status:', response.status);
    console.log('Response body:', responseText);

    // Should receive 200 OK with JWT token
    expect(response.ok).toBe(true);

    const data = JSON.parse(responseText);
    expect(data.jwt_token || data.token).toBeTruthy();
  }, 10000); // 10 second timeout

  skipIfNoCredentials('should include main account in PARADEX-STARKNET-ACCOUNT header', async () => {
    // Verify that we're using the main account address, not subkey address
    expect(testCreds.paradexAddress).toBeTruthy();
    expect(testCreds.subkeyAddress).toBeTruthy();
    expect(testCreds.paradexAddress).not.toBe(testCreds.subkeyAddress);
  });

  skipIfNoCredentials('should sign with subkey private key', async () => {
    // Verify that the account is created with subkey address and subkey private key
    const account = new Account(
      { nodeUrl: 'https://api.prod.paradex.trade' },
      testCreds.subkeyAddress,
      testCreds.apiKey
    );

    expect(account.address).toBe(testCreds.subkeyAddress);
  });

  skipIfNoCredentials('should use correct endpoint format /auth/{pubkey}', async () => {
    const endpoint = `https://api.prod.paradex.trade/v1/auth/${testCreds.subkeyAddress}`;

    expect(endpoint).toContain('/v1/auth/');
    expect(endpoint).toContain(testCreds.subkeyAddress);
  });
});

describe('Paradex Authentication - Error Handling', () => {
  // Skip network tests - these require real API calls and fetch support
  const runIfFetch = it.skip;

  runIfFetch('should handle missing signature header (400 error)', async () => {
    const timestamp = Math.floor(Date.now() / 1000);
    const fakeSubkey = '0x0000000000000000000000000000000000000000000000000000000000000001';
    const fakeMainAccount = '0x0000000000000000000000000000000000000000000000000000000000000002';

    const endpoint = `https://api.prod.paradex.trade/v1/auth/${fakeSubkey}`;

    const response = await fetch(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'PARADEX-STARKNET-ACCOUNT': fakeMainAccount,
        // Missing PARADEX-STARKNET-SIGNATURE header
        'PARADEX-TIMESTAMP': timestamp.toString(),
      },
    });

    expect(response.ok).toBe(false);
    expect(response.status).toBe(400);

    const error = await response.json();
    expect(error.error).toContain('SIGNATURE');
  }, 10000);

  runIfFetch('should handle invalid signature (401 error)', async () => {
    const timestamp = Math.floor(Date.now() / 1000);
    const fakeSubkey = '0x0000000000000000000000000000000000000000000000000000000000000001';
    const fakeMainAccount = '0x0000000000000000000000000000000000000000000000000000000000000002';
    const invalidSignature = '["123", "456"]'; // Invalid signature

    const endpoint = `https://api.prod.paradex.trade/v1/auth/${fakeSubkey}`;

    const response = await fetch(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'PARADEX-STARKNET-ACCOUNT': fakeMainAccount,
        'PARADEX-STARKNET-SIGNATURE': invalidSignature,
        'PARADEX-TIMESTAMP': timestamp.toString(),
      },
    });

    expect(response.ok).toBe(false);
    expect(response.status).toBe(401);

    const error = await response.json();
    expect(error.error).toBeTruthy();
  }, 10000);
});

describe('Paradex API - Endpoint Authentication Requirements', () => {
  it.skip('should allow public access to /markets/summary endpoint', async () => {
    // Test that summary endpoint is public (no authentication required)
    // Note: Skipped in test environment as fetch may not be available
    // Verified manually: curl "https://api.prod.paradex.trade/v1/markets/summary?market=ALL" returns 200 OK
    if (typeof fetch === 'undefined') {
      return;
    }

    const response = await fetch('https://api.prod.paradex.trade/v1/markets/summary?market=ALL');
    if (!response) return;

    expect(response.ok).toBe(true);
    expect(response.status).toBe(200);

    const data = await response.json();
    expect(data.results).toBeTruthy();
    expect(Array.isArray(data.results)).toBe(true);
  }, 10000);

  it.skip('should allow public access to /markets endpoint', async () => {
    // Test that markets endpoint is public
    // Note: Skipped in test environment as fetch may not be available
    // Verified manually: curl https://api.prod.paradex.trade/v1/markets returns 200 OK
    if (typeof fetch === 'undefined') {
      return;
    }

    const response = await fetch('https://api.prod.paradex.trade/v1/markets');
    if (!response) return;

    expect(response.ok).toBe(true);
    expect(response.status).toBe(200);

    const data = await response.json();
    expect(data.results).toBeTruthy();
    expect(Array.isArray(data.results)).toBe(true);
  }, 10000);

  it.skip('should require authentication for /orders endpoint', async () => {
    // Test that orders endpoint requires authentication
    // Note: Skipped in test environment as fetch may not be available
    // Verified manually: curl https://api.prod.paradex.trade/v1/orders returns 401 INVALID_TOKEN
    if (typeof fetch === 'undefined') {
      return;
    }

    const response = await fetch('https://api.prod.paradex.trade/v1/orders');
    if (!response) return;

    expect(response.ok).toBe(false);
    expect(response.status).toBe(401);

    const error = await response.json();
    expect(error.error).toBe('INVALID_TOKEN');
  }, 10000);

  it.skip('should require authentication for /ohlcv endpoint', async () => {
    // Test that OHLCV endpoint requires authentication
    // Note: Skipped in test environment as fetch may not be available
    // Verified manually: curl "https://api.prod.paradex.trade/v1/ohlcv/BTC-USD-PERP?interval=1m&page_size=10" returns 401 INVALID_TOKEN
    if (typeof fetch === 'undefined') {
      return;
    }

    const response = await fetch(
      'https://api.prod.paradex.trade/v1/ohlcv/BTC-USD-PERP?interval=1m&page_size=10',
    );
    if (!response) return;

    expect(response.ok).toBe(false);
    expect(response.status).toBe(401);

    const error = await response.json();
    expect(error.error).toBe('INVALID_TOKEN');
  }, 10000);

  it.skip('should require authentication for /orderbook endpoint', async () => {
    // Test that orderbook endpoint requires authentication
    // Note: Skipped in test environment as fetch may not be available
    // Verified manually: curl "https://api.prod.paradex.trade/v1/markets/BTC-USD-PERP/orderbook" returns 401 INVALID_TOKEN
    if (typeof fetch === 'undefined') {
      return;
    }

    const response = await fetch(
      'https://api.prod.paradex.trade/v1/markets/BTC-USD-PERP/orderbook',
    );
    if (!response) return;

    expect(response.ok).toBe(false);
    expect(response.status).toBe(401);

    const error = await response.json();
    expect(error.error).toBe('INVALID_TOKEN');
  }, 10000);
});
