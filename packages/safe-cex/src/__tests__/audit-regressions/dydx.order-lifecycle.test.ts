import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { DydxExchange } from '../../exchanges/dydx/dydx.exchange';
import { DydxSDKClient } from '../../exchanges/dydx/dydx.sdk';
import {
  DydxOrderFlags,
  DydxOrderSide,
  DydxOrderStatus,
  DydxOrderType,
  DydxTimeInForce,
  DYDX_STATEFUL_ORDER_DURATION_SECONDS,
} from '../../exchanges/dydx/dydx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';
import { OrderExecution as SdkOrderExecution } from '@dydxprotocol/v4-client-js';

const marketInfo = {
  ticker: 'BTC-USD',
  marketId: 1,
  clobPairId: '0',
  baseAsset: 'BTC',
  quoteAsset: 'USD',
  status: 'ACTIVE',
  priceChange24H: '0',
  volume24H: '1',
  trades24H: 1,
  nextFundingRate: '0',
  initialMarginFraction: '0.05',
  maintenanceMarginFraction: '0.03',
  openInterest: '1',
  atomicResolution: -10,
  quantumConversionExponent: -9,
  tickSize: '1',
  stepSize: '0.001',
  stepBaseQuantums: 1000000,
  subticksPerTick: 100000,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USD',
        symbol: 'BTC-USD',
        base: 'BTC',
        quote: 'USD',
        active: true,
        precision: { amount: 0.001, price: 1 },
        limits: { amount: { min: 0.001, max: 1_000_000 }, leverage: { min: 1, max: 20 } },
        contractSize: 1,
        takerFee: 0,
        makerFee: 0,
      },
    ],
  });

  const exchange = new DydxExchange('dydx', { key: 'dydx1test', secret: 'test-secret' }, store);
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);

  const sdkClient = {
    placeOrder: vi.fn().mockResolvedValue({ txhash: 'tx' }),
    placeShortTermOrder: vi.fn().mockResolvedValue({ txhash: 'tx' }),
    cancelOrder: vi.fn().mockResolvedValue({ txhash: 'tx' }),
    getCurrentBlockHeight: vi.fn().mockResolvedValue(100),
    getOrders: vi.fn(),
  };
  exchange.sdkClient = sdkClient as any;

  return { exchange, sdkClient };
};

describe('dYdX order lifecycle audit regressions', () => {
  it('places resting GTT limits as 90-day stateful orders', async () => {
    const { exchange, sdkClient } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTC-USD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 60_000,
      amount: 0.01,
      timeInForce: OrderTimeInForce.GoodTillCancel,
    });

    expect(sdkClient.placeShortTermOrder).not.toHaveBeenCalled();
    expect(sdkClient.getCurrentBlockHeight).not.toHaveBeenCalled();
    expect(sdkClient.placeOrder).toHaveBeenCalledWith(
      expect.objectContaining({
        market: 'BTC-USD',
        type: DydxOrderType.LIMIT,
        timeInForce: DydxTimeInForce.GTT,
        goodTilTimeInSeconds: DYDX_STATEFUL_ORDER_DURATION_SECONDS,
        goodTilBlock: undefined,
      }),
    );
  });

  it('gives conditional orders a real stateful expiry', async () => {
    const { exchange, sdkClient } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTC-USD',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 58_000,
      triggerPrice: 59_000,
      amount: 0.01,
    });

    expect(sdkClient.getCurrentBlockHeight).not.toHaveBeenCalled();
    expect(sdkClient.placeOrder).toHaveBeenCalledWith(
      expect.objectContaining({
        type: DydxOrderType.STOP_MARKET,
        goodTilTimeInSeconds: DYDX_STATEFUL_ORDER_DURATION_SECONDS,
        goodTilBlock: undefined,
      }),
    );
  });

  it('reserves the short-term wrapper for IOC/FOK limits', async () => {
    const { exchange, sdkClient } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTC-USD',
      type: OrderType.Limit,
      side: OrderSide.Buy,
      price: 60_000,
      amount: 0.01,
      timeInForce: OrderTimeInForce.ImmediateOrCancel,
    });

    expect(sdkClient.placeOrder).not.toHaveBeenCalled();
    expect(sdkClient.placeShortTermOrder).toHaveBeenCalledWith(
      expect.objectContaining({
        market: 'BTC-USD',
        timeInForce: DydxTimeInForce.IOC,
        goodTilBlock: 120,
      }),
    );
  });

  it('fails closed when the validator block height is unavailable', async () => {
    const { exchange, sdkClient } = createExchange();
    sdkClient.getCurrentBlockHeight.mockRejectedValueOnce(new Error('RPC unavailable'));

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USD',
        type: OrderType.Market,
        side: OrderSide.Buy,
        price: 60_000,
        amount: 0.01,
      }),
    ).rejects.toThrow('Unable to get the current dYdX block height');
    expect(sdkClient.placeOrder).not.toHaveBeenCalled();
  });

  it('maps CLOB pair ids back to tickers and cancels with original stateful flags', async () => {
    const { exchange, sdkClient } = createExchange();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({ data: { markets: { 'BTC-USD': marketInfo } } }),
    } as unknown as Axios;
    await exchange.fetchMarkets();
    sdkClient.getOrders.mockResolvedValue([
      {
        id: 'indexer-order-id',
        subaccountId: 'subaccount',
        clientId: '123',
        clobPairId: '0',
        side: DydxOrderSide.BUY,
        size: '0.01',
        totalFilled: '0',
        price: '60000',
        type: DydxOrderType.LIMIT,
        status: DydxOrderStatus.OPEN,
        timeInForce: DydxTimeInForce.GTT,
        postOnly: false,
        reduceOnly: false,
        orderFlags: String(DydxOrderFlags.LONG_TERM),
        createdAt: '2026-07-15T00:00:00.000Z',
        createdAtHeight: '60000000',
        clientMetadata: '0',
      },
    ]);

    const [order] = await exchange.fetchOpenOrders();
    expect(order).toMatchObject({ symbol: 'BTC-USD', info: { orderFlags: DydxOrderFlags.LONG_TERM } });

    await exchange.cancelOrder(order);

    expect(sdkClient.getCurrentBlockHeight).not.toHaveBeenCalled();
    expect(sdkClient.cancelOrder).toHaveBeenCalledWith({
      clientId: 123,
      orderFlags: DydxOrderFlags.LONG_TERM,
      marketId: 'BTC-USD',
      goodTilBlock: 0,
      goodTilTimeInSeconds: DYDX_STATEFUL_ORDER_DURATION_SECONDS,
    });
  });
});

describe('dYdX SDK positional contract', () => {
  it('passes stateful duration and short-term block height into their documented SDK positions', async () => {
    const sdk = new DydxSDKClient({ address: 'dydx1test', privateKey: '00', subaccountNumber: 0, testnet: false });
    const placeOrder = vi.fn().mockResolvedValue({ txhash: 'tx' });
    sdk.compositeClient = { placeOrder } as any;
    sdk.subaccount = { address: 'dydx1test', subaccountNumber: 0 };

    await sdk.placeOrder({
      market: 'BTC-USD',
      type: DydxOrderType.STOP_MARKET as any,
      side: DydxOrderSide.SELL as any,
      price: 58_000,
      size: 0.01,
      clientId: 1,
      timeInForce: DydxTimeInForce.GTT as any,
      postOnly: false,
      reduceOnly: true,
      triggerPrice: 59_000,
      goodTilTimeInSeconds: DYDX_STATEFUL_ORDER_DURATION_SECONDS,
    });

    expect(placeOrder.mock.calls[0][8]).toBe(DYDX_STATEFUL_ORDER_DURATION_SECONDS);
    expect(placeOrder.mock.calls[0][9]).toBe(SdkOrderExecution.IOC);
    expect(placeOrder.mock.calls[0][15]).toBeUndefined();

    await sdk.placeShortTermOrder({
      market: 'BTC-USD',
      side: DydxOrderSide.BUY as any,
      price: 60_000,
      size: 0.01,
      clientId: 2,
      goodTilBlock: 120,
      timeInForce: DydxTimeInForce.IOC as any,
      reduceOnly: false,
    });

    expect(placeOrder.mock.calls[1][8]).toBeUndefined();
    expect(placeOrder.mock.calls[1][15]).toBe(120);
  });
});
