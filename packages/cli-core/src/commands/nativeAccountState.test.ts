import { describe, expect, it, vi } from 'vitest';
import { OrderSide, OrderStatus, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import type { Order, Position } from '@tealstreet/safe-cex/src/types';
import type { CLIPlatformAdapter, ExchangeLike } from '../types';
import { executeCommand } from './CommandExecutor';
import { preprocessCommand } from '../preprocess/preprocess';
import { MemoryPreprocessStore } from '../preprocess/memoryStore';
import { parseConditionText } from '../preprocess/conditions';

function fixture() {
  const position = {
    id: 'BTCUSDT-long', symbol: 'BTCUSDT', side: PositionSide.Long,
    contracts: 5000, entryPrice: 2, notional: 10000,
    leverage: 2, liquidationPrice: 1, unrealizedPnl: 0,
  } as Position;
  const order = {
    id: '9007199254740995', symbol: 'BTCUSDT', side: OrderSide.Buy,
    type: OrderType.Limit, status: OrderStatus.Open, amount: 1,
    price: 2, remaining: 1, filled: 0, reduceOnly: false, lastUpdate: 1,
  } as Order;
  const exchange: ExchangeLike = {
    name: 'binance',
    store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', contractSize: 1000,
        precision: { amount: 1, price: 0.01 } } as never],
      tickers: [{ symbol: 'BTCUSDT', last: 2 } as never],
      orders: [], positions: [],
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: vi.fn().mockResolvedValue(['accepted']),
    cancelOrders: vi.fn().mockResolvedValue(undefined),
    syncAccountState: vi.fn(() => {
      exchange.store!.positions = [position];
      exchange.store!.orders = [order];
      exchange.store!.balance = { free: 123, used: 0, total: 123, upnl: 0 };
    }),
  };
  const adapter = {
    print: vi.fn(), printInfo: vi.fn(), printWarning: vi.fn(),
    printSuccess: vi.fn(), printError: vi.fn(),
  } as unknown as CLIPlatformAdapter;
  const ctx = { exchange, symbol: 'BTCUSDT', store: new MemoryPreprocessStore() };
  return { exchange, adapter, ctx, position, order };
}

describe('native state at CLI decision boundaries', () => {
  it('synchronizes close sizing before selecting positions and uses native contracts', async () => {
    const { exchange, adapter } = fixture();
    const result = await executeCommand('close BTCUSDT', 'BTCUSDT', 'account', exchange, adapter);
    expect(result.success).toBe(true);
    expect(exchange.syncAccountState).toHaveBeenCalledOnce();
    expect(exchange.placeOrder).toHaveBeenCalledWith(expect.objectContaining({
      symbol: 'BTCUSDT', amount: 5, reduceOnly: true, side: OrderSide.Sell,
    }));
  });

  it('selects current native orders for cancel rather than the unflushed mirror', async () => {
    const { exchange, adapter, order } = fixture();
    const result = await executeCommand('cancel buys', 'BTCUSDT', 'account', exchange, adapter);
    expect(result.success).toBe(true);
    expect(exchange.cancelOrders).toHaveBeenCalledWith([order]);
  });

  it.each([
    ['buy $size at $entry', ['buy 5000 at 2']],
    ['echo $free', ['echo 123']],
    ['loop longs: close', ['close BTCUSDT']],
    ['if (position) (close) else (echo empty)', ['close']],
  ])('synchronizes state while preprocessing %s', (input, commands) => {
    const { ctx, exchange } = fixture();
    const result = preprocessCommand(input, ctx);
    expect(result.steps.map((step) => step.command)).toEqual(commands);
    expect(exchange.syncAccountState).toHaveBeenCalled();
  });

  it('reevaluates live conditions from native state on each check', () => {
    const { exchange, position } = fixture();
    const condition = parseConditionText('position')!;
    expect(condition.check(exchange, 'BTCUSDT')).toBe(true);
    position.contracts = 0;
    expect(condition.check(exchange, 'BTCUSDT')).toBe(false);
    expect(exchange.syncAccountState).toHaveBeenCalledTimes(2);
  });

  it('fails before any write when native state is unavailable', async () => {
    const { exchange, adapter } = fixture();
    exchange.syncAccountState = () => { throw new Error('Engine unavailable'); };
    const result = await executeCommand('close BTCUSDT', 'BTCUSDT', 'account', exchange, adapter);
    expect(result).toMatchObject({ success: false, error: 'Engine unavailable', errorReported: true });
    expect(exchange.placeOrder).not.toHaveBeenCalled();
    expect(adapter.printError).toHaveBeenCalledOnce();
  });

  it('keeps session help available when the engine has failed', () => {
    const { ctx, exchange } = fixture();
    exchange.syncAccountState = () => { throw new Error('Engine unavailable'); };
    expect(preprocessCommand('help', ctx).steps).toEqual([{ command: 'help', block: false }]);
  });

  it('keeps ticker-only conditions independent of private snapshots', () => {
    const { exchange } = fixture();
    exchange.syncAccountState = () => { throw new Error('Private snapshots unavailable'); };
    expect(parseConditionText('price > 1')!.check(exchange, 'BTCUSDT')).toBe(true);
  });
});
