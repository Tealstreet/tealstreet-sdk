import type { IsReadRequest } from '../../utils/retryReadsOnly';
import type { AsterDEXExchange } from './asterdex.exchange';
import type { AsterDEXAPIVersion } from './asterdex.types';

import * as crypto from 'crypto';

import axios, { AxiosInstance, InternalAxiosRequestConfig } from 'axios';
import axiosRateLimit from 'axios-rate-limit';
import axiosRetry from 'axios-retry';
import { ethers } from 'ethers';

import { createApiWithSecondaryKey } from '../../createApiWithSecondaryKey';
import { installTransportAdapter, safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { ExchangeOptions, LogSeverity } from '../../types';
import { appendHttpFailure } from '../../utils/httpFailure';
import { isReadOnlyMethod, onlyReads } from '../../utils/retryReadsOnly';
import {
  SIGNATURE_RETRY_CONFIG_KEY,
  hasRetriedSignature,
  isBinanceStyleSignatureRejection,
} from '../../utils/signatureRejection';
import {
  ASTERDEX_ENDPOINTS,
  isAsterDEXV3PrivateKey,
  resolveAsterDEXApiVersion,
  resolveAsterDEXV3AuthContext,
} from './asterdex.types';
import {
  buildAsterDEXQueryString as buildQueryString,
  isAsterDEXPublicEndpoint as isPublicEndpoint,
  stripQueryString,
  withoutAsterDEXSigningFields as withoutSigningFields,
} from './asterdex.signing';
import { asterDEXVirtualClock } from './asterdex.virtualClock';

// The listen key is a POSTed read; its path is versioned, so match the suffix.
const isAsterdexRead: IsReadRequest = (config) =>
  isReadOnlyMethod(config) || (config.url?.split('?')[0] ?? '').endsWith('/listenKey');

interface AsterDEXAPIOptions {
  apiKey?: string;
  apiSecret?: string;
  apiVersion?: AsterDEXAPIVersion;
  v3UserAddress?: string;
  v3SignerAddress?: string;
  v3SignerPrivateKey?: string;
  testnet?: boolean;
}

const lastV3NonceBySigner = new Map<string, bigint>();
const ASTERDEX_CODE_CHAIN_ID = {
  production: 1666,
  testnet: 714,
} as const;
const ASTERDEX_ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';

const buildV3Nonce = (nonceScopeKey: string): string => {
  const microsecondsNow = BigInt(asterDEXVirtualClock.getCurrentTimeMs()) * 1000n;
  const prevNonce = lastV3NonceBySigner.get(nonceScopeKey) ?? 0n;
  const nextNonce = microsecondsNow > prevNonce ? microsecondsNow : prevNonce + 1n;
  lastV3NonceBySigner.set(nonceScopeKey, nextNonce);
  return nextNonce.toString();
};

export const buildAsterDEXV3Nonce = (nonceScopeKey: string): string => buildV3Nonce(nonceScopeKey);

export const getAsterDEXCodeChainId = (testnet?: boolean): number =>
  testnet ? ASTERDEX_CODE_CHAIN_ID.testnet : ASTERDEX_CODE_CHAIN_ID.production;

export const getAsterDEXAsterChain = (testnet?: boolean): 'Mainnet' | 'Testnet' => (testnet ? 'Testnet' : 'Mainnet');

export const buildAsterDEXCodeQueryString = (params: Record<string, any>): string =>
  Object.entries(params)
    .filter(([_, value]) => value !== undefined && value !== null && value !== '')
    .map(([key, value]) => `${key}=${String(value)}`)
    .join('&');

export const generateAsterDEXCodeMessageSignature = (
  queryString: string,
  signerPrivateKey: string,
  testnet?: boolean,
): string => {
  const normalizedPrivateKey = signerPrivateKey.startsWith('0x') ? signerPrivateKey : `0x${signerPrivateKey}`;
  const domain = {
    name: 'AsterSignTransaction',
    version: '1',
    chainId: getAsterDEXCodeChainId(testnet),
    verifyingContract: ASTERDEX_ZERO_ADDRESS,
  };
  const types = {
    Message: [{ name: 'msg', type: 'string' }],
  };
  const digest = ethers.TypedDataEncoder.hash(domain, types, { msg: queryString });
  const signerKey = new ethers.SigningKey(normalizedPrivateKey);
  return ethers.Signature.from(signerKey.sign(digest)).serialized;
};

const generateV1Signature = (queryString: string, secret: string): string => {
  // AsterDEX uses HMAC SHA256 signature (similar to Binance)
  return crypto.createHmac('sha256', secret).update(queryString).digest('hex');
};

const buildV3SigningPayload = (params: Record<string, any>): Record<string, string> => {
  const result: Record<string, string> = {};
  const sortedKeys = Object.keys(params).sort();

  for (const key of sortedKeys) {
    const value = params[key];
    if (value === undefined || value === null || value === '') {
      continue;
    }
    result[key] = String(value);
  }

  return result;
};

const generateV3Signature = (
  params: Record<string, any>,
  userAddress: string,
  signerAddress: string,
  signerPrivateKey: string,
  nonce: string,
): string => {
  const payload = buildV3SigningPayload(params);
  const serializedPayload = JSON.stringify(payload);
  const encodedPayload = ethers.AbiCoder.defaultAbiCoder().encode(
    ['string', 'address', 'address', 'uint256'],
    [serializedPayload, userAddress, signerAddress, BigInt(nonce)],
  );
  const payloadHash = ethers.keccak256(encodedPayload);
  const ethSignedHash = ethers.hashMessage(ethers.getBytes(payloadHash));
  const normalizedPrivateKey = signerPrivateKey.startsWith('0x') ? signerPrivateKey : `0x${signerPrivateKey}`;
  const signerKey = new ethers.SigningKey(normalizedPrivateKey);
  const signature = signerKey.sign(ethSignedHash);
  return ethers.Signature.from(signature).serialized;
};

const isAsterDEXCodeSignedEndpoint = (url?: string, method?: string): boolean => {
  if (!url) return false;
  const path = stripQueryString(url);
  const normalizedMethod = String(method || 'get').toLowerCase();
  return (
    (normalizedMethod === 'post' && path.endsWith('/fapi/v3/order')) ||
    (normalizedMethod === 'get' && (path.endsWith('/fapi/v3/builder') || path.endsWith('/fapi/v3/agent')))
  );
};

const shouldSuppressUnknownOrderError = (config: any, code: unknown, message?: string): boolean =>
  Boolean(
    config?.__suppressAsterUnknownOrderError &&
    String(code) === '-2011' &&
    String(message || '')
      .toLowerCase()
      .includes('unknown order'),
  );

const getAsterDEXErrorDetail = (message: unknown, code: unknown): string => {
  const rawMessage = typeof message === 'string' ? message : String(message || 'Unknown error');
  const normalizedMessage = rawMessage.toLowerCase();
  if (String(code) === '-5025' || normalizedMessage.includes('cannot found builder config')) {
    return [
      'Aster builder attribution is not synced for this API key.',
      'Open your Aster API key settings, sync or enable the Tealstreet builder authorization,',
      'then reconnect the account and try again.',
    ].join(' ');
  }
  return rawMessage;
};

export const createPublicAPI = (exchange?: AsterDEXExchange): AxiosInstance => {
  const baseURL = exchange?.options?.testnet ? ASTERDEX_ENDPOINTS.REST.TESTNET : ASTERDEX_ENDPOINTS.REST.PRODUCTION;

  const api = exchange
    ? safeCexAxiosCreate(
        {
          baseURL,
          timeout: 10000,
          headers: {
            'Content-Type': 'application/json',
          },
        },
        exchange.options,
        exchange,
      )
    : axios.create({
        baseURL,
        timeout: 10000,
        headers: {
          'Content-Type': 'application/json',
        },
      });

  // Set up HTTP over WebSocket adapter if tunnel is enabled
  if (exchange?.options?.tealstreetTunnel) {
    installTransportAdapter(api, exchange.options, exchange.name);
  }

  // Add retry logic for network errors
  axiosRetry(api, {
    retries: 3,
    retryDelay: (retryCount) => retryCount * 1000,
    retryCondition: onlyReads(
      (error) =>
        axiosRetry.isNetworkOrIdempotentRequestError(error) ||
        error.response?.status === 503 ||
        error.response?.status === 504,
      isAsterdexRead,
    ),
  });

  // Add rate limiting
  const rateLimitedAPI = axiosRateLimit(api, {
    maxRequests: 10,
    perMilliseconds: 1000,
  });

  return rateLimitedAPI;
};

export const createAPI = (
  exchange: AsterDEXExchange,
  options: AsterDEXAPIOptions = {},
  // Transport (proxy/tunnel/overrideProxyUrl) is read from here, defaulting to the
  // primary account. The secondary read-only client passes its own ExchangeOptions so
  // its GETs pick up overrideProxyUrl (secondary read transport) — mirrors bitget's createAPI.
  transportOptions: ExchangeOptions = exchange.options,
): AxiosInstance => {
  const baseURL = options.testnet ? ASTERDEX_ENDPOINTS.REST.TESTNET : ASTERDEX_ENDPOINTS.REST.PRODUCTION;

  const api = safeCexAxiosCreate(
    {
      baseURL,
      timeout: 10000,
      headers: {
        'Content-Type': 'application/json',
      },
    },
    transportOptions,
    exchange,
  );

  // Set up HTTP over WebSocket adapter if tunnel is enabled
  if (transportOptions.tealstreetTunnel) {
    installTransportAdapter(api, transportOptions, exchange.name);
  }

  // Add authentication interceptor
  api.interceptors.request.use(
    async (config) => {
      const urlPath = stripQueryString(config.url || '');
      const isPublic = isPublicEndpoint(urlPath);

      if (exchange.options.remoteSigner && !isPublic) {
        return config;
      }

      if (!isPublic && options.apiKey) {
        const mutableConfig = config as typeof config & { __asterUnsignedParams?: Record<string, any> };
        // Check if time sync is needed
        if (asterDEXVirtualClock.needsSync()) {
          if (exchange) {
            exchange.log('Time sync needed, syncing with server...', LogSeverity.Info);
          }
          await asterDEXVirtualClock.syncClientServerTime(options.testnet, options.apiVersion);
        }

        // Add API key to headers
        config.headers = config.headers || {};
        config.headers['X-MBX-APIKEY'] = options.apiKey;

        // Add timestamp and recvWindow
        // Use AsterDEX-specific virtualClock for time synchronization
        const timestamp = asterDEXVirtualClock.getCurrentTimeMs();
        const recvWindow = 20000; // 20 seconds

        // Prepare parameters for signature
        // Binance-style APIs send all parameters as query params, even for POST/PUT
        // A resend whose unsigned params were never recorded (hosted, where the server
        // returns before this) still carries the previous attempt's signing fields.
        const data = mutableConfig.__asterUnsignedParams || withoutSigningFields(config.data || config.params || {});
        mutableConfig.__asterUnsignedParams = { ...data };

        // Add timestamp and recvWindow at the end (order matters)
        const params: Record<string, any> = {
          ...data,
          recvWindow,
          timestamp,
        };

        const isV3Request = config.url?.includes('/fapi/v3/') ?? false;
        let signature = '';

        if (isV3Request) {
          const { v3UserAddress, v3SignerAddress, v3SignerPrivateKey } = options;
          if (!v3UserAddress || !v3SignerAddress || !v3SignerPrivateKey) {
            throw new Error(
              'AsterDEX v3 signing requires auth.userAddress, auth.signerAddress, and signer private key (auth.privateKey or secret)',
            );
          }
          if (!isAsterDEXV3PrivateKey(v3SignerPrivateKey)) {
            throw new Error('AsterDEX v3 signing requires a 64-byte hex private key');
          }

          const userAddress = ethers.getAddress(v3UserAddress);
          const signerAddress = ethers.getAddress(v3SignerAddress);
          const nonceScopeKey = `${userAddress}:${signerAddress}`;
          const nonce = buildV3Nonce(nonceScopeKey);

          if (isAsterDEXCodeSignedEndpoint(urlPath, config.method)) {
            const codeParams = {
              ...data,
              asterChain: getAsterDEXAsterChain(options.testnet),
              user: userAddress,
              signer: signerAddress,
              nonce,
            };
            const queryString = buildAsterDEXCodeQueryString(codeParams);
            signature = generateAsterDEXCodeMessageSignature(queryString, v3SignerPrivateKey, options.testnet);
            config.url = `${urlPath}?${queryString}&signature=${signature}`;
            delete config.params;
            if (config.data) {
              delete config.data;
            }
            return config;
          } else {
            signature = generateV3Signature(params, userAddress, signerAddress, v3SignerPrivateKey, nonce);
            params.user = userAddress;
            params.signer = signerAddress;
            params.nonce = nonce;
          }
        } else {
          if (!options.apiSecret) {
            throw new Error('AsterDEX v1 signing requires API secret');
          }
          const queryString = buildQueryString(params);
          signature = generateV1Signature(queryString, options.apiSecret);
        }

        // Add signature to parameters
        params.signature = signature;

        // For Binance-style API, all parameters go as query params
        config.params = params;

        // Remove data from POST/PUT/DELETE requests
        // as parameters are sent via query string
        if (config.data) {
          delete config.data;
        }
      }

      return config;
    },
    (error) => {
      return Promise.reject(error);
    },
  );

  // V1 only: V3 signs with a wallet key, where a rejection is not drift a resend fixes.
  // The request interceptor rebuilds the signature from `__asterUnsignedParams`.
  const resendV1SignatureRejection = (config: InternalAxiosRequestConfig | undefined, data: unknown) => {
    if (!config || config.url?.includes('/fapi/v3/') || hasRetriedSignature(config)) return undefined;
    if (!isBinanceStyleSignatureRejection(data)) return undefined;
    return api.request({ ...config, [SIGNATURE_RETRY_CONFIG_KEY]: true } as InternalAxiosRequestConfig);
  };

  // Add response interceptor for error handling
  api.interceptors.response.use(
    async (response) => {
      const signatureResend = resendV1SignatureRejection(response.config, response.data);
      if (signatureResend) return signatureResend;

      // Check if response contains an error (Binance-style API returns errors with 200 status)
      if (response.data?.code && response.data.code < 0) {
        const detail = getAsterDEXErrorDetail(response.data.msg, response.data.code);
        const errorMessage = appendHttpFailure(`AsterDEX API Error: ${detail} (code: ${response.data.code})`, response);

        if (!shouldSuppressUnknownOrderError(response.config, response.data.code, response.data.msg) && exchange) {
          exchange.emitAndLogError(errorMessage);
        }

        throw new Error(errorMessage);
      }

      return response;
    },
    async (error) => {
      if (error.response) {
        const { status, data } = error.response;

        const signatureResend = resendV1SignatureRejection(error.config, data);
        if (signatureResend) return signatureResend;

        // Check for timestamp error
        if (data?.code === -1021 && data?.msg?.includes('Timestamp')) {
          // Timestamp error detected, resync time and retry once
          if (exchange) {
            exchange.log('Timestamp error detected, resyncing with server time...', LogSeverity.Warning);
          }

          // Resync time with AsterDEX server
          await asterDEXVirtualClock.syncClientServerTime(options.testnet, options.apiVersion);

          // Mark this request as already retried to avoid infinite loop
          if (!error.config.__isRetryRequest) {
            error.config.__isRetryRequest = true;

            // Retry the original request with new timestamp
            // Remove old signature parameters
            if (error.config.params) {
              delete error.config.params.timestamp;
              delete error.config.params.signature;
              delete error.config.params.recvWindow;
              delete error.config.params.user;
              delete error.config.params.signer;
              delete error.config.params.nonce;
            }

            if (exchange) {
              exchange.log('Retrying request with synced timestamp...', LogSeverity.Info);
            }

            // The request will go through the request interceptor again
            // which will add fresh timestamp and signature
            return api.request(error.config);
          }
        }

        // Format error message for proper display
        let errorMessage = `AsterDEX API Error: ${status}`;

        // Add error details from response
        if (data?.msg) {
          errorMessage += ` - ${getAsterDEXErrorDetail(data.msg, data?.code)}`;
        } else if (data?.error) {
          errorMessage += ` - ${getAsterDEXErrorDetail(data.error, data?.code)}`;
        }

        // Include error code if available
        if (data?.code) {
          errorMessage += ` (code: ${data.code})`;
        }

        const shouldSuppressEmission = shouldSuppressUnknownOrderError(
          error.config,
          data?.code,
          data?.msg || data?.error,
        );

        // Log and emit error (similar to Hyperliquid pattern)
        if (!shouldSuppressEmission && exchange) {
          exchange.emitAndLogError(errorMessage);

          // Handle specific error codes
          switch (status) {
            case 401:
              exchange.log('Authentication failed. Check API credentials.', LogSeverity.Error);
              break;
            case 429:
              exchange.log('Rate limit exceeded. Reducing request frequency.', LogSeverity.Warning);
              break;
            case 503:
            case 504:
              exchange.log('Service temporarily unavailable. Will retry.', LogSeverity.Warning);
              break;
          }
        }

        // Throw error with formatted message
        throw new Error(errorMessage);
      }

      return Promise.reject(error);
    },
  );

  // Add retry logic
  axiosRetry(api, {
    retries: 3,
    retryDelay: (retryCount) => retryCount * 1000,
    retryCondition: onlyReads(
      (error) =>
        axiosRetry.isNetworkOrIdempotentRequestError(error) ||
        error.response?.status === 503 ||
        error.response?.status === 504,
      isAsterdexRead,
    ),
  });

  // Add rate limiting based on endpoint weight
  const rateLimitedAPI = axiosRateLimit(api, {
    maxRequests: 20, // Adjust based on AsterDEX limits
    perMilliseconds: 1000,
  });

  return rateLimitedAPI;
};

// Helper function to create multiple API instances with different rate limits
export const createAPIInstances = (exchange: AsterDEXExchange) => {
  // Resolve Aster REST auth from a primary OR secondary-key ExchangeOptions. For
  // V3 the read-only secondary agent shares the main wallet (applicationId) and
  // differs only in signer address (secondary_apiKey) + private key (secondary_apiSecret).
  // INVARIANT: Aster accounts carry creds flat (key/secret/applicationId), never
  // options.auth — the secondary_* overrides map to those. A populated auth.* block
  // would shadow the secondary signer creds; createExchangeFromAccount never sets it.
  const toAsterOptions = (o: ExchangeOptions): AsterDEXAPIOptions => {
    const v3Auth = resolveAsterDEXV3AuthContext(o);
    return {
      apiKey: o.key,
      apiSecret: o.secret,
      apiVersion: resolveAsterDEXApiVersion(o),
      v3UserAddress: v3Auth.userAddress,
      v3SignerAddress: v3Auth.signerAddress,
      v3SignerPrivateKey: v3Auth.signerPrivateKey,
      testnet: o.testnet,
    };
  };

  const options = toAsterOptions(exchange.options);

  // Primary read/write client: with a secondary (read-only) key set, GET (read)
  // requests route to a client built from the read-only V3 agent while writes stay
  // on the primary agent. Default-off (identical client) when no secondary key set.
  const [primary] = createApiWithSecondaryKey(exchange.options, exchange, (o) =>
    createAPI(exchange, toAsterOptions(o), o),
  );

  // Secondary API with reduced rate limit for heavy endpoints
  const secondary = axiosRateLimit(createAPI(exchange, options), {
    maxRequests: 5,
    perMilliseconds: 1000,
  });

  // Unlimited API for time-sensitive operations (use with caution)
  const unlimited = createAPI(exchange, options);

  // Public API for market data
  const publicAPI = createPublicAPI(exchange);

  return {
    primary,
    secondary,
    unlimited,
    public: publicAPI,
  };
};
