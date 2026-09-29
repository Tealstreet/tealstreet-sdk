import { describe, expect, it, vi } from 'vitest';

import type { CommandContext } from '../Command';
import { SymbolCommand } from './SymbolCommand';

const market = (symbol: string, base: string, quote: string) => ({ symbol, base, quote });

const MARKETS = [market('BTCUSDT', 'BTC', 'USDT'), market('BTCUSD', 'BTC', 'USD'), market('ETHUSDT', 'ETH', 'USDT')];

// NOTE: takes `null` for "no exchange" — passing `undefined` would trigger
// the default parameter and silently hand back the full market list.
const makeCtx = (input: string, markets: unknown[] | null = MARKETS) => {
  const setDefaultSymbol = vi.fn();
  const printed: string[] = [];
  const ctx = {
    raw: `symbol ${input}`,
    args: { _: ['symbol', input] },
    symbol: 'BTCUSDT',
    accountName: 'acct',
    exchange: markets ? ({ name: 'bybit', store: { markets } } as never) : null,
    adapter: {
      setDefaultSymbol,
      printInfo: (msg: string) => printed.push(msg),
      print: (msg: string) => printed.push(msg),
    } as never,
  } as unknown as CommandContext;
  return { ctx, setDefaultSymbol, printed };
};

/**
 * `sym BTC` and `switch ETH` are this command's own documented examples. It
 * stored whatever was typed, so the session symbol was a string the exchange
 * had never heard of, and the failure surfaced one command later as
 * "Price data is not available yet for a symbol" — reading as a market-data
 * outage rather than an unresolved ticker. Reported from Discord 2026-09-28.
 */
describe('SymbolCommand', () => {
  it('resolves a bare ticker to a real market', async () => {
    const { ctx, setDefaultSymbol, printed } = makeCtx('btc');
    const result = await new SymbolCommand().execute(ctx);

    expect(result.success).toBe(true);
    expect(result.updateSession?.symbol).toBe('BTCUSDT');
    expect(setDefaultSymbol).toHaveBeenCalledWith('BTCUSDT');
    expect(printed.join(' ')).toContain('BTCUSDT');
  });

  it('keeps an exact symbol untouched', async () => {
    const { ctx, setDefaultSymbol } = makeCtx('ETHUSDT');
    await new SymbolCommand().execute(ctx);
    expect(setDefaultSymbol).toHaveBeenCalledWith('ETHUSDT');
  });

  it('rejects a symbol the exchange does not list instead of failing later', async () => {
    const { ctx, setDefaultSymbol } = makeCtx('NOTACOIN');
    const result = await new SymbolCommand().execute(ctx);

    expect(result.success).toBe(false);
    expect(result.error).toContain('NOTACOIN');
    expect(setDefaultSymbol).not.toHaveBeenCalled();
  });

  // requiresConnection is false, so the command runs before any markets exist.
  // Rejecting there would block input the user has no way to validate yet.
  it('accepts anything when there is no exchange to resolve against', async () => {
    const { ctx, setDefaultSymbol } = makeCtx('btc', null);
    const result = await new SymbolCommand().execute(ctx);

    expect(result.success).toBe(true);
    expect(setDefaultSymbol).toHaveBeenCalledWith('BTC');
  });
});
