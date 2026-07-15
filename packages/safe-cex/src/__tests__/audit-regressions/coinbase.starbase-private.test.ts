import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
} from '../../types';
import { CoinbaseStarbaseProtocolError } from '../../exchanges/coinbase/coinbase.starbase';
import {
  assessCoinbaseStarbaseBalanceParity,
  assessCoinbaseStarbasePositionParity,
  buildCoinbaseStarbaseAccountSummary,
  buildCoinbaseStarbaseOpenOrders,
  buildCoinbaseStarbasePositions,
  buildCoinbaseStarbaseUserTrades,
  buildCoinbaseStarbaseUserTradesByTime,
  CoinbaseStarbaseReadOnlyState,
  decodeCoinbaseStarbaseAccountSummary,
  decodeCoinbaseStarbaseOrder,
  decodeCoinbaseStarbasePosition,
  decodeCoinbaseStarbasePositions,
  decodeCoinbaseStarbaseTrade,
  decodeCoinbaseStarbaseTradePage,
  normalizeCoinbaseStarbaseOrder,
  normalizeCoinbaseStarbaseTrade,
} from '../../exchanges/coinbase/coinbase.starbase-private';
import {
  STARBASE_ACCOUNT_SUMMARY_FIXTURE,
  STARBASE_OPEN_ORDER_FIXTURE,
  STARBASE_PARTIAL_ORDER_FIXTURE,
  STARBASE_POSITION_FIXTURE,
  STARBASE_USER_TRADE_FIXTURE,
} from './coinbase.starbase-private.fixtures';

describe('Coinbase Starbase inert read-only private state', () => {
  it('builds exact published USDC-perpetual read requests without write methods', () => {
    expect(buildCoinbaseStarbaseAccountSummary()).toEqual({
      method: 'private/get_account_summary',
      params: { currency: 'USDC', extended: false },
    });
    expect(buildCoinbaseStarbasePositions()).toEqual({
      method: 'private/get_positions',
      params: { currency: 'USDC', kind: 'future' },
    });
    expect(buildCoinbaseStarbaseOpenOrders()).toEqual({
      method: 'private/get_open_orders_by_currency',
      params: { currency: 'USDC', kind: 'future', type: 'all' },
    });
    expect(buildCoinbaseStarbaseUserTrades(250)).toEqual({
      method: 'private/get_user_trades_by_currency',
      params: { count: 250, currency: 'USDC', historical: false, kind: 'future', sorting: 'desc' },
    });
    expect(buildCoinbaseStarbaseUserTradesByTime(1_800_000_000_000, 1_800_000_060_000, 50)).toEqual({
      method: 'private/get_user_trades_by_currency_and_time',
      params: {
        count: 50,
        currency: 'USDC',
        end_timestamp: 1_800_000_060_000,
        historical: false,
        kind: 'future',
        sorting: 'asc',
        start_timestamp: 1_800_000_000_000,
      },
    });

    expect(() => buildCoinbaseStarbaseUserTrades(0)).toThrow('1 to 1000');
    expect(() => buildCoinbaseStarbaseUserTradesByTime(2, 1)).toThrow('precedes');
  });

  it('names account balance and free margin in requested USDC while gating unpublished PnL units', () => {
    const account = decodeCoinbaseStarbaseAccountSummary(STARBASE_ACCOUNT_SUMMARY_FIXTURE);
    expect(account).toEqual({
      availableFundsUsdc: 38.32,
      availableWithdrawalFundsUsdc: 36.11,
      balanceUsdc: 50,
      crossCollateralEnabled: false,
      currency: 'USDC',
      equityUsdc: 51.25,
      gatewayFuturesPnl: 1.25,
      gatewayFuturesSessionRealizedPnl: 0.4,
      gatewayFuturesSessionUnrealizedPnl: 0.85,
      gatewaySessionRealizedPnl: 0.4,
      gatewaySessionUnrealizedPnl: 0.85,
      gatewayTotalPnl: 1.25,
      initialMarginUsdc: 12.68,
      maintenanceMarginUsdc: 8.1,
      marginBalanceUsdc: 51.25,
      marginModel: 'segregated_sm',
      portfolioMarginingEnabled: false,
    });
    expect(assessCoinbaseStarbaseBalanceParity(account)).toEqual({
      status: 'blocked',
      gateway: account,
      blockers: ['account-pnl-unit-unpublished'],
    });
  });

  it('fails closed on non-USDC summaries, unknown margin models, and malformed free margin', () => {
    expect(() =>
      decodeCoinbaseStarbaseAccountSummary({ ...STARBASE_ACCOUNT_SUMMARY_FIXTURE, currency: 'BTC' }),
    ).toThrow('currency');
    expect(() =>
      decodeCoinbaseStarbaseAccountSummary({ ...STARBASE_ACCOUNT_SUMMARY_FIXTURE, margin_model: 'portfolio' }),
    ).toThrow('unknown enum');
    expect(() =>
      decodeCoinbaseStarbaseAccountSummary({ ...STARBASE_ACCOUNT_SUMMARY_FIXTURE, available_funds: '38.32' }),
    ).toThrow('available_funds');
  });

  it('preserves legacy symbol identity and separately proves base and USD position sizes', () => {
    const position = decodeCoinbaseStarbasePosition(STARBASE_POSITION_FIXTURE);
    expect(position).toEqual({
      baseAmount: 0.1,
      currentNotionalUsd: 5_040,
      direction: 'buy',
      entryPriceUsd: 50_000,
      gatewayFloatingPnl: 40,
      gatewayInitialMargin: 500,
      gatewayMaintenanceMargin: 250,
      gatewayOpenOrdersMargin: 25,
      gatewayRealizedPnl: 10,
      gatewaySizeUsd: 5_000,
      gatewayTotalPnl: 50,
      indexPriceUsd: 50_390,
      instrumentName: 'BTC_USDC-PERPETUAL',
      legacySavedSymbol: 'BTC-PERP-INTX',
      leverage: 10,
      liquidationPriceUsd: 41_000,
      markPriceUsd: 50_400,
    });
    expect(assessCoinbaseStarbasePositionParity(position)).toEqual({
      status: 'blocked',
      gateway: position,
      blockers: ['position-margin-pnl-units-unpublished'],
    });
  });

  it('rejects position enum/shape drift and duplicate instrument snapshots atomically', () => {
    expect(() => decodeCoinbaseStarbasePosition({ ...STARBASE_POSITION_FIXTURE, direction: 'long' })).toThrow(
      'unknown enum',
    );
    expect(() => decodeCoinbaseStarbasePosition({ ...STARBASE_POSITION_FIXTURE, kind: 'option' })).toThrow('kind');
    expect(() =>
      decodeCoinbaseStarbasePosition({ ...STARBASE_POSITION_FIXTURE, instrument_name: 'BTC-PERPETUAL' }),
    ).toThrow('perpetual instrument name');
    expect(() => decodeCoinbaseStarbasePositions([STARBASE_POSITION_FIXTURE, STARBASE_POSITION_FIXTURE])).toThrow(
      'Duplicate',
    );
  });

  it('retains gateway amount, contract, label, and filled fields before any unit conversion', () => {
    expect(decodeCoinbaseStarbaseOrder(STARBASE_OPEN_ORDER_FIXTURE)).toEqual({
      averagePrice: 0,
      creationTimestampMs: 1_800_000_000_000,
      direction: 'buy',
      gatewayAmount: 0.1,
      gatewayContracts: 100,
      gatewayFilledAmount: 0,
      id: 'USDC-ORDER-1',
      instrumentName: 'BTC_USDC-PERPETUAL',
      label: 'saved-client-label',
      lastUpdateTimestampMs: 1_800_000_000_100,
      legacySavedSymbol: 'BTC-PERP-INTX',
      postOnly: false,
      price: 50_000,
      reduceOnly: false,
      state: 'open',
      timeInForce: 'good_til_cancelled',
      type: 'limit',
    });
  });

  it('normalizes only an unfilled base-amount order and never treats label as canonical identity', () => {
    const gateway = decodeCoinbaseStarbaseOrder(STARBASE_OPEN_ORDER_FIXTURE);
    expect(normalizeCoinbaseStarbaseOrder(gateway)).toEqual({
      status: 'ready',
      value: {
        amount: 0.1,
        filled: 0,
        id: 'USDC-ORDER-1',
        info: gateway,
        lastUpdate: 1_800_000_000_100,
        price: 50_000,
        reduceOnly: false,
        remaining: 0.1,
        side: OrderSide.Buy,
        status: OrderStatus.Open,
        symbol: 'BTC-PERP-INTX',
        timeInForce: OrderTimeInForce.GoodTillCancel,
        type: OrderType.Limit,
      },
    });
  });

  it('keeps partially filled orders decodable but gates their contradictory filled-size unit', () => {
    const gateway = decodeCoinbaseStarbaseOrder(STARBASE_PARTIAL_ORDER_FIXTURE);
    expect(gateway.gatewayAmount).toBe(0.1);
    expect(gateway.gatewayFilledAmount).toBe(50);
    expect(normalizeCoinbaseStarbaseOrder(gateway)).toEqual({
      status: 'blocked',
      gateway,
      blockers: ['filled-amount-unit-conflict'],
    });

    const stopLimit = decodeCoinbaseStarbaseOrder({
      ...STARBASE_OPEN_ORDER_FIXTURE,
      order_type: 'stop_limit',
      trigger: 'mark_price',
      trigger_price: 45_000,
    });
    expect(normalizeCoinbaseStarbaseOrder(stopLimit)).toMatchObject({
      status: 'blocked',
      blockers: ['stop-limit-shape-unsupported'],
    });
  });

  it('strictly decodes fills while gating the inherited OpenAPI amount-unit conflict', () => {
    const trade = decodeCoinbaseStarbaseTrade(STARBASE_USER_TRADE_FIXTURE);
    expect(trade).toEqual({
      direction: 'buy',
      fee: 1.25,
      feeCurrency: 'USDC',
      gatewayAmount: 0.05,
      gatewayContracts: 50,
      gatewayProfitLoss: 20,
      id: 'USDC-TRADE-1',
      indexPriceUsd: 50_390,
      instrumentName: 'BTC_USDC-PERPETUAL',
      label: 'saved-client-label',
      legacySavedSymbol: 'BTC-PERP-INTX',
      liquidity: 'T',
      markPriceUsd: 50_400,
      orderId: 'USDC-ORDER-2',
      priceUsd: 50_400,
      sequence: 42,
      state: 'filled',
      timestampMs: 1_800_000_000_200,
      type: 'limit',
    });
    expect(normalizeCoinbaseStarbaseTrade(trade)).toEqual({
      status: 'blocked',
      gateway: trade,
      blockers: ['trade-amount-unit-conflict'],
    });
    expect(decodeCoinbaseStarbaseTradePage({ has_more: false, trades: [STARBASE_USER_TRADE_FIXTURE] })).toEqual({
      hasMore: false,
      trades: [trade],
    });
    expect(() => decodeCoinbaseStarbaseTrade({ ...STARBASE_USER_TRADE_FIXTURE, matching_id: 'match-1' })).toThrow(
      'matching_id',
    );
  });

  it('preserves last-known-good read state when a later snapshot is malformed', () => {
    const state = new CoinbaseStarbaseReadOnlyState();
    state.applyOpenOrders([STARBASE_OPEN_ORDER_FIXTURE]);
    state.applyPositions([STARBASE_POSITION_FIXTURE]);
    state.applyAccountSummary(STARBASE_ACCOUNT_SUMMARY_FIXTURE);
    state.applyTradePage({ has_more: false, trades: [STARBASE_USER_TRADE_FIXTURE] });
    const before = JSON.stringify({
      account: state.account,
      orders: state.orders,
      positions: state.positions,
      tradePage: state.tradePage,
    });

    expect(() =>
      state.applyOpenOrders([STARBASE_OPEN_ORDER_FIXTURE, { ...STARBASE_OPEN_ORDER_FIXTURE }]),
    ).toThrow('Duplicate');
    expect(() => state.applyPositions([{ ...STARBASE_POSITION_FIXTURE, size_currency: '0.1' }])).toThrow(
      CoinbaseStarbaseProtocolError,
    );
    expect(() =>
      state.applyAccountSummary({ ...STARBASE_ACCOUNT_SUMMARY_FIXTURE, margin_model: 'portfolio' }),
    ).toThrow('unknown enum');
    expect(() =>
      state.applyTradePage({
        has_more: false,
        trades: [STARBASE_USER_TRADE_FIXTURE, { ...STARBASE_USER_TRADE_FIXTURE }],
      }),
    ).toThrow('Duplicate');
    expect(
      JSON.stringify({
        account: state.account,
        orders: state.orders,
        positions: state.positions,
        tradePage: state.tradePage,
      }),
    ).toBe(before);

    expect(state.applyOpenOrders([])).toEqual([]);
  });

  it('remains absent from active Coinbase runtime, routing, and metadata imports', () => {
    const coinbaseDirectory = resolve(process.cwd(), 'src/exchanges/coinbase');
    const activeFiles = [
      'coinbase.api.ts',
      'coinbase.exchange.ts',
      'coinbase.types.ts',
      'coinbase.utils.ts',
      'coinbase.ws-private.ts',
      'coinbase.ws-public.ts',
      'coinbasespot.exchange.ts',
    ];
    for (const file of activeFiles) {
      const source = readFileSync(resolve(coinbaseDirectory, file), 'utf8');
      expect(source).not.toContain('coinbase.starbase-private');
    }
  });
});
