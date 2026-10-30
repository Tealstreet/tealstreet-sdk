/**
 * Combined unit tests for the small trading commands —
 * `LeverageCommand`, `MaxCommand`, `NukeCommand`, `MoveCommand`,
 * `EchoCommand`, `MarginCommand`, and the price-readout family
 * (`LastPriceCommand` / `MarkPriceCommand` / `IndexPriceCommand`).
 * Each is small enough that a dedicated file would be mostly
 * boilerplate; grouping them here keeps the suite readable.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LeverageCommand } from './LeverageCommand';
import { MaxCommand } from './MaxCommand';
import { NukeCommand } from './NukeCommand';
import { MoveCommand } from './MoveCommand';
import { EchoCommand } from './EchoCommand';
import { MarginCommand } from './MarginCommand';
import { PositionCommand } from './PositionCommand';
import {
  LastPriceCommand,
  MarkPriceCommand,
  IndexPriceCommand,
} from './PriceCommand';
import type { CommandContext } from '../Command';
import {
  PositionSide,
  MarginType,
  PositionMode,
  OrderSide,
  OrderStatus,
  OrderType,
  type Position,
  type Balance,
  type Order,
} from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike, CLIPlatformAdapter } from '../../types';

function pos(p: Partial<Position> = {}): Position {
  return {
    id: p.id ?? 'pos1',
    symbol: p.symbol ?? 'BTCUSDT',
    side: p.side ?? PositionSide.Long,
    entryPrice: p.entryPrice ?? 50_000,
    notional: p.notional ?? 50_000,
    leverage: p.leverage ?? 1,
    marginType: p.marginType ?? MarginType.Isolated,
    positionMode: p.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: p.unrealizedPnl ?? 0,
    contracts: p.contracts ?? 1,
    liquidationPrice: p.liquidationPrice ?? 0,
  } as Position;
}

function bal(p: Partial<Balance> = {}): Balance {
  return {
    used: p.used ?? 0,
    free: p.free ?? 1000,
    total: p.total ?? 1000,
    upnl: p.upnl ?? 0,
  } as Balance;
}

function makeAdapter(): CLIPlatformAdapter {
  return {
    print: vi.fn(),
    printInfo: vi.fn(),
    printError: vi.fn(),
    printSuccess: vi.fn(),
    printWarning: vi.fn(),
    printRaw: vi.fn(),
    listAccounts: vi.fn().mockReturnValue([]),
    getAccount: vi.fn(),
    addAccount: vi.fn(),
    updateAccount: vi.fn(),
    removeAccount: vi.fn(),
    setDefaultAccount: vi.fn(),
    getDefaultSymbol: vi.fn(),
    setDefaultSymbol: vi.fn(),
    getActiveAccounts: vi.fn().mockReturnValue([]),
    listExchanges: vi.fn().mockReturnValue([]),
    createExchange: vi.fn(),
    startExchange: vi.fn(),
    stopExchange: vi.fn(),
    startCapture: vi.fn(),
  } as unknown as CLIPlatformAdapter;
}

function makeExchange(opts: { positions?: Position[]; balance?: Balance } = {}): ExchangeLike {
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } }],
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
      orders: [],
      positions: opts.positions ?? [],
      balance: opts.balance,
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    setLeverage: vi.fn().mockResolvedValue(undefined),
    nuke: vi.fn().mockResolvedValue(undefined),
    nukePositions: vi.fn().mockResolvedValue(undefined),
    nukeOrders: vi.fn().mockResolvedValue(undefined),
  } as unknown as ExchangeLike;
}

function ctx(args: string[], exchange: ExchangeLike, adapter?: CLIPlatformAdapter, symbol = 'BTCUSDT'): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'a',
    exchange,
    adapter: adapter ?? makeAdapter(),
  } as unknown as CommandContext;
}

// ─── LeverageCommand ─────────────────────────────────────────────────

describe('LeverageCommand', () => {
  let cmd: LeverageCommand;
  beforeEach(() => {
    cmd = new LeverageCommand();
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(ctx(['lev'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
  });

  it('reads leverage from the open position', async () => {
    const ex = makeExchange({ positions: [pos({ leverage: 7 })] });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['lev'], ex, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('BTCUSDT: 7x');
  });

  it("informs when there's no position to read leverage from", async () => {
    const ex = makeExchange();
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['lev'], ex, adapter));
    expect(r.success).toBe(true);
    expect(adapter.printInfo).toHaveBeenCalledWith(
      expect.stringMatching(/No open position.*lev <n>/),
    );
  });

  it('"lev <n>" calls setLeverage with the focused symbol', async () => {
    const ex = makeExchange({ positions: [pos()] });
    await cmd.execute(ctx(['lev', '10'], ex));
    expect(ex.setLeverage).toHaveBeenCalledWith('BTCUSDT', 10);
  });

  it('accepts decimal leverage', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['lev', '2.5'], ex));
    expect(ex.setLeverage).toHaveBeenCalledWith('BTCUSDT', 2.5);
  });

  it.each([['0'], ['-5'], ['-2.5'], ['5x'], ['2..5']])(
    'rejects malformed / non-positive leverage %s with a clear error',
    async (n) => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['lev', n], ex));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/must be a positive number/);
      expect(ex.setLeverage).not.toHaveBeenCalled();
    },
  );

  it("doesn't mistake `lev <numeric-garbage>` for a read", async () => {
    // Anything containing a digit reads as a set attempt. So
    // `lev -5` errors loudly instead of silently printing leverage.
    const ex = makeExchange({ positions: [pos({ leverage: 5 })] });
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['lev', '-5'], ex, adapter));
    expect(r.success).toBe(false);
    expect(adapter.printInfo).not.toHaveBeenCalledWith(expect.stringMatching(/5x/));
  });

  it('non-numeric words like `lev show` still do a read', async () => {
    // No digit → read path. Locks the contract that bare-word args
    // are treated as reads, not set attempts.
    const ex = makeExchange({ positions: [pos({ leverage: 5 })] });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['lev', 'show'], ex, adapter));
    expect(adapter.printInfo).toHaveBeenCalledWith('BTCUSDT: 5x');
    expect(ex.setLeverage).not.toHaveBeenCalled();
  });

  it('errors when exchange does not support setLeverage', async () => {
    const ex = makeExchange();
    delete (ex as unknown as { setLeverage?: unknown }).setLeverage;
    const r = await cmd.execute(ctx(['lev', '10'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/does not support/);
  });

  it('surfaces setLeverage errors as command errors', async () => {
    const ex = makeExchange();
    (ex.setLeverage as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('bad leverage'));
    const r = await cmd.execute(ctx(['lev', '10'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Failed to set leverage/);
  });

  it('aliases include "lev"', () => {
    expect(cmd.aliases).toContain('lev');
  });
});

// ─── MaxCommand ──────────────────────────────────────────────────────

describe('MaxCommand', () => {
  let cmd: MaxCommand;
  beforeEach(() => {
    cmd = new MaxCommand();
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(ctx(['max'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
  });

  it('errors when balance is unavailable', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['max'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Balance/);
  });

  it('errors when ticker price is unavailable', async () => {
    const ex = makeExchange({ balance: bal({ free: 1000 }) });
    ex.store!.tickers = [];
    const r = await cmd.execute(ctx(['max'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Price not available/);
  });

  it('uses leverage 1x when no position is open', async () => {
    const ex = makeExchange({ balance: bal({ free: 1000 }) });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['max'], ex, adapter));
    // 1000 USD * 1x = 1000 max notional. /50000 = 0.02 BTC.
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    expect(lines.find((l) => l.includes('Max notional'))).toMatch(/\$1000\.00/);
    expect(lines.find((l) => l.includes('Max contracts'))).toMatch(/0\.02/);
  });

  it('uses leverage from the open position when available', async () => {
    const ex = makeExchange({
      balance: bal({ free: 1000 }),
      positions: [pos({ leverage: 5 })],
    });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['max'], ex, adapter));
    // 1000 * 5x = 5000 max notional. /50000 = 0.1 BTC.
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    expect(lines.find((l) => l.includes('Max notional'))).toMatch(/\$5000\.00/);
    expect(lines.find((l) => l.includes('Max contracts'))).toMatch(/0\.1/);
  });

  it('flags the leverage source in the output', async () => {
    const ex = makeExchange({ balance: bal({ free: 1000 }) });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['max'], ex, adapter));
    const lines = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0] as string[];
    // Without a position, the output flags the assumption.
    expect(lines.find((l) => l.includes('Leverage'))).toMatch(/no open position/);
  });
});

// ─── NukeCommand ─────────────────────────────────────────────────────

describe('NukeCommand', () => {
  let cmd: NukeCommand;
  beforeEach(() => {
    cmd = new NukeCommand();
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(ctx(['nuke'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
  });

  it('"nuke" with no args calls nuke(3)', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['nuke'], ex));
    expect(ex.nuke).toHaveBeenCalledWith(3);
  });

  it('"nuke all" also calls nuke(3)', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['nuke', 'all'], ex));
    expect(ex.nuke).toHaveBeenCalledWith(3);
  });

  it.each([
    ['positions'],
    ['pos'],
  ])('"nuke %s" calls nukePositions(3)', async (variant) => {
    const ex = makeExchange();
    await cmd.execute(ctx(['nuke', variant], ex));
    expect(ex.nukePositions).toHaveBeenCalledWith(3);
  });

  it.each([
    ['orders'],
    ['ord'],
  ])('"nuke %s" calls nukeOrders()', async (variant) => {
    const ex = makeExchange();
    await cmd.execute(ctx(['nuke', variant], ex));
    expect(ex.nukeOrders).toHaveBeenCalled();
  });

  it.each([
    [['nuke'], 'nuke'],
    [['nuke', 'pos'], 'nukePositions'],
    [['nuke', 'ord'], 'nukeOrders'],
  ] as const)('"%s" prints the running algos it stopped first', async (args, method) => {
    const ex = makeExchange();
    (ex[method] as ReturnType<typeof vi.fn>).mockResolvedValue({ stoppedChasers: ['c1'], stoppedTwaps: ['t1', 't2'] });
    const adapter = makeAdapter();
    await cmd.execute(ctx([...args], ex, adapter));
    await vi.waitFor(() =>
      expect(adapter.printWarning).toHaveBeenCalledWith(
        'Nuke stopped 1 chaser(s) (c1) and 2 TWAP(s) (t1, t2) before closing',
      ),
    );
  });

  it('prints nothing more when the nuke stopped no algos', async () => {
    const ex = makeExchange();
    (ex.nuke as ReturnType<typeof vi.fn>).mockResolvedValue({ stoppedChasers: [], stoppedTwaps: [] });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['nuke'], ex, adapter));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(adapter.printWarning).not.toHaveBeenCalled();
  });

  it('errors on a malformed sub-command', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['nuke', 'nope'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/syntax/);
  });

  it.each([
    ['nuke', 'nuke'],
    ['nuke all', 'nuke'],
    ['nuke positions', 'nukePositions'],
    ['nuke orders', 'nukeOrders'],
  ])('"%s" errors when exchange lacks the matching method', async (input, methodName) => {
    const ex = makeExchange();
    delete (ex as unknown as Record<string, unknown>)[methodName];
    const r = await cmd.execute(ctx(input.split(' '), ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/does not support/);
  });

  it('logs a warning when nuke promise rejects (still returns success synchronously)', async () => {
    const ex = makeExchange();
    (ex.nuke as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('boom'));
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['nuke'], ex, adapter));
    // nuke is fire-and-forget — execute returns success and the
    // .catch() prints a warning asynchronously. Wait a microtask.
    expect(r.success).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(adapter.printWarning).toHaveBeenCalled();
  });
});

// ─── MoveCommand ─────────────────────────────────────────────────────

function order(o: Partial<Order> = {}): Order {
  return {
    id: o.id ?? 'order-abc-1234',
    symbol: o.symbol ?? 'BTCUSDT',
    side: o.side ?? OrderSide.Buy,
    type: o.type ?? OrderType.Limit,
    status: o.status ?? OrderStatus.Open,
    price: o.price ?? 49_000,
    amount: o.amount ?? 0.001,
    filled: o.filled ?? 0,
    remaining: o.remaining ?? 0.001,
    reduceOnly: o.reduceOnly ?? false,
  } as Order;
}

function exchangeWithOrders(orders: Order[]): ExchangeLike {
  const ex = makeExchange();
  (ex.store as { orders: Order[] }).orders = orders;
  (ex as unknown as { updateOrder: ReturnType<typeof vi.fn> }).updateOrder =
    vi.fn().mockResolvedValue(undefined);
  return ex;
}

describe('MoveCommand', () => {
  let cmd: MoveCommand;
  beforeEach(() => {
    cmd = new MoveCommand();
  });

  it('errors with a usage hint when args are missing', async () => {
    const ex = exchangeWithOrders([order()]);
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['move'], ex, adapter));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Missing arguments/);
    expect(adapter.printError).toHaveBeenCalledWith(
      expect.stringMatching(/Usage: move/),
    );
  });

  it('errors when the exchange does not support order updates', async () => {
    const ex = exchangeWithOrders([order()]);
    delete (ex as unknown as { updateOrder?: unknown }).updateOrder;
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['move', 'order-abc', '50000'], ex, adapter));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/updateOrder not supported/);
    expect(adapter.printError).toHaveBeenCalledWith(
      'This exchange does not support order updates',
    );
  });

  it('errors when no order matches the prefix', async () => {
    const ex = exchangeWithOrders([order({ id: 'order-xyz-9999' })]);
    const r = await cmd.execute(ctx(['move', 'order-abc', '50000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Order not found/);
  });

  it('errors when the prefix is ambiguous and lists candidates', async () => {
    const ex = exchangeWithOrders([
      order({ id: 'order-abc-1111', price: 49_000 }),
      order({ id: 'order-abc-2222', price: 48_500 }),
    ]);
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['move', 'order-abc', '50000'], ex, adapter));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Ambiguous/);
    expect(adapter.printError).toHaveBeenCalledWith(
      expect.stringMatching(/Multiple orders match/),
    );
    // Both matches surfaced.
    expect(adapter.printInfo).toHaveBeenCalledWith(
      expect.stringContaining('order-abc-1111'),
    );
    expect(adapter.printInfo).toHaveBeenCalledWith(
      expect.stringContaining('order-abc-2222'),
    );
  });

  it('skips orders that are not Open + Limit', async () => {
    const ex = exchangeWithOrders([
      order({ id: 'order-abc-1111', status: OrderStatus.Closed }),
      order({ id: 'order-abc-2222', type: OrderType.Market }),
    ]);
    const r = await cmd.execute(ctx(['move', 'order-abc', '50000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Order not found/);
  });

  it('moves a uniquely-matched order to an absolute price', async () => {
    const ex = exchangeWithOrders([order({ id: 'order-abc-1234', price: 49_000 })]);
    const r = await cmd.execute(ctx(['move', 'order-abc-1234', '50000'], ex));
    expect(r.success).toBe(true);
    expect(ex.updateOrder).toHaveBeenCalledWith({
      order: expect.objectContaining({ id: 'order-abc-1234' }),
      update: { price: 50000 },
    });
  });

  it('moves an order via prefix match when the prefix is unique', async () => {
    const ex = exchangeWithOrders([order({ id: 'order-abc-1234' })]);
    const r = await cmd.execute(ctx(['move', 'order-abc', '49500'], ex));
    expect(r.success).toBe(true);
    expect(ex.updateOrder).toHaveBeenCalledWith({
      order: expect.objectContaining({ id: 'order-abc-1234' }),
      update: { price: 49500 },
    });
  });

  it('handles +N relative offsets', async () => {
    const ex = exchangeWithOrders([order({ price: 49_000 })]);
    await cmd.execute(ctx(['move', 'order-abc-1234', '+250'], ex));
    expect(ex.updateOrder).toHaveBeenCalledWith({
      order: expect.objectContaining({ price: 49_000 }),
      update: { price: 49_250 },
    });
  });

  it('handles -N relative offsets', async () => {
    const ex = exchangeWithOrders([order({ price: 49_000 })]);
    await cmd.execute(ctx(['move', 'order-abc-1234', '-100'], ex));
    expect(ex.updateOrder).toHaveBeenCalledWith({
      order: expect.objectContaining({ price: 49_000 }),
      update: { price: 48_900 },
    });
  });

  it('rejects negative absolute prices', async () => {
    const ex = exchangeWithOrders([order({ price: 100 })]);
    // `-200` parses as a relative offset → 100 - 200 = -100, which fails
    // the `<= 0` guard with "Invalid price".
    const r = await cmd.execute(ctx(['move', 'order-abc-1234', '-200'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid price/);
    expect(ex.updateOrder).not.toHaveBeenCalled();
  });

  it('rejects malformed price tokens', async () => {
    const ex = exchangeWithOrders([order()]);
    const r = await cmd.execute(ctx(['move', 'order-abc-1234', 'banana'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid price/);
  });

  it('is a no-op when the order is already at the requested price', async () => {
    const ex = exchangeWithOrders([order({ price: 49_000 })]);
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['move', 'order-abc-1234', '49000'], ex, adapter));
    expect(r.success).toBe(true);
    expect(ex.updateOrder).not.toHaveBeenCalled();
    expect(adapter.printInfo).toHaveBeenCalledWith(
      'Order is already at this price',
    );
  });

  it('surfaces updateOrder errors as command errors', async () => {
    const ex = exchangeWithOrders([order()]);
    (ex.updateOrder as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('rejected by exchange'));
    const r = await cmd.execute(ctx(['move', 'order-abc-1234', '50000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/rejected by exchange/);
  });

  it('exposes "mv" as an alias', () => {
    expect(cmd.aliases).toContain('mv');
  });
});

// ─── EchoCommand ─────────────────────────────────────────────────────

describe('EchoCommand', () => {
  let cmd: EchoCommand;
  beforeEach(() => {
    cmd = new EchoCommand();
  });

  it('echoes joined args back through print', async () => {
    const adapter = makeAdapter();
    const ex = makeExchange();
    const r = await cmd.execute(
      ctx(['echo', 'hello', 'world'], ex, adapter),
    );
    expect(r.success).toBe(true);
    expect(adapter.print).toHaveBeenCalledWith(['hello world']);
  });

  it('echoes an empty string when called with no args', async () => {
    const adapter = makeAdapter();
    const ex = makeExchange();
    await cmd.execute(ctx(['echo'], ex, adapter));
    expect(adapter.print).toHaveBeenCalledWith(['']);
  });

  it('does not require a connection', () => {
    expect(cmd.requiresConnection).toBe(false);
  });

  it('exposes `say` as an alias (ichibot parity)', () => {
    expect(cmd.aliases).toContain('say');
  });
});

// ─── PositionCommand `show` subcommand ──────────────────────────────

describe('PositionCommand — `show` subcommand', () => {
  let cmd: PositionCommand;
  beforeEach(() => {
    cmd = new PositionCommand();
  });

  it('`position show` (no symbol) defaults to the focused symbol', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'BTCUSDT', notional: 100_000, contracts: 2 })],
    });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['position', 'show'], ex, adapter, 'BTCUSDT'));
    // Lines start with an empty line + a separator + the title.
    // The title line shape is `<SYMBOL>  <SIDE>` — assert it appears.
    const lines = adapter.print.mock.calls[0]?.[0] as string[];
    expect(lines.some((l) => l.includes('BTCUSDT'))).toBe(true);
  });

  it('`position show BTCUSDT` uses the named symbol', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'BTCUSDT', notional: 100_000, contracts: 2 })],
    });
    const adapter = makeAdapter();
    // Session focus is on ETHUSDT, but explicit symbol wins.
    await cmd.execute(ctx(['position', 'show', 'BTCUSDT'], ex, adapter, 'ETHUSDT'));
    const lines = adapter.print.mock.calls[0]?.[0] as string[];
    expect(lines.some((l) => l.includes('BTCUSDT'))).toBe(true);
  });

  it('`position BTCUSDT` (no `show`) still works (back-compat)', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'BTCUSDT', notional: 100_000, contracts: 2 })],
    });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['position', 'BTCUSDT'], ex, adapter, 'ETHUSDT'));
    const lines = adapter.print.mock.calls[0]?.[0] as string[];
    expect(lines.some((l) => l.includes('BTCUSDT'))).toBe(true);
  });

  it('`position show` is case-insensitive on the keyword', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'BTCUSDT', notional: 100_000, contracts: 2 })],
    });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['position', 'SHOW'], ex, adapter, 'BTCUSDT'));
    const lines = adapter.print.mock.calls[0]?.[0] as string[];
    expect(lines.some((l) => l.includes('BTCUSDT'))).toBe(true);
  });

  it('`position show <unknown>` reports no position (same path as plain symbol)', async () => {
    const ex = makeExchange({ positions: [] });
    const adapter = makeAdapter();
    await cmd.execute(ctx(['position', 'show', 'NOSYM'], ex, adapter, 'BTCUSDT'));
    expect(adapter.printInfo).toHaveBeenCalledWith(
      expect.stringMatching(/No open position/),
    );
  });
});

// ─── MarginCommand ───────────────────────────────────────────────────

describe('MarginCommand', () => {
  let cmd: MarginCommand;
  beforeEach(() => {
    cmd = new MarginCommand();
  });

  it('errors visibly when there is no balance data', async () => {
    const ex = makeExchange();
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['margin'], ex, adapter));
    // execute returns success even on the no-balance branch — the
    // command treats an unloaded balance as a soft state, not a fatal
    // error. Lock that contract in.
    expect(r.success).toBe(true);
    expect(adapter.printError).toHaveBeenCalledWith('No balance data available');
  });

  it('prints a margin table when balance is loaded', async () => {
    const ex = makeExchange({ balance: bal({ free: 1234, total: 1234 }) });
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['margin'], ex, adapter));
    expect(r.success).toBe(true);
    // print is called with a non-empty array — formatMarginTable returns
    // a string. We don't assert table contents because the formatter is
    // tested elsewhere; we only lock in the contract that print is
    // reached.
    expect(adapter.print).toHaveBeenCalled();
    const arg = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(Array.isArray(arg)).toBe(true);
    expect(arg.length).toBe(1);
    expect(typeof arg[0]).toBe('string');
  });

  it('exposes "balance" and "bal" as aliases', () => {
    expect(cmd.aliases).toEqual(expect.arrayContaining(['balance', 'bal']));
  });
});

// ─── PriceCommand variants ───────────────────────────────────────────

function exchangeWithTickers(
  tickers: Array<{ symbol: string; last?: number; mark?: number; index?: number }>,
): ExchangeLike {
  const ex = makeExchange();
  (ex.store as { tickers: typeof tickers }).tickers = tickers;
  (ex.store as { markets: unknown[] }).markets = tickers.map((t) => ({
    symbol: t.symbol,
    base: t.symbol.replace(/USDT$/, ''),
    quote: 'USDT',
    precision: { amount: 0.001, price: 0.01 },
  }));
  return ex;
}

describe('PriceCommand family', () => {
  it('LastPrice prints the focused symbol when no arg is given', async () => {
    const ex = exchangeWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
    const adapter = makeAdapter();
    const cmd = new LastPriceCommand();
    const r = await cmd.execute(ctx(['lp'], ex, adapter));
    expect(r.success).toBe(true);
    const out = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(out[0]).toMatch(/Last Price: BTCUSDT @ 50000/);
  });

  it('MarkPrice / IndexPrice resolve their own ticker fields', async () => {
    const ex = exchangeWithTickers([
      { symbol: 'BTCUSDT', last: 50_000, mark: 50_100, index: 50_050 },
    ]);
    const adapter = makeAdapter();
    await new MarkPriceCommand().execute(ctx(['mp'], ex, adapter));
    await new IndexPriceCommand().execute(ctx(['ip'], ex, adapter));
    const printed = (adapter.print as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0][0]);
    expect(printed[0]).toMatch(/Mark Price: BTCUSDT @ 50100/);
    expect(printed[1]).toMatch(/Index Price: BTCUSDT @ 50050/);
  });

  it('prints one line per comma-separated symbol', async () => {
    const ex = exchangeWithTickers([
      { symbol: 'BTCUSDT', last: 50_000 },
      { symbol: 'ETHUSDT', last: 3_000 },
    ]);
    const adapter = makeAdapter();
    const r = await new LastPriceCommand().execute(
      ctx(['lp', 'BTCUSDT,ETHUSDT'], ex, adapter),
    );
    expect(r.success).toBe(true);
    const out = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(out).toHaveLength(2);
    expect(out[0]).toMatch(/BTCUSDT @ 50000/);
    expect(out[1]).toMatch(/ETHUSDT @ 3000/);
  });

  it('reports a not-found message when the ticker exists but lacks the priceKey', async () => {
    // The "was not found" branch fires when the ticker exists (so it
    // passes validateSymbols) but the requested price field is missing
    // — e.g. a listing whose tickers haven't backfilled `mark` yet.
    const ex = exchangeWithTickers([
      { symbol: 'BTCUSDT', last: 50_000, mark: 50_100 },
      { symbol: 'ETHUSDT', last: 3_000 /* no mark */ },
    ]);
    const adapter = makeAdapter();
    await new MarkPriceCommand().execute(
      ctx(['mp', 'BTCUSDT,ETHUSDT'], ex, adapter),
    );
    const out = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(out[0]).toMatch(/BTCUSDT @ 50100/);
    expect(out[1]).toMatch(/ETHUSDT was not found or is not loaded yet/);
  });

  it('drops symbols that are not in the ticker list (validateSymbols filter)', async () => {
    // Sanity / regression — validateSymbols filters by ticker
    // membership before transformSymbol's prefix-matching has a chance
    // to fire. So a symbol that doesn't have a ticker doesn't even
    // reach the "was not found" branch; it just disappears.
    const ex = exchangeWithTickers([{ symbol: 'BTCUSDT', last: 50_000 }]);
    const adapter = makeAdapter();
    await new LastPriceCommand().execute(
      ctx(['lp', 'BTCUSDT,DOGEUSDT'], ex, adapter),
    );
    const out = (adapter.print as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(out).toHaveLength(1);
    expect(out[0]).toMatch(/BTCUSDT @ 50000/);
  });

  it('errors when not connected', async () => {
    const cmd = new LastPriceCommand();
    const r = await cmd.execute(ctx(['lp'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
  });
});
