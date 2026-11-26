import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../types';
import { nativeMarket } from './cutover/nativeMarket';
import { nativePosition } from './cutover/nativePosition';
import { applyReferenceFixture, referenceStore } from './cutover/referenceFixture';
import { BinancePrivateWebsocket } from './reference/phemex/phemex.ws-private';

const createOrder = (ordStatus: string) => ({
  orderID: 'order-1',
  ordStatus,
  ordType: 'Market',
  side: 'Buy',
  symbol: 'BTCUSDT',
  stopPxRp: '0',
  priceRp: '50000',
  orderQty: '1',
  reduceOnly: false,
  execQty: ordStatus.toLowerCase() === 'new' ? '0' : '1',
  leavesQty: ordStatus.toLowerCase() === 'new' ? '1' : '0',
  slTrigger: 'ByMarkPrice',
  tpTrigger: 'ByLastPrice',
  stopLossRp: '0',
  takeProfitRp: '0',
  timeInForce: 'ImmediateOrCancel',
  execInst: '',
});

const createContext = () => {
  const store = referenceStore({ markets: [nativeMarket({ symbol: 'BTCUSDT' })], positions: [] });
  const positionChanges = vi.fn<(rows: typeof store.positions) => void>();
  store.subscribe('positions', (rows) => positionChanges(rows));

  const parent = {
    isDisposed: false,
    name: 'phemex',
    options: {
      testnet: false,
      key: '',
      secret: '',
    },
    store,
    emitter: {
      emit: vi.fn(),
    },
    emitAndLogError: vi.fn(),
    formatSymbolIn: (symbol?: string) => symbol?.toUpperCase() || '',
    mapOrderUpdate: vi.fn((order: any) => [
      {
        id: order.orderID,
        status: order.ordStatus?.toLowerCase() === 'filled' ? OrderStatus.Closed : OrderStatus.Open,
        symbol: order.symbol,
        type: OrderType.Market,
        side: OrderSide.Buy,
        price: 50_000,
        amount: 1,
        filled: order.ordStatus?.toLowerCase() === 'new' ? 0 : 1,
        remaining: order.ordStatus?.toLowerCase() === 'new' ? 1 : 0,
        reduceOnly: false,
        lastUpdate: Date.now(),
        info: order,
      },
    ]),
    mapPosition: vi.fn(),
    fetchPositions: vi
      .fn()
      .mockResolvedValue([
        nativePosition({ id: 'BTCUSDT:Short', symbol: 'BTCUSDT', side: PositionSide.Short, contracts: 2 }),
      ]),
    fetchPositionsAndBalance: vi.fn().mockResolvedValue({
      balance: { used: 1, free: 2, total: 3, upnl: 0 },
      positions: [
        nativePosition(
          nativePosition({ id: 'BTCUSDT:Long', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 }),
        ),
      ],
    }),
  };

  const ws = new BinancePrivateWebsocket(parent as any);
  ws.lastReceived = Date.now(); // Prevent BaseWebSocket from starting periodic watchdog timer in tests.

  return { ws, store, parent, positionChanges };
};

describe('Phemex private websocket reconciliation', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it('upserts non-zero positions from positions_p updates', () => {
    const { ws, store, parent, positionChanges } = createContext();
    const parsedPosition = nativePosition({
      id: 'BTCUSDT:Long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      contracts: 2,
    });
    parent.mapPosition.mockReturnValue(parsedPosition);

    ws.onMessage({
      data: JSON.stringify({
        type: 'incremental',
        positions_p: [{ symbol: 'BTCUSDT' }],
      }),
    } as MessageEvent);

    expect(store.positions).toMatchObject([parsedPosition]);
  });

  it('ignores brand-new zero-contract WS positions', () => {
    const { ws, store, parent, positionChanges } = createContext();
    parent.mapPosition.mockReturnValue(
      nativePosition({
        id: 'BTCUSDT:Long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        contracts: 0,
      }),
    );

    ws.onMessage({
      data: JSON.stringify({
        type: 'incremental',
        positions_p: [{ symbol: 'BTCUSDT' }],
      }),
    } as MessageEvent);

    expect(positionChanges).not.toHaveBeenCalled();
  });

  it('keeps zero-contract WS position updates for already-known positions', () => {
    const { ws, store, parent, positionChanges } = createContext();
    applyReferenceFixture(store, {
      positions: [nativePosition({ id: 'BTCUSDT:Long', symbol: 'BTCUSDT', contracts: 1 })],
    });
    positionChanges.mockClear();
    const parsedPosition = nativePosition({
      id: 'BTCUSDT:Long',
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      contracts: 0,
    });
    parent.mapPosition.mockReturnValue(parsedPosition);

    ws.onMessage({
      data: JSON.stringify({
        type: 'incremental',
        positions_p: [{ symbol: 'BTCUSDT' }],
      }),
    } as MessageEvent);

    expect(store.positions).toMatchObject([parsedPosition]);
  });

  it('debounces REST sync and explicitly reconciles terminal orders with unknown position side', async () => {
    const { ws, store, parent, positionChanges } = createContext();
    const payload = JSON.stringify({
      type: 'incremental',
      orders_p: [createOrder('Filled')],
    });

    ws.onMessage({ data: payload } as MessageEvent);
    ws.onMessage({ data: payload } as MessageEvent);

    expect(parent.fetchPositionsAndBalance).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1_000);

    expect(parent.fetchPositionsAndBalance).toHaveBeenCalledTimes(1);
    expect(parent.fetchPositions).toHaveBeenCalledTimes(1);
    expect(parent.fetchPositions).toHaveBeenCalledWith(['BTCUSDT']);
    expect(positionChanges).toHaveBeenCalledTimes(1);
    expect(store.positions).toMatchObject([
      { id: 'BTCUSDT:Long', symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      { id: 'BTCUSDT:Short', symbol: 'BTCUSDT', side: PositionSide.Short, contracts: 2 },
    ]);
    expect(store.balance).toEqual({ used: 1, free: 2, total: 3, upnl: 0 });
    expect(store.loaded).toMatchObject({ balance: true, positions: true });
  });

  it('does not schedule REST sync for non-terminal order events', async () => {
    const { ws, parent } = createContext();

    ws.onMessage({
      data: JSON.stringify({
        type: 'incremental',
        orders_p: [createOrder('New')],
      }),
    } as MessageEvent);

    await vi.advanceTimersByTimeAsync(1_000);

    expect(parent.fetchPositionsAndBalance).not.toHaveBeenCalled();
  });
});
