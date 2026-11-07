import { afterEach, describe, expect, it, vi } from 'vitest';

import { runsOnRustEngine, RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { virtualClock } from '../../utils/virtualClock';

describe('spot-enabled Hyperliquid accounts run on the engine', () => {
  afterEach(() => vi.restoreAllMocks());

  it('the spot toggle no longer routes any venue away from the engine', () => {
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust' })).toBe(true);
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust', enableSpotMarkets: true } as never)).toBe(true);
    expect(runsOnRustEngine('hyperliquid', {})).toBe(true);
    expect(runsOnRustEngine('bybit', { engine: 'rust', enableSpotMarkets: true } as never)).toBe(true);
    expect(runsOnRustEngine('bitunix', { engine: 'rust', enableSpotMarkets: true } as never)).toBe(true);
    expect(runsOnRustEngine('bitunix', {})).toBe(true);
  });

  it('createExchange builds the engine with or without spot markets', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    const options = { key: '', secret: '', publicDataOnly: true, engine: 'rust' as const };
    const spot = await createExchange('hyperliquid', { ...options, enableSpotMarkets: true });
    const perp = await createExchange('hyperliquid', options);
    try {
      expect(spot).toBeInstanceOf(RustEngineExchange);
      expect(perp).toBeInstanceOf(RustEngineExchange);
    } finally {
      spot.dispose();
      perp.dispose();
    }
  });
});
