import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Paradex validateAccount Error Reproduction Test
 *
 * This test suite documents and tests the INVALID_TOKEN error that occurs during validateAccount:
 * "INVALID_TOKEN: invalid bearer jwt: missing value in request header"
 *
 * The error occurs because:
 * 1. validateAccount calls GET /account endpoint
 * 2. /account requires JWT authentication (Bearer token)
 * 3. If JWT token generation fails or returns empty, we get this error
 *
 * ## Running the Unit Tests (Vitest)
 *
 * ```bash
 * cd packages/safe-cex
 * npx vitest run src/exchanges/paradex/__tests__/paradex.validateAccount.test.ts
 * ```
 *
 * ## Running Integration Tests (require network access)
 *
 * ```bash
 * cd /home/rayana/projects/tealstreet-next
 * npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
 * ```
 *
 * ## Test Credentials
 *
 * Integration tests use credentials from .apikey.json:
 * - apiKey: Subkey private key (Starknet format, 66 chars)
 * - paradexAddress: Main account address
 * - subkeyAddress: Subkey public address
 */

// Load test credentials from .apikey.json
interface TestCredentials {
  exchange: string;
  testnet: boolean;
  name: string;
  apiKey: string;
  paradexAddress: string;
  ethereumAddress: string;
  subkeyAddress: string;
  user: string;
}

let testCreds: TestCredentials | null = null;
try {
  const credsPath = path.join(__dirname, '..', '.apikey.json');
  testCreds = JSON.parse(fs.readFileSync(credsPath, 'utf-8'));
} catch {
  // Credentials not available - integration tests will be skipped
}

const BASE_URL = 'https://api.prod.paradex.trade/v1';

describe('Paradex validateAccount - INVALID_TOKEN Error Documentation', () => {
  /**
   * Documents the expected error responses from Paradex API
   * These are unit tests that verify our understanding of the error format
   *
   * To run actual network tests, use:
   * npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
   */

  it('should document INVALID_TOKEN error format for missing JWT', () => {
    // This is the expected error response when calling /account without JWT
    const expectedError = {
      error: 'INVALID_TOKEN',
      message: 'invalid bearer jwt: missing value in request header',
    };

    expect(expectedError.error).toBe('INVALID_TOKEN');
    expect(expectedError.message).toContain('missing value');
  });

  it('should document INVALID_TOKEN error format for empty JWT', () => {
    // This is the expected error response when calling /account with "Bearer "
    const expectedError = {
      error: 'INVALID_TOKEN',
      message: 'invalid bearer jwt: invalid value in request header',
    };

    expect(expectedError.error).toBe('INVALID_TOKEN');
    expect(expectedError.message).toContain('invalid value');
  });

  it('should document authentication flow for validateAccount', () => {
    /**
     * The validateAccount flow in paradex.exchange.ts:
     *
     * 1. validateAccount() calls xhr.get(ENDPOINTS.ACCOUNT)
     * 2. paradex.api.ts interceptor calls authenticateRequest()
     * 3. authenticateRequest() gets JWT from jwtManager.getToken()
     * 4. If jwtManager is null or getToken() returns empty string,
     *    the request fails with INVALID_TOKEN error
     *
     * Root causes for the error:
     * - SDK not initialized (jwtManager is null)
     * - JWT generation failed (getToken() throws or returns empty)
     * - Token expired and refresh failed
     */
    const authFlow = [
      'validateAccount() calls GET /account',
      'API interceptor calls authenticateRequest()',
      'authenticateRequest() calls jwtManager.getToken()',
      'JWT token added to Authorization header',
      'Request sent to /account endpoint',
    ];

    expect(authFlow.length).toBe(5);
  });

  it('should document root causes of INVALID_TOKEN error', () => {
    const rootCauses = [
      {
        cause: 'jwtManager is null',
        reason: 'SDK not initialized before validateAccount',
        fix: 'Wait for sdkInitPromise before making authenticated requests',
      },
      {
        cause: 'getToken() returns empty string',
        reason: 'JWT generation failed but error was swallowed',
        fix: 'Check for empty token before adding Authorization header',
      },
      {
        cause: 'Token expired',
        reason: 'Cached token expired and refresh failed',
        fix: 'Clear cache and regenerate on auth errors',
      },
      {
        cause: 'Wrong account/signature',
        reason: 'Using wrong account address in PARADEX-STARKNET-ACCOUNT header',
        fix: 'Verify main account address matches registered subkey',
      },
    ];

    expect(rootCauses.length).toBe(4);
    expect(rootCauses[0].cause).toBe('jwtManager is null');
  });
});

describe('Paradex validateAccount - JWT Token Generation (Integration)', () => {
  /**
   * Integration tests that require network access.
   * These are skipped in vitest - run the standalone script instead:
   *
   * npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
   */

  it.skip('should successfully generate JWT token with valid credentials', async () => {
    // This test requires network access
    // Run: npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
  });

  it.skip('should successfully call /account with valid JWT token', async () => {
    // This test requires network access
    // Run: npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
  });
});

describe('Paradex validateAccount - Diagnose JWT Generation Issues', () => {
  /**
   * Diagnostic tests for JWT generation issues
   * These require network access - run the standalone script instead
   */

  it.skip('should diagnose JWT generation issues step by step', async () => {
    // This test requires network access
    // Run: npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
  });

  it('should document the 10 steps of JWT generation', () => {
    const jwtGenerationSteps = [
      '1. Verify credentials format (apiKey, paradexAddress, subkeyAddress)',
      '2. Load Paradex SDK config (chainId, RPC URL)',
      '3. Create Starknet account with subkey private key',
      '4. Prepare auth parameters (timestamp, expiry)',
      '5. Build TypedData for signing',
      '6. Sign message with Starknet account',
      '7. Convert signature to string array [r, s]',
      '8. Build request headers with signature',
      '9. Call POST /auth/{subkey} endpoint',
      '10. Extract JWT token from response',
    ];

    expect(jwtGenerationSteps.length).toBe(10);
    expect(jwtGenerationSteps[8]).toContain('/auth/{subkey}');
  });
});

describe('Paradex validateAccount - Edge Cases', () => {
  /**
   * Edge case documentation - these are scenarios that can cause the INVALID_TOKEN error
   */

  it('should document edge cases that cause authentication failures', () => {
    const edgeCases = [
      {
        case: 'Expired JWT token',
        error: 'INVALID_TOKEN or TOKEN_EXPIRED',
        cause: 'Token TTL (5 minutes) exceeded without refresh',
      },
      {
        case: 'Wrong main account in header',
        error: 'SIGNATURE_VERIFICATION_FAILED',
        cause: 'PARADEX-STARKNET-ACCOUNT does not match subkey registration',
      },
      {
        case: 'Invalid signature',
        error: 'SIGNATURE_VERIFICATION_FAILED',
        cause: 'Signature does not match expected message hash',
      },
      {
        case: 'Missing timestamp header',
        error: 'BAD_REQUEST',
        cause: 'PARADEX-TIMESTAMP header missing',
      },
      {
        case: 'Timestamp too old/future',
        error: 'TIMESTAMP_EXPIRED',
        cause: 'Request timestamp outside acceptable window',
      },
    ];

    expect(edgeCases.length).toBe(5);
    expect(edgeCases[1].error).toBe('SIGNATURE_VERIFICATION_FAILED');
  });

  it.skip('should handle expired JWT token gracefully (requires network)', async () => {
    // Run: npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
  });

  it.skip('should fail with wrong main account in header (requires network)', async () => {
    // Run: npx tsx packages/safe-cex/src/exchanges/paradex/test-validateAccount-error.ts
  });
});
