import { describe, expect, it } from 'vitest';

import type { CommandContext } from '../Command';
import { CheckCommand } from './CheckCommand';

/**
 * The question a user report asked: "check symbol SHOP and gives back is crypto i.e.
 * tradeable or false - tradfi". These tests pin that the output names the
 * class correctly on both sides and uses the "may require" phrasing from
 * `bybit/CLAUDE.md` — never "you can trade this".
 */

const bybitStore = {
  markets: [
    {
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.5 },
      info: { symbolType: '' },
    },
    {
      symbol: 'SHOPUSDT',
      base: 'SHOP',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.01 },
      info: { symbolType: 'stock', underlyingTicker: 'SHOP', marketRegion: 'US' },
    },
    {
      symbol: 'XAUUSDT',
      base: 'XAU',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.01 },
      info: { symbolType: 'commodity' },
    },
  ],
};

const binanceStore = {
  markets: [
    {
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      precision: { amount: 0.001, price: 0.5 },
      info: {},
    },
  ],
};

const makeCtx = (name: string, store: unknown, symbol: string) => {
  const printed: string[] = [];
  const ctx = {
    raw: `check ${symbol}`,
    args: { _: ['check', symbol] },
    symbol: 'BTCUSDT',
    accountName: 'acct',
    exchange: { name, store } as never,
    adapter: {
      printInfo: (msg: string) => printed.push(msg),
      print: (msg: string) => printed.push(msg),
      printError: (msg: string) => printed.push(msg),
    } as never,
  } as unknown as CommandContext;
  return { ctx, printed };
};

describe('CheckCommand', () => {
  it('says "crypto perpetual" on a crypto symbol', async () => {
    const { ctx, printed } = makeCtx('bybit', bybitStore, 'BTCUSDT');
    const result = await new CheckCommand().execute(ctx);
    expect(result.success).toBe(true);
    expect(printed.join('\n')).toBe('BTCUSDT on bybit: crypto perpetual');
  });

  it('names a tokenized equity with its underlying and the "may require" warning', async () => {
    const { ctx, printed } = makeCtx('bybit', bybitStore, 'SHOPUSDT');
    const result = await new CheckCommand().execute(ctx);
    expect(result.success).toBe(true);
    expect(printed.join('\n')).toBe(
      'SHOPUSDT on bybit: tokenized equity (SHOP) — may require signing a trading agreement',
    );
  });

  it('names a tokenized commodity even where no underlying ticker is published', async () => {
    const { ctx, printed } = makeCtx('bybit', bybitStore, 'XAUUSDT');
    const result = await new CheckCommand().execute(ctx);
    expect(result.success).toBe(true);
    expect(printed.join('\n')).toBe('XAUUSDT on bybit: tokenized commodity — may require signing a trading agreement');
  });

  it('reports "classifier not implemented" on a venue whose classifier is not written', async () => {
    const { ctx, printed } = makeCtx('binance', binanceStore, 'BTCUSDT');
    const result = await new CheckCommand().execute(ctx);
    expect(result.success).toBe(true);
    // The reason text comes straight from `classifyMarket` — do not pin it
    // verbatim, pin the shape the user needs.
    expect(printed.join('\n')).toContain('BTCUSDT on binance:');
    expect(printed.join('\n')).toContain('binance');
  });

  it('rejects a symbol the exchange does not list', async () => {
    const { ctx, printed } = makeCtx('bybit', bybitStore, 'NOPEUSDT');
    const result = await new CheckCommand().execute(ctx);
    expect(result.success).toBe(false);
    expect(result.error).toContain('NOPEUSDT');
    expect(printed.length).toBe(0);
  });

  it('never says "you can trade this" or "you cannot trade this"', async () => {
    const printed: string[] = [];
    for (const sym of ['BTCUSDT', 'SHOPUSDT', 'XAUUSDT']) {
      const { ctx, printed: p } = makeCtx('bybit', bybitStore, sym);
      await new CheckCommand().execute(ctx);
      printed.push(...p);
    }
    const text = printed.join('\n').toLowerCase();
    expect(text).not.toContain('you can trade');
    expect(text).not.toContain('you cannot trade');
  });
});
