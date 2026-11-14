import type { OemsContext } from './context.types';
import type { OemsIntent, OemsPlan } from './intent.types';
import { describe, expect, it, vi } from 'vitest';
import { venueUsesBaseAmountRails } from '@tealstreet/script-api-core/src/order/orderPricing';
import { resolveOemsPlan } from './resolvePlan';

vi.mock('@tealstreet/script-api-core/src/order/orderPricing', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@tealstreet/script-api-core/src/order/orderPricing')>();
  return { ...actual, venueUsesBaseAmountRails: vi.fn(actual.venueUsesBaseAmountRails) };
});

const NOW = 1700000000000;
const book = (amountStep = 1, nativeContracts = 130): OemsContext => ({
  generatedAt: NOW,
  fatFingerNotional: 1_000_000,
  activeAccount: 'mexc',
  activeSymbol: 'DOGEUSDT',
  activeAccounts: ['mexc'],
  accounts: [{ name: 'mexc', exchange: 'mexc', active: true, connected: true, testnet: false }],
  positions: [
    {
      account: 'mexc',
      symbol: 'DOGEUSDT',
      side: 'long',
      contracts: nativeContracts * 100,
      nativeContracts,
      entryPrice: 0.1,
      markPrice: 0.1,
      notional: nativeContracts * 10,
      unrealizedPnl: 0,
      liquidationPrice: null,
      leverage: 1,
      positionMode: 'one-way',
    },
  ],
  orders: [
    {
      account: 'mexc',
      id: 'close',
      symbol: 'DOGEUSDT',
      side: 'sell',
      type: 'limit',
      amount: 13000,
      filled: 0,
      remaining: 13000,
      nativeRemaining: 130,
      price: 0.1,
      reduceOnly: true,
    },
  ],
  markets: [
    {
      account: 'mexc',
      symbol: 'DOGEUSDT',
      amountStep,
      priceStep: 0.00001,
      lastPrice: 0.1,
      minAmount: 100,
      maxAmount: 44500000,
      maxLeverage: 300,
      contractSize: 100,
      isInverse: false,
      isSpot: false,
    },
  ],
  balances: [],
});
const resolve = (step: OemsIntent, context = book()) =>
  resolveOemsPlan({ reply: '', reads: [], steps: [step] } satisfies OemsPlan, context, { now: NOW });

describe('OEMS refuses pre-rounding of MEXC close-like intent', () => {
  it('selects native close preservation through the shared venue policy', () => {
    vi.mocked(venueUsesBaseAmountRails).mockClear();
    const out = resolve({ kind: 'close_positions', scope: { accounts: ['mexc'] } });
    expect(out.rejections).toEqual([]);
    expect(out.commands[0].args[0]).toMatchObject({ amount: 130 });
    expect(venueUsesBaseAmountRails).toHaveBeenCalledExactlyOnceWith('mexc');
  });

  it('refuses an aligned override above the full position instead of silently clamping', () => {
    const out = resolveOemsPlan(
      { reply: '', reads: [], steps: [{ kind: 'close_positions', scope: { accounts: ['mexc'] } }] },
      book(),
      { now: NOW, overrides: [{ step: 0, symbol: 'DOGEUSDT', account: 'mexc', side: 'sell', amount: 150 }] },
    );
    expect(out.commands).toEqual([]);
    expect(out.rejections[0]?.reason).toBe(
      'MEXC close requested 150 native contracts but would send 130; request the available quantity explicitly',
    );
  });

  it.each([
    {
      kind: 'place_order',
      account: 'mexc',
      symbol: 'DOGEUSDT',
      side: 'sell',
      orderType: 'market',
      amount: 130.4,
      reduceOnly: true,
    },
    { kind: 'place_stop_loss', account: 'mexc', symbol: 'DOGEUSDT', side: 'sell', amount: 130.4, price: 0.09 },
    { kind: 'amend_order', account: 'mexc', orderId: 'close', amount: 130.4 },
  ] as OemsIntent[])('refuses $kind before rounding hides the requested quantity', (step) => {
    const out = resolve(step);
    expect(out.commands).toEqual([]);
    expect(out.rejections[0]?.reason).toMatch(/lot step/);
  });

  it('refuses a percentage whose native quantity is off-step', () => {
    const out = resolve({ kind: 'reduce_positions', scope: { accounts: ['mexc'] }, percent: 50 }, book(1, 261));
    expect(out.commands).toEqual([]);
    expect(out.rejections[0]?.reason).toMatch(/lot step/);
  });

  it('refuses an incompatible declared native step instead of sending a 100-contract close', () => {
    const out = resolve({ kind: 'close_positions', scope: { accounts: ['mexc'] } }, book(100));
    expect(out.commands).toEqual([]);
    expect(out.rejections[0]?.reason).toMatch(/lot step/);
  });

  it('preserves a 130-contract close when the context supplies a native step', () => {
    const out = resolve({ kind: 'close_positions', scope: { accounts: ['mexc'] } });
    expect(out.rejections).toEqual([]);
    expect(out.commands[0].args[0]).toMatchObject({ amount: 130 });
  });
});
