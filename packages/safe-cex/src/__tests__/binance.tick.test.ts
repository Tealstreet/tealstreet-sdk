import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from './reference/binance/binance.exchange';
import { loop } from '../utils/loop';

vi.mock('../utils/loop', () => ({ loop: vi.fn() }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-07-10T20:00:00Z'));
});

afterEach(() => {
  vi.useRealTimers();
});

describe('Binance REST reconciliation ordering', () => {
  it('does not let a stale account snapshot overwrite a newer websocket position', async () => {
    let resolveAccount!: (value: Record<string, unknown>) => void;
    const accountRequest = new Promise<Record<string, unknown>>((resolve) => {
      resolveAccount = resolve;
    });
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);
    const balance = { total: 100, free: 80, used: 20, upnl: 5 };
    const stalePositions = [{ id: 'BTCUSDT', contracts: 1 }];
    const fetchPositionsAndBalance = vi.fn().mockReturnValue(accountRequest);

    Object.assign(exchange, {
      isDisposed: false,
      fetchPositions: vi.fn().mockResolvedValue([]),
      fetchPositionsAndBalance,
      emitAndLogError: vi.fn(),
    });

    const tick = exchange.tick(1);
    await vi.waitFor(() => expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(1));
    exchange.markPositionStateUpdatedByWebsocket();
    resolveAccount({ balance, positions: stalePositions });
    await tick;

    expect(store.update).toHaveBeenLastCalledWith({
      balance,
      loaded: { balance: true, positions: true },
    });
  });

  it('uses a 25-second healthy snapshot and a 3-second degraded fallback', async () => {
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);
    const fetchPositions = vi.fn().mockResolvedValue([]);
    const fetchPositionsAndBalance = vi.fn().mockResolvedValue({
      balance: { total: 0, free: 0, used: 0, upnl: 0 },
      positions: [],
    });
    const privateWebsocket = { isConnected: true };

    Object.assign(exchange, {
      isDisposed: false,
      privateWebsocket,
      fetchPositions,
      fetchPositionsAndBalance,
      emitAndLogError: vi.fn(),
    });

    await exchange.tick();
    expect(fetchPositions).toHaveBeenCalledTimes(1);
    expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(3_000);
    await exchange.tick();
    expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(22_000);
    await exchange.tick();
    expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(2);

    privateWebsocket.isConnected = false;
    vi.advanceTimersByTime(3_000);
    await exchange.tick();
    vi.advanceTimersByTime(3_000);
    await exchange.tick();

    expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(4);
    expect(loop).toHaveBeenCalledTimes(5);
    expect(loop).toHaveBeenCalledWith(expect.any(Function), 3_000);
  });

  it('pauses background snapshots before the shared IP request-weight limit is exhausted', async () => {
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);
    const fetchPositions = vi.fn().mockResolvedValue([]);
    const fetchPositionsAndBalance = vi.fn().mockResolvedValue({
      balance: { total: 0, free: 0, used: 0, upnl: 0 },
      positions: [],
    });

    Object.assign(exchange, {
      isDisposed: false,
      privateWebsocket: { isConnected: true },
      fetchPositions,
      fetchPositionsAndBalance,
      emitAndLogError: vi.fn(),
    });

    (exchange as any).observeRequestWeight({
      status: 200,
      headers: { 'x-mbx-used-weight-1m': '1920' },
    });

    await exchange.tick();
    expect(fetchPositions).not.toHaveBeenCalled();
    expect(fetchPositionsAndBalance).not.toHaveBeenCalled();

    vi.advanceTimersByTime(60_000);
    await exchange.tick();
    expect(fetchPositions).toHaveBeenCalledTimes(1);
    expect(fetchPositionsAndBalance).toHaveBeenCalledTimes(1);
  });

  it('honors Binance retry-after after a rate-limit response', async () => {
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);

    (exchange as any).observeRequestWeight({
      status: 429,
      headers: { 'retry-after': '30' },
    });

    expect(exchange.isBackgroundReconciliationPaused()).toBe(true);
    vi.advanceTimersByTime(29_999);
    expect(exchange.isBackgroundReconciliationPaused()).toBe(true);
    vi.advanceTimersByTime(1);
    expect(exchange.isBackgroundReconciliationPaused()).toBe(false);
  });

  it('stops the second background snapshot when the first response reaches the weight threshold', async () => {
    const store = {
      loaded: {},
      subscribe: vi.fn(),
      update: vi.fn(),
    };
    const exchange = new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, store as any);
    const fetchPositions = vi.fn().mockImplementation(async () => {
      (exchange as any).observeRequestWeight({
        status: 200,
        headers: { 'x-mbx-used-weight-1m': '1920' },
      });
      return [];
    });
    const fetchPositionsAndBalance = vi.fn();

    Object.assign(exchange, {
      isDisposed: false,
      privateWebsocket: { isConnected: true },
      fetchPositions,
      fetchPositionsAndBalance,
      emitAndLogError: vi.fn(),
    });

    await exchange.tick();

    expect(fetchPositions).toHaveBeenCalledTimes(1);
    expect(fetchPositionsAndBalance).not.toHaveBeenCalled();
  });
});
