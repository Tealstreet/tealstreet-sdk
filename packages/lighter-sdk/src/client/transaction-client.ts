/**
 * Lighter Transaction Client
 *
 * Handles transaction creation, signing, and submission for Lighter protocol.
 */

import type { PoseidonCrypto } from '@tealstreet/poseidon-crypto';
import type { LighterApiClient } from './api-client';
import type {
  L2ApproveIntegratorTxInfo,
  L2CancelOrderTxInfo,
  L2CreateGroupedOrdersTxInfo,
  L2CreateOrderTxInfo,
  L2TransferTxInfo,
  L2UpdateLeverageTxInfo,
} from './http-types';

/**
 * Parameters for creating an order
 */
export interface CreateOrderParams {
  /** Unique order ID */
  orderId: bigint;
  /** Market ID */
  marketId: number;
  /** True for buy order, false for sell order */
  isBuy: boolean;
  /** Order price in base units */
  price: bigint;
  /** Order amount in base units */
  amount: bigint;
  /** Post-only order (default: false) */
  postOnly?: boolean;
  /** Reduce-only order (default: false) */
  reduceOnly?: boolean;
  /** Order type (default: limit) */
  orderType?: 'limit' | 'market' | 'stop_loss' | 'stop_loss_limit' | 'take_profit' | 'take_profit_limit';
  /** Trigger price for conditional orders */
  triggerPrice?: bigint;
  /** Optional explicit order expiry in Unix milliseconds */
  orderExpiry?: bigint;
  /** Optional explicit transaction expiry in Unix milliseconds */
  expiredAt?: bigint;
  /** Time in force */
  timeInForce?: 'GoodTillCancel' | 'ImmediateOrCancel' | 'FillOrKill' | 'PostOnly';
  /** Optional partner/integrator account index */
  integratorAccountIndex?: bigint;
  /** Optional partner maker fee value (fee_value / 1e6) */
  integratorMakerFee?: number;
  /** Optional partner taker fee value (fee_value / 1e6) */
  integratorTakerFee?: number;
}

export type LighterGroupedOrderType = 1 | 2 | 3;

export interface CreateGroupedOrdersParams {
  groupingType: LighterGroupedOrderType;
  orders: CreateOrderParams[];
  /** Optional partner/integrator account index */
  integratorAccountIndex?: bigint;
  /** Optional partner maker fee value (fee_value / 1e6) */
  integratorMakerFee?: number;
  /** Optional partner taker fee value (fee_value / 1e6) */
  integratorTakerFee?: number;
}

type CreateOrderDataBuildOptions = {
  expiredAt?: bigint;
  defaultOrderExpiry?: bigint;
};

/**
 * Parameters for canceling an order
 */
export interface CancelOrderParams {
  /** Order ID to cancel */
  orderId: bigint;
  /** Market ID */
  marketId: number;
}

/**
 * Parameters for transferring funds
 */
export interface TransferParams {
  /** Destination account index */
  toAccountIndex: bigint;
  /** Transfer amount in base units */
  amount: bigint;
  /** Token ID (default: 0) */
  tokenId?: number;
}

/**
 * Parameters for updating leverage
 */
export interface UpdateLeverageParams {
  /** Market ID */
  marketId: number;
  /** Desired leverage (e.g. 5 for 5x) */
  leverage: number;
  /** Margin mode (0 = cross, 1 = isolated) */
  marginMode: 0 | 1;
}

/**
 * Parameters for approving an integrator account for partner attribution
 */
export interface ApproveIntegratorParams {
  /** Partner account index where integrator fees accrue */
  integratorAccountIndex: bigint;
  /** Max perps taker fee value (fee_value / 1e6) */
  maxPerpsTakerFee: number;
  /** Max perps maker fee value (fee_value / 1e6) */
  maxPerpsMakerFee: number;
  /** Max spot taker fee value (fee_value / 1e6) */
  maxSpotTakerFee: number;
  /** Max spot maker fee value (fee_value / 1e6) */
  maxSpotMakerFee: number;
  /** Approval expiry in Unix milliseconds; use 0 when revoking */
  approvalExpiry: bigint;
  /** Optional explicit nonce override (useful for external L1 signature coordination) */
  nonce?: bigint;
  /** Optional L1 signature for cross-master approval path */
  l1Signature?: string;
}

/**
 * Transaction client for building, signing, and submitting transactions
 *
 * @example
 * ```typescript
 * const txClient = new TransactionClient({
 *   httpClient,
 *   crypto,
 *   chainId: 1,
 *   privateKey: '0x...',
 *   accountIndex: 1n,
 *   apiKeyIndex: 0
 * });
 *
 * const orderTx = await txClient.getCreateOrderTransaction({
 *   orderId: 123n,
 *   marketId: 1,
 *   isBuy: true,
 *   price: 100000n,
 *   amount: 50000n
 * });
 *
 * const txHash = await httpClient.sendRawTx(orderTx);
 * ```
 */
export class TransactionClient {
  private static readonly CREATE_ORDER_TX_TYPE = 14;
  private static readonly CANCEL_ORDER_TX_TYPE = 15;
  private static readonly UPDATE_LEVERAGE_TX_TYPE = 20;
  private static readonly APPROVE_INTEGRATOR_TX_TYPE = 45;
  private static readonly CREATE_GROUPED_ORDERS_TX_TYPE = 28;
  private static readonly MAX_GROUPED_ORDER_COUNT = 3;
  private static readonly GROUPING_TYPE_ONE_TRIGGERS_THE_OTHER = 1;
  private static readonly GROUPING_TYPE_ONE_CANCELS_THE_OTHER = 2;
  private static readonly GROUPING_TYPE_ONE_TRIGGERS_A_ONE_CANCELS_THE_OTHER = 3;
  private static readonly NB_ATTRIBUTES_PER_TX = 4;
  private static readonly ATTRIBUTE_TYPE_INTEGRATOR_ACCOUNT_INDEX = 1;
  private static readonly ATTRIBUTE_TYPE_INTEGRATOR_TAKER_FEE = 2;
  private static readonly ATTRIBUTE_TYPE_INTEGRATOR_MAKER_FEE = 3;
  private static readonly MARGIN_FRACTION_TICK = 10_000;
  private static readonly FEE_TICK = 1_000_000;
  private static readonly MAX_TIMESTAMP = (1n << 48n) - 1n;

  private readonly httpClient: LighterApiClient;
  private readonly crypto: PoseidonCrypto;
  private readonly chainId: number;
  private readonly privateKey: string;
  private readonly accountIndex: bigint;
  private readonly apiKeyIndex: number;
  private cachedNonce?: bigint;
  private nonceLock: Promise<void> = Promise.resolve();

  /**
   * Creates a new transaction client
   *
   * @param config - Transaction client configuration
   */
  constructor(config: {
    httpClient: LighterApiClient;
    crypto: PoseidonCrypto;
    chainId: number;
    privateKey: string;
    accountIndex: bigint;
    apiKeyIndex: number;
  }) {
    this.httpClient = config.httpClient;
    this.crypto = config.crypto;
    this.chainId = config.chainId;
    this.privateKey = config.privateKey;
    this.accountIndex = config.accountIndex;
    this.apiKeyIndex = config.apiKeyIndex;
  }

  /**
   * Get the next available nonce, with caching
   *
   * @param forceRefresh - Force refresh from server (default: false)
   * @returns Next available nonce
   */
  private async getNextNonce(forceRefresh: boolean = false): Promise<bigint> {
    // Serialize nonce allocation to prevent duplicate nonce usage under concurrent order placement.
    const previousLock = this.nonceLock;
    let releaseLock!: () => void;
    this.nonceLock = new Promise<void>((resolve) => {
      releaseLock = resolve;
    });

    await previousLock;

    try {
      if (!forceRefresh && this.cachedNonce !== undefined) {
        const nonce = this.cachedNonce;
        this.cachedNonce = nonce + 1n;
        return nonce;
      }

      const nonce = await this.httpClient.getNextNonce(this.accountIndex, this.apiKeyIndex);
      this.cachedNonce = nonce + 1n;
      return nonce;
    } finally {
      releaseLock();
    }
  }

  private bytesToHex(bytes: Uint8Array): string {
    return Array.from(bytes)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }

  private utf8ToHex(value: string): string {
    if (typeof TextEncoder !== 'undefined') {
      return this.bytesToHex(new TextEncoder().encode(value));
    }

    // Node fallback for tests/tools.
    const nodeBuffer = (globalThis as any).Buffer;
    if (nodeBuffer) {
      return nodeBuffer.from(value, 'utf-8').toString('hex');
    }

    throw new Error('No UTF-8 encoder available in current runtime');
  }

  private toHexNoPrefix(value: string): string {
    return value.startsWith('0x') ? value.slice(2) : value;
  }

  private toUint32FieldHex(value: number, fieldName: string): string {
    if (!Number.isInteger(value) || value < 0 || value > 0xffffffff) {
      throw new Error(`${fieldName} is out of uint32 range`);
    }
    const buffer = new ArrayBuffer(8);
    const view = new DataView(buffer);
    view.setUint32(0, value, true);
    return this.bytesToHex(new Uint8Array(buffer));
  }

  private toInt64FieldHex(value: bigint, fieldName: string): string {
    const min = -(1n << 63n);
    const max = (1n << 63n) - 1n;
    if (value < min || value > max) {
      throw new Error(`${fieldName} is out of int64 range`);
    }
    const buffer = new ArrayBuffer(8);
    const view = new DataView(buffer);
    view.setBigInt64(0, value, true);
    return this.bytesToHex(new Uint8Array(buffer));
  }

  private mapOrderTypeValue(orderType?: CreateOrderParams['orderType']): number {
    if (orderType === 'market') {
      return 1;
    }
    if (orderType === 'stop_loss') {
      return 2;
    }
    if (orderType === 'stop_loss_limit') {
      return 3;
    }
    if (orderType === 'take_profit') {
      return 4;
    }
    if (orderType === 'take_profit_limit') {
      return 5;
    }
    return 0;
  }

  private mapTimeInForceValue(params: CreateOrderParams): number {
    if (params.orderType === 'market') {
      return 0;
    }
    if (params.postOnly || params.timeInForce === 'PostOnly') {
      return 2;
    }
    if (params.timeInForce === 'ImmediateOrCancel' || params.timeInForce === 'FillOrKill') {
      return 0;
    }
    return 1;
  }

  /**
   * Build transaction data for create order
   *
   * @param params - Order parameters
   * @param nonce - Transaction nonce
   * @returns Serialized transaction data (hex string)
   */
  private buildCreateOrderData(
    params: CreateOrderParams,
    nonce: bigint,
    options: CreateOrderDataBuildOptions = {},
  ): {
    messageHex: string;
    l2AttributesMessageHex?: string;
    expiredAt: bigint;
    orderTypeValue: number;
    timeInForceValue: number;
    triggerPrice: bigint;
    orderExpiry: bigint;
    integratorAccountIndex?: bigint;
    integratorMakerFee?: number;
    integratorTakerFee?: number;
  } {
    const expiredAt = params.expiredAt ?? options.expiredAt ?? BigInt(Date.now() + 10 * 60 * 1000);
    const orderTypeValue = this.mapOrderTypeValue(params.orderType);
    const timeInForceValue = this.mapTimeInForceValue(params);
    const triggerPrice = params.triggerPrice ?? 0n;
    const isConditionalOrderType =
      params.orderType === 'stop_loss' ||
      params.orderType === 'stop_loss_limit' ||
      params.orderType === 'take_profit' ||
      params.orderType === 'take_profit_limit';
    const shouldUseImmediateExpiry = timeInForceValue === 0 && !isConditionalOrderType;
    const orderExpiry =
      params.orderExpiry ??
      (shouldUseImmediateExpiry ? 0n : (options.defaultOrderExpiry ?? BigInt(Date.now() + 28 * 24 * 60 * 60 * 1000)));
    const reduceOnly = params.reduceOnly ? 1 : 0;
    const isAsk = params.isBuy ? 0 : 1;
    const hasIntegratorFields =
      params.integratorAccountIndex !== undefined &&
      params.integratorMakerFee !== undefined &&
      params.integratorTakerFee !== undefined;

    if (hasIntegratorFields) {
      if (params.integratorAccountIndex! < 0n) {
        throw new Error('integratorAccountIndex must be non-negative');
      }
      if (!Number.isInteger(params.integratorMakerFee!) || params.integratorMakerFee! < 0) {
        throw new Error('integratorMakerFee must be a non-negative integer');
      }
      if (!Number.isInteger(params.integratorTakerFee!) || params.integratorTakerFee! < 0) {
        throw new Error('integratorTakerFee must be a non-negative integer');
      }
    }

    const parts = [
      this.toUint32FieldHex(this.chainId, 'chainId'),
      this.toUint32FieldHex(TransactionClient.CREATE_ORDER_TX_TYPE, 'txType'),
      this.toInt64FieldHex(nonce, 'nonce'),
      this.toInt64FieldHex(expiredAt, 'expiredAt'),
      this.toInt64FieldHex(this.accountIndex, 'accountIndex'),
      this.toUint32FieldHex(this.apiKeyIndex, 'apiKeyIndex'),
      this.toUint32FieldHex(params.marketId, 'marketId'),
      this.toInt64FieldHex(params.orderId, 'orderId'),
      this.toInt64FieldHex(params.amount, 'amount'),
      this.toUint32FieldHex(Number(params.price), 'price'),
      this.toUint32FieldHex(isAsk, 'isAsk'),
      this.toUint32FieldHex(orderTypeValue, 'orderType'),
      this.toUint32FieldHex(timeInForceValue, 'timeInForce'),
      this.toUint32FieldHex(reduceOnly, 'reduceOnly'),
      this.toUint32FieldHex(Number(triggerPrice), 'triggerPrice'),
      this.toInt64FieldHex(orderExpiry, 'orderExpiry'),
    ];

    return {
      messageHex: parts.join(''),
      l2AttributesMessageHex: hasIntegratorFields
        ? this.buildCreateOrderL2AttributesData({
            integratorAccountIndex: params.integratorAccountIndex!,
            integratorTakerFee: params.integratorTakerFee!,
            integratorMakerFee: params.integratorMakerFee!,
          })
        : undefined,
      expiredAt,
      orderTypeValue,
      timeInForceValue,
      triggerPrice,
      orderExpiry,
      integratorAccountIndex: hasIntegratorFields ? params.integratorAccountIndex : undefined,
      integratorMakerFee: hasIntegratorFields ? params.integratorMakerFee : undefined,
      integratorTakerFee: hasIntegratorFields ? params.integratorTakerFee : undefined,
    };
  }

  /**
   * Build L2 tx-attributes data for create-order hashing.
   *
   * This follows lighter-go `L2TxAttributes.Hash`:
   * - include only non-zero attributes,
   * - sort by attribute type ascending,
   * - pad to NB_ATTRIBUTES_PER_TX with (type=0, value=0),
   * - hash type/value pairs.
   */
  private buildCreateOrderL2AttributesData(params: {
    integratorAccountIndex: bigint;
    integratorTakerFee: number;
    integratorMakerFee: number;
  }): string {
    const attributes: Array<{ type: number; value: bigint }> = [
      {
        type: TransactionClient.ATTRIBUTE_TYPE_INTEGRATOR_ACCOUNT_INDEX,
        value: params.integratorAccountIndex,
      },
      {
        type: TransactionClient.ATTRIBUTE_TYPE_INTEGRATOR_TAKER_FEE,
        value: BigInt(params.integratorTakerFee),
      },
      {
        type: TransactionClient.ATTRIBUTE_TYPE_INTEGRATOR_MAKER_FEE,
        value: BigInt(params.integratorMakerFee),
      },
    ]
      .filter((attribute) => attribute.value !== 0n)
      .sort((left, right) => left.type - right.type);

    while (attributes.length < TransactionClient.NB_ATTRIBUTES_PER_TX) {
      attributes.push({ type: 0, value: 0n });
    }

    const parts: string[] = [];
    for (const attribute of attributes) {
      parts.push(this.toUint32FieldHex(attribute.type, `l2AttributeType_${attribute.type}`));
      parts.push(this.toInt64FieldHex(attribute.value, `l2AttributeValue_${attribute.type}`));
    }

    return parts.join('');
  }

  private buildGroupedOrderInfoData(
    params: CreateOrderParams,
    txPayload: {
      orderTypeValue: number;
      timeInForceValue: number;
      triggerPrice: bigint;
      orderExpiry: bigint;
    },
  ): string {
    const reduceOnly = params.reduceOnly ? 1 : 0;
    const isAsk = params.isBuy ? 0 : 1;
    const parts = [
      this.toUint32FieldHex(params.marketId, 'marketId'),
      this.toInt64FieldHex(params.orderId, 'orderId'),
      this.toInt64FieldHex(params.amount, 'amount'),
      this.toUint32FieldHex(Number(params.price), 'price'),
      this.toUint32FieldHex(isAsk, 'isAsk'),
      this.toUint32FieldHex(txPayload.orderTypeValue, 'orderType'),
      this.toUint32FieldHex(txPayload.timeInForceValue, 'timeInForce'),
      this.toUint32FieldHex(reduceOnly, 'reduceOnly'),
      this.toUint32FieldHex(Number(txPayload.triggerPrice), 'triggerPrice'),
      this.toInt64FieldHex(txPayload.orderExpiry, 'orderExpiry'),
    ];
    return parts.join('');
  }

  private isStopLossOrderType(orderTypeValue: number): boolean {
    return orderTypeValue === 2 || orderTypeValue === 3;
  }

  private isTakeProfitOrderType(orderTypeValue: number): boolean {
    return orderTypeValue === 4 || orderTypeValue === 5;
  }

  private isChildGroupedOrderType(orderTypeValue: number): boolean {
    return this.isStopLossOrderType(orderTypeValue) || this.isTakeProfitOrderType(orderTypeValue);
  }

  private validateCreateGroupedOrdersParams(params: CreateGroupedOrdersParams): void {
    if (params.orders.length === 0 || params.orders.length > TransactionClient.MAX_GROUPED_ORDER_COUNT) {
      throw new Error(`Lighter grouped orders require 1-${TransactionClient.MAX_GROUPED_ORDER_COUNT} orders`);
    }

    const marketId = params.orders[0].marketId;
    if (!Number.isInteger(marketId) || marketId < 0 || marketId > 254) {
      throw new Error(`Grouped orders require a valid perps market id (0-254): ${marketId}`);
    }

    const orderIds = new Set<string>();
    for (const order of params.orders) {
      if (order.marketId !== marketId) {
        throw new Error('Grouped orders must all use the same market id');
      }
      const orderId = order.orderId.toString();
      if (orderIds.has(orderId)) {
        throw new Error(`Grouped orders contain duplicate client order id ${orderId}`);
      }
      orderIds.add(orderId);
    }

    const orderTypes = params.orders.map((order) => this.mapOrderTypeValue(order.orderType));
    if (params.groupingType === TransactionClient.GROUPING_TYPE_ONE_CANCELS_THE_OTHER) {
      if (params.orders.length !== 2) {
        throw new Error('Lighter OCO grouped orders require exactly two orders');
      }
      const [first, second] = params.orders;
      if (first.amount !== second.amount) {
        throw new Error('Lighter OCO grouped orders require matching base amounts');
      }
      if (first.isBuy !== second.isBuy) {
        throw new Error('Lighter OCO grouped orders require the same side');
      }
      if (!first.reduceOnly || !second.reduceOnly) {
        throw new Error('Lighter OCO grouped orders must be reduce-only');
      }
      if (!orderTypes.every((orderType) => this.isChildGroupedOrderType(orderType))) {
        throw new Error('Lighter OCO grouped orders must be stop-loss/take-profit children');
      }
      if (
        !orderTypes.some((orderType) => this.isStopLossOrderType(orderType)) ||
        !orderTypes.some((orderType) => this.isTakeProfitOrderType(orderType))
      ) {
        throw new Error('Lighter OCO grouped orders require one stop-loss and one take-profit order');
      }
      return;
    }

    if (params.groupingType === TransactionClient.GROUPING_TYPE_ONE_TRIGGERS_THE_OTHER) {
      if (params.orders.length !== 2) {
        throw new Error('Lighter OTO grouped orders require exactly two orders');
      }
      const [parent, child] = params.orders;
      const parentType = orderTypes[0];
      const childType = orderTypes[1];
      if (parentType !== 0 && parentType !== 1) {
        throw new Error('Lighter OTO parent order must be limit or market');
      }
      if (!this.isChildGroupedOrderType(childType)) {
        throw new Error('Lighter OTO child order must be stop-loss or take-profit');
      }
      if (parent.isBuy === child.isBuy) {
        throw new Error('Lighter OTO child order must be opposite side from parent');
      }
      if (child.amount !== 0n) {
        throw new Error('Lighter OTO child order amount must be 0');
      }
      return;
    }

    if (params.groupingType === TransactionClient.GROUPING_TYPE_ONE_TRIGGERS_A_ONE_CANCELS_THE_OTHER) {
      if (params.orders.length !== 3) {
        throw new Error('Lighter OTOCO grouped orders require exactly three orders');
      }
      const [parent, firstChild, secondChild] = params.orders;
      const parentType = orderTypes[0];
      const childTypes = orderTypes.slice(1);
      if (parentType !== 0 && parentType !== 1) {
        throw new Error('Lighter OTOCO parent order must be limit or market');
      }
      if (!childTypes.every((orderType) => this.isChildGroupedOrderType(orderType))) {
        throw new Error('Lighter OTOCO children must be stop-loss/take-profit orders');
      }
      if (
        !childTypes.some((orderType) => this.isStopLossOrderType(orderType)) ||
        !childTypes.some((orderType) => this.isTakeProfitOrderType(orderType))
      ) {
        throw new Error('Lighter OTOCO children require one stop-loss and one take-profit order');
      }
      if (parent.isBuy === firstChild.isBuy || parent.isBuy === secondChild.isBuy) {
        throw new Error('Lighter OTOCO child orders must be opposite side from parent');
      }
      if (firstChild.amount !== 0n || secondChild.amount !== 0n) {
        throw new Error('Lighter OTOCO child order amounts must be 0');
      }
      return;
    }

    throw new Error(`Unsupported Lighter grouped order type: ${params.groupingType}`);
  }

  private buildCreateGroupedOrdersData(
    params: CreateGroupedOrdersParams,
    nonce: bigint,
  ): {
    messageHex: string;
    l2AttributesMessageHex?: string;
    expiredAt: bigint;
    orders: L2CreateGroupedOrdersTxInfo['groupedOrderDetails']['orders'];
    integratorAccountIndex?: bigint;
    integratorMakerFee?: number;
    integratorTakerFee?: number;
  } {
    this.validateCreateGroupedOrdersParams(params);
    if (!this.crypto.isReady()) {
      throw new Error('Crypto module not ready. Please wait for initialization.');
    }

    const expiredAt = BigInt(Date.now() + 10 * 60 * 1000);
    const defaultOrderExpiry = BigInt(Date.now() + 28 * 24 * 60 * 60 * 1000);
    const orderPayloads = params.orders.map((order) =>
      this.buildCreateOrderData(order, nonce, {
        expiredAt,
        defaultOrderExpiry,
      }),
    );

    const orderHashes = orderPayloads.map((orderPayload, index) => {
      const orderHash = this.crypto.poseidon2Hash(
        this.buildGroupedOrderInfoData(params.orders[index], orderPayload),
      );
      if (typeof orderHash === 'object' && 'error' in orderHash) {
        throw new Error(`Failed to hash grouped order ${index}: ${orderHash.error}`);
      }
      return orderHash;
    });

    let aggregatedOrderHash = orderHashes[0];
    for (let index = 1; index < orderHashes.length; index++) {
      const nextHash = this.crypto.poseidon2HashTwoToOne(aggregatedOrderHash, orderHashes[index]);
      if (typeof nextHash === 'object' && 'error' in nextHash) {
        throw new Error(`Failed to aggregate grouped order hash ${index}: ${nextHash.error}`);
      }
      aggregatedOrderHash = nextHash;
    }

    const parts = [
      this.toUint32FieldHex(this.chainId, 'chainId'),
      this.toUint32FieldHex(TransactionClient.CREATE_GROUPED_ORDERS_TX_TYPE, 'txType'),
      this.toInt64FieldHex(nonce, 'nonce'),
      this.toInt64FieldHex(expiredAt, 'expiredAt'),
      this.toInt64FieldHex(this.accountIndex, 'accountIndex'),
      this.toUint32FieldHex(this.apiKeyIndex, 'apiKeyIndex'),
      this.toUint32FieldHex(params.groupingType, 'groupingType'),
      aggregatedOrderHash,
    ];

    const hasIntegratorFields =
      params.integratorAccountIndex !== undefined &&
      params.integratorMakerFee !== undefined &&
      params.integratorTakerFee !== undefined;

    return {
      messageHex: parts.join(''),
      l2AttributesMessageHex: hasIntegratorFields
        ? this.buildCreateOrderL2AttributesData({
            integratorAccountIndex: params.integratorAccountIndex!,
            integratorTakerFee: params.integratorTakerFee!,
            integratorMakerFee: params.integratorMakerFee!,
          })
        : undefined,
      expiredAt,
      orders: params.orders.map((order, index) => ({
        orderId: order.orderId,
        marketId: order.marketId,
        isBuy: order.isBuy,
        price: order.price,
        amount: order.amount,
        reduceOnly: order.reduceOnly,
        orderTypeValue: orderPayloads[index].orderTypeValue,
        timeInForceValue: orderPayloads[index].timeInForceValue,
        triggerPrice: orderPayloads[index].triggerPrice,
        orderExpiry: orderPayloads[index].orderExpiry,
      })),
      integratorAccountIndex: hasIntegratorFields ? params.integratorAccountIndex : undefined,
      integratorMakerFee: hasIntegratorFields ? params.integratorMakerFee : undefined,
      integratorTakerFee: hasIntegratorFields ? params.integratorTakerFee : undefined,
    };
  }

  /**
   * Build transaction data for cancel order
   *
   * @param params - Cancel parameters
   * @param nonce - Transaction nonce
   * @returns Serialized transaction data (hex string)
   */
  private buildCancelOrderData(
    params: CancelOrderParams,
    nonce: bigint,
  ): {
    messageHex: string;
    expiredAt: bigint;
  } {
    const expiredAt = BigInt(Date.now() + 10 * 60 * 1000);
    const parts = [
      this.toUint32FieldHex(this.chainId, 'chainId'),
      this.toUint32FieldHex(TransactionClient.CANCEL_ORDER_TX_TYPE, 'txType'),
      this.toInt64FieldHex(nonce, 'nonce'),
      this.toInt64FieldHex(expiredAt, 'expiredAt'),
      this.toInt64FieldHex(this.accountIndex, 'accountIndex'),
      this.toUint32FieldHex(this.apiKeyIndex, 'apiKeyIndex'),
      this.toUint32FieldHex(params.marketId, 'marketId'),
      this.toInt64FieldHex(params.orderId, 'orderId'),
    ];
    return { messageHex: parts.join(''), expiredAt };
  }

  /**
   * Build transaction data for transfer
   *
   * @param params - Transfer parameters
   * @param nonce - Transaction nonce
   * @returns Serialized transaction data (hex string)
   */
  private buildTransferData(params: TransferParams, nonce: bigint): string {
    // Transaction format (hex-encoded):
    // - chainId (4 bytes)
    // - accountIndex (32 bytes)
    // - nonce (32 bytes)
    // - toAccountIndex (32 bytes)
    // - amount (32 bytes)
    // - tokenId (4 bytes)

    const tokenId = params.tokenId ?? 0;

    const parts = [
      this.chainId.toString(16).padStart(8, '0'),
      this.accountIndex.toString(16).padStart(64, '0'),
      nonce.toString(16).padStart(64, '0'),
      params.toAccountIndex.toString(16).padStart(64, '0'),
      params.amount.toString(16).padStart(64, '0'),
      tokenId.toString(16).padStart(8, '0'),
    ];

    return parts.join('');
  }

  /**
   * Build transaction data for update leverage
   *
   * Based on lighter-go L2UpdateLeverage hash order:
   * chainId, txType, nonce, expiredAt, accountIndex, apiKeyIndex, marketIndex, initialMarginFraction, marginMode.
   */
  private buildUpdateLeverageData(
    params: UpdateLeverageParams,
    nonce: bigint,
  ): {
    messageHex: string;
    expiredAt: bigint;
    initialMarginFraction: number;
  } {
    if (!Number.isFinite(params.leverage) || params.leverage <= 0) {
      throw new Error(`Invalid leverage value: ${params.leverage}`);
    }
    if (!Number.isInteger(params.marketId) || params.marketId < 0 || params.marketId > 254) {
      throw new Error(`marketId is out of valid range (0-254): ${params.marketId}`);
    }
    if (params.marginMode !== 0 && params.marginMode !== 1) {
      throw new Error(`marginMode must be 0 or 1. Received: ${params.marginMode}`);
    }

    const initialMarginFraction = Math.floor(TransactionClient.MARGIN_FRACTION_TICK / params.leverage);
    if (initialMarginFraction <= 0 || initialMarginFraction > TransactionClient.MARGIN_FRACTION_TICK) {
      throw new Error(
        `Derived initial margin fraction is out of range: ${initialMarginFraction} (from leverage ${params.leverage})`,
      );
    }

    const expiredAt = BigInt(Date.now() + 10 * 60 * 1000);
    const parts = [
      this.toUint32FieldHex(this.chainId, 'chainId'),
      this.toUint32FieldHex(TransactionClient.UPDATE_LEVERAGE_TX_TYPE, 'txType'),
      this.toInt64FieldHex(nonce, 'nonce'),
      this.toInt64FieldHex(expiredAt, 'expiredAt'),
      this.toInt64FieldHex(this.accountIndex, 'accountIndex'),
      this.toUint32FieldHex(this.apiKeyIndex, 'apiKeyIndex'),
      this.toInt64FieldHex(BigInt(params.marketId), 'marketId'),
      this.toInt64FieldHex(BigInt(initialMarginFraction), 'initialMarginFraction'),
      this.toUint32FieldHex(params.marginMode, 'marginMode'),
    ];

    return {
      messageHex: parts.join(''),
      expiredAt,
      initialMarginFraction,
    };
  }

  /**
   * Build transaction data for approve integrator
   *
   * Hash/input order follows lighter-go `L2ApproveIntegratorTxInfo.Hash`:
   * chainId, txType, nonce, expiredAt, accountIndex, apiKeyIndex, integratorAccountIndex,
   * maxPerpsTakerFee, maxPerpsMakerFee, maxSpotTakerFee, maxSpotMakerFee, approvalExpiry.
   */
  private buildApproveIntegratorData(
    params: ApproveIntegratorParams,
    nonce: bigint,
  ): {
    messageHex: string;
    expiredAt: bigint;
  } {
    if (params.integratorAccountIndex < 0n) {
      throw new Error('integratorAccountIndex must be non-negative');
    }

    const feeFields: Array<
      [
        keyof Pick<
          ApproveIntegratorParams,
          'maxPerpsTakerFee' | 'maxPerpsMakerFee' | 'maxSpotTakerFee' | 'maxSpotMakerFee'
        >,
        number,
      ]
    > = [
      ['maxPerpsTakerFee', params.maxPerpsTakerFee],
      ['maxPerpsMakerFee', params.maxPerpsMakerFee],
      ['maxSpotTakerFee', params.maxSpotTakerFee],
      ['maxSpotMakerFee', params.maxSpotMakerFee],
    ];

    for (const [fieldName, value] of feeFields) {
      if (!Number.isInteger(value) || value < 0 || value > TransactionClient.FEE_TICK) {
        throw new Error(`${fieldName} must be an integer in [0, ${TransactionClient.FEE_TICK}]`);
      }
    }

    const isRevokingApproval = feeFields.every(([, value]) => value === 0);
    if (isRevokingApproval !== (params.approvalExpiry === 0n)) {
      throw new Error('approvalExpiry must be 0 iff all max fee fields are 0 (revocation semantics)');
    }
    if (params.approvalExpiry < 0n || params.approvalExpiry > TransactionClient.MAX_TIMESTAMP) {
      throw new Error(`approvalExpiry must be in [0, ${TransactionClient.MAX_TIMESTAMP.toString()}]`);
    }

    const expiredAt = BigInt(Date.now() + 10 * 60 * 1000);
    const parts = [
      this.toUint32FieldHex(this.chainId, 'chainId'),
      this.toUint32FieldHex(TransactionClient.APPROVE_INTEGRATOR_TX_TYPE, 'txType'),
      this.toInt64FieldHex(nonce, 'nonce'),
      this.toInt64FieldHex(expiredAt, 'expiredAt'),
      this.toInt64FieldHex(this.accountIndex, 'accountIndex'),
      this.toUint32FieldHex(this.apiKeyIndex, 'apiKeyIndex'),
      this.toInt64FieldHex(params.integratorAccountIndex, 'integratorAccountIndex'),
      this.toUint32FieldHex(params.maxPerpsTakerFee, 'maxPerpsTakerFee'),
      this.toUint32FieldHex(params.maxPerpsMakerFee, 'maxPerpsMakerFee'),
      this.toUint32FieldHex(params.maxSpotTakerFee, 'maxSpotTakerFee'),
      this.toUint32FieldHex(params.maxSpotMakerFee, 'maxSpotMakerFee'),
      this.toInt64FieldHex(params.approvalExpiry, 'approvalExpiry'),
    ];

    return {
      messageHex: parts.join(''),
      expiredAt,
    };
  }

  /**
   * Sign transaction data using Schnorr signature
   *
   * @param data - Transaction data (hex string)
   * @returns Signature (hex string)
   */
  private signTransaction(data: string): { signature: string; txHash: string } {
    if (!this.crypto.isReady()) {
      throw new Error('Crypto module not ready. Please wait for initialization.');
    }

    const txHashResult = this.crypto.poseidon2Hash(data);
    if (typeof txHashResult === 'object' && 'error' in txHashResult) {
      throw new Error(`Failed to hash transaction: ${txHashResult.error}`);
    }

    // IMPORTANT:
    // schnorrSign() in @tealstreet/poseidon-crypto already hashes the input message internally.
    // Passing txHashResult here would hash again and produce an invalid signature.
    const result = this.crypto.schnorrSign(data, this.privateKey);

    if (typeof result === 'object' && 'error' in result) {
      throw new Error(`Failed to sign transaction: ${result.error}`);
    }

    return { signature: result as string, txHash: txHashResult };
  }

  /**
   * Sign create-order data with optional L2 tx-attributes hash aggregation.
   */
  private signCreateOrderTransaction(data: string, l2AttributesData?: string): { signature: string; txHash: string } {
    if (!l2AttributesData) {
      return this.signTransaction(data);
    }

    if (!this.crypto.isReady()) {
      throw new Error('Crypto module not ready. Please wait for initialization.');
    }

    const hashFp5 = (
      this.crypto as PoseidonCrypto & { poseidon2HashFp5?: (input: string) => string | { error: string } }
    ).poseidon2HashFp5;
    if (!hashFp5) {
      throw new Error('Crypto module does not support poseidon2HashFp5');
    }

    const baseTxHash = hashFp5(data);
    if (typeof baseTxHash === 'object' && 'error' in baseTxHash) {
      throw new Error(`Failed to fp5-hash create order transaction: ${baseTxHash.error}`);
    }

    const attributesHash = hashFp5(l2AttributesData);
    if (typeof attributesHash === 'object' && 'error' in attributesHash) {
      throw new Error(`Failed to fp5-hash create order attributes: ${attributesHash.error}`);
    }

    // Match lighter-go attribute aggregation:
    // aggregatedHash = HashToQuinticExtension(baseFp5Hash || attributesFp5Hash)
    const aggregatedPreimage = `${this.toHexNoPrefix(baseTxHash)}${this.toHexNoPrefix(attributesHash)}`;
    const aggregatedTxHash = hashFp5(aggregatedPreimage);
    if (typeof aggregatedTxHash === 'object' && 'error' in aggregatedTxHash) {
      throw new Error(`Failed to fp5-hash aggregated create order payload: ${aggregatedTxHash.error}`);
    }

    const signature = this.crypto.schnorrSign(aggregatedPreimage, this.privateKey);
    if (typeof signature === 'object' && 'error' in signature) {
      throw new Error(`Failed to sign aggregated create order payload: ${signature.error}`);
    }

    return { signature, txHash: aggregatedTxHash };
  }

  /**
   * Create and sign a create order transaction
   *
   * @param params - Order parameters
   * @returns Signed transaction info ready for submission
   *
   * @example
   * ```typescript
   * const orderTx = await txClient.getCreateOrderTransaction({
   *   orderId: 123n,
   *   marketId: 1,
   *   isBuy: true,
   *   price: 100000n,
   *   amount: 50000n,
   *   postOnly: true
   * });
   * ```
   */
  public async getCreateOrderTransaction(params: CreateOrderParams): Promise<L2CreateOrderTxInfo> {
    const nonce = await this.getNextNonce();
    return this.buildCreateOrderTransaction(params, nonce);
  }

  private buildCreateOrderTransaction(params: CreateOrderParams, nonce: bigint): L2CreateOrderTxInfo {
    const txPayload = this.buildCreateOrderData(params, nonce);
    const signed = this.signCreateOrderTransaction(txPayload.messageHex, txPayload.l2AttributesMessageHex);

    return {
      type: 'create_order',
      data: txPayload.messageHex,
      signature: signed.signature,
      accountIndex: this.accountIndex,
      apiKeyIndex: this.apiKeyIndex,
      nonce,
      expiredAt: txPayload.expiredAt,
      orderDetails: {
        orderId: params.orderId,
        marketId: params.marketId,
        isBuy: params.isBuy,
        price: params.price,
        amount: params.amount,
        postOnly: params.postOnly,
        reduceOnly: params.reduceOnly,
        orderTypeValue: txPayload.orderTypeValue,
        timeInForceValue: txPayload.timeInForceValue,
        triggerPrice: txPayload.triggerPrice,
        orderExpiry: txPayload.orderExpiry,
        integratorAccountIndex: txPayload.integratorAccountIndex,
        integratorMakerFee: txPayload.integratorMakerFee,
        integratorTakerFee: txPayload.integratorTakerFee,
      },
    };
  }

  public async getCreateOrderTransactions(params: CreateOrderParams[]): Promise<L2CreateOrderTxInfo[]> {
    const txInfos: L2CreateOrderTxInfo[] = [];
    for (const orderParams of params) {
      const nonce = await this.getNextNonce();
      txInfos.push(this.buildCreateOrderTransaction(orderParams, nonce));
    }
    return txInfos;
  }

  public async getCreateGroupedOrdersTransaction(
    params: CreateGroupedOrdersParams,
  ): Promise<L2CreateGroupedOrdersTxInfo> {
    const nonce = await this.getNextNonce();
    const txPayload = this.buildCreateGroupedOrdersData(params, nonce);
    const signed = this.signCreateOrderTransaction(txPayload.messageHex, txPayload.l2AttributesMessageHex);

    return {
      type: 'create_grouped_orders',
      data: txPayload.messageHex,
      signature: signed.signature,
      accountIndex: this.accountIndex,
      apiKeyIndex: this.apiKeyIndex,
      nonce,
      expiredAt: txPayload.expiredAt,
      groupedOrderDetails: {
        groupingType: params.groupingType,
        orders: txPayload.orders,
        integratorAccountIndex: txPayload.integratorAccountIndex,
        integratorMakerFee: txPayload.integratorMakerFee,
        integratorTakerFee: txPayload.integratorTakerFee,
      },
    };
  }

  /**
   * Create and sign a cancel order transaction
   *
   * @param params - Cancel parameters
   * @returns Signed transaction info ready for submission
   *
   * @example
   * ```typescript
   * const cancelTx = await txClient.getCancelOrderTransaction({
   *   orderId: 123n,
   *   marketId: 1
   * });
   * ```
   */
  public async getCancelOrderTransaction(params: CancelOrderParams): Promise<L2CancelOrderTxInfo> {
    const nonce = await this.getNextNonce();
    const txPayload = this.buildCancelOrderData(params, nonce);
    const signed = this.signTransaction(txPayload.messageHex);

    return {
      type: 'cancel_order',
      data: txPayload.messageHex,
      signature: signed.signature,
      accountIndex: this.accountIndex,
      apiKeyIndex: this.apiKeyIndex,
      nonce,
      expiredAt: txPayload.expiredAt,
      cancelDetails: {
        orderId: params.orderId,
        marketId: params.marketId,
        expiredAt: txPayload.expiredAt,
      },
    };
  }

  /**
   * Create and sign a transfer transaction
   *
   * @param params - Transfer parameters
   * @returns Signed transaction info ready for submission
   *
   * @example
   * ```typescript
   * const transferTx = await txClient.getTransferTransaction({
   *   toAccountIndex: 2n,
   *   amount: 1000000n,
   *   tokenId: 0
   * });
   * ```
   */
  public async getTransferTransaction(params: TransferParams): Promise<L2TransferTxInfo> {
    const nonce = await this.getNextNonce();
    const data = this.buildTransferData(params, nonce);
    const signature = this.signTransaction(data);

    return {
      type: 'transfer',
      data,
      signature: signature.signature,
      accountIndex: this.accountIndex,
      apiKeyIndex: this.apiKeyIndex,
      nonce,
      transferDetails: {
        toAccountIndex: params.toAccountIndex,
        amount: params.amount,
        tokenId: params.tokenId ?? 0,
      },
    };
  }

  /**
   * Create and sign an update leverage transaction
   */
  public async getUpdateLeverageTransaction(params: UpdateLeverageParams): Promise<L2UpdateLeverageTxInfo> {
    const nonce = await this.getNextNonce();
    const txPayload = this.buildUpdateLeverageData(params, nonce);
    const signed = this.signTransaction(txPayload.messageHex);

    return {
      type: 'update_leverage',
      data: txPayload.messageHex,
      signature: signed.signature,
      accountIndex: this.accountIndex,
      apiKeyIndex: this.apiKeyIndex,
      nonce,
      expiredAt: txPayload.expiredAt,
      leverageDetails: {
        marketId: params.marketId,
        initialMarginFraction: txPayload.initialMarginFraction,
        marginMode: params.marginMode,
        expiredAt: txPayload.expiredAt,
      },
    };
  }

  /**
   * Create and sign an approve integrator transaction
   */
  public async getApproveIntegratorTransaction(params: ApproveIntegratorParams): Promise<L2ApproveIntegratorTxInfo> {
    const nonce = params.nonce ?? (await this.getNextNonce());
    const txPayload = this.buildApproveIntegratorData(params, nonce);
    const signed = this.signTransaction(txPayload.messageHex);

    return {
      type: 'approve_integrator',
      data: txPayload.messageHex,
      signature: signed.signature,
      accountIndex: this.accountIndex,
      apiKeyIndex: this.apiKeyIndex,
      nonce,
      expiredAt: txPayload.expiredAt,
      approveIntegratorDetails: {
        integratorAccountIndex: params.integratorAccountIndex,
        maxPerpsTakerFee: params.maxPerpsTakerFee,
        maxPerpsMakerFee: params.maxPerpsMakerFee,
        maxSpotTakerFee: params.maxSpotTakerFee,
        maxSpotMakerFee: params.maxSpotMakerFee,
        approvalExpiry: params.approvalExpiry,
        expiredAt: txPayload.expiredAt,
        l1Signature: params.l1Signature,
      },
    };
  }

  /**
   * Generate an authentication token for API requests
   *
   * @param deadline - Token expiration timestamp (unix timestamp in seconds)
   * @returns Signed authentication token (hex string)
   *
   * @example
   * ```typescript
   * const deadline = BigInt(Date.now() / 1000 + 3600); // 1 hour from now
   * const authToken = await txClient.getAuthToken(deadline);
   * ```
   */
  public async getAuthToken(deadline: bigint): Promise<string> {
    if (!this.crypto.isReady()) {
      throw new Error('Crypto module not ready. Please wait for initialization.');
    }

    const message = `${deadline.toString()}:${this.accountIndex.toString()}:${this.apiKeyIndex.toString()}`;
    const asciiHex = this.utf8ToHex(message);
    const paddedHex = asciiHex.padEnd(Math.ceil(asciiHex.length / 16) * 16, '0');
    const signed = this.signTransaction(paddedHex);
    return `${message}:${signed.signature}`;
  }

  /**
   * Reset the cached nonce (useful after errors or chain reorganizations)
   */
  public resetNonce(): void {
    this.cachedNonce = undefined;
  }

  /**
   * Get current account index
   */
  public getAccountIndex(): bigint {
    return this.accountIndex;
  }

  /**
   * Get current API key index
   */
  public getApiKeyIndex(): number {
    return this.apiKeyIndex;
  }

  /**
   * Get chain ID
   */
  public getChainId(): number {
    return this.chainId;
  }

  /**
   * Submit a transaction directly to the network
   *
   * @param params - Order, cancel, or transfer parameters
   * @returns Transaction hash
   *
   * @example
   * ```typescript
   * const txHash = await txClient.submitCreateOrder({
   *   orderId: 123n,
   *   marketId: 1,
   *   isBuy: true,
   *   price: 100000n,
   *   amount: 50000n
   * });
   * ```
   */
  public async submitCreateOrder(params: CreateOrderParams): Promise<string> {
    const txInfo = await this.getCreateOrderTransaction(params);
    return await this.httpClient.sendRawTx(txInfo);
  }

  public async submitCreateOrders(params: CreateOrderParams[]): Promise<string[]> {
    const txInfos = await this.getCreateOrderTransactions(params);
    const response = await this.httpClient.sendRawTxBatch(txInfos);
    return response.txHashes;
  }

  public async submitCreateGroupedOrders(params: CreateGroupedOrdersParams): Promise<string> {
    const txInfo = await this.getCreateGroupedOrdersTransaction(params);
    return await this.httpClient.sendRawTx(txInfo);
  }

  /**
   * Submit a cancel order transaction directly to the network
   *
   * @param params - Cancel parameters
   * @returns Transaction hash
   */
  public async submitCancelOrder(params: CancelOrderParams): Promise<string> {
    const txInfo = await this.getCancelOrderTransaction(params);
    return await this.httpClient.sendRawTx(txInfo);
  }

  /**
   * Submit a transfer transaction directly to the network
   *
   * @param params - Transfer parameters
   * @returns Transaction hash
   */
  public async submitTransfer(params: TransferParams): Promise<string> {
    const txInfo = await this.getTransferTransaction(params);
    return await this.httpClient.sendRawTx(txInfo);
  }

  /**
   * Submit an update leverage transaction directly to the network
   */
  public async submitUpdateLeverage(params: UpdateLeverageParams): Promise<string> {
    const txInfo = await this.getUpdateLeverageTransaction(params);
    return await this.httpClient.sendRawTx(txInfo);
  }

  /**
   * Submit an approve integrator transaction directly to the network
   */
  public async submitApproveIntegrator(params: ApproveIntegratorParams): Promise<string> {
    const txInfo = await this.getApproveIntegratorTransaction(params);
    return await this.httpClient.sendRawTx(txInfo);
  }
}
