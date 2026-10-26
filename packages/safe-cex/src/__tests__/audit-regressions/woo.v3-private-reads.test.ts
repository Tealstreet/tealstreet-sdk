import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import {
  decodeWooV3AccountInfo,
  decodeWooV3AssetBalances,
  decodeWooV3FuturesPositions,
  decodeWooV3PendingAlgoOrders,
  decodeWooV3PendingOrders,
  decodeWooV3Transactions,
  mapWooV3AccountBalance,
  mapWooV3AssetBalances,
  mapWooV3FuturesPositions,
  mapWooV3PendingAlgoOrders,
  mapWooV3PendingOrders,
  mapWooV3Transactions,
} from '../reference/woo/woo.v3.private';
import {
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const envelope = <T>(data: T) => ({ success: true, timestamp: 1_725_000_000_000, data });

const accountInfoFixture = envelope({
  applicationId: 'safe-cex',
  account: 'account-id',
  alias: 'Main account',
  otpauth: true,
  accountMode: 'MARGIN',
  positionMode: 'ONE_WAY',
  leverage: 10,
  marginRatio: '0.125',
  openMarginRatio: '0.1',
  initialMarginRatio: '0.08',
  maintenanceMarginRatio: '0.04',
  totalCollateral: '1200.75',
  freeCollateral: '875.25',
  totalAccountValue: '1300.5',
  totalTradingValue: '1000.5',
  totalVaultValue: '100',
  totalStakingValue: '80',
  totalEarnValue: '70',
  totalLaunchpadValue: '50',
  referrerID: 'WOO2023',
  accountType: 'Main',
});

const assetBalancesFixture = envelope({
  holding: [
    {
      token: 'USDT',
      holding: '1000.5',
      frozen: '25.25',
      staked: '10',
      unbonding: '2',
      vault: '100',
      interest: '-0.5',
      earn: '7.5',
      pendingShortQty: '0.15',
      pendingLongQty: '0.25',
      availableBalance: '875.25',
      averageOpenPrice: '1',
      markPrice: '1',
      pnl24H: '-3.5',
      fee24H: '1.25',
      timestamp: 1_696_663_746_259,
    },
  ],
});

const positionsFixture = envelope({
  // WOO's response example uses `positions`; its OpenAPI schema says `rows`.
  positions: [
    {
      symbol: 'PERP_BTC_USDT',
      holding: '-1.0234',
      positionSide: 'BOTH',
      pendingLongQty: '0.1',
      pendingShortQty: '0.2',
      settlePrice: '27903.2',
      averageOpenPrice: '28000.5',
      pnl24H: '-10.25',
      fee24H: '1.5',
      markPrice: '27903.2',
      estLiqPrice: '35000',
      adlQuantile: 1,
      timestamp: 1_696_663_260_695,
    },
  ],
});

const regularOrdersFixture = envelope({
  rows: [
    {
      symbol: 'PERP_BTC_USDT',
      status: 'PARTIAL_FILLED',
      side: 'SELL',
      positionSide: 'BOTH',
      createdTime: 1_578_565_539_808,
      orderId: 135,
      orderTag: 'default',
      price: '62123.5',
      type: 'POST_ONLY',
      quantity: '0.1000',
      amount: null,
      visible: '0.1000',
      executed: '0.025',
      totalFee: '0.00123',
      feeAsset: 'USDT',
      totalRebate: 0,
      rebateCurrency: 'USDT',
      clientOrderId: 0,
      reduceOnly: true,
      realizedPnl: '2',
      averageExecutedPrice: '62100',
      leverage: 10,
      marginMode: 'CROSSED',
      bidAskLevel: 2,
    },
    {
      symbol: 'PERP_ETH_USDT',
      status: 'NEW',
      side: 'BUY',
      positionSide: 'BOTH',
      createdTime: 1_578_565_539_900,
      orderId: 136,
      orderTag: 'default',
      price: '3200',
      type: 'IOC',
      quantity: '2',
      amount: 'null',
      visible: '2',
      executed: '0',
      totalFee: '0',
      feeAsset: '',
      totalRebate: 0,
      rebateCurrency: '',
      clientOrderId: 42,
      reduceOnly: false,
      averageExecutedPrice: '0',
    },
  ],
  meta: { total: 2, recordsPerPage: 25, currentPage: 1 },
});

const algoLeaf = (overrides: Record<string, unknown> = {}) => ({
  algoOrderId: 792409,
  clientAlgoOrderId: 0,
  rootAlgoOrderId: 792407,
  parentAlgoOrderId: 792408,
  symbol: 'PERP_BTC_USDT',
  algoOrderTag: 'default',
  algoType: 'TAKE_PROFIT',
  side: 'SELL',
  positionSide: 'BOTH',
  quantity: '0.1',
  isTriggered: false,
  triggerPrice: '63000.5',
  triggerStatus: 'USELESS',
  type: 'MARKET',
  rootAlgoStatus: 'NEW',
  algoStatus: 'PARTIAL_FILLED',
  triggerPriceType: 'INDEX_PRICE',
  triggerTime: '0',
  totalExecutedQuantity: '0.025',
  visibleQuantity: '0.1',
  averageExecutedPrice: '0',
  totalFee: '0',
  feeAsset: '',
  totalRebate: 0,
  rebateCurrency: 'USDT',
  reduceOnly: true,
  createdTime: 1_696_589_756_118,
  updatedTime: 1_696_589_756_123,
  isActivated: false,
  ...overrides,
});

const algoOrdersFixture = envelope({
  rows: [
    {
      ...algoLeaf({
        algoOrderId: 792407,
        rootAlgoOrderId: 792407,
        parentAlgoOrderId: 0,
        algoType: 'BRACKET',
        quantity: '0.1',
        triggerStatus: 'SUCCESS',
        algoStatus: 'NEW',
        childOrders: [
          algoLeaf(),
          algoLeaf({
            algoOrderId: 792410,
            algoType: 'STOP_LOSS',
            triggerPrice: '59000',
            triggerPriceType: 'MARKET_PRICE',
            totalExecutedQuantity: '0',
          }),
        ],
      }),
    },
    algoLeaf({
      algoOrderId: 792419,
      rootAlgoOrderId: 792419,
      parentAlgoOrderId: 0,
      algoType: 'TRAILING_STOP',
      quantity: 1,
      triggerPrice: undefined,
      triggerPriceType: 'LAST_PRICE',
      callbackRate: '0.03',
      activatedPrice: '62000',
      totalExecutedQuantity: '0',
      algoStatus: 'NEW',
    }),
  ],
  meta: { total: 2, recordsPerPage: 25, currentPage: 1 },
});

const transactionsFixture = envelope({
  rows: [
    {
      id: 9001,
      symbol: 'PERP_BTC_USDT',
      orderId: 135,
      orderTag: 'default',
      executedPrice: '62100.25',
      executedQuantity: '0.025',
      isMaker: 1,
      isMatchRpi: false,
      side: 'SELL',
      fee: 0.1234,
      feeAsset: 'USDT',
      realizedPnl: '-2.5',
      executedTimestamp: 1_725_000_123_456,
    },
  ],
  meta: { total: 1, recordsPerPage: 25, currentPage: 1 },
});

describe('WooX namespaced V3 authenticated read foundation', () => {
  it('maps official account collateral fields to the existing safe-cex balance semantics', () => {
    const info = decodeWooV3AccountInfo(accountInfoFixture);
    const balance = mapWooV3AccountBalance(info, -12.25);

    expect(info).toMatchObject({
      accountMode: 'MARGIN',
      positionMode: 'ONE_WAY',
      leverage: 10,
      totalCollateral: '1200.75',
      freeCollateral: '875.25',
      totalAccountValue: '1300.5',
    });
    expect(balance).toMatchObject({ free: 875.25, used: 325.5, total: 1300.5, upnl: -12.25 });
    expect(() => mapWooV3AccountBalance({ ...info, accountMode: 'SPOT' }, 0)).toThrow(/not futures-capable/);
    expect(() =>
      decodeWooV3AccountInfo({ ...accountInfoFixture, data: { ...accountInfoFixture.data, freeCollateral: 875.25 } }),
    ).toThrow(/finite decimal string/);
  });

  it('keeps every asset bucket distinct instead of treating vault, stake, earn, or interest as free collateral', () => {
    const rows = decodeWooV3AssetBalances(assetBalancesFixture);
    const mapped = mapWooV3AssetBalances(rows);

    expect(mapped).toEqual([
      {
        token: 'USDT',
        tradingTotal: 1000.5,
        tradingFree: 875.25,
        tradingFrozen: 25.25,
        staked: 10,
        unbonding: 2,
        vault: 100,
        interest: -0.5,
        earn: 7.5,
        pendingShortQuantity: 0.15,
        pendingLongQuantity: 0.25,
        averageOpenPrice: 1,
        markPrice: 1,
        pnl24H: -3.5,
        fee24H: 1.25,
        timestamp: 1_696_663_746_259,
      },
    ]);
    expect(() =>
      decodeWooV3AssetBalances({
        ...assetBalancesFixture,
        data: { holding: [{ ...assetBalancesFixture.data.holding[0], availableBalance: 875.25 }] },
      }),
    ).toThrow(/finite decimal string/);
  });

  it('normalizes one-way and hedge position sides but defers leverage and margin mode joins', () => {
    const decoded = decodeWooV3FuturesPositions(positionsFixture);
    const mapped = mapWooV3FuturesPositions(decoded, 'ONE_WAY');

    expect(mapped).toEqual([
      {
        symbol: 'BTCUSDT',
        side: PositionSide.Short,
        positionMode: PositionMode.OneWay,
        gatewayPositionSide: 'BOTH',
        signedHolding: -1.0234,
        contracts: 1.0234,
        entryPrice: 28000.5,
        markPrice: 27903.2,
        notional: 1.0234 * 27903.2,
        estimatedLiquidationPrice: 35000,
        pendingLongQuantity: 0.1,
        pendingShortQuantity: 0.2,
        pnl24H: -10.25,
        fee24H: 1.5,
        adlQuantile: 1,
        timestamp: 1_696_663_260_695,
      },
    ]);
    expect(mapped[0]).not.toHaveProperty('leverage');
    expect(mapped[0]).not.toHaveProperty('marginType');

    const hedged = decodeWooV3FuturesPositions(
      envelope({ rows: [{ ...positionsFixture.data.positions[0], holding: '1', positionSide: 'LONG' }] }),
    );
    expect(mapWooV3FuturesPositions(hedged, 'HEDGE_MODE')[0]).toMatchObject({
      side: PositionSide.Long,
      positionMode: PositionMode.Hedged,
    });
    expect(() => mapWooV3FuturesPositions(hedged, 'ONE_WAY')).toThrow(/incompatible/);
    expect(() =>
      decodeWooV3FuturesPositions(
        envelope({ rows: positionsFixture.data.positions, positions: positionsFixture.data.positions }),
      ),
    ).toThrow(/exactly one/);
  });

  it('maps pending regular order units, IDs, timestamps, TIF, reduce-only, filled and remaining exactly', () => {
    const decoded = decodeWooV3PendingOrders(regularOrdersFixture);
    const mapped = mapWooV3PendingOrders(decoded.rows, 'ONE_WAY');

    expect(decoded.meta).toEqual({ total: 2, recordsPerPage: 25, currentPage: 1 });
    expect(mapped).toEqual([
      expect.objectContaining({
        id: '135',
        status: OrderStatus.Open,
        symbol: 'BTCUSDT',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        price: 62123.5,
        amount: 0.1,
        filled: 0.025,
        remaining: 0.075,
        reduceOnly: true,
        timeInForce: OrderTimeInForce.PostOnly,
        lastUpdate: 1_578_565_539_808,
      }),
      expect.objectContaining({
        id: '136',
        symbol: 'ETHUSDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
        remaining: 2,
      }),
    ]);
  });

  it('fails the complete pending-order page on unknown enums, non-decimal units, impossible modes, or overfills', () => {
    const first = regularOrdersFixture.data.rows[0];
    expect(() =>
      decodeWooV3PendingOrders(envelope({ rows: [{ ...first, type: 'PEGGED' }], meta: regularOrdersFixture.data.meta })),
    ).toThrow(/must be one of/);
    expect(() =>
      decodeWooV3PendingOrders(envelope({ rows: [{ ...first, quantity: 0.1 }], meta: regularOrdersFixture.data.meta })),
    ).toThrow(/finite decimal string/);
    const longSide = decodeWooV3PendingOrders(
      envelope({ rows: [{ ...first, positionSide: 'LONG' }], meta: regularOrdersFixture.data.meta }),
    );
    expect(() => mapWooV3PendingOrders(longSide.rows, 'ONE_WAY')).toThrow(/incompatible/);
    const overfilled = decodeWooV3PendingOrders(
      envelope({ rows: [{ ...first, executed: '0.2' }], meta: regularOrdersFixture.data.meta }),
    );
    expect(() => mapWooV3PendingOrders(overfilled.rows, 'ONE_WAY')).toThrow(/must not exceed/);
  });

  it('maps only lossless TP/SL algo leaves and explicitly defers WOO schema ambiguities', () => {
    const decoded = decodeWooV3PendingAlgoOrders(algoOrdersFixture);
    const mapped = mapWooV3PendingAlgoOrders(decoded.rows, 'ONE_WAY');

    expect(mapped.orders).toEqual([
      expect.objectContaining({
        id: '792409',
        parentId: '792407',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        price: 63000.5,
        amount: 0.1,
        filled: 0.025,
        remaining: 0.075,
        reduceOnly: true,
        tpTriggerType: OrderStopTriggerType.IndexPrice,
        lastUpdate: 1_696_589_756_123,
      }),
      expect.objectContaining({
        id: '792410',
        parentId: '792407',
        type: OrderType.StopLoss,
        price: 59000,
        slTriggerType: OrderStopTriggerType.LastPrice,
      }),
    ]);
    expect(mapped.deferred).toHaveLength(1);
    expect(mapped.deferred[0]).toMatchObject({
      order: { algoOrderId: 792419, algoType: 'TRAILING_STOP', quantity: 1 },
    });
    expect(mapped.deferred[0].reason).toContain('no lossless absolute-trigger');
  });

  it('rejects unknown algo enums and defers the official number/string quantity drift without guessing', () => {
    const trailing = algoOrdersFixture.data.rows[1];
    expect(() =>
      decodeWooV3PendingAlgoOrders(
        envelope({ rows: [{ ...trailing, triggerPriceType: 'MARK_PRICE' }], meta: algoOrdersFixture.data.meta }),
      ),
    ).toThrow(/must be one of/);
    const numericTakeProfit = decodeWooV3PendingAlgoOrders(
      envelope({ rows: [algoLeaf({ quantity: 1 })], meta: { total: 1, recordsPerPage: 25, currentPage: 1 } }),
    );
    const result = mapWooV3PendingAlgoOrders(numericTakeProfit.rows, 'ONE_WAY');
    expect(result.orders).toEqual([]);
    expect(result.deferred[0].reason).toContain('number/string drift');
  });

  it('maps official transaction price/quantity strings, millisecond timestamps, IDs, sides, fees, and maker data', () => {
    const decoded = decodeWooV3Transactions(transactionsFixture);
    const mapped = mapWooV3Transactions(decoded.rows, 'ONE_WAY');

    expect(mapped).toEqual([
      {
        execution: {
          id: '9001',
          timestamp: 1_725_000_123,
          symbol: 'BTCUSDT',
          orderId: '135',
          price: 62100.25,
          qty: 0.025,
          side: OrderSide.Sell,
          commission: 0.1234,
          commissionAsset: 'USDT',
        },
        historicalTrade: {
          id: '9001',
          symbol: 'BTCUSDT',
          orderId: '135',
          price: 62100.25,
          qty: 0.025,
          realizedPnl: -2.5,
          positionSide: 'both',
          side: OrderSide.Sell,
          maker: true,
          commission: 0.1234,
          commissionAsset: 'USDT',
          time: 1_725_000_123_456,
        },
        maker: true,
        isMatchRpi: false,
      },
    ]);
    const hedgeMapped = mapWooV3Transactions(decoded.rows, 'HEDGE_MODE');
    expect(hedgeMapped[0].execution).toEqual(mapped[0].execution);
    expect(hedgeMapped[0]).not.toHaveProperty('historicalTrade');
    expect(hedgeMapped[0].deferredHistoricalTradeReason).toContain('omits positionSide');
    const transaction = transactionsFixture.data.rows[0];
    expect(() =>
      decodeWooV3Transactions(
        envelope({ rows: [{ ...transaction, fee: '0.1234' }], meta: transactionsFixture.data.meta }),
      ),
    ).toThrow(/finite number/);
    expect(() =>
      decodeWooV3Transactions(
        envelope({ rows: [{ ...transaction, executedPrice: 62100.25 }], meta: transactionsFixture.data.meta }),
      ),
    ).toThrow(/finite decimal string/);
  });

  it('remains disconnected from live transport and keeps every migration gate hard false', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);

    const wooDirectory = join(__dirname, '../../exchanges/woo');
    for (const runtimeFile of ['woo.exchange.ts', 'woo.api.ts', 'woo.ws-public.ts', 'woo.ws-private.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.private');
    }

    const privateSource = readFileSync(join(wooDirectory, 'woo.v3.private.ts'), 'utf8');
    expect(privateSource).not.toMatch(/\b(axios|fetch|WebSocket|createAPI|apiKey|apiSecret)\b/);
    expect(privateSource).toContain('retain the previous');
    expect(privateSource).toContain('known-good slice');
  });
});
