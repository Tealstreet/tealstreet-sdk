/**
 * GRVT Builder Authorization Utilities
 *
 * This module provides functions for authorizing and checking builder status on GRVT.
 * Builder authorization allows a third-party (like Tealstreet) to receive
 * builder fees for orders placed through their interface.
 *
 * @see https://api-docs.grvt.io/builder_codes/
 */

import { BrowserProvider, Eip1193Provider, isAddress } from 'ethers';

import { getTransport } from '../../transport/registry';
import { BASE_URLS } from './grvt.types';

// Tealstreet builder wallet addresses
export const TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET = '0xee8f3559f298fffa79b8253437969d15b5dccff3';
export const TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET = '0xb0108ae4c07218ce320e5e22dd911a01c2042162';

// Legacy export for backwards compatibility
export const TEALSTREET_GRVT_BUILDER_ADDRESS = TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET;

// Helper to get the right address based on network
export const getGrvtBuilderAddress = (testnet: boolean): string =>
  testnet ? TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET : TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET;

// Default builder fee rate as a percentage string (0.03 = 0.03% = 3 bps).
// Sent in the API payload as-is; for EIP-712 signing it's converted to uint32
// via `grvtBuilderFeeRateToRaw` (so "0.03" → 300).
export const DEFAULT_GRVT_BUILDER_FEE_RATE = '0.03';

/**
 * Convert a GRVT builder fee percentage string into the uint32 form used for EIP-712
 * signing and returned by `get_authorized_builders`. The convention is
 * `raw = percent * 10000`, so `"0.03"` (= 0.03% = 3 bps) becomes `300`.
 *
 * Throws if the input isn't a finite number.
 */
export const grvtBuilderFeeRateToRaw = (feeRate: string): number => {
  const parsed = parseFloat(feeRate);
  if (!Number.isFinite(parsed)) {
    throw new Error(`Invalid GRVT builder fee rate: ${feeRate}`);
  }
  return Math.round(parsed * 10_000);
};

/**
 * Reason an order was blocked because the GRVT builder fee is not properly authorized.
 *
 * - `unauthorized`: The user has never authorized the Tealstreet builder for this sub-account.
 * - `cap-too-low`:  An authorization exists, but the signed `maxFutureFeeRate` cap is below
 *                   the fee Tealstreet now sends, so GRVT would reject the order.
 */
export type GrvtBuilderUnauthorizedReason = 'unauthorized' | 'cap-too-low';

/**
 * Thrown by the GRVT exchange during order placement when the user's builder fee
 * authorization is missing or insufficient. Tealstreet refuses to place GRVT orders
 * without a valid `builder` / `builder_fee` pair, so the consumer must catch this
 * error and prompt the user to (re-)authorize.
 *
 * The error includes the raw uint32 caps so the UI can show specifics
 * (`raw = percent * 10000`, see `grvtBuilderFeeRateToRaw`).
 */
export class GrvtBuilderUnauthorizedError extends Error {
  readonly code = 'GRVT_BUILDER_UNAUTHORIZED';
  readonly reason: GrvtBuilderUnauthorizedReason;
  readonly requiredRaw: number;
  readonly authorizedRaw?: number;

  constructor(opts: {
    reason: GrvtBuilderUnauthorizedReason;
    requiredRaw: number;
    authorizedRaw?: number;
    message?: string;
  }) {
    super(
      opts.message ??
        (opts.reason === 'cap-too-low'
          ? `GRVT builder authorization cap (${opts.authorizedRaw ?? 'unknown'}) is below required (${opts.requiredRaw}). Please re-authorize.`
          : 'GRVT builder is not authorized for this account. Please authorize before placing orders.'),
    );
    this.name = 'GrvtBuilderUnauthorizedError';
    this.reason = opts.reason;
    this.requiredRaw = opts.requiredRaw;
    this.authorizedRaw = opts.authorizedRaw;
  }
}

// Builder authorization result types (matching Hyperliquid pattern)
export interface GrvtBuilderAuthorizationResult {
  success: boolean;
  message: string;
}

export interface GrvtBuilderCheckResult {
  authorized: boolean;
  /** Display-friendly percentage string for the authorized cap, e.g. "0.0300%" */
  maxFeeRate?: string;
  /**
   * Raw uint32 cap returned by GRVT, using the same convention as the signed/sent value:
   * `raw = percent * 10000` (so `"0.03"` → `300`).
   * Use this field when comparing an existing authorization against a required fee,
   * e.g. `DEFAULT_GRVT_BUILDER_FEE_RATE`.
   */
  maxFeeRateRaw?: number;
}

// Builder info for orders
export interface GrvtBuilderInfo {
  builder: string; // Builder address (lowercase)
  builderFee: string; // Fee rate as percentage string for API payload (e.g., "0.03" = 0.03% = 3 bps)
  builderFeeInt: number; // Fee as uint32 for EIP-712 signing: percent * 10000 (e.g., "0.03" → 300)
}

// EIP-712 domain for GRVT
// chainId: 325 for production, 326 for testnet (matches official builder-examples repo)
const getEIP712Domain = (testnet: boolean) => ({
  name: 'GRVT Exchange',
  version: '0',
  chainId: testnet ? 326 : 325,
});

// EIP-712 types for AuthorizeBuilder
// Based on official GRVT builder-examples repo
const AuthorizeBuilderTypes = {
  AuthorizeBuilder: [
    { name: 'mainAccountID', type: 'address' },
    { name: 'builderAccountID', type: 'address' },
    { name: 'maxFutureFeeRate', type: 'uint32' },
    { name: 'maxSpotFeeRate', type: 'uint32' },
    { name: 'nonce', type: 'uint32' },
    { name: 'expiration', type: 'int64' },
  ],
};

interface AuthorizeBuilderMessage {
  mainAccountID: string;
  builderAccountID: string;
  maxFutureFeeRate: number;
  maxSpotFeeRate: number;
  nonce: number;
  expiration: string;
}

// The transport's whitelisted proxy, when it has one; otherwise GRVT is called directly.
const grvtProxy = () => getTransport({}).privateProxy('grvt');

const viaGrvtProxy = (baseUrl: string): string => {
  const proxy = grvtProxy();
  return proxy ? `${proxy.url}/${baseUrl}` : baseUrl;
};

/**
 * Proxy credentials, empty when there are none. Never `Bearer ` with nothing
 * after it: the proxy answers that with a 403 "Invalid JWT" instead of a 401
 * naming the real problem. exchange-server throws instead of returning empty.
 */
const resolveGrvtProxyHeaders = async (): Promise<Record<string, string>> =>
  (await grvtProxy()?.resolveHeaders()) ?? {};

const getStringField = (value: unknown): string | undefined => (typeof value === 'string' ? value : undefined);

const getValidAddress = (...values: Array<string | undefined>): string | undefined =>
  values.find((value) => value !== undefined && isAddress(value));

/**
 * Get GRVT Edge API base URL for authentication endpoints (via proxy)
 */
const getGrvtEdgeApiUrl = (testnet: boolean): string => {
  const baseUrl = testnet ? BASE_URLS.TESTNET_EDGE : BASE_URLS.PRODUCTION_EDGE;
  return viaGrvtProxy(baseUrl);
};

/**
 * Get GRVT API base URL for trading endpoints (via proxy)
 */
const getGrvtTradingApiUrl = (testnet: boolean): string => {
  const baseUrl = testnet ? BASE_URLS.TESTNET_TDG : BASE_URLS.PRODUCTION_TDG;
  return viaGrvtProxy(baseUrl);
};

/**
 * Authenticate with GRVT using API key (via proxy).
 * This is Step 1 of the builder fee approval flow — obtaining a session
 * before checking or setting builder authorization.
 *
 * The proxy exposes the session cookie via X-Ts-Grvt-Session header
 * because browsers can't read Set-Cookie headers due to CORS restrictions.
 *
 * ## API Call
 *
 * **Endpoint**: `POST {transport proxy}/{EDGE_BASE_URL}/auth/api_key/login`
 *   - Testnet:  `POST https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/https://edge.testnet.grvt.io/auth/api_key/login`
 *   - Mainnet:  `POST https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/https://edge.grvt.io/auth/api_key/login`
 *
 * **Request Headers**:
 * ```
 * Content-Type: application/json
 * <transport proxy headers>
 * X-Grvt-Remember-Me: true
 * ```
 *
 * **Request Body**:
 * ```json
 * { "api_key": "<grvt_api_key>" }
 * ```
 *
 * **Response Headers (on success)**:
 * ```
 * X-Grvt-Account-Id: <accountId>         // Used in subsequent API calls
 * X-Ts-Grvt-Session: <sessionCookie>      // Proxy-forwarded session cookie (CORS workaround)
 * ```
 *
 * **Error Response** (non-200):
 * ```json
 * { "message": "Error description" }
 * ```
 *
 * @returns `{ success: true, accountId, sessionCookie }` on success
 * @returns `{ success: false, message }` on failure or timeout (10s)
 */
const authenticateGrvt = async (
  apiKey: string,
  testnet: boolean,
  proxyHeaders: Record<string, string>,
): Promise<{
  success: boolean;
  accountId?: string;
  fundingAccountAddress?: string;
  sessionCookie?: string;
  message?: string;
}> => {
  const edgeUrl = getGrvtEdgeApiUrl(testnet);

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 10_000);

  const url = `${edgeUrl}/auth/api_key/login`;
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...proxyHeaders,
    'X-Grvt-Remember-Me': 'true',
  };
  const body = { api_key: apiKey };

  try {
    const response = await fetch(url, {
      method: 'POST',
      headers,
      credentials: 'include',
      body: JSON.stringify(body),
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    if (!response.ok) {
      const errorData = (await response.json().catch(() => ({}))) as Record<string, any>;
      return {
        success: false,
        message: errorData.message || `Authentication failed: HTTP ${response.status}`,
      };
    }

    const responseData = (await response.json().catch(() => ({}))) as Record<string, any>;
    // Get the account ID from response header
    const accountId = response.headers.get('x-grvt-account-id');
    const fundingAccountAddress =
      getStringField(responseData.funding_account_address) ??
      getStringField(responseData.fundingAccountAddress) ??
      getStringField(responseData.main_account_id) ??
      getStringField(responseData.ma);
    // Get session cookie from proxy's custom header (browser can't read Set-Cookie due to CORS)
    const sessionCookie = response.headers.get('x-ts-grvt-session');

    if (!accountId) {
      return {
        success: false,
        message: 'Authentication succeeded but no account ID returned',
      };
    }

    return {
      success: true,
      accountId,
      fundingAccountAddress,
      sessionCookie: sessionCookie || undefined,
    };
  } catch (err: unknown) {
    clearTimeout(timeoutId);
    const error = err as { name?: string; message?: string };
    const message =
      error.name === 'AbortError'
        ? 'GRVT authentication timed out (10s)'
        : error.message || 'Failed to authenticate with GRVT';
    return { success: false, message };
  }
};

const fetchGrvtMainAccountId = async (
  testnet: boolean,
  proxyHeaders: Record<string, string>,
  authResult: { accountId?: string; sessionCookie?: string },
): Promise<string | undefined> => {
  const apiUrl = getGrvtTradingApiUrl(testnet);
  const requestHeaders: Record<string, string> = {
    'Content-Type': 'application/json',
    ...proxyHeaders,
  };

  if (authResult.accountId) {
    requestHeaders['X-Grvt-Account-Id'] = authResult.accountId;
  }
  if (authResult.sessionCookie) {
    requestHeaders['X-Ts-Grvt-Session'] = authResult.sessionCookie;
  }

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 10_000);
  try {
    const response = await fetch(`${apiUrl}/full/v1/aggregated_account_summary`, {
      method: 'POST',
      headers: requestHeaders,
      body: JSON.stringify({}),
      signal: controller.signal,
    });

    if (!response.ok) {
      return undefined;
    }

    const data = (await response.json().catch(() => ({}))) as Record<string, any>;
    const result = data.result || data.r || data;
    return getValidAddress(
      getStringField(result.main_account_id),
      getStringField(result.ma),
      getStringField(data.main_account_id),
      getStringField(data.ma),
    );
  } finally {
    clearTimeout(timeoutId);
  }
};

/**
 * Get the browser ethereum provider if available
 */
const getEthereumProvider = (): Eip1193Provider | null => {
  if (typeof window === 'undefined') {
    return null;
  }
  const ethereum = (window as unknown as Record<string, unknown>).ethereum;
  if (!ethereum) {
    return null;
  }
  return ethereum as Eip1193Provider;
};

/**
 * Check if browser wallet (window.ethereum) is available
 */
export const isBrowserWalletAvailable = (): boolean => {
  return getEthereumProvider() !== null;
};

/**
 * Get the connected wallet address from browser wallet
 */
export const getConnectedWalletAddress = async (): Promise<string | null> => {
  const ethereum = getEthereumProvider();
  if (!ethereum) {
    return null;
  }

  try {
    const provider = new BrowserProvider(ethereum);
    const accounts = await provider.send('eth_accounts', []);
    return accounts[0] || null;
  } catch {
    return null;
  }
};

/**
 * Request wallet connection from browser wallet
 */
export const requestWalletConnection = async (): Promise<string | null> => {
  const ethereum = getEthereumProvider();
  if (!ethereum) {
    throw new Error('No browser wallet detected. Please install MetaMask or another Web3 wallet.');
  }

  try {
    const provider = new BrowserProvider(ethereum);
    const accounts = await provider.send('eth_requestAccounts', []);
    return accounts[0] || null;
  } catch (err: unknown) {
    const error = err as { code?: number; message?: string };
    if (error.code === 4001) {
      throw new Error('Wallet connection rejected by user');
    }
    throw new Error(`Failed to connect wallet: ${error.message || 'Unknown error'}`);
  }
};

/**
 * Check if a builder is already authorized for a given account.
 * This is called during exchange startup (in background) to cache authorization status.
 *
 * ## Flow
 * 1. Resolve the transport's proxy headers
 * 2. Authenticate with GRVT via `authenticateGrvt()` to get session credentials
 * 3. Call `get_authorized_builders` API to check if Tealstreet builder is in the authorized list
 *
 * ## API Call
 *
 * **Endpoint**: `POST {transport proxy}/{TDG_BASE_URL}/full/v1/get_authorized_builders`
 *   - Testnet:  `POST https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/https://trades.testnet.grvt.io/full/v1/get_authorized_builders`
 *   - Mainnet:  `POST https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/https://trades.grvt.io/full/v1/get_authorized_builders`
 *
 * **Request Headers**:
 * ```
 * Content-Type: application/json
 * <transport proxy headers>
 * X-Grvt-Account-Id: <accountId>           // From authenticateGrvt() response
 * X-Ts-Grvt-Session: <sessionCookie>       // From authenticateGrvt() response (optional)
 * ```
 *
 * **Request Body**:
 * ```json
 * {}
 * ```
 *
 * **Success Response (full format)**:
 * ```json
 * {
 *   "results": [
 *     { "builder_address": "0x79a52a...", "max_fee_rate": 10000, ... },
 *     ...
 *   ]
 * }
 * ```
 *
 * **Success Response (lite format)**:
 * ```json
 * {
 *   "r": [
 *     { "ba": "0x79a52a...", "mfr": 10000, ... },
 *     ...
 *   ]
 * }
 * ```
 *
 * The function checks if the Tealstreet builder address is in the returned list.
 * `max_fee_rate` is converted to a percentage string (e.g., 10000 → "0.0100%").
 *
 * @param apiKey - The GRVT API key for authentication
 * @param subAccountId - Retained for call-site compatibility. GRVT now scopes
 * builder authorization to the authenticated main account and expects an empty request body.
 * @param builderAddress - The builder address to check (defaults to Tealstreet based on network)
 * @param testnet - Whether to use testnet API
 * @returns `{ authorized: true, maxFeeRate: "0.0300%", maxFeeRateRaw: 300 }` if authorized
 * @returns `{ authorized: false }` if not authorized or on error
 */
export const checkGrvtBuilderAuthorized = async (
  apiKey: string,
  _subAccountId: string,
  builderAddress?: string,
  testnet: boolean = false,
): Promise<GrvtBuilderCheckResult> => {
  const builder = builderAddress ?? getGrvtBuilderAddress(testnet);

  // Deliberately OUTSIDE the try. A missing proxy token is our own auth
  // failing, not GRVT declining the builder, and the catch below collapses
  // everything to `{ authorized: false }` — which is exactly how an auth
  // failure disguises itself as a venue answer.
  const proxyHeaders = await resolveGrvtProxyHeaders();

  try {
    // Authenticate with GRVT via proxy
    const authResult = await authenticateGrvt(apiKey, testnet, proxyHeaders);
    if (!authResult.success) {
      return { authorized: false };
    }

    // Get trading API URL
    const apiUrl = getGrvtTradingApiUrl(testnet);
    const url = `${apiUrl}/full/v1/get_authorized_builders`;

    // Build headers — pass session cookie via proxy header
    const requestHeaders: Record<string, string> = {
      'Content-Type': 'application/json',
      ...proxyHeaders,
      'X-Grvt-Account-Id': authResult.accountId!,
    };

    if (authResult.sessionCookie) {
      requestHeaders['X-Ts-Grvt-Session'] = authResult.sessionCookie;
    }

    const requestBody = {};

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 10_000);

    // Call get_authorized_builders API
    const response = await fetch(url, {
      method: 'POST',
      headers: requestHeaders,
      body: JSON.stringify(requestBody),
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    if (!response.ok) {
      return { authorized: false };
    }

    const data = (await response.json()) as Record<string, any>;

    // Check if our builder is in the list
    // Response format: { results: [{ builder_address, max_fee_rate, ... }] } or lite format { r: [...] }
    const builders = data.results || data.r || [];

    for (const builderInfo of builders) {
      // Full format: builder_account_id, Lite format: ba
      const addr = (builderInfo.builder_account_id || builderInfo.ba || '').toLowerCase();
      if (addr === builder.toLowerCase()) {
        // Found our builder — get the max fee rate.
        // GRVT returns the percentage *string* that was originally sent during
        // authorization (e.g. "0.03" for 0.03% = 3 bps), NOT the EIP-712 uint32.
        // Convert it through `grvtBuilderFeeRateToRaw` so callers can compare against
        // `DEFAULT_GRVT_BUILDER_FEE_RATE` using the same `percent * 10000` convention.
        const rawFeeRateString = builderInfo.max_futures_fee_rate ?? builderInfo.max_fee_rate ?? builderInfo.mfr;
        const feePercent = rawFeeRateString != null ? parseFloat(String(rawFeeRateString)) : NaN;
        const maxFeeRateRaw = Number.isFinite(feePercent) ? Math.round(feePercent * 10_000) : undefined;
        const feePercentage = Number.isFinite(feePercent) ? feePercent.toFixed(4) + '%' : undefined;
        return {
          authorized: true,
          maxFeeRate: feePercentage,
          maxFeeRateRaw,
        };
      }
    }

    return { authorized: false };
  } catch (_error) {
    return { authorized: false };
  }
};

/**
 * Create BuilderInfo object for including in order placement.
 * Produces two representations of the fee rate needed by different parts of the order flow:
 *
 * - `builderFee` (string): Percentage string for the REST API payload (e.g., "0.03" = 0.03% = 3 bps)
 *   → Used in full format: `{ builder_fee: "0.03" }`
 *   → Used in lite format: `{ bf: "0.03" }`
 *
 * - `builderFeeInt` (number): Integer for EIP-712 signing = percent * 10000 (e.g., "0.03" → 300)
 *   → Used in EIP-712 message: `{ builderFee: 300 }` (uint32)
 *
 * These two representations are NOT interchangeable — the API expects a percentage string,
 * while the EIP-712 typed data expects a uint32 integer.
 *
 * @param testnet - Whether to use testnet builder address
 * @param builderAddress - The builder's wallet address (optional, defaults based on network)
 * @param builderFeeRate - Fee rate as percentage string (default: "0.03" = 0.03% = 3 bps)
 * @returns BuilderInfo object with both API and EIP-712 fee representations
 */
export function createGrvtBuilderInfo(
  testnet: boolean = false,
  builderAddress?: string,
  builderFeeRate: string = DEFAULT_GRVT_BUILDER_FEE_RATE,
): GrvtBuilderInfo {
  const builder = builderAddress ?? getGrvtBuilderAddress(testnet);
  return {
    builder: builder.toLowerCase(),
    builderFee: builderFeeRate,
    builderFeeInt: grvtBuilderFeeRateToRaw(builderFeeRate),
  };
}

/**
 * Authorize a builder address using browser wallet (EIP-712 signature).
 * This is the main entry point for the builder fee approval flow from the UI.
 *
 * ## Complete Flow
 *
 * **Step 1**: Resolve the transport's proxy headers
 * **Step 2**: Authenticate with GRVT via `authenticateGrvt()` (see its docs for API details)
 * **Step 3**: Connect browser wallet via `window.ethereum` / `eth_requestAccounts`
 * **Step 4**: Sign EIP-712 typed data with user's wallet (prompts wallet popup)
 * **Step 5**: Send signed authorization to GRVT Edge API
 *
 * ## EIP-712 Signature Details
 *
 * **Domain**:
 * ```json
 * { "name": "GRVT Exchange", "version": "0", "chainId": 325 }
 * ```
 * (chainId: 325 for production, 326 for testnet)
 *
 * **Types**:
 * ```
 * AuthorizeBuilder {
 *   mainAccountID:    address,  // User's funding account address
 *   builderAccountID: address,  // Tealstreet builder: 0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca
 *   maxFutureFeeRate: uint32,   // Fee rate as integer: feeRate * 10000 (e.g., 0.0001 → 1)
 *   maxSpotFeeRate:   uint32,   // Fee rate as integer: feeRate * 10000 (e.g., 0.0001 → 1)
 *   nonce:            uint32,   // Random nonce up to 1 billion
 *   expiration:       int64,    // 24 hours from now in nanoseconds
 * }
 * ```
 *
 * ## API Call (Step 5)
 *
 * **Endpoint**: `POST {transport proxy}/{EDGE_BASE_URL}/auth/builder/authorize`
 *   - Testnet:  `POST https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/https://edge.testnet.grvt.io/auth/builder/authorize`
 *   - Mainnet:  `POST https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/https://edge.grvt.io/auth/builder/authorize`
 *
 * **Request Headers**:
 * ```
 * Content-Type: application/json
 * <transport proxy headers>
 * X-Grvt-Account-Id: <accountId>           // From authenticateGrvt()
 * X-Ts-Grvt-Session: <sessionCookie>       // From authenticateGrvt() (optional)
 * ```
 *
 * **Request Body**:
 * ```json
 * {
 *   "main_account_id": "0xFundingAccountAddress",
 *   "builder_account_id": "0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca",
 *   "max_futures_fee_rate": "0.0001",
 *   "max_spot_fee_rate": "0.0001",
 *   "signature": {
 *     "signer": "0xUserWalletAddress",
 *     "r": "0x...",
 *     "s": "0x...",
 *     "v": 27,
 *     "nonce": 123456789,
 *     "expiration": "1741234567000000000",
 *     "chain_id": 325
 *   }
 * }
 * ```
 *
 * **Success Response**:
 * ```json
 * { "success": true }
 * ```
 * or lite format: `{ "s": true }`
 *
 * **Error Response**:
 * ```json
 * { "success": false, "message": "Error reason" }
 * ```
 * or lite format: `{ "s": false, "m": "Error reason" }`
 *
 * @returns `{ success: true, message: "Builder authorized successfully" }` on success
 * @returns `{ success: false, message: "..." }` on failure (auth, wallet, signing, or API error)
 */
export const authorizeGrvtBuilderWithBrowserWallet = async (
  apiKey: string,
  builderAddress: string,
  maxFeeRate: string,
  testnet: boolean,
): Promise<{ success: boolean; message: string }> => {
  const ethereum = getEthereumProvider();
  if (!ethereum) {
    return { success: false, message: 'No browser wallet detected' };
  }

  if (!isAddress(builderAddress)) {
    return { success: false, message: 'Invalid builder address' };
  }

  if (!apiKey) {
    return { success: false, message: 'API key is required for GRVT authentication' };
  }

  // Outside the try, for the same reason as `checkGrvtBuilderAuthorized`: the
  // catch turns everything into `{ success: false }`, so a token failure caught
  // there is indistinguishable from GRVT saying no.
  const proxyHeaders = await resolveGrvtProxyHeaders();

  try {
    // Step 1: Authenticate with GRVT via proxy to get session
    const authResult = await authenticateGrvt(apiKey, testnet, proxyHeaders);
    if (!authResult.success) {
      return { success: false, message: authResult.message || 'GRVT authentication failed' };
    }

    const provider = new BrowserProvider(ethereum);

    // Request account access
    const accounts = await provider.send('eth_requestAccounts', []);
    if (!accounts || accounts.length === 0) {
      return { success: false, message: 'No accounts available' };
    }

    const signerAddress = accounts[0];
    // Switch wallet to the correct chain (325 prod, 326 testnet)
    // Wallets reject EIP-712 signatures when the domain chainId doesn't match the connected chain
    const requiredChainId = testnet ? 326 : 325;
    const requiredChainIdHex = '0x' + requiredChainId.toString(16);
    try {
      await provider.send('wallet_switchEthereumChain', [{ chainId: requiredChainIdHex }]);
    } catch (switchErr: unknown) {
      // ethers v6 wraps provider errors — check both top-level and nested error codes
      const switchError = switchErr as { code?: number | string; error?: { code?: number } };
      const errorCode = switchError.error?.code ?? switchError.code;

      // 4001 / ACTION_REJECTED = user rejected the switch
      if (errorCode === 4001 || errorCode === 'ACTION_REJECTED') {
        return { success: false, message: 'Chain switch rejected by user' };
      }

      // Any other error (4902 = chain unknown, or wallet-specific codes) — try adding the chain
      try {
        await provider.send('wallet_addEthereumChain', [
          {
            chainId: requiredChainIdHex,
            chainName: testnet ? 'GRVT Testnet' : 'GRVT Mainnet',
            nativeCurrency: { name: 'Ethereum', symbol: 'ETH', decimals: 18 },
            rpcUrls: [testnet ? 'https://rpc.testnet.grvt.io' : 'https://rpc.grvt.io'],
          },
        ]);
      } catch (addErr: unknown) {
        const addError = addErr as { code?: number | string; error?: { code?: number } };
        const addCode = addError.error?.code ?? addError.code;
        if (addCode === 4001 || addCode === 'ACTION_REJECTED') {
          return { success: false, message: 'Adding GRVT chain rejected by user' };
        }
        return {
          success: false,
          message: `Please add GRVT chain (${requiredChainId}) to your wallet manually`,
        };
      }
    }
    // Generate nonce and expiration
    const nonce = Math.floor(Math.random() * 1e9); // Random nonce up to 1 billion
    const expirationMs = Date.now() + 24 * 60 * 60 * 1000; // 24 hours from now
    const expiration = (expirationMs * 1_000_000).toString(); // Convert to nanoseconds

    // Convert fee rate to integer: percent * 10000 (matches order signing convention).
    // e.g., "0.03" (3 bps) -> 300
    const feeRateInt = grvtBuilderFeeRateToRaw(maxFeeRate);

    // Prepare EIP-712 typed data
    const domain = getEIP712Domain(testnet);
    // `main_account_id` is the authenticated GRVT funding/main account, while
    // `signature.signer` is the wallet that produced this signature. Some auth
    // responses put a non-address account id in `X-Grvt-Account-Id`, so prefer
    // address-shaped values and fall back to account summary when auth omits one.
    const mainAccountId =
      getValidAddress(authResult.fundingAccountAddress, authResult.accountId) ||
      (await fetchGrvtMainAccountId(testnet, proxyHeaders, authResult));
    if (!mainAccountId) {
      return { success: false, message: 'GRVT authentication did not return a valid main account ID' };
    }

    const message: AuthorizeBuilderMessage = {
      mainAccountID: mainAccountId,
      builderAccountID: builderAddress,
      maxFutureFeeRate: feeRateInt,
      maxSpotFeeRate: feeRateInt,
      nonce,
      expiration,
    };

    // Sign using EIP-712
    const signer = await provider.getSigner();
    const signature = await signer.signTypedData(domain, AuthorizeBuilderTypes, message);

    // Split signature into r, s, v components
    const sigBytes = signature.slice(2); // Remove '0x'
    const r = '0x' + sigBytes.slice(0, 64);
    const s = '0x' + sigBytes.slice(64, 128);
    const v = parseInt(sigBytes.slice(128, 130), 16);

    // Step 2: Send authorization request to GRVT Edge API via proxy
    const apiUrl = getGrvtEdgeApiUrl(testnet);
    const url = `${apiUrl}/auth/builder/authorize`;

    // Build headers with session cookie if available
    const requestHeaders: Record<string, string> = {
      'Content-Type': 'application/json',
      ...proxyHeaders,
      'X-Grvt-Account-Id': authResult.accountId!, // Include account ID from auth
    };

    // Include session cookie via proxy header if available
    // The proxy will convert X-Ts-Grvt-Session to Cookie header for GRVT
    if (authResult.sessionCookie) {
      requestHeaders['X-Ts-Grvt-Session'] = authResult.sessionCookie;
    }

    const chainId = testnet ? '326' : '325';
    const requestBody = {
      main_account_id: mainAccountId,
      builder_account_id: builderAddress,
      max_futures_fee_rate: maxFeeRate,
      max_spot_fee_rate: maxFeeRate,
      signature: {
        signer: signerAddress,
        r,
        s,
        v,
        nonce,
        expiration,
        chain_id: chainId,
      },
    };

    const response = await fetch(url, {
      method: 'POST',
      headers: requestHeaders,
      credentials: 'include', // Also include any cookies set by browser
      body: JSON.stringify(requestBody),
    });

    const responseData = (await response.json()) as Record<string, any>;

    // Check for error response
    if (!response.ok) {
      const errorMessage = responseData.message || responseData.m || `HTTP ${response.status}`;
      return { success: false, message: errorMessage };
    }

    // Check for success in response
    const success = responseData.success || responseData.s || response.ok;
    if (success) {
      return { success: true, message: 'Builder authorized successfully' };
    }

    const errorMsg = responseData.message || responseData.m || 'Unknown error';
    return { success: false, message: errorMsg };
  } catch (err: unknown) {
    const error = err as { code?: number | string; message?: string };
    // Handle user rejection
    if (error.code === 4001 || error.code === 'ACTION_REJECTED') {
      return { success: false, message: 'Signature request rejected by user' };
    }

    return { success: false, message: error.message || 'Failed to authorize builder' };
  }
};
