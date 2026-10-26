import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { describe, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

describe('Rust Binance history capabilities', () => {
  it.each(['binancespot', 'binanceinverse'] as const)('%s answers unsupported histories without venue I/O', async (venue) => {
    const loader = vi.fn();
    const transport = vi.fn();
    setWasmEngineLoader(loader);
    setRustEngineTransport(transport);
    const exchange = new RustEngineExchange(venue, { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    try {
      adoptEngineCapabilities(exchange);
      expect(exchange.capabilities).toMatchObject({
        fillHistory: false,
        orderHistory: false,
        incomeHistory: false,
        incomeRange: false,
      });
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
      adoptEngineCapabilities(exchange);
      expect(exchange.capabilities).toMatchObject({
        fillHistory: true,
        orderHistory: true,
        incomeHistory: true,
        incomeRange: true,
      });
    } finally {
      exchange.dispose();
    }
  });
});
