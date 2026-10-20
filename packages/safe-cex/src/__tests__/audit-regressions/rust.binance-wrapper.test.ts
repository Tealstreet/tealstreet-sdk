import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { describe, expect, it, vi } from 'vitest';

import { isBaseCapabilityFallback, isInertCapability } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

describe('Rust Binance history capabilities', () => {
  it.each(['binancespot', 'binanceinverse'] as const)('%s brands unsupported histories without starting a host', async (venue) => {
    const loader = vi.fn();
    const transport = vi.fn();
    setWasmEngineLoader(loader);
    setRustEngineTransport(transport);
    const exchange = new RustEngineExchange(venue, { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    try {
      for (const name of ['fetchHistoricalFills', 'fetchHistoricalOrders', 'fetchIncomeHistory', 'fetchIncomeHistoryRange'] as const) {
        expect(isBaseCapabilityFallback(exchange[name], name), name).toBe(true);
        expect(isInertCapability(exchange[name], name), name).toBe(true);
      }
      expect(isBaseCapabilityFallback(exchange.fetchIncomeHistory, 'fetchHistoricalOrders')).toBe(false);
      await expect(exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' })).resolves.toEqual([]);
      await expect(exchange.fetchHistoricalOrders({ symbol: 'BTCUSDT' })).resolves.toEqual([]);
      await expect(exchange.fetchIncomeHistory({ startTime: 1, endTime: 2 })).resolves.toEqual([]);
      await expect(exchange.fetchIncomeHistoryRange(1, 2)).resolves.toEqual([]);
      expect(loader).not.toHaveBeenCalled();
      expect(transport).not.toHaveBeenCalled();
    } finally {
      exchange.dispose();
      setWasmEngineLoader(loadNodeEngine);
      setRustEngineTransport(nativeTransport);
    }
  });

  it.each(['binance', 'binancepm'] as const)('%s retains supported histories', (venue) => {
    const exchange = new RustEngineExchange(venue, { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    try {
      for (const name of ['fetchHistoricalFills', 'fetchHistoricalOrders', 'fetchIncomeHistory', 'fetchIncomeHistoryRange'] as const) {
        expect(isBaseCapabilityFallback(exchange[name], name), name).toBe(false);
      }
    } finally {
      exchange.dispose();
    }
  });
});
