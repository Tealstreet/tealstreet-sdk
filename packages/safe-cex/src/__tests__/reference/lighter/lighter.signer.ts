import type { ExchangeName, ExchangeOptions } from '../../../types';

import { LighterApiClient, TransactionClient } from '@tealstreet/lighter-sdk';
import { getCryptoApi, initWasm, isWasmReady } from '@tealstreet/poseidon-crypto';
import { resolveLighterDeploymentProfile } from '../../../exchanges/lighter/lighter.types';

export type LighterSignerProvider = 'tealstreet-sdk';

export type LighterCreateOrderRequest = {
  orderId: bigint;
  marketId: number;
  isBuy: boolean;
  price: bigint;
  amount: bigint;
  postOnly: boolean;
  reduceOnly: boolean;
  orderType: 'limit' | 'market' | 'stop_loss' | 'take_profit';
  triggerPrice?: bigint;
  timeInForce?: 'GoodTillCancel' | 'ImmediateOrCancel' | 'FillOrKill' | 'PostOnly';
  integratorAccountIndex?: bigint;
  integratorMakerFee?: number;
  integratorTakerFee?: number;
};

export type LighterCreateGroupedOrdersRequest = {
  groupingType: 1 | 2 | 3;
  orders: LighterCreateOrderRequest[];
  integratorAccountIndex?: bigint;
  integratorMakerFee?: number;
  integratorTakerFee?: number;
};

export type LighterCancelOrderRequest = {
  orderId: bigint;
  marketId: number;
};

export type LighterUpdateLeverageRequest = {
  marketId: number;
  leverage: number;
  marginMode: 0 | 1;
};

export type LighterApproveIntegratorRequest = {
  integratorAccountIndex: bigint;
  maxPerpsTakerFee: number;
  maxPerpsMakerFee: number;
  maxSpotTakerFee: number;
  maxSpotMakerFee: number;
  approvalExpiry: bigint;
  l1Signature?: string;
};

export interface LighterSignerAdapter {
  initialize(): Promise<void>;
  createOrder(request: LighterCreateOrderRequest): Promise<string>;
  createOrders(requests: LighterCreateOrderRequest[]): Promise<string[]>;
  createGroupedOrders(request: LighterCreateGroupedOrdersRequest): Promise<string>;
  cancelOrder(request: LighterCancelOrderRequest): Promise<string>;
  updateLeverage(request: LighterUpdateLeverageRequest): Promise<void>;
  approveIntegrator(request: LighterApproveIntegratorRequest): Promise<string>;
  getAuthToken(ttlSeconds: number): Promise<string | undefined>;
  dispose?(): Promise<void>;
}

function getProvider(options: ExchangeOptions): string | undefined {
  return (options.extra as { lighterSignerProvider?: string } | undefined)?.lighterSignerProvider;
}

function isLighterInvalidNonceError(error: unknown): boolean {
  const record = error as Record<string, unknown> | undefined;
  const code = String(record?.code ?? '');
  const message = String(record?.message ?? error ?? '').toLowerCase();
  return code === '21104' || message.includes('invalid nonce');
}

class TealstreetLighterSignerAdapter implements LighterSignerAdapter {
  private transactionClient: TransactionClient | undefined;
  private submissionLock: Promise<void> = Promise.resolve();

  constructor(
    private readonly options: ExchangeOptions,
    private readonly exchangeName: ExchangeName,
    private readonly apiClient: LighterApiClient,
    private readonly accountIndex: bigint,
    private readonly apiKeyIndex: number,
  ) {}

  private runWithSubmissionLock = async <T>(fn: () => Promise<T>): Promise<T> => {
    const prev = this.submissionLock;
    let releaseLock!: () => void;
    this.submissionLock = new Promise<void>((resolve) => {
      releaseLock = resolve;
    });

    await prev;
    try {
      return await fn();
    } finally {
      releaseLock();
    }
  };

  async initialize(): Promise<void> {
    if (this.transactionClient) {
      return;
    }

    if (!this.options.secret) {
      throw new Error('Missing secret for signer initialization');
    }

    const buildClient = () => {
      const crypto = getCryptoApi();
      this.transactionClient = new TransactionClient({
        httpClient: this.apiClient,
        crypto,
        chainId: resolveLighterDeploymentProfile({
          exchangeName: this.exchangeName,
          testnet: this.options.testnet,
        }).signerChainId,
        privateKey: this.options.secret,
        accountIndex: this.accountIndex,
        apiKeyIndex: this.apiKeyIndex,
      });
    };

    try {
      if (!isWasmReady()) {
        await initWasm();
      }
      buildClient();
    } catch (error) {
      const message = String((error as { message?: string } | undefined)?.message || error || '').toLowerCase();
      if (!message.includes('wasm not initialized')) {
        throw error;
      }
      // Handle rare race where runtime reports ready but crypto client is not actually initialized yet.
      await initWasm();
      buildClient();
    }
  }

  async createOrder(request: LighterCreateOrderRequest): Promise<string> {
    return await this.runWithSubmissionLock(async () => {
      if (!this.transactionClient) {
        await this.initialize();
      }
      if (!this.transactionClient) {
        throw new Error('Signer not initialized');
      }
      const submitOrder = async () => {
        const txInfo = await this.transactionClient!.getCreateOrderTransaction(request);
        return this.apiClient.sendRawTx(txInfo);
      };

      try {
        return await submitOrder();
      } catch (error) {
        this.transactionClient.resetNonce();
        if (!isLighterInvalidNonceError(error)) {
          throw error;
        }
        // Nonce endpoints can lag right after a previous tx; brief delay improves retry reliability.
        await new Promise((resolve) => setTimeout(resolve, 150));
        // One retry after forcing nonce refresh from server.
        try {
          return await submitOrder();
        } catch (retryError) {
          this.transactionClient.resetNonce();
          throw retryError;
        }
      }
    });
  }

  async createOrders(requests: LighterCreateOrderRequest[]): Promise<string[]> {
    return await this.runWithSubmissionLock(async () => {
      if (!this.transactionClient) {
        await this.initialize();
      }
      if (!this.transactionClient) {
        throw new Error('Signer not initialized');
      }
      if (requests.length === 0) {
        return [];
      }

      const submitOrders = async () => {
        const txInfos = await this.transactionClient!.getCreateOrderTransactions(requests);
        const response = await this.apiClient.sendRawTxBatch(txInfos);
        return response.txHashes;
      };

      try {
        return await submitOrders();
      } catch (error) {
        this.transactionClient.resetNonce();
        if (!isLighterInvalidNonceError(error)) {
          throw error;
        }
        await new Promise((resolve) => setTimeout(resolve, 150));
        try {
          return await submitOrders();
        } catch (retryError) {
          this.transactionClient.resetNonce();
          throw retryError;
        }
      }
    });
  }

  async createGroupedOrders(request: LighterCreateGroupedOrdersRequest): Promise<string> {
    return await this.runWithSubmissionLock(async () => {
      if (!this.transactionClient) {
        await this.initialize();
      }
      if (!this.transactionClient) {
        throw new Error('Signer not initialized');
      }

      const submitGroupedOrders = async () => {
        const txInfo = await this.transactionClient!.getCreateGroupedOrdersTransaction(request);
        return this.apiClient.sendRawTx(txInfo);
      };

      try {
        return await submitGroupedOrders();
      } catch (error) {
        this.transactionClient.resetNonce();
        if (!isLighterInvalidNonceError(error)) {
          throw error;
        }
        await new Promise((resolve) => setTimeout(resolve, 150));
        try {
          return await submitGroupedOrders();
        } catch (retryError) {
          this.transactionClient.resetNonce();
          throw retryError;
        }
      }
    });
  }

  async cancelOrder(request: LighterCancelOrderRequest): Promise<string> {
    return await this.runWithSubmissionLock(async () => {
      if (!this.transactionClient) {
        await this.initialize();
      }
      if (!this.transactionClient) {
        throw new Error('Signer not initialized');
      }
      const submitCancel = async () => {
        const txInfo = await this.transactionClient!.getCancelOrderTransaction(request);
        return this.apiClient.sendRawTx(txInfo);
      };

      try {
        return await submitCancel();
      } catch (error) {
        this.transactionClient.resetNonce();
        if (!isLighterInvalidNonceError(error)) {
          throw error;
        }
        // Nonce endpoints can lag right after a previous tx; brief delay improves retry reliability.
        await new Promise((resolve) => setTimeout(resolve, 150));
        // One retry after forcing nonce refresh from server.
        try {
          return await submitCancel();
        } catch (retryError) {
          this.transactionClient.resetNonce();
          throw retryError;
        }
      }
    });
  }

  async updateLeverage(request: LighterUpdateLeverageRequest): Promise<void> {
    await this.runWithSubmissionLock(async () => {
      if (!this.transactionClient) {
        await this.initialize();
      }
      if (!this.transactionClient) {
        throw new Error('Signer not initialized');
      }

      const submitUpdateLeverage = async () => {
        const txInfo = await this.transactionClient!.getUpdateLeverageTransaction(request);
        return await this.apiClient.sendRawTx(txInfo);
      };

      try {
        await submitUpdateLeverage();
      } catch (error) {
        this.transactionClient.resetNonce();
        if (!isLighterInvalidNonceError(error)) {
          throw error;
        }
        // Nonce endpoints can lag right after a previous tx; brief delay improves retry reliability.
        await new Promise((resolve) => setTimeout(resolve, 150));
        // One retry after forcing nonce refresh from server.
        try {
          await submitUpdateLeverage();
        } catch (retryError) {
          this.transactionClient.resetNonce();
          throw retryError;
        }
      }
    });
  }

  async approveIntegrator(request: LighterApproveIntegratorRequest): Promise<string> {
    return await this.runWithSubmissionLock(async () => {
      if (!this.transactionClient) {
        await this.initialize();
      }
      if (!this.transactionClient) {
        throw new Error('Signer not initialized');
      }

      const submitApproveIntegrator = async () => {
        const txInfo = await this.transactionClient!.getApproveIntegratorTransaction(request);
        return await this.apiClient.sendRawTx(txInfo);
      };

      try {
        return await submitApproveIntegrator();
      } catch (error) {
        this.transactionClient.resetNonce();
        if (!isLighterInvalidNonceError(error)) {
          throw error;
        }
        await new Promise((resolve) => setTimeout(resolve, 150));
        try {
          return await submitApproveIntegrator();
        } catch (retryError) {
          this.transactionClient.resetNonce();
          throw retryError;
        }
      }
    });
  }

  async getAuthToken(ttlSeconds: number): Promise<string> {
    if (!this.transactionClient) {
      await this.initialize();
    }
    if (!this.transactionClient) {
      throw new Error('Signer not initialized');
    }

    const deadline = BigInt(Math.floor(Date.now() / 1000) + ttlSeconds);
    return this.transactionClient.getAuthToken(deadline);
  }
}

export async function createLighterSignerAdapter(params: {
  exchangeName: ExchangeName;
  options: ExchangeOptions;
  apiClient: LighterApiClient;
  accountIndex: bigint;
  apiKeyIndex: number;
}): Promise<LighterSignerAdapter> {
  const provider = getProvider(params.options);
  if (provider && provider !== 'tealstreet-sdk') {
    throw new Error(
      `Unsupported Lighter signer provider "${provider}". Legacy third-party signer providers are removed; use tealstreet-sdk.`,
    );
  }

  const adapter = new TealstreetLighterSignerAdapter(
    params.options,
    params.exchangeName,
    params.apiClient,
    params.accountIndex,
    params.apiKeyIndex,
  );
  await adapter.initialize();
  return adapter;
}
