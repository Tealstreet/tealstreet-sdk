import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import { afterEach, expect, it, vi } from 'vitest';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

it.each(['binance', 'binancepm', 'binancespot', 'binanceinverse', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'mexc'] as const)(
  'preserves a caller-selected short receive window for %s',
  async (name) => {
    const construct = vi.fn();
    class Engine {
      constructor(venue: string, options: string) { construct(venue, JSON.parse(options)); }
      free() {}
    }
    setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
    const exchange = new RustEngineExchange(name, { key: '', secret: '', publicDataOnly: true, extra: { recvWindow: 5000 } }, new DefaultStore());
    running.push(exchange);
    await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
    expect(construct).toHaveBeenCalledExactlyOnceWith(name, expect.objectContaining({ recvWindow: 5000 }));
  },
);
