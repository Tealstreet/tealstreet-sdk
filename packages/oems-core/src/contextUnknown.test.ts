import type { OemsContext } from './context.types';

import { boundOemsContext } from './boundContext';
import { buildOemsPrompt } from './prompt';

const snapshot = (value: unknown): OemsContext => ({
  generatedAt: 1,
  activeAccount: 'main',
  activeSymbol: 'BTCUSDT',
  activeAccounts: ['main'],
  accounts: [{ name: 'main', exchange: 'kraken', active: true, testnet: false, connected: true }],
  positions: [
    {
      account: 'main',
      symbol: 'BTCUSDT',
      side: 'long',
      positionMode: 'one-way',
      contracts: 1,
      nativeContracts: 1,
      entryPrice: 100,
      markPrice: 100,
      notional: value as number | null,
      unrealizedPnl: 0,
      liquidationPrice: null,
      leverage: 1,
    },
  ],
  orders: [
    {
      account: 'main',
      id: 'order-1',
      symbol: 'BTCUSDT',
      side: 'buy',
      type: 'limit',
      amount: 1,
      filled: 0,
      remaining: 1,
      nativeRemaining: value as number | null,
      price: 100,
      reduceOnly: false,
    },
  ],
  balances: [],
  markets: [],
  fatFingerNotional: 0,
});

const paths = {
  browser: boundOemsContext,
  // The route receives JSON after the browser's first bounding pass.
  route: (raw: OemsContext) => boundOemsContext(JSON.parse(JSON.stringify(boundOemsContext(raw)))),
};

describe.each(Object.entries(paths))('nullable context numbers at the shared %s boundary', (_name, bound) => {
  it.each([null, undefined, Number.NaN, Infinity, -Infinity, 'UNKNOWN'])(
    'preserves unknown as null in the snapshot and both prompt modes: %s',
    (value) => {
      const context = bound(snapshot(value));
      expect(context.positions[0].notional).toBeNull();
      expect(context.orders[0].nativeRemaining).toBeNull();
      expect(boundOemsContext(context)).toEqual(context);
      for (const final of [false, true]) {
        const prompt = buildOemsPrompt({ context, transcript: [], message: 'Describe my book', final }, 'test');
        expect(prompt.input).toContain('"notional":null');
        expect(prompt.input).toContain('"nativeRemaining":null');
        expect(prompt.instructions).toContain('A null notional or nativeRemaining means unknown, never zero');
      }
    },
  );

  it.each([0, 123.45])('preserves an actual numeric value in the snapshot and prompt: %s', (value) => {
    const context = bound(snapshot(value));
    expect(context.positions[0].notional).toBe(value);
    expect(context.orders[0].nativeRemaining).toBe(value);
    for (const final of [false, true]) {
      const prompt = buildOemsPrompt({ context, transcript: [], message: 'Describe my book', final }, 'test');
      expect(prompt.input).toContain(`"notional":${value}`);
      expect(prompt.input).toContain(`"nativeRemaining":${value}`);
    }
  });
});
