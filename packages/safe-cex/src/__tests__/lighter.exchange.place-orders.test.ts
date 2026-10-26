import type { ExchangeOptions } from '../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
  LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
} from '../exchanges/lighter/lighter.builder-fee';
import { LighterExchange } from './reference/lighter/lighter.exchange';
import { DefaultStore } from '../store/store.base';
import { MarginType, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../types';

const { cancelOrderMock, createOrderMock, createOrdersMock } = vi.hoisted(() => ({
  cancelOrderMock: vi.fn(),
  createOrderMock: vi.fn(),
  createOrdersMock: vi.fn(),
}));

vi.mock('@tealstreet/poseidon-crypto', () => ({
  initWasm: vi.fn(),
  isWasmReady: vi.fn(() => true),
  getCryptoApi: vi.fn(() => ({})),
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {},
  OrderApi: class {},
  CandlestickApi: class {},
}));

vi.mock('./reference/lighter/lighter.api', () => ({
  createLighterSDKClient: vi.fn(() => ({
    get: vi.fn(),
    sendRawTx: vi.fn(),
    sendRawTxBatch: vi.fn(),
  })),
}));

vi.mock('./reference/lighter/lighter.ws-public', () => ({
  LighterPublicWebsocket: class {
    connectAndSubscribe = vi.fn();
    listenOHLCV = vi.fn();
    listenOrderBook = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock('./reference/lighter/lighter.ws-private', () => ({
  LighterPrivateWebsocket: class {
    connectAndSubscribe = vi.fn();
    throttleFetchOrders = vi.fn();
    dispose = vi.fn();
  },
}));

describe('LighterExchange batched placeOrders', () => {
  const createExchange = (): LighterExchange => {
    const options = {
      key: 'lighter-key',
      secret: 'lighter-secret',
      apiKeyIndex: '1',
      applicationId: '123',
      testnet: false,
    } as ExchangeOptions;

    const exchange = new LighterExchange('lighter', options, new DefaultStore());
    exchange.store.update({
      markets: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: {
            amount: { min: 0.001, max: 100 },
            leverage: { min: 1, max: 50 },
          },
          isExpiring: false,
          info: {
            amountDecimals: 3,
            priceDecimals: 1,
            min_quote_amount: '1',
          },
        },
        {
          id: '2',
          symbol: 'ETH/USDT',
          base: 'ETH',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: {
            amount: { min: 0.001, max: 100 },
            leverage: { min: 1, max: 50 },
          },
          isExpiring: false,
          info: {
            amountDecimals: 3,
            priceDecimals: 1,
            min_quote_amount: '1',
          },
        },
      ],
    });

    (exchange as any).ensureSignerAdapter = vi.fn().mockResolvedValue({
      cancelOrder: cancelOrderMock,
      createOrder: createOrderMock,
      createOrders: createOrdersMock,
    });
    return exchange;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    cancelOrderMock.mockResolvedValue('cancel-tx-hash');
    createOrderMock.mockResolvedValue('single-tx-hash');
    createOrdersMock.mockResolvedValue(['batch-tx-1', 'batch-tx-2']);
  });

  it('mentions Plus/Premium only for cached Standard account rate limits', () => {
    const exchange = createExchange();
    (exchange as any).accountLimits = { user_tier_name: 'standard' };

    const message = exchange.getAccountRateLimitGuidanceMessage();

    expect(message).toContain('Standard account rate limit');
    expect(message).toContain('Plus or Premium');
  });

  it('does not suggest Plus/Premium when cached account tier is already Premium or Plus', () => {
    const premiumExchange = createExchange();
    (premiumExchange as any).accountLimits = { user_tier_name: 'premium' };
    const plusExchange = createExchange();
    (plusExchange as any).accountLimits = { data: { userTierName: 'plus' } };

    expect(premiumExchange.getAccountRateLimitGuidanceMessage()).toBe(
      'Lighter rate limit temporarily active for this account/session. Please wait before retrying.',
    );
    expect(plusExchange.getAccountRateLimitGuidanceMessage()).toBe(
      'Lighter rate limit temporarily active for this account/session. Please wait before retrying.',
    );
  });

  it('keeps neutral wording when cached account tier is unknown', () => {
    const exchange = createExchange();

    const message = exchange.getAccountRateLimitGuidanceMessage();

    expect(message).toContain('account/session');
    expect(message).toContain('check your Lighter account tier and connectivity route');
    expect(message).not.toContain('Plus or Premium');
  });

  it('uses live bid/ask as market-order fallback before mark price is loaded', async () => {
    const exchange = createExchange();
    exchange.store.update({
      tickers: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 0,
          index: 0,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });

    await exchange.placeOrder({
      symbol: 'BTC/USDT',
      type: OrderType.Market,
      side: OrderSide.Sell,
      amount: 1,
    });

    expect(createOrderMock).toHaveBeenCalledWith(expect.objectContaining({ price: 970n }));
  });

  it('applies configured max slippage to conditional order execution price', async () => {
    const exchange = createExchange();

    await exchange.placeOrder({
      symbol: 'BTC/USDT',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 1,
      price: 100,
      maxSlippagePct: 5,
    });

    expect(createOrderMock).toHaveBeenCalledWith(
      expect.objectContaining({
        orderType: 'stop_loss',
        price: 950n,
        triggerPrice: 1000n,
      }),
    );
  });

  it('refreshes stale Lighter margin state after invalid margin mode order rejection', async () => {
    const exchange = createExchange();
    exchange.store.update({
      markets: [
        {
          id: '182',
          symbol: 'H100',
          base: 'H100',
          quote: 'USDC',
          active: true,
          precision: { amount: 0.01, price: 0.0001 },
          limits: {
            amount: { min: 2, max: 5000000 },
            leverage: { min: 1, max: 10 },
          },
          isExpiring: false,
          info: {
            amountDecimals: 2,
            priceDecimals: 4,
            min_quote_amount: '10',
            market_config: { market_margin_mode: 1 },
          },
        },
      ],
      tickers: [
        {
          id: '182',
          symbol: 'H100',
          bid: 2.62,
          ask: 2.63,
          last: 2.625,
          mark: 2.624,
          index: 2.637,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });
    const refreshedPositions = [
      {
        id: 'H100:long:one-way',
        symbol: 'H100',
        side: PositionSide.Long,
        entryPrice: 0,
        notional: 0,
        unrealizedPnl: 0,
        contracts: 0,
        leverage: 10,
        liquidationPrice: 0,
        positionMode: PositionMode.OneWay,
        marginType: MarginType.Cross,
      },
    ];
    vi.spyOn(exchange, 'fetchPositions').mockResolvedValue(refreshedPositions);
    createOrderMock.mockRejectedValueOnce(new Error('invalid margin mode'));

    await expect(
      exchange.placeOrder({
        symbol: 'H100',
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 4,
      }),
    ).rejects.toThrow(
      'Lighter rejected this order because the H100 margin mode appears out of sync. This market requires isolated to trade.',
    );

    expect(exchange.fetchPositions).toHaveBeenCalledTimes(1);
    expect(exchange.store.positions.find((position) => position.symbol === 'H100')?.marginType).toBe(MarginType.Cross);
  });

  it('uses sendTxBatch signer path for same-market scaled limit ladders', async () => {
    const exchange = createExchange();

    const result = await exchange.placeOrders([
      {
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 100,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      },
      {
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.2,
        price: 99,
        timeInForce: OrderTimeInForce.PostOnly,
      },
    ]);

    expect(createOrdersMock).toHaveBeenCalledTimes(1);
    expect(createOrderMock).not.toHaveBeenCalled();
    const batchRequests = createOrdersMock.mock.calls[0][0] as Array<{ orderId: bigint }>;
    expect(result).toEqual(batchRequests.map((request) => request.orderId.toString()));
    expect(result).toHaveLength(2);
    expect(result.every((orderId) => /^\d+$/.test(orderId))).toBe(true);
    expect(createOrdersMock).toHaveBeenCalledWith([
      expect.objectContaining({
        marketId: 1,
        isBuy: true,
        price: 1000n,
        amount: 100n,
        orderType: 'limit',
        integratorAccountIndex: 158385n,
        integratorMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
        integratorTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
      }),
      expect.objectContaining({
        marketId: 1,
        isBuy: true,
        price: 990n,
        amount: 200n,
        postOnly: true,
        orderType: 'limit',
        integratorAccountIndex: 158385n,
        integratorMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
        integratorTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
      }),
    ]);
    expect(exchange.privateWebsocket.throttleFetchOrders).toHaveBeenCalledTimes(1);

    const firstRequest = createOrdersMock.mock.calls[0][0][0];
    expect((exchange as any).placedOrderHints.get(firstRequest.orderId.toString())).toEqual(
      expect.objectContaining({
        type: OrderType.Limit,
        side: OrderSide.Buy,
        symbol: 'BTC/USDT',
      }),
    );
  });

  it('falls back to sequential placeOrder for mixed markets', async () => {
    const exchange = createExchange();

    const result = await exchange.placeOrders([
      {
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 100,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      },
      {
        symbol: 'ETH/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.1,
        price: 100,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      },
    ]);

    expect(createOrdersMock).not.toHaveBeenCalled();
    expect(createOrderMock).toHaveBeenCalledTimes(2);
    expect(result).toEqual(createOrderMock.mock.calls.map(([request]) => request.orderId.toString()));
    expect(result.every((orderId) => /^\d+$/.test(orderId))).toBe(true);
  });

  it('falls back to sequential placeOrder for conditional TP/SL orders', async () => {
    const exchange = createExchange();

    await exchange.placeOrders([
      {
        symbol: 'BTC/USDT',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        amount: 0.1,
        price: 90,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
      },
      {
        symbol: 'BTC/USDT',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        amount: 0.1,
        price: 110,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
      },
    ]);

    expect(createOrdersMock).not.toHaveBeenCalled();
    expect(createOrderMock).toHaveBeenCalledTimes(2);
  });

  it('surfaces batch errors and clears remembered hints', async () => {
    const exchange = createExchange();
    createOrdersMock.mockRejectedValueOnce(new Error('21149 integrator is not approved'));

    await expect(
      exchange.placeOrders([
        {
          symbol: 'BTC/USDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 0.1,
          price: 100,
          timeInForce: OrderTimeInForce.GoodTillCancel,
        },
        {
          symbol: 'BTC/USDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 0.2,
          price: 99,
          timeInForce: OrderTimeInForce.GoodTillCancel,
        },
      ]),
    ).rejects.toThrow('Lighter batch order placement failed');

    expect((exchange as any).placedOrderHints.size).toBe(0);
    expect(exchange.privateWebsocket.throttleFetchOrders).toHaveBeenCalledTimes(1);
  });

  it('rejects non-numeric cancel ids before mutating local orders', async () => {
    const exchange = createExchange();
    const optimisticOrder = {
      id: `0x${'ab'.repeat(32)}`,
      status: OrderStatus.Open,
      symbol: 'BTC/USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 100,
      amount: 0.1,
      filled: 0,
      remaining: 0.1,
      reduceOnly: false,
      lastUpdate: Date.now(),
    };
    exchange.store.update({ orders: [optimisticOrder] });

    await expect(exchange.cancelOrders([optimisticOrder])).rejects.toThrow('expected a numeric client order id');

    expect(cancelOrderMock).not.toHaveBeenCalled();
    expect(exchange.store.orders).toEqual([optimisticOrder]);
  });

  it('keeps ambiguous reduce-only sell orders as stop-loss when updating below market', async () => {
    const exchange = createExchange();
    exchange.store.update({
      tickers: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 100,
          index: 100,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });

    await exchange.updateOrder({
      order: {
        id: '111',
        status: OrderStatus.Open,
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        price: 90,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: true,
        lastUpdate: Date.now(),
      },
      update: { price: 88 },
    });

    expect(createOrderMock).toHaveBeenCalledWith(
      expect.objectContaining({
        orderType: 'stop_loss',
        reduceOnly: true,
        price: 871n,
        triggerPrice: 880n,
      }),
    );
  });

  it('maps ambiguous reduce-only sell orders below market as stop-loss', () => {
    const exchange = createExchange();
    exchange.store.update({
      tickers: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 100,
          index: 100,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });

    const orders = exchange.mapOrder({
      order: {
        order_id: '222',
        market_id: 1,
        order_type: 0,
        time_in_force: 1,
        is_ask: true,
        price: '90',
        initial_base_amount: '0.1',
        remaining_base_amount: '0.1',
        reduce_only: true,
        status: 'OPEN',
      },
    });

    expect(orders[0]).toEqual(
      expect.objectContaining({
        id: '222',
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        reduceOnly: true,
        price: 90,
      }),
    );
  });

  it('maps documented Lighter take-profit snapshots at trigger price, not execution guard price', () => {
    const exchange = createExchange();

    const orders = exchange.mapOrder({
      order: {
        order_id: '555',
        market_index: 1,
        type: 'take-profit',
        time_in_force: 'immediate-or-cancel',
        is_ask: true,
        price: '0.13663',
        trigger_price: '0.13801',
        initial_base_amount: '100',
        remaining_base_amount: '100',
        reduce_only: true,
        status: 'open',
      },
    });

    expect(orders[0]).toEqual(
      expect.objectContaining({
        id: '555',
        type: OrderType.TakeProfit,
        side: OrderSide.Sell,
        reduceOnly: true,
        price: 0.13801,
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
      }),
    );
  });

  it('keeps profit-side reduce-only sell limits as limits', async () => {
    const exchange = createExchange();
    exchange.store.update({
      positions: [
        {
          id: 'BTC/USDT:long:oneway',
          symbol: 'BTC/USDT',
          side: PositionSide.Long,
          entryPrice: 100,
          notional: 10,
          leverage: 1,
          unrealizedPnl: 0,
          contracts: 0.1,
          liquidationPrice: 0,
          positionMode: PositionMode.OneWay,
        },
      ],
      tickers: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 100,
          index: 100,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });

    await exchange.updateOrder({
      order: {
        id: '333',
        status: OrderStatus.Open,
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        price: 110,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: true,
        lastUpdate: Date.now(),
      },
      update: { price: 112 },
    });

    expect(createOrderMock).toHaveBeenCalledWith(
      expect.objectContaining({
        orderType: 'limit',
        reduceOnly: true,
        price: 1120n,
        triggerPrice: undefined,
      }),
    );
  });

  it('uses the updated loss-side price when inferring an ambiguous stop-loss', async () => {
    const exchange = createExchange();
    exchange.store.update({
      tickers: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          bid: 99,
          ask: 101,
          last: 100,
          mark: 100,
          index: 100,
          percentage: 0,
          openInterest: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
        },
      ],
    });

    await exchange.updateOrder({
      order: {
        id: '444',
        status: OrderStatus.Open,
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Sell,
        price: 110,
        amount: 0.1,
        filled: 0,
        remaining: 0.1,
        reduceOnly: true,
        lastUpdate: Date.now(),
      },
      update: { price: 90 },
    });

    expect(createOrderMock).toHaveBeenCalledWith(
      expect.objectContaining({
        orderType: 'stop_loss',
        reduceOnly: true,
        price: 891n,
        triggerPrice: 900n,
      }),
    );
  });
});
