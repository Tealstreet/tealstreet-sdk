import {
  CompositeClient,
  IndexerClient,
  LocalWallet,
  Network,
  OrderExecution,
  OrderSide,
  OrderTimeInForce,
  OrderType,
  ValidatorClient,
} from '@dydxprotocol/v4-client-js';
import type { ExchangeOptions } from '../../types';
import { DydxClientConfig } from './dydx.types';

/**
 * Creates a LocalWallet from a private key
 * @param privateKey - Hex string with or without 0x prefix
 * @param prefix - Address prefix (default: 'dydx')
 */
export const createWalletFromPrivateKey = async (privateKey: string, prefix: string = 'dydx'): Promise<LocalWallet> => {
  // Remove 0x prefix if present
  const pkHex = privateKey.startsWith('0x') ? privateKey.slice(2) : privateKey;
  return await LocalWallet.fromPrivateKey(pkHex, prefix);
};

/**
 * dYdX SDK Client wrapper
 */
export class DydxSDKClient {
  public compositeClient: CompositeClient | null = null;
  public indexerClient: IndexerClient | null = null;
  public validatorClient: ValidatorClient | null = null;
  public wallet: LocalWallet | null = null;
  public subaccount: any | null = null;
  public config: DydxClientConfig;

  constructor(config: DydxClientConfig) {
    this.config = config;
  }

  /**
   * Initialize the dYdX SDK clients
   * @param mnemonic - Optional 12 or 24-word mnemonic phrase. If not provided, will use private key from config.
   */
  async initialize(mnemonic?: string): Promise<void> {
    // Create network configuration
    const network = this.config.testnet ? Network.testnet() : Network.mainnet();

    // Initialize wallet from mnemonic or private key
    if (mnemonic) {
      this.wallet = await LocalWallet.fromMnemonic(mnemonic);
    } else if (this.config.privateKey) {
      // Use private key if mnemonic not provided
      this.wallet = await createWalletFromPrivateKey(this.config.privateKey, 'dydx');
    } else {
      throw new Error('Either mnemonic or private key is required for dYdX wallet initialization');
    }

    // Verify wallet address matches config
    if (this.wallet.address !== this.config.address) {
      throw new Error(
        `Wallet address mismatch: expected ${this.config.address}, got ${this.wallet.address}. Check your credentials.`,
      );
    }

    // Initialize CompositeClient
    this.compositeClient = await CompositeClient.connect(network);

    // Initialize IndexerClient
    this.indexerClient = new IndexerClient(network.indexerConfig);

    // Initialize ValidatorClient
    this.validatorClient = this.compositeClient.validatorClient;

    // Store subaccount info (don't create SubaccountInfo - use wallet and number directly)
    this.subaccount = {
      wallet: this.wallet,
      subaccountNumber: this.config.subaccountNumber,
      address: this.wallet.address,
    } as any;
  }

  /**
   * Get the current block height
   */
  async getCurrentBlockHeight(): Promise<number> {
    if (!this.validatorClient) {
      throw new Error('Validator client not initialized');
    }
    return await this.validatorClient.get.latestBlockHeight();
  }

  /** Get the wallet's current governance-controlled perpetual fee tier. */
  async getUserFeeTier(address: string) {
    if (!this.validatorClient) {
      throw new Error('Validator client not initialized');
    }
    return await this.validatorClient.get.getUserFeeTier(address);
  }

  /**
   * Place an order using the CompositeClient
   */
  async placeOrder(params: {
    market: string;
    type: OrderType;
    side: OrderSide;
    price: number;
    size: number;
    clientId: number;
    timeInForce: OrderTimeInForce;
    postOnly: boolean;
    reduceOnly: boolean;
    triggerPrice?: number;
    goodTilTimeInSeconds?: number;
    currentHeight?: number;
    goodTilBlock?: number;
  }) {
    if (!this.compositeClient || !this.subaccount) {
      throw new Error('SDK client not initialized');
    }

    const {
      market,
      type,
      side,
      price,
      size,
      clientId,
      timeInForce,
      postOnly,
      reduceOnly,
      triggerPrice,
      goodTilTimeInSeconds,
      currentHeight,
      goodTilBlock,
    } = params;

    const isConditionalMarket = type === OrderType.STOP_MARKET || type === OrderType.TAKE_PROFIT_MARKET;
    const execution = postOnly
      ? OrderExecution.POST_ONLY
      : isConditionalMarket
        ? timeInForce === OrderTimeInForce.FOK
          ? OrderExecution.FOK
          : OrderExecution.IOC
        : OrderExecution.DEFAULT;

    const tx = await this.compositeClient.placeOrder(
      this.subaccount,
      market,
      type,
      side,
      price,
      size,
      clientId,
      timeInForce,
      goodTilTimeInSeconds,
      execution,
      postOnly,
      reduceOnly,
      triggerPrice,
      undefined,
      currentHeight,
      goodTilBlock,
    );

    return tx;
  }

  /**
   * Place a short-term order (expires in blocks)
   */
  async placeShortTermOrder(params: {
    market: string;
    side: OrderSide;
    price: number;
    size: number;
    clientId: number;
    goodTilBlock: number;
    timeInForce: OrderTimeInForce;
    reduceOnly: boolean;
  }) {
    if (!this.compositeClient || !this.subaccount) {
      throw new Error('SDK client not initialized');
    }

    const { market, side, price, size, clientId, goodTilBlock, timeInForce, reduceOnly } = params;

    const tx = await this.compositeClient.placeOrder(
      this.subaccount,
      market,
      OrderType.LIMIT,
      side,
      price,
      size,
      clientId,
      timeInForce,
      undefined,
      OrderExecution.DEFAULT,
      false,
      reduceOnly,
      undefined,
      undefined,
      undefined,
      goodTilBlock,
    );

    return tx;
  }

  /**
   * Cancel an order
   */
  async cancelOrder(params: {
    clientId: number;
    orderFlags: number;
    marketId: string;
    goodTilBlock?: number;
    goodTilTimeInSeconds?: number;
  }) {
    if (!this.compositeClient || !this.subaccount) {
      throw new Error('SDK client not initialized');
    }

    const { clientId, orderFlags, marketId, goodTilBlock, goodTilTimeInSeconds } = params;

    const tx = await this.compositeClient.cancelOrder(
      this.subaccount as any,
      clientId,
      orderFlags,
      marketId,
      goodTilBlock,
      goodTilTimeInSeconds,
    );

    return tx;
  }

  /**
   * Batch cancel orders for specific markets
   */
  async batchCancelOrders(params: {
    marketIds: string[];
    clientIds: number[];
    clobPairIds: string[];
    goodTilBlocks: number[];
  }) {
    if (!this.compositeClient || !this.subaccount) {
      throw new Error('SDK client not initialized');
    }

    const { clientIds, goodTilBlocks } = params;

    // Note: API signature may have changed - using type assertion for now
    const tx = await (this.compositeClient as any).batchCancelShortTermOrders(
      this.subaccount as any,
      clientIds,
      goodTilBlocks,
    );

    return tx;
  }

  /**
   * Get subaccount information
   */
  async getSubaccount(address: string, subaccountNumber: number) {
    if (!this.indexerClient) {
      throw new Error('Indexer client not initialized');
    }

    return await this.indexerClient.account.getSubaccount(address, subaccountNumber);
  }

  /**
   * Get all subaccounts for an address
   */
  async getSubaccounts(address: string) {
    if (!this.indexerClient) {
      throw new Error('Indexer client not initialized');
    }

    return await this.indexerClient.account.getSubaccounts(address);
  }

  /**
   * Get perpetual positions
   */
  async getPerpetualPositions(address: string, subaccountNumber: number) {
    if (!this.indexerClient) {
      throw new Error('Indexer client not initialized');
    }

    return await this.indexerClient.account.getSubaccountPerpetualPositions(address, subaccountNumber);
  }

  /**
   * Get orders
   */
  async getOrders(address: string, subaccountNumber: number, market?: string) {
    if (!this.indexerClient) {
      throw new Error('Indexer client not initialized');
    }

    return await this.indexerClient.account.getSubaccountOrders(address, subaccountNumber, market);
  }

  /**
   * Get fills (trade history)
   */
  async getFills(address: string, subaccountNumber: number, market?: string) {
    if (!this.indexerClient) {
      throw new Error('Indexer client not initialized');
    }

    return await this.indexerClient.account.getSubaccountFills(address, subaccountNumber, market);
  }
}

/**
 * Initialize dYdX SDK from ExchangeOptions
 */
export const initializeDydxSDK = async (options: ExchangeOptions): Promise<DydxSDKClient> => {
  // Extract configuration
  const address = options.key; // Wallet address stored in apiKey field
  const secret = options.secret; // Either mnemonic phrase or private key
  const subaccountNumber = options.dydxSubaccountNumber || 0;
  const testnet = options.testnet || false;

  if (!address) {
    throw new Error('Missing dYdX wallet address (stored in apiKey field)');
  }

  if (!secret) {
    throw new Error('Missing dYdX credentials (mnemonic or private key in secret field)');
  }

  // Determine if secret is a mnemonic (space-separated words) or private key (hex string)
  const isMnemonic = secret.trim().split(/\s+/).length >= 12;

  // Create SDK client
  const config: DydxClientConfig = {
    address,
    privateKey: isMnemonic ? '' : secret,
    subaccountNumber,
    testnet,
  };

  const sdkClient = new DydxSDKClient(config);

  // Initialize with mnemonic if detected, otherwise will use private key from config
  await sdkClient.initialize(isMnemonic ? secret : undefined);

  return sdkClient;
};
