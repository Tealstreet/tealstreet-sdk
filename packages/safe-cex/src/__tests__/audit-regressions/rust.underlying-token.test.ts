import type { Market } from '../../types';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { describe, expect, it } from 'vitest';

import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { consumerFrame } from '../cutover/consumerFrame';

const market = (symbol: string, base: string, info?: Record<string, unknown>): Market =>
  ({
    id: symbol,
    symbol,
    base,
    quote: 'USDC',
    active: true,
    isSpot: true,
    precision: { amount: 0.00001, price: 1 },
    limits: { amount: { min: 0.00001, max: 1e9 }, leverage: { min: 1, max: 1 } },
    ...(info && { info }),
  }) as Market;

describe('the underlying token behind a wrapped spot pair reaches market.info as TS sets it', () => {
  it('applies hlTokenName and hlTokenFullName, absent full names stay absent, other markets untouched', () => {
    const markets = [
      market('UBTC/USDC', 'BTC', { hlTokenName: 'UBTC', hlTokenFullName: 'Unit Bitcoin' }),
      market('PURR/USDC', 'PURR', { hlTokenName: 'PURR' }),
      market('BTC-PERP', 'BTC'),
    ];
    const store = new DefaultStore();
    new FrameApplier(store).apply(new Frame(consumerFrame({ markets })));
    const bySymbol = Object.fromEntries(store.markets.map((m) => [m.symbol, m]));
    expect(bySymbol['UBTC/USDC'].info).toEqual({ hlTokenName: 'UBTC', hlTokenFullName: 'Unit Bitcoin' });
    expect(bySymbol['PURR/USDC'].info).toEqual({ hlTokenName: 'PURR', hlTokenFullName: undefined });
    expect(bySymbol['BTC-PERP'].info).toBeUndefined();
  });

  it('decodes the section with an absent full name', () => {
    const frame = new Frame(consumerFrame({ markets: [market('UBTC/USDC', 'BTC', { hlTokenName: 'UBTC' })] }));
    const table = frame.underlyingTokens()!;
    expect(table.count).toBe(1);
    expect([table.market[0], table.name(0), table.fullName(0)]).toEqual([0, 'UBTC', undefined]);
  });
});
