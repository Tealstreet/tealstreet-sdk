import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { DydxExchange } from '../../exchanges/dydx/dydx.exchange';
import {
  DydxOrderFlags,
  DydxOrderSide,
  DydxOrderStatus,
  DydxOrderType,
  DydxPositionSide,
  DydxTimeInForce,
  WS_CHANNELS,
} from '../../exchanges/dydx/dydx.types';
import { DefaultStore } from '../../store/store.base';
import { OrderStatus, OrderTimeInForce } from '../../types';

const symbol = 'BTC-USD';
const marketInfo = {
  ticker: symbol,
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

const createExchange = async (publicDataOnly = false) => {
  const store = new DefaultStore();
  const exchange = new DydxExchange(
    'dydx',
    publicDataOnly ? ({ publicDataOnly: true } as any) : { key: 'dydx1test', secret: 'test-secret' },
    store,
  );
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  exchange.xhr = {
    get: vi.fn().mockResolvedValue({ data: { markets: { [symbol]: marketInfo } } }),
  } as unknown as Axios;

  const markets = await exchange.fetchMarkets();
  store.update({
    markets,
    tickers: [
      {
        id: symbol,
        symbol,
        bid: 0,
        ask: 0,
        last: 0,
        mark: 0,
        index: 0,
        percentage: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
        openInterest: 0,
      },
    ],
  });

  return { exchange, store };
};

const order = {
  id: 'order-id',
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
  postOnly: true,
  reduceOnly: false,
  orderFlags: String(DydxOrderFlags.LONG_TERM),
  createdAt: '2026-07-14T00:00:00.000Z',
  updatedAt: '2026-07-15T00:00:00.000Z',
  createdAtHeight: '60000000',
  clientMetadata: '0',
};

const position = {
  market: symbol,
  status: 'OPEN',
  side: DydxPositionSide.LONG,
  size: '0.5',
  maxSize: '0.5',
  entryPrice: '59000',
  realizedPnl: '1',
  createdAt: '2026-07-14T00:00:00.000Z',
  createdAtHeight: '60000000',
  sumOpen: '0.5',
  sumClose: '0',
  netFunding: '0',
  unrealizedPnl: '7',
};

describe('dYdX private state audit regressions', () => {
  it('consumes the subscribed snapshot into orders, positions, balance, and fills', async () => {
    const { exchange, store } = await createExchange();
    const onFill = vi.fn();
    exchange.emitter.on('fill', onFill);

    exchange.privateWebsocket.onMessage({
      data: JSON.stringify({
        type: 'subscribed',
        channel: WS_CHANNELS.SUBACCOUNT,
        contents: {
          orders: [order],
          positions: [position],
          subaccount: { equity: '125', freeCollateral: '80' },
          fills: [
            {
              id: 'fill-id',
              side: DydxOrderSide.BUY,
              liquidity: 'TAKER',
              type: 'LIMIT',
              market: symbol,
              marketType: 'PERPETUAL',
              price: '60000',
              size: '0.01',
              fee: '0.1',
              createdAt: '2026-07-15T00:00:00.000Z',
              createdAtHeight: '60000001',
            },
          ],
        },
      }),
    } as MessageEvent);

    expect(store.orders).toContainEqual(
      expect.objectContaining({
        id: 'order-id',
        symbol,
        status: OrderStatus.Open,
        timeInForce: OrderTimeInForce.PostOnly,
        lastUpdate: Date.parse('2026-07-15T00:00:00.000Z'),
      }),
    );
    expect(store.positions).toContainEqual(
      expect.objectContaining({ symbol, contracts: 0.5, entryPrice: 59000, unrealizedPnl: 7 }),
    );
    expect(store.balance).toEqual({ total: 125, free: 80, used: 45, upnl: 7 });
    expect(onFill).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'fill-id', symbol, price: 60000, amount: 0.01 }),
    );
  });

  it('reads exact equity and freeCollateral without a leverage estimate or zero clamp', async () => {
    const { exchange } = await createExchange();
    exchange.sdkClient = {
      getSubaccount: vi.fn().mockResolvedValue({
        subaccount: { equity: '40', freeCollateral: '-2.5' },
      }),
    } as any;

    await expect(exchange.fetchBalance()).resolves.toEqual({ total: 40, free: -2.5, used: 42.5, upnl: 0 });
  });

  it('starts public-only mode without wallet validation or the private tick loop', async () => {
    const { exchange, store } = await createExchange(true);
    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue(store.markets);
    vi.spyOn(exchange, 'fetchTickers').mockResolvedValue(store.tickers);
    const validateAccount = vi.spyOn(exchange, 'validateAccount');
    const tick = vi.spyOn(exchange, 'tick');
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => undefined);

    await exchange.start();

    expect(validateAccount).not.toHaveBeenCalled();
    expect(tick).not.toHaveBeenCalled();
    expect(store.loaded).toMatchObject({ markets: true, tickers: true, orders: true, positions: true, balance: true });
    expect(store.orders).toEqual([]);
    expect(store.positions).toEqual([]);
    expect(store.balance).toEqual({ total: 0, free: 0, used: 0, upnl: 0 });
  });
});
