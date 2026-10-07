import type { OemsContext, OemsPositionRef } from './context.types';

import { boundOemsContext, boundOemsTranscript, OEMS_CONTEXT_LIMITS } from './boundContext';

const position = (index: number): OemsPositionRef => ({
  account: 'main',
  symbol: `SYM${index}`,
  side: 'long',
  contracts: 1,
  nativeContracts: 1,
  entryPrice: 1,
  markPrice: 1,
  notional: 1,
  unrealizedPnl: 0,
  liquidationPrice: 0,
  leverage: 1,
  positionMode: 'one-way',
});

const context = (over: Partial<OemsContext> = {}): OemsContext => ({
  generatedAt: 1,
  activeAccount: 'main',
  activeSymbol: 'BTCUSDT',
  activeAccounts: ['main'],
  accounts: [{ name: 'main', exchange: 'binance', active: true, testnet: false, connected: true }],
  positions: [],
  orders: [],
  balances: [],
  markets: [],
  fatFingerNotional: 0,
  ...over,
});

describe('boundOemsContext', () => {
  it('caps a list rather than refusing the request', () => {
    const positions = Array.from({ length: 500 }, (_, index) => position(index));
    const bounded = boundOemsContext(context({ positions }));
    expect(bounded.positions).toHaveLength(OEMS_CONTEXT_LIMITS.positions);
  });

  it('clips a name long enough to be a prompt of its own', () => {
    const bounded = boundOemsContext(context({ activeAccount: 'a'.repeat(5_000) }));
    expect(bounded.activeAccount).toHaveLength(OEMS_CONTEXT_LIMITS.stringChars);
  });

  it('zeroes a non-finite number instead of letting it serialise to null', () => {
    // `JSON.stringify(NaN)` is `null`, which reads as "unknown" on a field that
    // is never optional.
    const bounded = boundOemsContext(context({ positions: [{ ...position(0), contracts: Number.NaN }] }));
    expect(bounded.positions[0].contracts).toBe(0);
  });

  it('rebuilds every row by name, so an extra field cannot ride along', () => {
    const smuggled = { ...position(0), apiSecret: 'sk-live-should-never-appear' } as unknown as OemsPositionRef;
    const bounded = boundOemsContext(context({ positions: [smuggled] }));
    expect(JSON.stringify(bounded)).not.toContain('sk-live');
    expect(Object.keys(bounded.positions[0])).not.toContain('apiSecret');
  });

  it('normalises an unknown order type instead of forwarding noise', () => {
    const order = {
      account: 'main',
      id: '1',
      symbol: 'BTCUSDT',
      side: 'buy' as const,
      type: 'weird_venue_type' as never,
      amount: 1,
      filled: 0,
      remaining: 1,
      nativeRemaining: 1,
      price: 1,
      reduceOnly: false,
    };
    expect(boundOemsContext(context({ orders: [order] })).orders[0].type).toBe('limit');
  });

  it('survives a malformed context rather than throwing mid-request', () => {
    const bounded = boundOemsContext({ ...context(), positions: undefined as never });
    expect(bounded.positions).toEqual([]);
  });

  it('normalises an unexpected enum to the safe end', () => {
    const bounded = boundOemsContext(
      context({ positions: [{ ...position(0), side: 'sideways' as never, positionMode: 'weird' as never }] }),
    );
    expect(bounded.positions[0].side).toBe('long');
    expect(bounded.positions[0].positionMode).toBe('one-way');
  });
});

describe('boundOemsTranscript', () => {
  it('keeps the most recent turns, not the first ones', () => {
    const turns = Array.from({ length: 40 }, (_, index) => ({ role: 'user' as const, content: `m${index}` }));
    const bounded = boundOemsTranscript(turns);
    expect(bounded).toHaveLength(OEMS_CONTEXT_LIMITS.transcriptTurns);
    expect(bounded[bounded.length - 1].content).toBe('m39');
  });

  it('clips a turn and normalises an unknown role to the user', () => {
    const bounded = boundOemsTranscript([{ role: 'system' as never, content: 'x'.repeat(9_000) }]);
    expect(bounded[0].role).toBe('user');
    expect(bounded[0].content).toHaveLength(OEMS_CONTEXT_LIMITS.messageChars);
  });
});

/**
 * A context is bounded in the browser (so the allowlist is what the model was
 * shown) and again in the route (which cannot trust a browser about its own
 * payload). A second pass that changed anything would corrupt what was sent,
 * so this is a load-bearing property rather than a nicety.
 */
describe('boundOemsContext is idempotent', () => {
  it('is a fixed point after one pass', () => {
    const raw = {
      generatedAt: 1,
      activeAccount: 'a'.repeat(200),
      activeSymbol: 'BTCUSDT',
      activeAccounts: Array.from({ length: 60 }, (_, index) => `acct-${index}`),
      accounts: Array.from({ length: 45 }, (_, index) => ({
        name: `acct-${index}`,
        exchange: 'binance',
        active: true,
        testnet: false,
        connected: true,
      })),
      positions: Array.from({ length: 80 }, (_, index) => ({
        account: `acct-${index % 45}`,
        symbol: `S${index}`,
        side: 'long',
        contracts: 1,
        nativeContracts: 1,
        entryPrice: 1,
        notional: 1,
        upnl: 0,
        liquidationPrice: 0,
        leverage: 1,
      })),
      orders: [],
      balances: [],
      markets: [],
      fatFingerNotional: 0,
    } as never;

    const once = boundOemsContext(raw);
    expect(boundOemsContext(once)).toEqual(once);
    expect(once.accounts).toHaveLength(OEMS_CONTEXT_LIMITS.accounts);
    expect(once.positions).toHaveLength(OEMS_CONTEXT_LIMITS.positions);
  });
});

/**
 * The snapshot has to say when it is a subset, for the same reason a read
 * result does. Without it the model counts fifty of eighty positions, says
 * "reducing your fifty positions", and the confirm card — resolved against the
 * RAW book on purpose — shows eighty.
 */
describe('the snapshot reports what it dropped', () => {
  const many = <T>(count: number, make: (index: number) => T) => Array.from({ length: count }, (_, i) => make(i));

  const big = () =>
    ({
      generatedAt: 1,
      activeAccount: 'main',
      activeSymbol: 'BTCUSDT',
      activeAccounts: ['main'],
      accounts: many(45, (i) => ({
        name: `acct-${i}`,
        exchange: 'binance',
        active: true,
        testnet: false,
        connected: true,
      })),
      positions: many(80, position),
      orders: [],
      balances: [],
      markets: [],
      fatFingerNotional: 0,
    }) as never as OemsContext;

  it('counts what each list lost', () => {
    expect(boundOemsContext(big()).dropped).toEqual({ accounts: 5, positions: 30 });
  });

  it('says nothing when nothing was dropped', () => {
    expect(boundOemsContext(boundOemsContext({ ...big(), accounts: [], positions: [] })).dropped).toBeUndefined();
  });

  /** The route re-bounds what the browser already bounded, and by then the
   *  excess is gone — a pass reporting only its own subtraction would answer
   *  zero and erase the record. */
  it('keeps the count across a second pass that drops nothing further', () => {
    expect(boundOemsContext(boundOemsContext(big())).dropped).toEqual({ accounts: 5, positions: 30 });
  });
});

/**
 * `dropped` arrives as JSON from the browser like the rest of the context, so
 * it is rebuilt by name from clamped numbers rather than carried through.
 */
describe('a forged dropped count cannot ride along', () => {
  const withDropped = (dropped: unknown) =>
    boundOemsContext({
      generatedAt: 1,
      activeAccount: 'main',
      activeSymbol: 'BTCUSDT',
      activeAccounts: ['main'],
      accounts: [],
      positions: [],
      orders: [],
      balances: [],
      markets: [],
      fatFingerNotional: 0,
      dropped,
    } as never);

  it('ignores a non-numeric count', () => {
    expect(withDropped({ positions: '9' }).dropped).toBeUndefined();
  });

  it('ignores an infinite count', () => {
    expect(withDropped({ positions: Number.POSITIVE_INFINITY }).dropped).toBeUndefined();
  });

  it('ignores a negative count', () => {
    expect(withDropped({ positions: -5 }).dropped).toBeUndefined();
  });

  it('drops a key it does not know', () => {
    expect(withDropped({ positions: 3, apiSecret: 'sk-live' }).dropped).toEqual({ positions: 3 });
  });
});
