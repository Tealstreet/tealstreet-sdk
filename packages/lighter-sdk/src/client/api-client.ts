/**
 * Lighter API HTTP Client
 *
 * Provides low-level HTTP methods for interacting with the Lighter API.
 */

import axios, { AxiosInstance, AxiosRequestConfig } from 'axios';
import type {
  ApiErrorResponse,
  ApiKey,
  ApiResponse,
  ErrorInterceptor,
  GetApiKeysResponse,
  GetTransferFeeResponse,
  AnyTxInfo,
  Interceptors,
  LighterApiClientConfig,
  RequestInterceptor,
  ResponseInterceptor,
  SendTxBatchResponse,
  TransferFeeInfo,
  TxInfo,
} from './http-types';
import { LighterApiError } from './http-types';

const TX_TYPE_BY_NAME = {
  create_order: 14,
  cancel_order: 15,
  transfer: 12,
  withdrawal: 13,
  update_leverage: 20,
  approve_integrator: 45,
  create_grouped_orders: 28,
} as const;

type LegacyLikeResponse = {
  success?: boolean;
  data?: {
    txHash?: string;
    nextNonce?: string | number;
  };
  error?: {
    code?: string;
    message?: string;
  };
};

function toBase64FromHex(hexValue: string): string {
  const normalized = hexValue.startsWith('0x') ? hexValue.slice(2) : hexValue;
  if (normalized.length % 2 !== 0) {
    throw new Error('Invalid hex signature length');
  }

  const bytes = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < normalized.length; i += 2) {
    bytes[i / 2] = parseInt(normalized.slice(i, i + 2), 16);
  }

  if (typeof btoa === 'function') {
    let binary = '';
    for (const byte of bytes) {
      binary += String.fromCharCode(byte);
    }
    return btoa(binary);
  }

  // Node fallback for tests/tools.
  const nodeBuffer = (globalThis as any).Buffer;
  if (nodeBuffer) {
    return nodeBuffer.from(bytes).toString('base64');
  }

  throw new Error('No base64 encoder available in current runtime');
}

function toSafeNumber(value: bigint, fieldName: string): number {
  if (value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(Number.MIN_SAFE_INTEGER)) {
    throw new Error(`${fieldName} exceeds Number safe integer range`);
  }
  return Number(value);
}

function isSafeIntegerBigInt(value: bigint): boolean {
  return value <= BigInt(Number.MAX_SAFE_INTEGER) && value >= BigInt(Number.MIN_SAFE_INTEGER);
}

/**
 * Main HTTP client for the Lighter API
 *
 * @example
 * ```typescript
 * const client = new LighterApiClient({
 *   baseUrl: 'https://api.lighter.xyz',
 *   timeout: 30000,
 *   debug: true
 * });
 *
 * const nonce = await client.getNextNonce(accountIndex, apiKeyIndex);
 * ```
 */
export class LighterApiClient {
  private readonly axiosInstance: AxiosInstance;
  private readonly config: Required<LighterApiClientConfig>;
  private readonly interceptors: Interceptors = {
    request: [],
    response: [],
    error: [],
  };

  /**
   * Creates a new Lighter API client
   *
   * @param config - Client configuration
   */
  constructor(config: LighterApiClientConfig) {
    this.config = {
      baseUrl: config.baseUrl,
      timeout: config.timeout ?? 30000,
      headers: config.headers ?? {},
      debug: config.debug ?? false,
    };

    // Create axios instance
    this.axiosInstance = axios.create({
      baseURL: this.config.baseUrl,
      timeout: this.config.timeout,
      headers: {
        'Content-Type': 'application/json',
        ...this.config.headers,
      },
    });

    // Setup default interceptors
    this.setupDefaultInterceptors();
  }

  /**
   * Setup default request/response interceptors
   */
  private setupDefaultInterceptors(): void {
    // Request interceptor for logging
    this.axiosInstance.interceptors.request.use(
      (config) => {
        if (this.config.debug) {
          console.log('[LighterApiClient] Request:', {
            method: config.method,
            url: config.url,
            params: config.params,
            data: config.data,
          });
        }

        // Apply custom request interceptors
        let modifiedConfig = config;
        for (const interceptor of this.interceptors.request ?? []) {
          modifiedConfig = interceptor(modifiedConfig) || modifiedConfig;
        }

        return modifiedConfig;
      },
      (error) => {
        if (this.config.debug) {
          console.error('[LighterApiClient] Request Error:', error);
        }
        return Promise.reject(error);
      },
    );

    // Response interceptor for logging and error handling
    this.axiosInstance.interceptors.response.use(
      (response) => {
        if (this.config.debug) {
          console.log('[LighterApiClient] Response:', {
            status: response.status,
            data: response.data,
          });
        }

        // Apply custom response interceptors
        let modifiedResponse = response;
        for (const interceptor of this.interceptors.response ?? []) {
          modifiedResponse = interceptor(modifiedResponse) || modifiedResponse;
        }

        return modifiedResponse;
      },
      (error) => {
        if (this.config.debug) {
          console.error('[LighterApiClient] Response Error:', error);
        }

        // Apply custom error interceptors
        let modifiedError = error;
        for (const interceptor of this.interceptors.error ?? []) {
          const result = interceptor(modifiedError);
          if (result) {
            modifiedError = result;
          }
        }

        // Convert to LighterApiError
        const apiError = this.handleApiError(modifiedError);
        return Promise.reject(apiError);
      },
    );
  }

  /**
   * Convert axios error to LighterApiError
   */
  private handleApiError(error: any): LighterApiError {
    if (error.response) {
      // Server responded with error status
      const response = error.response;
      const responseBody = response.data as Record<string, any> | undefined;
      const explicitError = responseBody?.error as ApiErrorResponse | undefined;
      const flatMessage = responseBody?.message;
      const flatCode = responseBody?.code;
      const errorData: ApiErrorResponse = explicitError || {
        code: flatCode ? String(flatCode) : 'HTTP_ERROR',
        message:
          (typeof flatMessage === 'string' && flatMessage.length > 0 ? flatMessage : undefined) ||
          error.message ||
          'HTTP request failed',
      };

      return new LighterApiError(
        errorData.message,
        errorData.code,
        errorData.details,
        errorData.requestId,
        response.status,
      );
    } else if (error.request) {
      // Request made but no response received
      return new LighterApiError('No response received from server', 'NO_RESPONSE', { originalError: error.message });
    } else {
      // Error setting up the request
      return new LighterApiError(error.message || 'Request setup failed', 'REQUEST_SETUP_ERROR', {
        originalError: error.toString(),
      });
    }
  }

  /**
   * Add a request interceptor
   *
   * @param interceptor - Request interceptor function
   */
  public addRequestInterceptor(interceptor: RequestInterceptor): void {
    this.interceptors.request = this.interceptors.request || [];
    this.interceptors.request.push(interceptor);
  }

  /**
   * Add a response interceptor
   *
   * @param interceptor - Response interceptor function
   */
  public addResponseInterceptor(interceptor: ResponseInterceptor): void {
    this.interceptors.response = this.interceptors.response || [];
    this.interceptors.response.push(interceptor);
  }

  /**
   * Add an error interceptor
   *
   * @param interceptor - Error interceptor function
   */
  public addErrorInterceptor(interceptor: ErrorInterceptor): void {
    this.interceptors.error = this.interceptors.error || [];
    this.interceptors.error.push(interceptor);
  }

  /**
   * Get the next available nonce for an account/API key combination
   *
   * @param accountIndex - Account index
   * @param apiKeyIndex - API key index
   * @returns Next available nonce
   *
   * @example
   * ```typescript
   * const nonce = await client.getNextNonce(1n, 0);
   * console.log('Next nonce:', nonce);
   * ```
   */
  public async getNextNonce(accountIndex: bigint, apiKeyIndex: number): Promise<bigint> {
    const response = await this.axiosInstance.get<any>('/api/v1/nextNonce', {
      params: {
        account_index: accountIndex.toString(),
        api_key_index: apiKeyIndex,
      },
    });

    const payload = response.data as Record<string, any>;
    if (payload && (payload.code === 200 || payload.code === '200') && payload.nonce !== undefined) {
      return BigInt(payload.nonce);
    }

    const legacyPayload = payload as LegacyLikeResponse;
    if (legacyPayload.success && legacyPayload.data?.nextNonce !== undefined) {
      return BigInt(legacyPayload.data.nextNonce);
    }

    throw new LighterApiError(
      (payload?.message as string | undefined) || legacyPayload.error?.message || 'Failed to get nonce',
      String(payload?.code || legacyPayload.error?.code || 'GET_NONCE_FAILED'),
    );
  }

  /**
   * Get all API keys for an account
   *
   * @param accountIndex - Account index
   * @param apiKeyIndex - API key index for authentication
   * @returns List of API keys
   *
   * @example
   * ```typescript
   * const apiKeys = await client.getApiKeys(1n, 0);
   * console.log('API Keys:', apiKeys);
   * ```
   */
  public async getApiKeys(accountIndex: bigint, apiKeyIndex: number): Promise<ApiKey[]> {
    const response = await this.axiosInstance.get<ApiResponse<GetApiKeysResponse>>(
      `/api/v1/accounts/${accountIndex.toString()}/api-keys`,
      {
        params: { apiKeyIndex },
      },
    );

    if (!response.data.success || !response.data.data) {
      throw new LighterApiError(
        response.data.error?.message || 'Failed to get API keys',
        response.data.error?.code || 'GET_API_KEYS_FAILED',
      );
    }

    return response.data.data.apiKeys;
  }

  private buildL2TxAttributesPayload(params: {
    integratorAccountIndex?: bigint;
    integratorMakerFee?: number;
    integratorTakerFee?: number;
  }): Record<string, number> | undefined {
    if (
      params.integratorAccountIndex === undefined ||
      params.integratorMakerFee === undefined ||
      params.integratorTakerFee === undefined
    ) {
      return undefined;
    }

    return {
      1: toSafeNumber(params.integratorAccountIndex, 'integratorAccountIndex'),
      2: params.integratorTakerFee,
      3: params.integratorMakerFee,
    };
  }

  private buildOrderInfoPayload(orderInfo: {
    orderId: bigint;
    marketId: number;
    isBuy: boolean;
    price: bigint;
    amount: bigint;
    reduceOnly?: boolean;
    orderTypeValue?: number;
    timeInForceValue?: number;
    triggerPrice?: bigint;
    orderExpiry?: bigint;
  }): Record<string, number> {
    const defaultOrderExpiry = BigInt(Date.now() + 28 * 24 * 60 * 60 * 1000);
    const orderTypeValue = orderInfo.orderTypeValue ?? 0;
    const timeInForceValue = orderInfo.timeInForceValue ?? (orderTypeValue === 1 ? 0 : 1);
    const triggerPrice = orderInfo.triggerPrice ?? 0n;
    const isConditionalOrderType =
      orderTypeValue === 2 || orderTypeValue === 3 || orderTypeValue === 4 || orderTypeValue === 5;
    const shouldUseImmediateExpiry = timeInForceValue === 0 && !isConditionalOrderType;
    const orderExpiry = orderInfo.orderExpiry ?? (shouldUseImmediateExpiry ? 0n : defaultOrderExpiry);

    return {
      MarketIndex: orderInfo.marketId,
      ClientOrderIndex: toSafeNumber(orderInfo.orderId, 'clientOrderIndex'),
      BaseAmount: toSafeNumber(orderInfo.amount, 'baseAmount'),
      Price: toSafeNumber(orderInfo.price, 'price'),
      IsAsk: orderInfo.isBuy ? 0 : 1,
      Type: orderTypeValue,
      TimeInForce: timeInForceValue,
      ReduceOnly: orderInfo.reduceOnly ? 1 : 0,
      TriggerPrice: toSafeNumber(triggerPrice, 'triggerPrice'),
      OrderExpiry: toSafeNumber(orderExpiry, 'orderExpiry'),
    };
  }

  private serializeTxInfo(txInfo: TxInfo): { txType: number; txInfoJson: string } {
    const txType = TX_TYPE_BY_NAME[txInfo.type];
    if (!txType) {
      throw new LighterApiError(`Unsupported tx type for sendTx: ${txInfo.type}`, 'UNSUPPORTED_TX_TYPE');
    }

    const defaultExpiredAt = BigInt(Date.now() + 10 * 60 * 1000);
    const expiredAt = ((txInfo as any).expiredAt as bigint | undefined) ?? defaultExpiredAt;
    const createOrderInfo = (txInfo as any).orderDetails as
      | {
          orderId: bigint;
          marketId: number;
          isBuy: boolean;
          price: bigint;
          amount: bigint;
          reduceOnly?: boolean;
          orderTypeValue?: number;
          timeInForceValue?: number;
          triggerPrice?: bigint;
          orderExpiry?: bigint;
          integratorAccountIndex?: bigint;
          integratorMakerFee?: number;
          integratorTakerFee?: number;
        }
      | undefined;
    const groupedOrderInfo = (txInfo as any).groupedOrderDetails as
      | {
          groupingType: 1 | 2 | 3;
          orders: Array<{
            orderId: bigint;
            marketId: number;
            isBuy: boolean;
            price: bigint;
            amount: bigint;
            reduceOnly?: boolean;
            orderTypeValue?: number;
            timeInForceValue?: number;
            triggerPrice?: bigint;
            orderExpiry?: bigint;
          }>;
          integratorAccountIndex?: bigint;
          integratorMakerFee?: number;
          integratorTakerFee?: number;
        }
      | undefined;
    const cancelInfo = (txInfo as any).cancelDetails as
      | {
          orderId: bigint;
          marketId: number;
          expiredAt?: bigint;
        }
      | undefined;
    const leverageInfo = (txInfo as any).leverageDetails as
      | {
          marketId: number;
          initialMarginFraction: number;
          marginMode: 0 | 1;
          expiredAt?: bigint;
        }
      | undefined;
    const approveIntegratorInfo = (txInfo as any).approveIntegratorDetails as
      | {
          integratorAccountIndex: bigint;
          maxPerpsTakerFee: number;
          maxPerpsMakerFee: number;
          maxSpotTakerFee: number;
          maxSpotMakerFee: number;
          approvalExpiry: bigint;
          expiredAt?: bigint;
          l1Signature?: string;
        }
      | undefined;

    let txInfoPayload: Record<string, unknown> | undefined;
    let txInfoJson: string | undefined;
    if (txInfo.type === 'create_order' && createOrderInfo) {
      txInfoPayload = {
        AccountIndex: toSafeNumber(txInfo.accountIndex, 'accountIndex'),
        ApiKeyIndex: txInfo.apiKeyIndex,
        ...this.buildOrderInfoPayload(createOrderInfo),
        ExpiredAt: toSafeNumber(expiredAt, 'expiredAt'),
        Nonce: toSafeNumber(txInfo.nonce, 'nonce'),
        Sig: toBase64FromHex(txInfo.signature),
        ...(this.buildL2TxAttributesPayload(createOrderInfo)
          ? {
              // Mirror official lighter-go tx_info shape for integrator-attributed orders.
              L2TxAttributes: this.buildL2TxAttributesPayload(createOrderInfo),
            }
          : {}),
      };
    } else if (txInfo.type === 'create_grouped_orders' && groupedOrderInfo) {
      txInfoPayload = {
        AccountIndex: toSafeNumber(txInfo.accountIndex, 'accountIndex'),
        ApiKeyIndex: txInfo.apiKeyIndex,
        GroupingType: groupedOrderInfo.groupingType,
        Orders: groupedOrderInfo.orders.map((order) => this.buildOrderInfoPayload(order)),
        ExpiredAt: toSafeNumber(expiredAt, 'expiredAt'),
        Nonce: toSafeNumber(txInfo.nonce, 'nonce'),
        Sig: toBase64FromHex(txInfo.signature),
        ...(this.buildL2TxAttributesPayload(groupedOrderInfo)
          ? {
              L2TxAttributes: this.buildL2TxAttributesPayload(groupedOrderInfo),
            }
          : {}),
      };
    } else if (txInfo.type === 'cancel_order' && cancelInfo) {
      const accountIndex = toSafeNumber(txInfo.accountIndex, 'accountIndex');
      const apiKeyIndex = txInfo.apiKeyIndex;
      const marketIndex = cancelInfo.marketId;
      const expiredAtValue = toSafeNumber(cancelInfo.expiredAt ?? expiredAt, 'expiredAt');
      const nonceValue = toSafeNumber(txInfo.nonce, 'nonce');
      const signature = toBase64FromHex(txInfo.signature);

      if (isSafeIntegerBigInt(cancelInfo.orderId)) {
        txInfoPayload = {
          AccountIndex: accountIndex,
          ApiKeyIndex: apiKeyIndex,
          MarketIndex: marketIndex,
          Index: Number(cancelInfo.orderId),
          ExpiredAt: expiredAtValue,
          Nonce: nonceValue,
          Sig: signature,
        };
      } else {
        // Preserve full integer precision for large order indexes while still
        // sending JSON numeric token (not quoted string) expected by API schema.
        txInfoJson =
          `{"AccountIndex":${accountIndex},` +
          `"ApiKeyIndex":${apiKeyIndex},` +
          `"MarketIndex":${marketIndex},` +
          `"Index":${cancelInfo.orderId.toString()},` +
          `"ExpiredAt":${expiredAtValue},` +
          `"Nonce":${nonceValue},` +
          `"Sig":${JSON.stringify(signature)}}`;
      }
    } else if (txInfo.type === 'update_leverage' && leverageInfo) {
      const accountIndex = toSafeNumber(txInfo.accountIndex, 'accountIndex');
      const apiKeyIndex = txInfo.apiKeyIndex;
      const marketIndex = leverageInfo.marketId;
      const initialMarginFraction = leverageInfo.initialMarginFraction;
      const marginMode = leverageInfo.marginMode;
      const expiredAtValue = toSafeNumber(leverageInfo.expiredAt ?? expiredAt, 'expiredAt');
      const nonceValue = toSafeNumber(txInfo.nonce, 'nonce');
      const signature = toBase64FromHex(txInfo.signature);

      txInfoPayload = {
        AccountIndex: accountIndex,
        ApiKeyIndex: apiKeyIndex,
        MarketIndex: marketIndex,
        InitialMarginFraction: initialMarginFraction,
        MarginMode: marginMode,
        ExpiredAt: expiredAtValue,
        Nonce: nonceValue,
        Sig: signature,
      };
    } else if (txInfo.type === 'approve_integrator' && approveIntegratorInfo) {
      txInfoPayload = {
        AccountIndex: toSafeNumber(txInfo.accountIndex, 'accountIndex'),
        ApiKeyIndex: txInfo.apiKeyIndex,
        IntegratorAccountIndex: toSafeNumber(approveIntegratorInfo.integratorAccountIndex, 'integratorAccountIndex'),
        MaxPerpsTakerFee: approveIntegratorInfo.maxPerpsTakerFee,
        MaxPerpsMakerFee: approveIntegratorInfo.maxPerpsMakerFee,
        MaxSpotTakerFee: approveIntegratorInfo.maxSpotTakerFee,
        MaxSpotMakerFee: approveIntegratorInfo.maxSpotMakerFee,
        ApprovalExpiry: toSafeNumber(approveIntegratorInfo.approvalExpiry, 'approvalExpiry'),
        ExpiredAt: toSafeNumber(approveIntegratorInfo.expiredAt ?? expiredAt, 'expiredAt'),
        Nonce: toSafeNumber(txInfo.nonce, 'nonce'),
        Sig: toBase64FromHex(txInfo.signature),
        ...(approveIntegratorInfo.l1Signature && {
          L1Sig: approveIntegratorInfo.l1Signature,
        }),
      };
    } else {
      // Fallback for tx types we don't currently adapt in safe-cex.
      txInfoPayload = {
        AccountIndex: toSafeNumber(txInfo.accountIndex, 'accountIndex'),
        ApiKeyIndex: txInfo.apiKeyIndex,
        Nonce: toSafeNumber(txInfo.nonce, 'nonce'),
        Data: txInfo.data,
        Sig: toBase64FromHex(txInfo.signature),
      };
    }

    if (!txInfoJson) {
      txInfoJson = JSON.stringify(txInfoPayload);
    }

    return { txType, txInfoJson };
  }

  /**
   * Send a signed raw transaction to the network
   *
   * @param txInfo - Transaction information including signature
   * @returns Transaction hash
   *
   * @example
   * ```typescript
   * const txHash = await client.sendRawTx({
   *   type: 'create_order',
   *   data: '0x...',
   *   signature: '0x...',
   *   accountIndex: 1n,
   *   apiKeyIndex: 0,
   *   nonce: 5n
   * });
   * console.log('Transaction hash:', txHash);
   * ```
   */
  public async sendRawTx(txInfo: AnyTxInfo): Promise<string> {
    const { txType, txInfoJson } = this.serializeTxInfo(txInfo);

    const body = new URLSearchParams({
      tx_type: String(txType),
      tx_info: txInfoJson,
    });

    const response = await this.axiosInstance.post<any>('/api/v1/sendTx', body.toString(), {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    });

    const payload = response.data as Record<string, any>;
    if ((payload.code === 200 || payload.code === '200') && typeof payload.tx_hash === 'string') {
      return payload.tx_hash;
    }

    const legacyPayload = payload as LegacyLikeResponse;
    if (legacyPayload.success && typeof legacyPayload.data?.txHash === 'string') {
      return legacyPayload.data.txHash;
    }

    throw new LighterApiError(
      (payload?.message as string | undefined) || legacyPayload.error?.message || 'Failed to send transaction',
      String(payload?.code || legacyPayload.error?.code || 'SEND_TX_FAILED'),
    );
  }

  public async sendRawTxBatch(txInfos: AnyTxInfo[]): Promise<SendTxBatchResponse> {
    if (txInfos.length === 0) {
      return { txHashes: [] };
    }

    const serializedTxs = txInfos.map((txInfo) => this.serializeTxInfo(txInfo));
    const body = new URLSearchParams({
      tx_types: JSON.stringify(serializedTxs.map((tx) => tx.txType)),
      tx_infos: JSON.stringify(serializedTxs.map((tx) => tx.txInfoJson)),
    });

    const response = await this.axiosInstance.post<any>('/api/v1/sendTxBatch', body.toString(), {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    });

    const payload = response.data as Record<string, any>;
    if (payload.code === 200 || payload.code === '200') {
      const txHashes = Array.isArray(payload.tx_hash)
        ? payload.tx_hash
        : Array.isArray(payload.txHash)
          ? payload.txHash
          : undefined;
      if (txHashes?.every((txHash) => typeof txHash === 'string')) {
        return {
          txHashes,
          predictedExecutionTimeMs: payload.predicted_execution_time_ms ?? payload.predictedExecutionTimeMs,
          volumeQuotaRemaining: payload.volume_quota_remaining ?? payload.volumeQuotaRemaining,
        };
      }
    }

    const legacyPayload = payload as LegacyLikeResponse & { data?: { txHashes?: string[]; txHash?: string[] } };
    const legacyTxHashes = legacyPayload.data?.txHashes ?? legacyPayload.data?.txHash;
    if (legacyPayload.success && legacyTxHashes?.every((txHash) => typeof txHash === 'string')) {
      return { txHashes: legacyTxHashes };
    }

    throw new LighterApiError(
      (payload?.message as string | undefined) || legacyPayload.error?.message || 'Failed to send transaction batch',
      String(payload?.code || legacyPayload.error?.code || 'SEND_TX_BATCH_FAILED'),
    );
  }

  /**
   * Get transfer fee information for a transfer between accounts
   *
   * @param accountIndex - Source account index
   * @param toAccountIndex - Destination account index
   * @param auth - Authentication token
   * @returns Transfer fee information
   *
   * @example
   * ```typescript
   * const feeInfo = await client.getTransferFeeInfo(1n, 2n, 'auth-token');
   * console.log('Transfer fee:', feeInfo.fee);
   * ```
   */
  public async getTransferFeeInfo(
    accountIndex: bigint,
    toAccountIndex: bigint,
    auth: string,
  ): Promise<TransferFeeInfo> {
    const response = await this.axiosInstance.get<ApiResponse<GetTransferFeeResponse>>(
      `/api/v1/accounts/${accountIndex.toString()}/transfer-fee`,
      {
        params: {
          toAccountIndex: toAccountIndex.toString(),
        },
        headers: {
          Authorization: `Bearer ${auth}`,
        },
      },
    );

    if (!response.data.success || !response.data.data) {
      throw new LighterApiError(
        response.data.error?.message || 'Failed to get transfer fee info',
        response.data.error?.code || 'GET_TRANSFER_FEE_FAILED',
      );
    }

    const data = response.data.data;
    return {
      fee: BigInt(data.fee),
      minAmount: BigInt(data.minAmount),
      maxAmount: BigInt(data.maxAmount),
      feePercentage: data.feePercentage,
    };
  }

  /**
   * Make a custom GET request
   *
   * @param path - API endpoint path
   * @param config - Axios request config
   * @returns Response data
   */
  public async get<T = any>(path: string, config?: AxiosRequestConfig): Promise<T> {
    const response = await this.axiosInstance.get<T>(path, config);
    return response.data;
  }

  /**
   * Make a custom POST request
   *
   * @param path - API endpoint path
   * @param data - Request body data
   * @param config - Axios request config
   * @returns Response data
   */
  public async post<T = any>(path: string, data?: any, config?: AxiosRequestConfig): Promise<T> {
    const response = await this.axiosInstance.post<T>(path, data, config);
    return response.data;
  }

  /**
   * Make a custom PUT request
   *
   * @param path - API endpoint path
   * @param data - Request body data
   * @param config - Axios request config
   * @returns Response data
   */
  public async put<T = any>(path: string, data?: any, config?: AxiosRequestConfig): Promise<T> {
    const response = await this.axiosInstance.put<T>(path, data, config);
    return response.data;
  }

  /**
   * Make a custom DELETE request
   *
   * @param path - API endpoint path
   * @param config - Axios request config
   * @returns Response data
   */
  public async delete<T = any>(path: string, config?: AxiosRequestConfig): Promise<T> {
    const response = await this.axiosInstance.delete<T>(path, config);
    return response.data;
  }

  /**
   * Get the base URL being used
   */
  public getBaseUrl(): string {
    return this.config.baseUrl;
  }

  /**
   * Get the current configuration
   */
  public getConfig(): Readonly<Required<LighterApiClientConfig>> {
    return { ...this.config };
  }
}
