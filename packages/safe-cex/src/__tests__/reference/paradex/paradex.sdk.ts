/**
 * Paradex SDK Integration
 *
 * This module provides integration with the official @tealstreet/paradex-sdk
 * for Ethereum-based authentication and Paraclear contract operations.
 */

import type { ExchangeOptions, PayloadSignerResult } from '../../../types';

import BigNumber from 'bignumber.js';
import { Wallet } from 'ethers';

let paradexSdk: typeof import('@tealstreet/paradex-sdk') | null = null;

/**
 * Lazy-load the Paradex SDK to avoid loading it if not needed
 */
async function getParadexSDK() {
  if (!paradexSdk) {
    paradexSdk = await import('@tealstreet/paradex-sdk');
  }
  return paradexSdk;
}

export interface ParadexSDKAccount {
  account: any; // Starknet Account from SDK
  config: any; // ParadexConfig from SDK
  provider: any; // ParaclearProvider from SDK
}

/**
 * Initialize Paradex SDK - supports both Ethereum and Subkey authentication
 *
 * For SUBKEY authentication (delegated trading):
 * - opts.key should contain the SUBKEY private key (Starknet private key)
 * - opts.paradexAddress should be the MAIN account address
 * - Account is created with: address=mainAccount, privateKey=subkey
 *
 * For ETHEREUM authentication (full account):
 * - opts.key should contain the Ethereum private key
 * - Account is derived from Ethereum signature
 */
export async function initializeParadexSDK(opts: ExchangeOptions): Promise<ParadexSDKAccount | null> {
  // Check if we have the required Paradex authentication fields
  if (!opts.paradexAddress || !opts.key) {
    return null;
  }

  try {
    const SDK = await getParadexSDK();
    const environment = opts.testnet ? 'testnet' : 'prod';

    // Fetch Paradex configuration
    const config = await SDK.Config.fetchConfig(environment);

    // Create provider for Paraclear operations
    const provider = new SDK.ParaclearProvider.DefaultProvider(config);

    // With a payloadSigner the private key never enters this process, and `key` is
    // the subkey PUBLIC key, which is as long as a private one.
    if (opts.payloadSigner) {
      return { account: { address: opts.paradexAddress.toLowerCase() }, config, provider };
    }

    let account: any;

    // Check if this is subkey authentication
    // If the key looks like a Starknet private key (0x0... format, 66 chars), use subkey auth
    const isSubkeyAuth = opts.key.length === 66;

    if (isSubkeyAuth) {
      // SUBKEY AUTHENTICATION: Create Starknet Account directly
      // - address: main account address (paradexAddress)
      // - privateKey: subkey private key (opts.key)
      console.log('[Paradex SDK] Using subkey authentication mode');
      console.log('[Paradex SDK] Main account:', opts.paradexAddress);
      console.log('[Paradex SDK] Subkey will be used for signing');

      // Import Starknet Account class
      const { Account } = await import('starknet');

      account = new Account(
        { nodeUrl: config.paradexFullNodeRpcUrl },
        opts.paradexAddress, // Main account address
        opts.key, // Subkey private key
      );
    } else {
      // ETHEREUM AUTHENTICATION: Use SDK's fromEthSigner
      console.log('[Paradex SDK] Using Ethereum authentication mode');

      // Create Ethereum wallet from private key
      const wallet = new Wallet(opts.key);

      // Create Ethereum signer adapter for the SDK
      const ethereumSigner = SDK.Signer.ethersSignerAdapter(wallet);

      // Generate Paradex account from Ethereum signer
      account = await SDK.Account.fromEthSigner({
        provider: { nodeUrl: config.paradexFullNodeRpcUrl },
        config,
        signer: ethereumSigner,
      });
    }

    console.log('[Paradex SDK] Account initialized with address:', account.address);

    return {
      account,
      config,
      provider,
    };
  } catch (error) {
    console.error('[Paradex SDK] Failed to initialize:', error);
    return null;
  }
}

/**
 * Get token balance from Paraclear contract using the SDK
 */
export async function getParaclearBalance(sdkAccount: ParadexSDKAccount, tokenSymbol: string): Promise<string> {
  const SDK = await getParadexSDK();

  const balance = await SDK.Paraclear.getTokenBalance({
    config: sdkAccount.config,
    provider: sdkAccount.provider,
    account: sdkAccount.account.address,
    token: tokenSymbol,
  });

  return balance.toString();
}

/**
 * Get socialized loss factor for a token
 */
export async function getSocializedLossFactor(sdkAccount: ParadexSDKAccount, _tokenSymbol: string): Promise<string> {
  const SDK = await getParadexSDK();

  const factor = await SDK.Paraclear.getSocializedLossFactor({
    config: sdkAccount.config,
    provider: sdkAccount.provider,
  });

  return factor.toString();
}

/**
 * Signs Paradex typed data and returns the serialised `["<r>","<s>"]` the API expects,
 * through `payloadSigner` when set.
 */
export async function signParadexTypedData(
  sdkAccount: ParadexSDKAccount,
  typedData: any,
  options: ExchangeOptions,
): Promise<PayloadSignerResult> {
  if (options.payloadSigner) {
    const { typedData: starknetTypedData } = await import('starknet');
    return options.payloadSigner({
      exchangeName: 'paradex',
      key: options.key,
      scheme: 'stark',
      encoding: 'felt-pair',
      payload: starknetTypedData.getMessageHash(typedData, sdkAccount.account.address),
    });
  }

  const sig = await sdkAccount.account.signMessage(typedData);
  const sigArray = Array.isArray(sig)
    ? sig.map((value) => (typeof value === 'bigint' ? value.toString() : String(value)))
    : [String(sig.r), String(sig.s)];
  return { signature: JSON.stringify(sigArray) };
}

/**
 * Request Signer for Paradex API authentication
 * Signs each API request with the Starknet account
 */
export class ParadexRequestSigner {
  private readonly sdkAccount: ParadexSDKAccount;
  private readonly options: ExchangeOptions;

  constructor(sdkAccount: ParadexSDKAccount, options: ExchangeOptions = {} as ExchangeOptions) {
    this.sdkAccount = sdkAccount;
    this.options = options;
  }

  /**
   * Sign a request for Paradex API authentication
   * @param method HTTP method (GET, POST, etc.)
   * @param path API path (e.g., /v1/account)
   * @param timestamp Request timestamp in milliseconds
   * @param body Request body as string (empty string for GET requests)
   * @returns Signature array
   */
  async signRequest(method: string, path: string, timestamp: number, body: string): Promise<string[]> {
    try {
      // Import hash utility from starknet
      const { hash } = await import('starknet');

      // Build the message to sign
      // Format: method + timestamp + path + body
      const fullMessage = `${method}${timestamp}${path}${body}`;

      // Hash the message if it's too long for a felt (>31 characters typically)
      // Starknet felts can only hold ~252 bits, so long strings need to be hashed
      let messageToSign: string;
      if (fullMessage.length > 31) {
        // A Starknet felt only carries a 31-byte short string; hash longer request
        // messages into a felt before placing them in the typed-data payload.
        const messageHash = hash.starknetKeccak(fullMessage);
        messageToSign = '0x' + messageHash.toString(16);
      } else {
        messageToSign = fullMessage;
      }

      // Sign the message with the Starknet account using typed data
      const signature = await this.sdkAccount.account.signMessage({
        domain: {
          name: 'Paradex',
          version: '1',
          chainId: this.sdkAccount.config.paradexChainId,
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

      // Convert signature to string array (handles BigInt conversion)
      if (Array.isArray(signature)) {
        const result = signature.map((s) => (typeof s === 'bigint' ? s.toString() : String(s)));
        return result;
      } else {
        // Handle WeierstrassSignatureType
        const result = [String(signature.r), String(signature.s)];
        return result;
      }
    } catch (error) {
      // Error will be logged by the API interceptor
      throw error;
    }
  }

  /**
   * Get the Starknet account address
   */
  getAccountAddress(): string {
    return this.sdkAccount.account.address;
  }

  /**
   * Convert decimal string to Paradex chain quantum integer (scaled by 1e8).
   * Uses BigNumber to avoid IEEE754 drift that can invalidate signatures.
   */
  private toChainQuantum(value: string, fieldName: string): string {
    const decimal = new BigNumber(value);
    if (!decimal.isFinite() || decimal.isNaN()) {
      throw new Error(`Invalid ${fieldName} value for signing: ${value}`);
    }
    if (decimal.isNegative()) {
      throw new Error(`${fieldName} cannot be negative for signing: ${value}`);
    }
    return decimal.multipliedBy(1e8).integerValue(BigNumber.ROUND_DOWN).toFixed(0);
  }

  /**
   * Convert Paradex chain id into canonical hex format expected by typed-data domain.
   */
  private toChainIdHex(chainId: string | number | bigint): string {
    if (typeof chainId === 'number' || typeof chainId === 'bigint') {
      return `0x${BigInt(chainId).toString(16)}`;
    }

    const normalized = String(chainId).trim();
    if (!normalized) {
      throw new Error('Invalid empty chain id for signing');
    }

    if (/^0x[0-9a-fA-F]+$/.test(normalized)) {
      return normalized;
    }

    if (/^\d+$/.test(normalized)) {
      return `0x${BigInt(normalized).toString(16)}`;
    }

    // Match Python int_from_bytes(chain_id.encode()) semantics for short-string chain ids.
    const bytes = new TextEncoder().encode(normalized);
    let chainIdInt = BigInt(0);
    for (const byte of bytes) {
      chainIdInt = (chainIdInt << BigInt(8)) | BigInt(byte);
    }
    return `0x${chainIdInt.toString(16)}`;
  }

  /**
   * Sign an order for Paradex order placement
   * @param orderParams Order parameters
   * @returns Object with signature and signature_timestamp
   */
  async signOrder(orderParams: {
    market: string;
    side: 'BUY' | 'SELL';
    type: string; // LIMIT, MARKET, etc.
    size: string; // Decimal string like "0.01"
    price?: string; // Optional - Decimal string like "100.5" (omit for market orders)
  }): Promise<{ signature: string; signature_timestamp: number; headers?: Record<string, string> }> {
    try {
      // Generate signature timestamp in milliseconds (acts as nonce)
      const signature_timestamp = Date.now();

      // Convert side to chain format: "1" for BUY, "2" for SELL
      const chainSide = orderParams.side === 'BUY' ? '1' : '2';

      // Scale size and price by 10^8 (quantum values) deterministically.
      // Following Python SDK's Decimal.scaleb(8) + int() behavior.
      const chainSize = this.toChainQuantum(orderParams.size, 'size');
      const chainPrice =
        orderParams.type === 'MARKET' || !orderParams.price ? '0' : this.toChainQuantum(orderParams.price, 'price');

      const chainIdHex = this.toChainIdHex(this.sdkAccount.config.paradexChainId);

      // Build TypedData message for the order
      // Based on Python SDK's build_order_message
      const typedData = {
        domain: {
          name: 'Paradex',
          chainId: chainIdHex,
          version: '1',
        },
        types: {
          StarkNetDomain: [
            { name: 'name', type: 'felt' },
            { name: 'chainId', type: 'felt' },
            { name: 'version', type: 'felt' },
          ],
          Order: [
            { name: 'timestamp', type: 'felt' },
            { name: 'market', type: 'felt' },
            { name: 'side', type: 'felt' },
            { name: 'orderType', type: 'felt' },
            { name: 'size', type: 'felt' },
            { name: 'price', type: 'felt' },
          ],
        },
        primaryType: 'Order',
        message: {
          timestamp: signature_timestamp.toString(),
          market: orderParams.market,
          side: chainSide, // "1" or "2"
          orderType: orderParams.type,
          size: chainSize, // Scaled by 10^8
          price: chainPrice, // Scaled by 10^8 or "0"
        },
      };

      const { signature, headers } = await signParadexTypedData(this.sdkAccount, typedData, this.options);

      return {
        signature,
        signature_timestamp,
        headers,
      };
    } catch (error) {
      console.error('[Paradex Order Signing] Failed to sign order:', error);
      throw error;
    }
  }

  /**
   * Sign a native Paradex order amendment.
   *
   * Paradex uses a distinct ModifyOrder typed-data payload for amendments. It
   * is not safe to reuse signOrder(): the stable upstream order id is part of
   * the signed message.
   */
  async signModifyOrder(orderParams: {
    id: string;
    market: string;
    side: 'BUY' | 'SELL';
    type: string;
    size: string;
    price: string;
  }): Promise<{ signature: string; signature_timestamp: number; headers?: Record<string, string> }> {
    if (!orderParams.id) {
      throw new Error('Cannot sign Paradex order amendment without an order id');
    }

    const signature_timestamp = Date.now();
    const chainSide = orderParams.side === 'BUY' ? '1' : '2';
    const chainSize = this.toChainQuantum(orderParams.size, 'size');
    const chainPrice = this.toChainQuantum(orderParams.price, 'price');
    const chainIdHex = this.toChainIdHex(this.sdkAccount.config.paradexChainId);

    const typedData = {
      domain: {
        name: 'Paradex',
        chainId: chainIdHex,
        version: '1',
      },
      types: {
        StarkNetDomain: [
          { name: 'name', type: 'felt' },
          { name: 'chainId', type: 'felt' },
          { name: 'version', type: 'felt' },
        ],
        ModifyOrder: [
          { name: 'timestamp', type: 'felt' },
          { name: 'market', type: 'felt' },
          { name: 'side', type: 'felt' },
          { name: 'orderType', type: 'felt' },
          { name: 'size', type: 'felt' },
          { name: 'price', type: 'felt' },
          { name: 'id', type: 'felt' },
        ],
      },
      primaryType: 'ModifyOrder',
      message: {
        timestamp: signature_timestamp.toString(),
        market: orderParams.market,
        side: chainSide,
        orderType: orderParams.type,
        size: chainSize,
        price: chainPrice,
        id: orderParams.id,
      },
    };

    const { signature, headers } = await signParadexTypedData(this.sdkAccount, typedData, this.options);

    return {
      signature,
      signature_timestamp,
      headers,
    };
  }
}

/**
 * JWT Token Manager for Paradex WebSocket authentication
 * Handles token generation, caching, and automatic refresh
 *
 * Note: JWT is used for WebSocket authentication, while REST API uses signature-based auth
 */
export class ParadexJWTManager {
  private token: string | null = null;
  private tokenExpiry: number = 0;
  private refreshPromise: Promise<string> | null = null;
  private readonly sdkAccount: ParadexSDKAccount;
  private readonly apiBaseUrl: string;
  private readonly subkeyAddress: string;
  private readonly mainAccountAddress: string;
  private readonly options: ExchangeOptions;

  // Refresh token 2 minutes before expiry (tokens last 5 minutes)
  private readonly REFRESH_BUFFER_MS = 2 * 60 * 1000;
  // Token validity duration (5 minutes as per Paradex docs)
  private readonly TOKEN_VALIDITY_MS = 5 * 60 * 1000;

  constructor(
    sdkAccount: ParadexSDKAccount,
    testnet: boolean,
    subkeyAddress: string,
    options: ExchangeOptions = {} as ExchangeOptions,
  ) {
    this.sdkAccount = sdkAccount;
    this.options = options;
    this.subkeyAddress = subkeyAddress;
    this.mainAccountAddress = sdkAccount.account.address;

    // Import BASE_URL dynamically to avoid circular dependencies
    const BASE_URL = {
      livenet: 'https://api.prod.paradex.trade/v1',
      testnet: 'https://api.testnet.paradex.trade/v1',
    };
    this.apiBaseUrl = BASE_URL[testnet ? 'testnet' : 'livenet'];
  }

  /**
   * Get a valid JWT token, refreshing if necessary
   */
  async getToken(): Promise<string> {
    const now = Date.now();

    // Return cached token if still valid
    if (this.token && now < this.tokenExpiry - this.REFRESH_BUFFER_MS) {
      return this.token;
    }

    // If already refreshing, wait for that request
    if (this.refreshPromise) {
      return this.refreshPromise;
    }

    // Start new refresh
    this.refreshPromise = this.generateNewToken();

    try {
      this.token = await this.refreshPromise;
      this.tokenExpiry = now + this.TOKEN_VALIDITY_MS;
      return this.token;
    } catch (error) {
      console.error('[Paradex JWT] ✗ Failed to obtain token:', error);
      throw error;
    } finally {
      this.refreshPromise = null;
    }
  }

  /**
   * Generate a new JWT token from Paradex API using subkey authentication
   * Always uses /auth/{pubkey} endpoint with subkey address
   */
  private async generateNewToken(): Promise<string> {
    try {
      // Paradex expects Unix timestamp in SECONDS, not milliseconds
      const timestamp = Math.floor(Date.now() / 1000);
      const expiry = timestamp + 24 * 60 * 60; // 24 hours expiration (matches Python SDK)
      const method = 'POST';

      // Subkey authentication: /auth/{pubkey} for the endpoint, but /v1/auth for signing
      const endpoint = `${this.apiBaseUrl}/auth/${this.subkeyAddress}`;
      const path = '/v1/auth'; // Path without subkey for signing (matches Python SDK)
      const body = '';

      // Sign using the EXACT TypedData structure from paradex-py auth.py
      // Message has 5 fields: method, path, body, timestamp, expiration
      const { signature, headers: signerHeaders } = await signParadexTypedData(
        this.sdkAccount,
        {
          domain: {
            name: 'Paradex',
            chainId: this.sdkAccount.config.paradexChainId, // e.g. 'PRIVATE_SN_PARACLEAR_MAINNET'
            version: '1',
          },
          types: {
            StarkNetDomain: [
              { name: 'name', type: 'felt' },
              { name: 'chainId', type: 'felt' },
              { name: 'version', type: 'felt' },
            ],
            Request: [
              { name: 'method', type: 'felt' },
              { name: 'path', type: 'felt' },
              { name: 'body', type: 'felt' },
              { name: 'timestamp', type: 'felt' },
              { name: 'expiration', type: 'felt' },
            ],
          },
          primaryType: 'Request',
          message: {
            method: method,
            path: path,
            body: body,
            timestamp: timestamp,
            expiration: expiry,
          },
        },
        this.options,
      );

      // Build headers for subkey authentication (matches Python SDK auth_headers)
      // PARADEX-STARKNET-ACCOUNT must be the main account address
      const headers: Record<string, string> = {
        ...signerHeaders,
        'Content-Type': 'application/json',
        Accept: 'application/json',
        'PARADEX-STARKNET-ACCOUNT': this.mainAccountAddress,
        'PARADEX-STARKNET-SIGNATURE': signature,
        'PARADEX-TIMESTAMP': timestamp.toString(),
        'PARADEX-SIGNATURE-EXPIRATION': expiry.toString(), // Required header!
      };

      // Call POST /auth/{pubkey} for subkey authentication
      const authResponse = await fetch(endpoint, {
        method: 'POST',
        headers,
      });

      if (!authResponse.ok) {
        const errorText = await authResponse.text();
        console.error('[Paradex JWT] ✗ Authentication failed');
        console.error('[Paradex JWT] Status:', authResponse.status, authResponse.statusText);
        console.error('[Paradex JWT] Response body:', errorText);
        try {
          const errorJson = JSON.parse(errorText);
          console.error('[Paradex JWT] Error details:', errorJson);
        } catch {
          // Not JSON, already logged as text
        }
        throw new Error(`Failed to generate JWT token: ${authResponse.status} ${errorText}`);
      }

      const authData = (await authResponse.json()) as { jwt_token?: string; token?: string };
      const token = authData.jwt_token || authData.token;

      if (!token) {
        console.error('[Paradex JWT] ✗ No token in response');
        console.error('[Paradex JWT] Response data:', authData);
        throw new Error('No JWT token in authentication response');
      }

      return token;
    } catch (error) {
      console.error('[Paradex JWT] ✗ Token generation failed with exception:', error);
      throw error;
    }
  }

  /**
   * Clear cached token (useful for logout or error recovery)
   */
  clearToken(): void {
    this.token = null;
    this.tokenExpiry = 0;
    this.refreshPromise = null;
  }
}
