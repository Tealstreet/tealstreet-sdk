import { describe, expect, it, vi } from 'vitest';
import { OrderSide, PositionSide } from '@tealstreet/safe-cex/src/types';
import { createCoreAdapter } from '../adapter/createCoreAdapter';
import { CORE_API_VERSION } from '../version';
import { runScript, type ScriptRecord } from './runScript';

const fixture = (side: PositionSide) => {
  const placeOrder = vi.fn(async () => ['close']);
  const transpile = vi.fn(async (source: string) => source);
  const market = {
    id: 'DOGE_USDT',
    symbol: 'DOGEUSDT',
    contractSize: 100,
    precision: { amount: 100, price: 0.00001 },
    limits: { amount: { min: 100, max: 44500000 } },
  };
  const store = {
    markets: [market],
    positions: [{ symbol: market.symbol, side, contracts: 26100, extra: { contractsNative: 261 } }],
    orders: [],
    tickers: [],
  };
  const adapter = createCoreAdapter({
    getExchange: () => ({ name: 'mexc', placeOrder, store }),
    getStore: () => store,
    readActiveAccount: () => 'mexc',
    readStreamingAccounts: () => ['mexc'],
    readAllAccounts: () => [{ name: 'mexc', exchange: 'mexc' }],
  } as never);
  return {
    placeOrder,
    transpile,
    run: (runtimeVersion: string, opts = 'percent: 50', stampedVersion = CORE_API_VERSION) => {
      const script: ScriptRecord = {
        id: 'percentage-close',
        name: 'percentage-close',
        targets: ['core'],
        coreApiVersion: stampedVersion,
        capabilities: { accounts: ['mexc'], writes: ['closePosition', 'placeOrder'], idempotent: false },
        code: `const result = await api.exchange.closePosition({symbol: 'DOGEUSDT', side: '${side}', account: 'mexc', ${opts}}); api.io.return(result.orderIds);`,
      };
      return runScript(script, '', {
        adapter,
        transpile,
        runtime: { mountedTargets: ['core'], coreApiVersion: runtimeVersion },
        invokedFrom: 'cli',
      });
    },
  };
};

describe.each([PositionSide.Long, PositionSide.Short])('percentage-close script version (%s)', (side) => {
  it('refuses a newly stamped percentage script on the previous 1.3.3 runtime before dispatch', async () => {
    const { run, placeOrder, transpile } = fixture(side);
    const result = await run('1.3.3');
    expect(result.ok).toBe(false);
    expect(result.error).toBe('script requires CORE v1.4.0; runtime provides v1.3.3');
    expect(placeOrder).not.toHaveBeenCalled();
    expect(transpile).not.toHaveBeenCalled();
  });

  it.each([
    [50, 130],
    [100, 261],
  ])('%i percent traverses runner, proxy and adapter with amount %i', async (percent, amount) => {
    const { run, placeOrder } = fixture(side);
    const result = await run(CORE_API_VERSION, `percent: ${percent}`);
    expect(result).toMatchObject({ ok: true, returnValue: ['close'] });
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        amount,
        reduceOnly: true,
        side: side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
      }),
    );
  });

  it('retains compatibility for an older-stamped explicit-amount script', async () => {
    const { run, placeOrder } = fixture(side);
    const result = await run(CORE_API_VERSION, 'amount: 130', '1.3.3');
    expect(result).toMatchObject({ ok: true, returnValue: ['close'] });
    expect(placeOrder).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ amount: 130, reduceOnly: true }));
  });
});
