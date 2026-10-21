import { afterEach, describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../../exchanges/hyperliquid/hyperliquid.exchange';
import { RustEngineExchange, runsOnRustEngine } from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

describe('spot-enabled Hyperliquid accounts stay on the TS venue', () => {
  afterEach(() => vi.restoreAllMocks());

  it('selects the engine only where it serves the account', () => {
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust' })).toBe(true);
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust', enableSpotMarkets: false })).toBe(true);
    expect(runsOnRustEngine('hyperliquid', { engine: 'rust', enableSpotMarkets: true })).toBe(false);
    expect(runsOnRustEngine('hyperliquid', {})).toBe(false);
    // The flag means nothing to other venues.
    expect(runsOnRustEngine('bybit', { engine: 'rust', enableSpotMarkets: true })).toBe(true);
    expect(runsOnRustEngine('bitunix', { engine: 'rust' })).toBe(false);
  });

  it('createExchange builds the TS venue for a spot-enabled account and the engine otherwise', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    const options = { key: '', secret: '', publicDataOnly: true, engine: 'rust' as const };
    const spot = await createExchange('hyperliquid', { ...options, enableSpotMarkets: true });
    const perp = await createExchange('hyperliquid', options);
    try {
      expect(spot).toBeInstanceOf(HyperliquidExchange);
      expect(spot).not.toBeInstanceOf(RustEngineExchange);
      expect(perp).toBeInstanceOf(RustEngineExchange);
    } finally {
      spot.dispose();
      perp.dispose();
    }
  });

  it('a direct engine construction still refuses spot with the reason', async () => {
    const exchange = new RustEngineExchange(
      'hyperliquid',
      { key: '', secret: '', publicDataOnly: true, enableSpotMarkets: true },
      new DefaultStore(),
    );
    await expect(exchange.start()).rejects.toThrow('spot-enabled accounts run on the TS venue');
    exchange.dispose();
  });
});
