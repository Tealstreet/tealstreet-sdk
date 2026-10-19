import { beforeEach, describe, expect, it, vi } from 'vitest';

const init = vi.hoisted(() => ({
  backpack: vi.fn(async () => undefined),
  binance: vi.fn(async () => undefined),
  bingx: vi.fn(async () => undefined),
  blofin: vi.fn(async () => undefined),
  bybit: vi.fn(async () => undefined),
  cryptocom: vi.fn(async () => undefined),
  mexc: vi.fn(async () => undefined),
  okx: vi.fn(async () => undefined),
  weex: vi.fn(async () => undefined),
}));

vi.mock('@tealstreet/safe-cex-wasm/web/backpack', () => ({ default: init.backpack, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/binance', () => ({ default: init.binance, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/bingx', () => ({ default: init.bingx, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/blofin', () => ({ default: init.blofin, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/bybit', () => ({ default: init.bybit, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/cryptocom', () => ({ default: init.cryptocom, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/mexc', () => ({ default: init.mexc, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/okx', () => ({ default: init.okx, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/weex', () => ({ default: init.weex, WasmEngine: class {} }));

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
});

describe.runIf(process.env.SAFE_CEX_WASM_TESTS)('browser Rust engine selection', () => {
  it('shares one initialization across concurrent aliases and loads no other family', async () => {
    const { loadWebEngine } = await import('../exchanges/rust/loadWebEngine');
    const engines = await Promise.all(['okx', 'okxeu', 'okxdemo', 'okxeudemo'].map(loadWebEngine));
    expect(engines.every((engine) => engine === engines[0])).toBe(true);
    expect(init.okx).toHaveBeenCalledTimes(1);
    expect(init.bybit).not.toHaveBeenCalled();
    expect(init.cryptocom).not.toHaveBeenCalled();
    expect(init.mexc).not.toHaveBeenCalled();
    expect(init.weex).not.toHaveBeenCalled();
    expect(init.binance).not.toHaveBeenCalled();
    expect(init.backpack).not.toHaveBeenCalled();
    expect(init.bingx).not.toHaveBeenCalled();
    expect(init.blofin).not.toHaveBeenCalled();
  });

  it('initializes each requested family separately', async () => {
    const { loadWebEngine } = await import('../exchanges/rust/loadWebEngine');
    const [backpack, binance, pm, bingx, blofin, bybit, cryptocom, mexc, okx, weex] = await Promise.all(['backpack', 'binance', 'binancepm', 'bingx', 'blofin', 'bybit', 'cryptocom', 'mexc', 'okx', 'weex'].map(loadWebEngine));
    expect(binance).toBe(pm);
    expect(new Set([backpack, binance, bingx, blofin, bybit, cryptocom, mexc, okx, weex]).size).toBe(9);
    for (const initialize of Object.values(init)) expect(initialize).toHaveBeenCalledTimes(1);
  });

  it('allows retry after a failed download or initialization', async () => {
    const { loadWebEngine } = await import('../exchanges/rust/loadWebEngine');
    init.okx.mockRejectedValueOnce(new Error('download interrupted'));
    await expect(loadWebEngine('okx')).rejects.toThrow('download interrupted');
    await loadWebEngine('okxeu');
    expect(init.okx).toHaveBeenCalledTimes(2);
  });

  it('rejects unknown and inherited names before loading', async () => {
    const { loadWebEngine } = await import('../exchanges/rust/loadWebEngine');
    for (const name of ['unknown', '__proto__', 'constructor']) {
      expect(() => loadWebEngine(name)).toThrow('unsupported Rust venue');
    }
    for (const initialize of Object.values(init)) expect(initialize).not.toHaveBeenCalled();
  });
});
