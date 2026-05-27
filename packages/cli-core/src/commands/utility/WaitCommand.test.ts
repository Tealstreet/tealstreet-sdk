/**
 * Unit tests for the WaitCommand. The command polls ctx.exchange.store
 * every 100ms with a configurable timeout, so these tests use vitest
 * fake timers to advance time deterministically.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { WaitCommand } from './WaitCommand';
import type { CommandContext } from '../Command';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';
import {
  PositionSide,
  MarginType,
  PositionMode,
  type Position,
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

function makeAdapter(): CLIPlatformAdapter {
  return {
    print: vi.fn(),
    printInfo: vi.fn(),
    printError: vi.fn(),
    printSuccess: vi.fn(),
    printWarning: vi.fn(),
  } as unknown as CLIPlatformAdapter;
}

function makeExchange(opts: {
  positions?: Position[];
  tickers?: Array<{
    symbol: string;
    last?: number;
    mark?: number;
    index?: number;
    bid?: number;
    ask?: number;
    fundingRate?: number;
  }>;
  orders?: Array<{ symbol: string; status: string }>;
} = {}): ExchangeLike {
  return {
    name: 'test',
    store: {
      markets: [{ symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } }],
      tickers: opts.tickers ?? [],
      orders: opts.orders ?? [],
      positions: opts.positions ?? [],
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
  } as unknown as ExchangeLike;
}

function ctx(
  args: string[],
  exchange: ExchangeLike | null,
  adapter?: CLIPlatformAdapter,
  symbol = 'BTCUSDT',
  preprocessStore?: MemoryPreprocessStore,
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'a',
    exchange,
    adapter: adapter ?? makeAdapter(),
    preprocessStore,
  } as unknown as CommandContext;
}

describe('WaitCommand — duration form', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('"wait 5s" sleeps for 5 seconds', async () => {
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', '5s'], ex));
    // Time hasn't advanced — still pending.
    await vi.advanceTimersByTimeAsync(4_999);
    let resolved = false;
    promise.then(() => {
      resolved = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(resolved).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('"wait 250ms" accepts ms', async () => {
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', '250ms'], ex));
    await vi.advanceTimersByTimeAsync(250);
    const r = await promise;
    expect(r.success).toBe(true);
  });
});

describe('WaitCommand — error handling', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
  });

  it('errors when no condition is provided', async () => {
    const adapter = makeAdapter();
    const r = await cmd.execute(ctx(['wait'], null, adapter));
    expect(r.success).toBe(false);
    expect(adapter.printError).toHaveBeenCalledWith(expect.stringMatching(/Usage: wait/));
  });

  it('errors when not connected and condition needs the exchange', async () => {
    const r = await cmd.execute(ctx(['wait', 'position'], null));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/not connected/);
  });

  it('errors on unrecognized condition', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['wait', 'lol'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid condition/);
  });

  it('errors on invalid timeout value', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['wait', 'timeout=banana', 'position'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid timeout/);
  });
});

describe('WaitCommand — position / long / short', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns immediately when position already exists', async () => {
    const ex = makeExchange({
      positions: [pos({ contracts: 1 })],
    });
    const r = await cmd.execute(ctx(['wait', 'position'], ex));
    expect(r.success).toBe(true);
  });

  it('polls until a position appears', async () => {
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', 'position'], ex));
    // After 200ms (2 polls) — still no position.
    await vi.advanceTimersByTimeAsync(200);
    // Add a position. Next poll picks it up.
    (ex.store as { positions: Position[] }).positions = [pos({ contracts: 1 })];
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('"wait long" only releases on a long-side position', async () => {
    const ex = makeExchange({
      positions: [pos({ side: PositionSide.Short, contracts: 1 })],
    });
    const promise = cmd.execute(ctx(['wait', 'long'], ex));
    // Won't fire on the short.
    await vi.advanceTimersByTimeAsync(500);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);

    (ex.store as { positions: Position[] }).positions = [
      pos({ side: PositionSide.Short, contracts: 1 }),
      pos({ id: 'l', side: PositionSide.Long, contracts: 0.5 }),
    ];
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('symbol arg overrides focused symbol', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'ETHUSDT', contracts: 1 })],
    });
    // Focused on BTCUSDT, but `wait position ETHUSDT` checks ETH.
    const r = await cmd.execute(ctx(['wait', 'position', 'ETHUSDT'], ex));
    expect(r.success).toBe(true);
  });

  it('zero-contract positions don\'t satisfy', async () => {
    const ex = makeExchange({
      positions: [pos({ contracts: 0 })],
    });
    const promise = cmd.execute(ctx(['wait', 'position', 'timeout=300ms'], ex));
    await vi.advanceTimersByTimeAsync(400);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/wait timed out/);
  });
});

describe('WaitCommand — time HHMM (UTC clock)', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('waits until the next HHMM UTC', async () => {
    // Pin system time to 2026-05-10T10:00:00.000Z. Target 12:00 UTC
    // is 2 hours away.
    vi.setSystemTime(new Date('2026-05-10T10:00:00.000Z'));
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', 'time', '1200'], ex));
    // Just under 2h — still pending.
    await vi.advanceTimersByTimeAsync(2 * 60 * 60_000 - 1);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('schedules tomorrow when HHMM has already passed today', async () => {
    vi.setSystemTime(new Date('2026-05-10T14:00:00.000Z'));
    const ex = makeExchange();
    // Target 12:00 UTC — already 2h ago today, so it should fire at
    // tomorrow's 12:00 UTC (22 hours from now).
    const promise = cmd.execute(ctx(['wait', 'time', '1200'], ex));
    await vi.advanceTimersByTimeAsync(22 * 60 * 60_000 - 1);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('schedules ~24h ahead when HHMM matches "now" exactly', async () => {
    vi.setSystemTime(new Date('2026-05-10T12:00:00.000Z'));
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', 'time', '1200'], ex));
    // Just under 24h.
    await vi.advanceTimersByTimeAsync(24 * 60 * 60_000 - 1);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it.each(['12345', '1', 'abcd', '2400', '1260', '-100'])(
    'rejects malformed HHMM input "%s"',
    async (val) => {
      const ex = makeExchange();
      const r = await cmd.execute(ctx(['wait', 'time', val], ex));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/Invalid time/);
    },
  );

  it('rejects when HHMM arg is missing', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['wait', 'time'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid time/);
  });
});

describe('WaitCommand — anyposition', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns immediately when any non-zero position exists', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'ETHUSDT', contracts: 1 })],
    });
    // Focused on BTCUSDT, but ETH position satisfies anyposition.
    const r = await cmd.execute(ctx(['wait', 'anyposition'], ex));
    expect(r.success).toBe(true);
  });

  it('polls until a position appears on any symbol', async () => {
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', 'anyposition'], ex));
    await vi.advanceTimersByTimeAsync(200);
    (ex.store as { positions: Position[] }).positions = [
      pos({ symbol: 'SOLUSDT', contracts: 5 }),
    ];
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('treats zero-contract positions as not-a-position', async () => {
    const ex = makeExchange({
      positions: [pos({ symbol: 'ETHUSDT', contracts: 0 })],
    });
    const promise = cmd.execute(ctx(['wait', 'anyposition', 'timeout=300ms'], ex));
    await vi.advanceTimersByTimeAsync(400);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/wait timed out/);
  });

  it('ignores symbol argument — anyposition is global by definition', async () => {
    // Even when args[1] looks like a symbol, the global semantic
    // wins. (Documents the contract — adding `anyposition <SYMBOL>`
    // would just be redundant with `position <SYMBOL>`.)
    const ex = makeExchange({
      positions: [pos({ symbol: 'ETHUSDT', contracts: 1 })],
    });
    const r = await cmd.execute(ctx(['wait', 'anyposition', 'BTCUSDT'], ex));
    expect(r.success).toBe(true);
  });
});

describe('WaitCommand — price comparators', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it.each([
    ['>', 49_000, 50_000, true],
    ['>', 50_000, 50_000, false],
    ['>=', 50_000, 50_000, true],
    ['<', 51_000, 50_000, true],
    ['<=', 50_000, 50_000, true],
    ['<', 50_000, 50_000, false],
  ])('"wait price %s %s" against last=%i → already satisfied=%s', async (op, target, last, sat) => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last }],
    });
    if (sat) {
      const r = await cmd.execute(ctx(['wait', 'price', op, String(target)], ex));
      expect(r.success).toBe(true);
    } else {
      // Use a tight timeout to avoid hanging.
      const promise = cmd.execute(
        ctx(['wait', 'price', op, String(target), 'timeout=200ms'], ex),
      );
      await vi.advanceTimersByTimeAsync(300);
      const r = await promise;
      expect(r.success).toBe(false);
    }
  });

  it('polls until ticker.last crosses the threshold', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 49_900 }],
    });
    const promise = cmd.execute(ctx(['wait', 'price', '>', '50000'], ex));
    await vi.advanceTimersByTimeAsync(300);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);

    (ex.store!.tickers as { symbol: string; last: number }[])[0].last = 50_001;
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('returns false if the ticker is missing for the symbol', async () => {
    const ex = makeExchange({ tickers: [] });
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '50000', 'timeout=200ms'], ex),
    );
    await vi.advanceTimersByTimeAsync(300);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/timed out/);
  });
});

describe('WaitCommand — mark / index price', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('"wait mark > N" reads ticker.mark', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, mark: 50_500, index: 49_900 }],
    });
    const r = await cmd.execute(ctx(['wait', 'mark', '>', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('"wait index < N" reads ticker.index', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, mark: 50_500, index: 49_900 }],
    });
    const r = await cmd.execute(ctx(['wait', 'index', '<', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('"wait mark" vs "wait price" diverge when mark and last differ', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 49_000, mark: 51_000, index: 50_000 }],
    });
    // mark already > 50_000 → satisfied.
    const rMark = await cmd.execute(ctx(['wait', 'mark', '>', '50000'], ex));
    expect(rMark.success).toBe(true);
    // last is 49_000 → NOT satisfied within tight timeout.
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '50000', 'timeout=200ms'], ex),
    );
    await vi.advanceTimersByTimeAsync(300);
    const rPrice = await promise;
    expect(rPrice.success).toBe(false);
  });

  it('polls until the ticker field crosses the threshold', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, mark: 49_900, index: 49_900 }],
    });
    const promise = cmd.execute(ctx(['wait', 'mark', '>=', '50000'], ex));
    await vi.advanceTimersByTimeAsync(200);
    (ex.store!.tickers as { mark: number }[])[0].mark = 50_001;
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('symbol arg overrides focused symbol for mark/index conditions', async () => {
    const ex = makeExchange({
      tickers: [
        { symbol: 'BTCUSDT', last: 50_000, mark: 50_000, index: 50_000 },
        { symbol: 'ETHUSDT', last: 3_000, mark: 3_010, index: 2_990 },
      ],
    });
    const r = await cmd.execute(ctx(['wait', 'mark', '>', '3005', 'ETHUSDT'], ex));
    expect(r.success).toBe(true);
  });

  it('returns false if the requested field is missing on the ticker', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
    });
    // No mark field set — condition stays false until timeout.
    const promise = cmd.execute(
      ctx(['wait', 'mark', '>', '40000', 'timeout=200ms'], ex),
    );
    await vi.advanceTimersByTimeAsync(300);
    const r = await promise;
    expect(r.success).toBe(false);
  });
});

describe('WaitCommand — fundingrate / premium', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('"wait fundingrate > N" reads ticker.fundingRate', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, fundingRate: 0.0005 }],
    });
    const r = await cmd.execute(ctx(['wait', 'fundingrate', '>', '0.0003'], ex));
    expect(r.success).toBe(true);
  });

  it('"wait fundingrate < N" polls until rate drops below', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, fundingRate: 0.001 }],
    });
    const promise = cmd.execute(ctx(['wait', 'fundingrate', '<', '0.0005'], ex));
    await vi.advanceTimersByTimeAsync(200);
    (ex.store!.tickers as { fundingRate: number }[])[0].fundingRate = 0.0001;
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('negative funding rates are honoured (e.g. wait < -0.001)', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, fundingRate: -0.002 }],
    });
    const r = await cmd.execute(ctx(['wait', 'fundingrate', '<', '-0.001'], ex));
    expect(r.success).toBe(true);
  });

  it('"wait premium > N" computes mark - index', async () => {
    const ex = makeExchange({
      tickers: [
        { symbol: 'BTCUSDT', last: 50_000, mark: 50_100, index: 50_000 },
      ],
    });
    // mark - index = 100 → > 50 is true.
    const r = await cmd.execute(ctx(['wait', 'premium', '>', '50'], ex));
    expect(r.success).toBe(true);
  });

  it('"wait premium" returns false when mark or index missing', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, mark: 50_100 }],
    });
    const promise = cmd.execute(
      ctx(['wait', 'premium', '>', '50', 'timeout=200ms'], ex),
    );
    await vi.advanceTimersByTimeAsync(300);
    const r = await promise;
    expect(r.success).toBe(false);
  });

  it('symbol arg overrides focused symbol', async () => {
    const ex = makeExchange({
      tickers: [
        { symbol: 'BTCUSDT', last: 50_000, fundingRate: 0 },
        { symbol: 'ETHUSDT', last: 3_000, fundingRate: 0.005 },
      ],
    });
    const r = await cmd.execute(
      ctx(['wait', 'fundingrate', '>', '0.003', 'ETHUSDT'], ex),
    );
    expect(r.success).toBe(true);
  });

  it('returns false if the ticker is missing', async () => {
    const ex = makeExchange({ tickers: [] });
    const promise = cmd.execute(
      ctx(['wait', 'fundingrate', '>', '0', 'timeout=200ms'], ex),
    );
    await vi.advanceTimersByTimeAsync(300);
    const r = await promise;
    expect(r.success).toBe(false);
  });
});

describe('WaitCommand — sustained `for=<dur>`', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('releases only after condition holds for the full sustain duration', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
    });
    // Condition is already true (50_000 > 49_000). With for=500ms,
    // we should NOT release immediately.
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '49000', 'for=500ms'], ex),
    );
    // After 400ms condition still true but sustain not yet elapsed.
    await vi.advanceTimersByTimeAsync(400);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    // After 100 more ms, sustain elapsed → release.
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('resets the sustain timer when condition goes false', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
    });
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '49500', 'for=400ms', 'timeout=2s'], ex),
    );
    // 300ms in — sustain not yet met.
    await vi.advanceTimersByTimeAsync(300);
    // Flip the price below threshold — sustain resets.
    (ex.store!.tickers as { last: number }[])[0].last = 49_000;
    await vi.advanceTimersByTimeAsync(200);
    // Flip back above — sustain restarts from zero.
    (ex.store!.tickers as { last: number }[])[0].last = 50_000;
    // 300ms back-above — still not enough.
    await vi.advanceTimersByTimeAsync(300);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    // 200 more → cumulative 500 above-threshold, which is past 400ms.
    await vi.advanceTimersByTimeAsync(200);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('times out if condition never sustains long enough', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
    });
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '49000', 'for=2s', 'timeout=500ms'], ex),
    );
    await vi.advanceTimersByTimeAsync(600);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/sustain 2000ms/);
  });

  it('for=0ms is rejected as invalid', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['wait', 'position', 'for=0ms'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid for/);
  });

  it('for=<huge> rejected (over 10-minute cap)', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['wait', 'position', 'for=20m'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid for/);
  });

  it('for + timeout compose — long sustain inside long timeout works', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
    });
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '49000', 'for=1s', 'timeout=5s'], ex),
    );
    await vi.advanceTimersByTimeAsync(1_100);
    const r = await promise;
    expect(r.success).toBe(true);
  });
});

describe('WaitCommand — `wait price` reads bid/ask side-aware', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('`wait price > N` reads ASK (upside trigger fires sooner)', async () => {
    // bid 49_995, ask 50_005, last 49_999.
    // Target > 50_000: ask (50_005) crosses; last (49_999) wouldn't.
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 49_999, bid: 49_995, ask: 50_005 }],
    });
    const r = await cmd.execute(ctx(['wait', 'price', '>', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('`wait price < N` reads BID (downside trigger fires sooner)', async () => {
    // bid 49_995, ask 50_005, last 50_001.
    // Target < 50_000: bid (49_995) is below; last (50_001) isn't.
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_001, bid: 49_995, ask: 50_005 }],
    });
    const r = await cmd.execute(ctx(['wait', 'price', '<', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('`>=` and `<=` follow the same side rule as `>` / `<`', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, bid: 49_995, ask: 50_005 }],
    });
    // 50_005 >= 50_005 → true via ask.
    const r1 = await cmd.execute(ctx(['wait', 'price', '>=', '50005'], ex));
    expect(r1.success).toBe(true);
    // 49_995 <= 49_995 → true via bid.
    const r2 = await cmd.execute(ctx(['wait', 'price', '<=', '49995'], ex));
    expect(r2.success).toBe(true);
  });

  it('falls back to `last` when bid/ask is missing', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_005 }],
    });
    // No bid, no ask — falls back to last (50_005 > 50_000).
    const r = await cmd.execute(ctx(['wait', 'price', '>', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('upside fallback uses last when ASK is missing only', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_005, bid: 49_995 }],
    });
    // No ask field — falls back to last (50_005 > 50_000).
    const r = await cmd.execute(ctx(['wait', 'price', '>', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('downside fallback uses last when BID is missing only', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 49_995, ask: 50_005 }],
    });
    // No bid field — falls back to last (49_995 < 50_000).
    const r = await cmd.execute(ctx(['wait', 'price', '<', '50000'], ex));
    expect(r.success).toBe(true);
  });

  it('== and != comparators on price conditions', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, bid: 49_995, ask: 50_005 }],
    });
    // last is 50_000 exactly → == 50000 is satisfied.
    expect((await cmd.execute(ctx(['wait', 'price', '==', '50000'], ex))).success).toBe(true);
    // != 50000 is false → times out.
    const ne = cmd.execute(ctx(['wait', 'price', '!=', '50000', 'timeout=200ms'], ex));
    await vi.advanceTimersByTimeAsync(300);
    expect((await ne).success).toBe(false);
  });

  it('== reads `last` even on price (not bid/ask) since direction doesn\'t apply', async () => {
    // Side-aware reading only kicks in for > / >= / < / <=. For == / !=,
    // we fall through to `last` (the canonical price). Lock that in.
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, bid: 49_900, ask: 50_100 }],
    });
    // ask (50_100) != 50_000 BUT last (50_000) == 50_000 → satisfied.
    expect((await cmd.execute(ctx(['wait', 'price', '==', '50000'], ex))).success).toBe(true);
  });

  it('wait mark / index are NOT side-aware — they always read their own field', async () => {
    // mark 50_100, bid 49_995, ask 50_005.
    // For `wait mark > 50050`, mark (50_100) crosses regardless of ask.
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, bid: 49_995, ask: 50_005, mark: 50_100, index: 49_900 }],
    });
    const rMark = await cmd.execute(ctx(['wait', 'mark', '>', '50050'], ex));
    expect(rMark.success).toBe(true);
    // For `wait index < 49950`, index (49_900) is below regardless of bid.
    const rIdx = await cmd.execute(ctx(['wait', 'index', '<', '49950'], ex));
    expect(rIdx.success).toBe(true);
  });
});

describe('WaitCommand — either / both combinators', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('`either` releases when the first sub-condition is true', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_001 }],
      positions: [],
    });
    // ask > 50000 is true. pnl side is false (no position).
    // Should release immediately.
    const r = await cmd.execute(
      ctx(['wait', 'either', '(price', '>', '50000)', 'or', '(pnl', '>', '100)'], ex),
    );
    expect(r.success).toBe(true);
  });

  it('`either` releases when the second sub-condition becomes true', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 49_000, ask: 49_005 }],
      positions: [pos({ contracts: 1, unrealizedPnl: 50 })],
    });
    // Neither true initially (price < 50000, pnl 50 not > 200).
    const promise = cmd.execute(
      ctx(['wait', 'either', '(price', '>', '50000)', 'or', '(pnl', '>', '200)'], ex),
    );
    await vi.advanceTimersByTimeAsync(200);
    // Bump pnl past 200 → second sub-condition triggers.
    (ex.store!.positions as Position[])[0] = pos({
      contracts: 1,
      unrealizedPnl: 250,
    });
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('`both` requires every sub-condition to be true', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_001 }],
      positions: [pos({ contracts: 1, unrealizedPnl: 50 })],
    });
    // price > 50000 true; pnl > 100 false (50 < 100). Should NOT release.
    const promise = cmd.execute(
      ctx(
        ['wait', 'both', '(price', '>', '50000)', 'and', '(pnl', '>', '100)', 'timeout=300ms'],
        ex,
      ),
    );
    await vi.advanceTimersByTimeAsync(400);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/timed out/);
  });

  it('`both` releases when the last false sub-condition turns true', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_001 }],
      positions: [pos({ contracts: 1, unrealizedPnl: 50 })],
    });
    const promise = cmd.execute(
      ctx(
        ['wait', 'both', '(price', '>', '50000)', 'and', '(pnl', '>', '100)', 'timeout=2s'],
        ex,
      ),
    );
    await vi.advanceTimersByTimeAsync(300);
    (ex.store!.positions as Position[])[0] = pos({
      contracts: 1,
      unrealizedPnl: 150,
    });
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('three-way `either` releases on any', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_001 }],
      positions: [pos({ contracts: 1 })],
    });
    // position is true; the others are false (price hasn't crossed,
    // pnl is 0 not > 100). Should release immediately.
    const r = await cmd.execute(
      ctx(
        [
          'wait',
          'either',
          '(price',
          '>',
          '60000)',
          'or',
          '(pnl',
          '>',
          '100)',
          'or',
          '(position)',
        ],
        ex,
      ),
    );
    expect(r.success).toBe(true);
  });

  it('nested combinators — both (either (...) or (...)) and (...)', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_001 }],
      positions: [pos({ contracts: 1 })],
    });
    // Inner either: price > 50000 (true) OR pnl > 100 (false) → true.
    // Outer both: <inner> AND position → true.
    const r = await cmd.execute(
      ctx(
        [
          'wait',
          'both',
          '(either',
          '(price',
          '>',
          '50000)',
          'or',
          '(pnl',
          '>',
          '100))',
          'and',
          '(position)',
        ],
        ex,
      ),
    );
    expect(r.success).toBe(true);
  });

  it('rejects unmatched parens', async () => {
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
    const r = await cmd.execute(
      ctx(['wait', 'either', '(price', '>', '50000', 'or', '(pnl', '>', '100)'], ex),
    );
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid condition/);
  });

  it('rejects when the connecting keyword is wrong (`and` inside an `either`)', async () => {
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
    const r = await cmd.execute(
      ctx(
        ['wait', 'either', '(price', '>', '50000)', 'and', '(pnl', '>', '100)'],
        ex,
      ),
    );
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid condition/);
  });

  it('rejects a single-sub combinator (needs >=2)', async () => {
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
    const r = await cmd.execute(ctx(['wait', 'either', '(price', '>', '50000)'], ex));
    expect(r.success).toBe(false);
  });

  it('combinators compose with `timeout=` and `for=`', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_001 }],
    });
    // price > 50000 is true via ask. With for=200ms, should release
    // after the sustain elapses.
    const promise = cmd.execute(
      ctx(
        [
          'wait',
          'either',
          '(price',
          '>',
          '50000)',
          'or',
          '(pnl',
          '>',
          '100)',
          'for=200ms',
          'timeout=5s',
        ],
        ex,
      ),
    );
    await vi.advanceTimersByTimeAsync(100);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
  });
});

describe('WaitCommand — fill', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns immediately when there are no live orders on the symbol', async () => {
    const ex = makeExchange({ orders: [] });
    const r = await cmd.execute(ctx(['wait', 'fill'], ex));
    expect(r.success).toBe(true);
  });

  it('treats `closed` and `filled` as not live', async () => {
    const ex = makeExchange({
      orders: [
        { symbol: 'BTCUSDT', status: 'filled' },
        { symbol: 'BTCUSDT', status: 'cancelled' },
      ],
    });
    const r = await cmd.execute(ctx(['wait', 'fill'], ex));
    expect(r.success).toBe(true);
  });

  it('waits while there is at least one open order', async () => {
    const ex = makeExchange({
      orders: [{ symbol: 'BTCUSDT', status: 'open' }],
    });
    const promise = cmd.execute(ctx(['wait', 'fill'], ex));
    await vi.advanceTimersByTimeAsync(300);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);

    (ex.store!.orders as { symbol: string; status: string }[])[0].status = 'filled';
    await vi.advanceTimersByTimeAsync(100);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('respects the symbol filter', async () => {
    const ex = makeExchange({
      orders: [{ symbol: 'ETHUSDT', status: 'open' }],
    });
    // Focused BTCUSDT — the ETHUSDT open order doesn't block.
    const r = await cmd.execute(ctx(['wait', 'fill'], ex));
    expect(r.success).toBe(true);
  });

  describe('per-order ID — `wait fill <id-or-list>`', () => {
    it('releases when the named order is gone (filled or cancelled)', async () => {
      const ex = makeExchange({
        orders: [
          { symbol: 'BTCUSDT', status: 'filled' } as unknown as Parameters<typeof makeExchange>[0]['orders'] extends infer T ? T extends Array<infer U> ? U : never : never,
        ] as never,
      });
      // Use a manual order shape with an id field.
      (ex.store!.orders as unknown as Array<{ id: string; symbol: string; status: string }>) = [
        { id: 'order-abc-123', symbol: 'BTCUSDT', status: 'filled' },
      ];
      const r = await cmd.execute(ctx(['wait', 'fill', 'order-abc-123'], ex));
      expect(r.success).toBe(true);
    });

    it('matches by trailing-id suffix (like cancel <id>)', async () => {
      const ex = makeExchange();
      (ex.store!.orders as unknown as Array<{ id: string; symbol: string; status: string }>) = [
        { id: 'order-xyz-9999', symbol: 'BTCUSDT', status: 'filled' },
      ];
      const r = await cmd.execute(ctx(['wait', 'fill', '9999'], ex));
      expect(r.success).toBe(true);
    });

    it('waits while the named order is still live', async () => {
      const ex = makeExchange();
      const orders = [
        { id: 'order-abc-123', symbol: 'BTCUSDT', status: 'open' },
      ];
      (ex.store!.orders as unknown as typeof orders) = orders;
      const promise = cmd.execute(ctx(['wait', 'fill', 'order-abc-123'], ex));
      await vi.advanceTimersByTimeAsync(300);
      let done = false;
      promise.then(() => {
        done = true;
      });
      await vi.advanceTimersByTimeAsync(0);
      expect(done).toBe(false);
      orders[0].status = 'filled';
      await vi.advanceTimersByTimeAsync(100);
      const r = await promise;
      expect(r.success).toBe(true);
    });

    it('comma-list — releases when ALL named orders are gone (bracket-capture pattern)', async () => {
      const ex = makeExchange();
      const orders = [
        { id: 'entry-1', symbol: 'BTCUSDT', status: 'filled' },
        { id: 'sl-2', symbol: 'BTCUSDT', status: 'open' },
      ];
      (ex.store!.orders as unknown as typeof orders) = orders;
      const promise = cmd.execute(
        ctx(['wait', 'fill', 'entry-1,sl-2', 'timeout=1s'], ex),
      );
      await vi.advanceTimersByTimeAsync(150);
      let done = false;
      promise.then(() => {
        done = true;
      });
      await vi.advanceTimersByTimeAsync(0);
      // sl-2 still open → wait still blocks.
      expect(done).toBe(false);
      orders[1].status = 'cancelled';
      await vi.advanceTimersByTimeAsync(150);
      const r = await promise;
      expect(r.success).toBe(true);
    });

    it('comma-list with a non-matching id releases on first poll', async () => {
      // If neither id matches any LIVE order, the wait releases
      // immediately. Useful for the `cancel @x; wait fill @x` pattern
      // where we want to confirm cancellation completed.
      const ex = makeExchange();
      (ex.store!.orders as unknown as Array<{ id: string; symbol: string; status: string }>) = [];
      const r = await cmd.execute(ctx(['wait', 'fill', 'nomatch-123'], ex));
      expect(r.success).toBe(true);
    });

    it('known symbol takes precedence over ID-shape (backwards compat)', async () => {
      // Arg "BTCUSDT" matches a known market — wait scopes to all
      // orders on BTCUSDT regardless of id. Locks the cancel-style
      // precedence in.
      const ex = makeExchange();
      (ex.store!.orders as unknown as Array<{ id: string; symbol: string; status: string }>) = [
        { id: 'BTCUSDT', symbol: 'ETHUSDT', status: 'open' },
      ];
      // Order's ID happens to equal "BTCUSDT" but it's on ETHUSDT.
      // Symbol-precedence: wait fill BTCUSDT scopes to BTCUSDT
      // orders, which is empty → release immediately.
      const r = await cmd.execute(ctx(['wait', 'fill', 'BTCUSDT'], ex));
      expect(r.success).toBe(true);
    });

    it('multi-symbol — finds orders on any symbol matching the id', async () => {
      // Capture syntax allows multi-symbol fan-out (`@grp = buy
      // BTCUSDT,ETHUSDT $100`). `wait fill @grp` then waits for
      // both legs regardless of their symbol.
      const ex = makeExchange();
      const orders = [
        { id: 'btc-1', symbol: 'BTCUSDT', status: 'filled' },
        { id: 'eth-1', symbol: 'ETHUSDT', status: 'open' },
      ];
      (ex.store!.orders as unknown as typeof orders) = orders;
      const promise = cmd.execute(
        ctx(['wait', 'fill', 'btc-1,eth-1', 'timeout=1s'], ex),
      );
      await vi.advanceTimersByTimeAsync(150);
      orders[1].status = 'filled';
      await vi.advanceTimersByTimeAsync(150);
      const r = await promise;
      expect(r.success).toBe(true);
    });
  });
});

describe('WaitCommand — timeout', () => {
  let cmd: WaitCommand;
  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('uses default 30s timeout when none given', async () => {
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', 'position'], ex));
    await vi.advanceTimersByTimeAsync(29_900);
    let done = false;
    promise.then(() => {
      done = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(200);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/wait timed out \(30000ms\)/);
  });

  it('honours `timeout=<dur>` override', async () => {
    const ex = makeExchange();
    const promise = cmd.execute(ctx(['wait', 'position', 'timeout=500ms'], ex));
    await vi.advanceTimersByTimeAsync(600);
    const r = await promise;
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/wait timed out \(500ms\)/);
  });

  it('rejects a timeout above the 10-minute cap', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['wait', 'position', 'timeout=20m'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Invalid timeout/);
  });
});

describe('WaitCommand — reactive (@<liveVar>) conditions', () => {
  let cmd: WaitCommand;

  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('immediate satisfy — live-evaluated @x already greater than target', async () => {
    const ex = makeExchange({
      tickers: [{ symbol: 'BTCUSDT', last: 50_000 }],
    });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('x', '$last');
    const r = await cmd.execute(ctx(['wait', '@x', '>', '40000'], ex, undefined, 'BTCUSDT', store));
    expect(r.success).toBe(true);
  });

  it('reactive polling — @x value crosses the threshold during the wait', async () => {
    const tickers = [{ symbol: 'BTCUSDT', last: 100 }];
    const ex = makeExchange({ tickers });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('x', '$last');
    // Start: @x = 100, target = 150. Should NOT satisfy yet.
    const promise = cmd.execute(
      ctx(['wait', '@x', '>', '150', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    // After 200ms still under; bump to 160 — should satisfy on next poll.
    await vi.advanceTimersByTimeAsync(200);
    tickers[0].last = 160;
    await vi.advanceTimersByTimeAsync(200);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('reactive polling — math expression source', async () => {
    const tickers = [{ symbol: 'BTCUSDT', bid: 100, ask: 110 }];
    const ex = makeExchange({ tickers });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('spread', '$ask-$bid');
    // spread starts at 10. Wait for it to exceed 20.
    const promise = cmd.execute(
      ctx(['wait', '@spread', '>', '20', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    await vi.advanceTimersByTimeAsync(200);
    tickers[0].ask = 130; // spread now 30
    await vi.advanceTimersByTimeAsync(200);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('all comparators work on @x LHS', async () => {
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50 }] });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('x', '$last');
    const ctxArgs = (cmpArgs: string[]) =>
      ctx(['wait', '@x', ...cmpArgs], ex, undefined, 'BTCUSDT', store);
    // 50 > 10
    expect((await cmd.execute(ctxArgs(['>', '10']))).success).toBe(true);
    // 50 < 100
    expect((await cmd.execute(ctxArgs(['<', '100']))).success).toBe(true);
    // 50 >= 50
    expect((await cmd.execute(ctxArgs(['>=', '50']))).success).toBe(true);
    // 50 <= 50
    expect((await cmd.execute(ctxArgs(['<=', '50']))).success).toBe(true);
    // 50 == 50
    expect((await cmd.execute(ctxArgs(['==', '50']))).success).toBe(true);
    // 50 != 49
    expect((await cmd.execute(ctxArgs(['!=', '49']))).success).toBe(true);
  });

  it('@<staticVar> on the LHS works too (frozen value, but still comparable)', async () => {
    // Static var case — set @x 75 then wait for it. Not particularly
    // useful in production (the value never changes), but the parser
    // shouldn't reject it and the comparison should run.
    const ex = makeExchange();
    const store = new MemoryPreprocessStore();
    store.setVar('x', '75');
    const r = await cmd.execute(ctx(['wait', '@x', '>', '50'], ex, undefined, 'BTCUSDT', store));
    expect(r.success).toBe(true);
  });

  it('errors loudly when @<name> is bound nowhere', async () => {
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
    const store = new MemoryPreprocessStore();
    const adapter = makeAdapter();
    // No binding for @missing — the check throws on the first poll,
    // which surfaces as a wait failure rather than a silent timeout.
    // Attach the rejection assertion BEFORE running timers so the
    // unhandled-rejection detector doesn't fire.
    const expectation = expect(
      cmd.execute(
        ctx(['wait', '@missing', '>', '100'], ex, adapter, 'BTCUSDT', store),
      ),
    ).rejects.toThrow(/@missing is not set/);
    await expectation;
  });

  it('combinator: `wait either (@x > 100) or (@y > 100)` plumbs ctx into both subs', async () => {
    const tickers = [{ symbol: 'BTCUSDT', last: 50, mark: 200 }];
    const ex = makeExchange({ tickers });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('x', '$last'); // 50
    store.setLiveVar('y', '$mark'); // 200
    const r = await cmd.execute(
      ctx(
        ['wait', 'either', '(@x', '>', '100)', 'or', '(@y', '>', '100)'],
        ex,
        undefined,
        'BTCUSDT',
        store,
      ),
    );
    // y already exceeds — should satisfy immediately.
    expect(r.success).toBe(true);
  });

  it('flipping the live source mid-wait keeps the binding live', async () => {
    // Live binding evaluates the CURRENT source on each poll. If the
    // user re-runs `sets @x …` while a wait is in flight, the new
    // source should take effect on the next tick.
    const tickers = [{ symbol: 'BTCUSDT', last: 50, mark: 200 }];
    const ex = makeExchange({ tickers });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('x', '$last'); // value: 50
    const promise = cmd.execute(
      ctx(['wait', '@x', '>', '100', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    // Initial poll: 50 > 100 false. Re-bind to $mark; next poll sees 200.
    await vi.advanceTimersByTimeAsync(150);
    store.setLiveVar('x', '$mark');
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
  });
});

describe('WaitCommand — tracked-var (@<trackedVar>) conditions', () => {
  let cmd: WaitCommand;

  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('`wait @hi > target` polls the tracker each tick', async () => {
    // Direct comparison on a tracked var. The tracker updates on
    // every poll; the condition releases when the high-water mark
    // crosses target.
    const tickers = [{ symbol: 'BTCUSDT', last: 10 }];
    const ex = makeExchange({ tickers });
    const store = new MemoryPreprocessStore();
    store.setTrackedVar('hi', 'max', '$last');
    const promise = cmd.execute(
      ctx(['wait', '@hi', '>', '100', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    await vi.advanceTimersByTimeAsync(150);
    tickers[0].last = 50;
    await vi.advanceTimersByTimeAsync(150);
    tickers[0].last = 150;
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
    expect(store.getTrackedVar('hi')!.value).toBe(150);
  });

  it('drawdown alert via `track min` — `wait @lo < <floor>`', async () => {
    const positions = [
      {
        id: 'pos1',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 50_000,
        notional: 50_000,
        leverage: 1,
        marginType: MarginType.Isolated,
        positionMode: PositionMode.OneWay,
        unrealizedPnl: -10,
        contracts: 1,
        liquidationPrice: 0,
      } as Position,
    ];
    const ex = makeExchange({ positions });
    const store = new MemoryPreprocessStore();
    store.setTrackedVar('lo', 'min', '$pnl');
    const promise = cmd.execute(
      ctx(['wait', '@lo', '<', '-50', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    // Initial: pnl=-10, @lo seeds to -10. -10 < -50 false.
    await vi.advanceTimersByTimeAsync(150);
    // Drawdown deepens.
    positions[0].unrealizedPnl = -75;
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
    expect(store.getTrackedVar('lo')!.value).toBe(-75);
  });
});

describe('WaitCommand — @<var> RHS conditions', () => {
  let cmd: WaitCommand;

  beforeEach(() => {
    cmd = new WaitCommand();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('`wait pnl < @stop` — full trailing-stop pattern via tracker + live var RHS', async () => {
    // The motivating use case: `track max @hi $pnl; sets @stop @hi-1`
    // then `wait pnl < @stop`. Each poll re-resolves @stop (which
    // re-evaluates `@hi-1`, which ratchets @hi). The RHS @-var
    // support in parseCondition is what makes this composable in
    // ONE wait command instead of polling math at the wait layer.
    const positions = [
      {
        id: 'pos1',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 50_000,
        notional: 50_000,
        leverage: 1,
        marginType: MarginType.Isolated,
        positionMode: PositionMode.OneWay,
        unrealizedPnl: 100,
        contracts: 1,
        liquidationPrice: 0,
      } as Position,
    ];
    const ex = makeExchange({ positions });
    const store = new MemoryPreprocessStore();
    store.setTrackedVar('hi', 'max', '$pnl');
    store.setLiveVar('stop', '@hi-1');
    // Initial: pnl=100, @hi seeds to 100, @stop = 99. 100 < 99 false.
    const promise = cmd.execute(
      ctx(['wait', 'pnl', '<', '@stop', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    await vi.advanceTimersByTimeAsync(150);
    // Climb — @hi ratchets to 200, @stop = 199.
    positions[0].unrealizedPnl = 200;
    await vi.advanceTimersByTimeAsync(150);
    // Pullback — @hi sticks, @stop = 199. 150 < 199 → release.
    positions[0].unrealizedPnl = 150;
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
    expect(store.getTrackedVar('hi')!.value).toBe(200);
  });

  it('`wait price > @target` — static RHS resolves once-frozen at preprocess', async () => {
    // Static @target gets substituted at preprocess time (`wait price >
    // 50000`). Then parseCondition reads a literal RHS. The whole
    // chain reduces to today's pre-RHS-support behavior — locked in
    // here to confirm we didn't regress that case.
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50_000, ask: 50_000 }] });
    const store = new MemoryPreprocessStore();
    store.setVar('target', '40000');
    const r = await cmd.execute(
      ctx(['wait', 'price', '>', '40000'], ex, undefined, 'BTCUSDT', store),
    );
    expect(r.success).toBe(true);
  });

  it('`wait price > @threshold` — live RHS re-resolves per tick', async () => {
    const tickers = [{ symbol: 'BTCUSDT', last: 100, ask: 100 }];
    const ex = makeExchange({ tickers });
    const store = new MemoryPreprocessStore();
    store.setLiveVar('threshold', '$last');
    // price > @threshold is `last > last` = false (equality). Move
    // ask above last to make the wait release.
    const promise = cmd.execute(
      ctx(['wait', 'price', '>', '@threshold', 'timeout=1s'], ex, undefined, 'BTCUSDT', store),
    );
    await vi.advanceTimersByTimeAsync(150);
    // Ask climbs while last stays — wait price reads ask for upside.
    tickers[0].ask = 200;
    await vi.advanceTimersByTimeAsync(150);
    const r = await promise;
    expect(r.success).toBe(true);
  });

  it('unbound RHS @-var throws on first check', async () => {
    const ex = makeExchange({ tickers: [{ symbol: 'BTCUSDT', last: 50_000 }] });
    const store = new MemoryPreprocessStore();
    const expectation = expect(
      cmd.execute(
        ctx(['wait', 'price', '>', '@missing'], ex, undefined, 'BTCUSDT', store),
      ),
    ).rejects.toThrow(/@missing is not set/);
    await expectation;
  });
});
