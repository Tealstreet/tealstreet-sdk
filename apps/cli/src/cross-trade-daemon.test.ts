import type { CrossTradeConfig, CrossTradeCopier } from '@tealstreet/safe-cex/src/crossTrade/types';

import { describe, it, expect } from 'vitest';

import {
  collectAccountsToBoot,
  dropChaseConfigs,
  getCrossTradeDaemonStats,
  isCrossTradeDaemonRunning,
} from './cross-trade-daemon';

const makeCopier = (overrides: Partial<CrossTradeCopier> = {}): CrossTradeCopier => ({
  enabled: true,
  account: 'c1',
  weight: 1,
  execution: 'Market',
  weightMode: 'Absolute',
  symbolMappings: {},
  ...overrides,
});

const makeCfg = (overrides: Partial<CrossTradeConfig> = {}): CrossTradeConfig => ({
  enabled: true,
  masterAccount: 'master',
  masterSymbol: undefined,
  masterWeight: 1,
  copiers: [makeCopier()],
  ...overrides,
});

describe('collectAccountsToBoot', () => {
  it('collects master + enabled copier accounts (deduped)', () => {
    const accounts = collectAccountsToBoot([
      makeCfg({ masterAccount: 'A', copiers: [makeCopier({ account: 'B' }), makeCopier({ account: 'C' })] }),
      makeCfg({ masterAccount: 'A', copiers: [makeCopier({ account: 'B' })] }),
    ]);
    expect([...accounts].sort()).toEqual(['A', 'B', 'C']);
  });

  it('skips disabled copiers', () => {
    const accounts = collectAccountsToBoot([
      makeCfg({
        masterAccount: 'A',
        copiers: [makeCopier({ account: 'B', enabled: false }), makeCopier({ account: 'C', enabled: true })],
      }),
    ]);
    expect([...accounts].sort()).toEqual(['A', 'C']);
  });

  it('skips copiers without an account', () => {
    const accounts = collectAccountsToBoot([
      makeCfg({
        masterAccount: 'A',
        copiers: [makeCopier({ account: undefined as unknown as string })],
      }),
    ]);
    expect([...accounts]).toEqual(['A']);
  });
});

describe('dropChaseConfigs', () => {
  it('keeps a config whose only copier is Market/LimitSlippage', () => {
    const { kept, dropped } = dropChaseConfigs([makeCfg()]);
    expect(kept).toHaveLength(1);
    expect(dropped).toBe(0);
  });

  it('strips a Chase copier in place, keeping siblings', () => {
    const { kept, dropped } = dropChaseConfigs([
      makeCfg({
        copiers: [makeCopier({ execution: 'Chase' }), makeCopier({ execution: 'Market', account: 'c2' })],
      }),
    ]);
    expect(kept).toHaveLength(1);
    expect(kept[0]?.copiers).toHaveLength(1);
    expect(kept[0]?.copiers[0]?.execution).toBe('Market');
    expect(dropped).toBe(1);
  });

  it('drops the whole config when every enabled copier is Chase', () => {
    const { kept, dropped } = dropChaseConfigs([
      makeCfg({
        copiers: [makeCopier({ execution: 'Chase' }), makeCopier({ execution: 'Chase', account: 'c2' })],
      }),
    ]);
    expect(kept).toHaveLength(0);
    expect(dropped).toBe(2);
  });

  it('does not count disabled Chase copiers as dropped', () => {
    const { kept, dropped } = dropChaseConfigs([
      makeCfg({
        copiers: [
          makeCopier({ execution: 'Chase', enabled: false }),
          makeCopier({ execution: 'Market', account: 'c2' }),
        ],
      }),
    ]);
    expect(kept).toHaveLength(1);
    expect(dropped).toBe(0);
  });
});

describe('daemon status helpers (no daemon running)', () => {
  it('isCrossTradeDaemonRunning reports false', () => {
    expect(isCrossTradeDaemonRunning()).toBe(false);
  });

  it('getCrossTradeDaemonStats reports empty', () => {
    expect(getCrossTradeDaemonStats()).toEqual({ running: false, configCount: 0 });
  });
});
