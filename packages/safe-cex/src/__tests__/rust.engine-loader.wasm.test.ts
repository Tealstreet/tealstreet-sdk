import { beforeEach, describe, expect, it, vi } from 'vitest';

const init = vi.hoisted(() => ({
  kraken: vi.fn(async () => undefined),
  asterdex: vi.fn(async () => undefined),
  kucoin: vi.fn(async () => undefined),
  orderly: vi.fn(async () => undefined),
  woo: vi.fn(async () => undefined),
  lighter: vi.fn(async () => undefined),
  bitrue: vi.fn(async () => undefined),
  backpack: vi.fn(async () => undefined),
  binance: vi.fn(async () => undefined),
  bingx: vi.fn(async () => undefined),
  bitget: vi.fn(async () => undefined),
  blofin: vi.fn(async () => undefined),
  bybit: vi.fn(async () => undefined),
  cryptocom: vi.fn(async () => undefined),
  hyperliquid: vi.fn(async () => undefined),
  mexc: vi.fn(async () => undefined),
  okx: vi.fn(async () => undefined),
  weex: vi.fn(async () => undefined),
}));

vi.mock('@tealstreet/safe-cex-wasm/web/kraken', () => ({ default: init.kraken, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/asterdex', () => ({ default: init.asterdex, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/kucoin', () => ({ default: init.kucoin, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/orderly', () => ({ default: init.orderly, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/bitrue', () => ({ default: init.bitrue, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/lighter', () => ({ default: init.lighter, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/woo', () => ({ default: init.woo, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/backpack', () => ({ default: init.backpack, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/binance', () => ({ default: init.binance, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/bingx', () => ({ default: init.bingx, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/bitget', () => ({ default: init.bitget, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/blofin', () => ({ default: init.blofin, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/bybit', () => ({ default: init.bybit, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/cryptocom', () => ({ default: init.cryptocom, WasmEngine: class {} }));
vi.mock('@tealstreet/safe-cex-wasm/web/hyperliquid', () => ({ default: init.hyperliquid, WasmEngine: class {} }));
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
    expect(init.hyperliquid).not.toHaveBeenCalled();
    expect(init.mexc).not.toHaveBeenCalled();
    expect(init.weex).not.toHaveBeenCalled();
    expect(init.binance).not.toHaveBeenCalled();
    expect(init.backpack).not.toHaveBeenCalled();
    expect(init.bingx).not.toHaveBeenCalled();
    expect(init.bitget).not.toHaveBeenCalled();
    expect(init.blofin).not.toHaveBeenCalled();
    expect(init.bitrue).not.toHaveBeenCalled();
    expect(init.lighter).not.toHaveBeenCalled();
    expect(init.woo).not.toHaveBeenCalled();
    expect(init.orderly).not.toHaveBeenCalled();
    expect(init.asterdex).not.toHaveBeenCalled();
    expect(init.kucoin).not.toHaveBeenCalled();
    expect(init.kraken).not.toHaveBeenCalled();
  });

  it('initializes each requested family separately', async () => {
    const { loadWebEngine } = await import('../exchanges/rust/loadWebEngine');
    const families = {
      kraken: ['kraken'],
      asterdex: ['asterdex'],
      kucoin: ['kucoin'],
      backpack: ['backpack'],
      binance: ['binance', 'binancepm'],
      bingx: ['bingx'],
      bitget: ['bitget', 'bitgetusdc', 'bitgetinverse'],
      bitrue: ['bitrue'],
      blofin: ['blofin'],
      bybit: ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'],
      cryptocom: ['cryptocom'],
      hyperliquid: ['hyperliquid'],
      lighter: ['lighter'],
      mexc: ['mexc'],
      okx: ['okx'],
      orderly: ['orderly', 'tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'],
      weex: ['weex'],
      woo: ['woo'],
    };
    const modules = await Promise.all(Object.values(families).map(async (aliases) => {
      const engines = await Promise.all(aliases.map(loadWebEngine));
      expect(engines.every((engine) => engine === engines[0])).toBe(true);
      return engines[0];
    }));
    expect(new Set(modules).size).toBe(18);
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
