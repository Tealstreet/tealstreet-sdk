import type { ExchangeOptions } from '../types';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE,
  LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE,
} from '../exchanges/lighter/lighter.account-tier';
import { LighterExchange } from './reference/lighter/lighter.exchange';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../types';

const { createOrderMock } = vi.hoisted(() => ({
  createOrderMock: vi.fn(),
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

describe('LighterExchange builder fee order wiring', () => {
  const createExchange = (overrides: Partial<ExchangeOptions> = {}): LighterExchange => {
    const options = {
      key: 'lighter-key',
      secret: 'lighter-secret',
      apiKeyIndex: '1',
      applicationId: '123',
      testnet: false,
      ...overrides,
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
          },
        },
      ],
      tickers: [
        {
          id: '1',
          symbol: 'BTC/USDT',
          bid: 100,
          ask: 100,
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

    (exchange as any).ensureSignerAdapter = vi.fn().mockResolvedValue({
      createOrder: createOrderMock,
    });
    return exchange;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    createOrderMock.mockResolvedValue('tx-hash');
  });

  it('uses maker-free and taker 2.5 bps fee config by default', async () => {
    const exchange = createExchange();

    await exchange.placeOrder({
      symbol: 'BTC/USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 100,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(createOrderMock).toHaveBeenCalledTimes(1);
    expect(createOrderMock).toHaveBeenCalledWith(
      expect.objectContaining({
        integratorAccountIndex: 158385n,
        integratorMakerFee: 0,
        integratorTakerFee: 250,
      }),
    );
  });

  it('uses maker-free and taker 2.5 bps fee config when dynamic mode is explicitly selected', async () => {
    const exchange = createExchange({
      extra: {
        lighterBuilderFeeMode: 'dynamic',
      },
    });

    await exchange.placeOrder({
      symbol: 'BTC/USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 100,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(createOrderMock).toHaveBeenCalledTimes(1);
    expect(createOrderMock).toHaveBeenCalledWith(
      expect.objectContaining({
        integratorAccountIndex: 158385n,
        integratorMakerFee: 0,
        integratorTakerFee: 250,
      }),
    );
  });

  it('blocks attributed orders for known Standard accounts before signing', async () => {
    const exchange = createExchange();
    (exchange as any).fetchAccountLimits = vi.fn().mockResolvedValue({
      code: 200,
      data: {
        user_tier_name: 'Standard',
      },
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).rejects.toThrow(LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE);

    expect(createOrderMock).not.toHaveBeenCalled();
  });

  it('normalizes Lighter 21520 attributed-order rejection', async () => {
    const exchange = createExchange();
    (exchange as any).fetchAccountLimits = vi.fn().mockResolvedValue(undefined);
    (exchange as any).fetchAccountSnapshot = vi.fn().mockResolvedValue({
      approved_integrators: [
        {
          account_index: '158385',
          approval_expiry: Date.now() + 30 * 24 * 60 * 60 * 1000,
          max_perps_taker_fee: 300,
          max_perps_maker_fee: 300,
          max_spot_taker_fee: 300,
          max_spot_maker_fee: 300,
        },
      ],
    });
    createOrderMock.mockRejectedValueOnce({
      code: 21520,
      message: 'trades via partner attribution with non-zero integrator fees require plus or premium accounts',
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).rejects.toThrow(LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE);
  });

  it('blocks attributed orders when tier is eligible but ApproveIntegrator has never been signed', async () => {
    const exchange = createExchange();
    (exchange as any).fetchAccountLimits = vi.fn().mockResolvedValue({
      code: 200,
      data: { user_tier_name: 'Plus' },
    });
    (exchange as any).fetchAccountSnapshot = vi.fn().mockResolvedValue({
      approved_integrators: [],
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).rejects.toThrow(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE);

    expect(createOrderMock).not.toHaveBeenCalled();
  });

  it('blocks attributed orders when the existing ApproveIntegrator has expired', async () => {
    const exchange = createExchange();
    (exchange as any).fetchAccountLimits = vi.fn().mockResolvedValue({
      code: 200,
      data: { user_tier_name: 'Premium' },
    });
    (exchange as any).fetchAccountSnapshot = vi.fn().mockResolvedValue({
      approved_integrators: [
        {
          account_index: '158385',
          approval_expiry: Date.now() - 60_000,
          max_perps_taker_fee: 300,
          max_perps_maker_fee: 300,
          max_spot_taker_fee: 300,
          max_spot_maker_fee: 300,
        },
      ],
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).rejects.toThrow(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE);

    expect(createOrderMock).not.toHaveBeenCalled();
  });

  it('lets attributed orders through when tier is eligible and an unexpired approval exists', async () => {
    const exchange = createExchange();
    (exchange as any).fetchAccountLimits = vi.fn().mockResolvedValue({
      code: 200,
      data: { user_tier_name: 'Plus' },
    });
    (exchange as any).fetchAccountSnapshot = vi.fn().mockResolvedValue({
      approved_integrators: [
        {
          account_index: '158385',
          approval_expiry: Date.now() + 30 * 24 * 60 * 60 * 1000,
          max_perps_taker_fee: 300,
          max_perps_maker_fee: 300,
          max_spot_taker_fee: 300,
          max_spot_maker_fee: 300,
        },
      ],
    });

    await exchange.placeOrder({
      symbol: 'BTC/USDT',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 1,
      price: 100,
      reduceOnly: false,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(createOrderMock).toHaveBeenCalledTimes(1);
  });

  it('normalizes Lighter 21149 integrator-not-approved rejection to the branded message', async () => {
    const exchange = createExchange();
    (exchange as any).fetchAccountLimits = vi.fn().mockResolvedValue(undefined);
    // Skip the pre-send approval check so the raw venue rejection reaches sendTx normalization.
    (exchange as any).fetchAccountSnapshot = vi.fn().mockRejectedValue(new Error('snapshot unavailable'));
    createOrderMock.mockRejectedValueOnce({
      code: 21149,
      message: 'integrator is not approved',
    });

    await expect(
      exchange.placeOrder({
        symbol: 'BTC/USDT',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 1,
        price: 100,
        reduceOnly: false,
        timeInForce: OrderTimeInForce.GoodTillCancel,
      }),
    ).rejects.toThrow(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE);
  });
});
