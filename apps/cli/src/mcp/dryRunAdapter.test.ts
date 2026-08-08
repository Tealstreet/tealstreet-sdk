import type { CoreAdapter } from '@tealstreet/script-api-core';

import { describe, expect, it, vi } from 'vitest';

import { createDryRunAdapter } from './dryRunAdapter';

/** Every member throws — so a write reaching through is unmissable. */
const explodingReal = (): CoreAdapter => {
  const boom = (name: string) => () => {
    throw new Error(`LIVE ${name} REACHED`);
  };
  return {
    getActiveAccount: () => 'acc',
    getActiveAccounts: () => ['acc'],
    getActiveSymbol: () => 'BTCUSDT',
    getActiveExchange: () => 'bybit',
    getAccounts: () => [],
    getMarkets: () => [],
    getMarket: () => undefined,
    getTicker: () => undefined,
    getTickers: () => [],
    getPositions: () => [],
    getHistoricalPosition: () => undefined,
    getOrders: () => [],
    getBalance: () => undefined,
    getCombinedBalance: () => ({
      totalEquity: 1,
      totalAvailable: 1,
      totalUsed: 0,
      totalUpnl: 0,
      longExposure: 0,
      shortExposure: 0,
      currency: 'USD',
    }),
    getOrderbook: () => undefined,
    getCandles: () => [],
    placeOrder: boom('placeOrder'),
    placeOrders: boom('placeOrders'),
    cancelOrder: boom('cancelOrder'),
    cancelOrders: boom('cancelOrders'),
    updateOrder: boom('updateOrder'),
    closePosition: boom('closePosition'),
    setLeverage: boom('setLeverage'),
    setPositionMode: boom('setPositionMode'),
    setMarginType: boom('setMarginType'),
    runCommand: boom('runCommand'),
    print: boom('print'),
    signer: undefined,
  } as unknown as CoreAdapter;
};

describe('createDryRunAdapter', () => {
  // The safety claim of the whole MCP server rests on this: no write may
  // reach the real adapter.
  it('never lets any write reach the live adapter', async () => {
    const { adapter, intents } = createDryRunAdapter(explodingReal());

    await adapter.placeOrder({ symbol: 'BTCUSDT' } as never, 'acc');
    await adapter.placeOrders([{ symbol: 'BTCUSDT' }] as never, 'acc');
    await adapter.cancelOrder('id', { account: 'acc' });
    await adapter.cancelOrders({} as never, 'acc');
    await adapter.updateOrder('id', {} as never, 'acc');
    await adapter.closePosition({ symbol: 'BTCUSDT' } as never, 'acc');
    await adapter.setLeverage('BTCUSDT', 5, 'acc');
    await adapter.setPositionMode('hedged' as never, 'acc');
    await adapter.setMarginType('BTCUSDT', 'isolated' as never, 'acc');

    expect(intents.map((i) => i.method)).toEqual([
      'placeOrder',
      'placeOrders',
      'cancelOrder',
      'cancelOrders',
      'updateOrder',
      'closePosition',
      'setLeverage',
      'setPositionMode',
      'setMarginType',
    ]);
  });

  // runCommand dispatches arbitrary REPL commands including buy/sell, so it is
  // the single most important stub.
  it('stubs runCommand and records the command text', async () => {
    const { adapter, intents } = createDryRunAdapter(explodingReal());
    const result = await adapter.runCommand('buy 1000', { account: 'acc', depth: 1 } as never);

    expect(intents).toHaveLength(1);
    expect(intents[0].detail).toContain('buy 1000');
    // ok:true on purpose — a failure would push a correct script down its
    // error branch, so the dry run would report its failure path instead.
    expect(result.ok).toBe(true);
    expect(result.output[0].text).toContain('[dry-run]');
  });

  it('passes reads through to live data', async () => {
    const real = explodingReal();
    const spy = vi.spyOn(real, 'getPositions');
    const { adapter } = createDryRunAdapter(real);

    await adapter.getPositions({ account: 'acc' });
    expect(spy).toHaveBeenCalled();
  });

  it('captures print output instead of writing to the terminal', () => {
    const { adapter, output } = createDryRunAdapter(explodingReal());
    adapter.print('hello', 'info');
    expect(output).toEqual(['hello']);
  });

  it('returns synthetic order ids so downstream logic still runs', async () => {
    const { adapter } = createDryRunAdapter(explodingReal());
    const placed = await adapter.placeOrder({ symbol: 'X' } as never, 'acc');
    expect(placed.orderIds).toHaveLength(1);
    expect(placed.orderIds[0]).toContain('dry-run');
  });
});
