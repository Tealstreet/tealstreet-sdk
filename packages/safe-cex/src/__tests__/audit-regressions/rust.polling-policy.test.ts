import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, Market, Ticker } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { RUST_VENUES, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';

vi.mock('../../exchanges/bybit/bybit.types', async (original) => ({
  ...(await original<typeof import('../../exchanges/bybit/bybit.types')>()),
  HYROTRADER_SIMULATOR_URL: { livenet: 'https://hyro.fixture', testnet: 'https://hyro.fixture' },
  HYROTRADER_SIMULATOR_WS_URL: {
    public: { linear: { livenet: 'wss://hyro.fixture/public', testnet: 'wss://hyro.fixture/public' } },
    private: { livenet: 'wss://hyro.fixture/private', testnet: 'wss://hyro.fixture/private' },
  },
}));

// Scales are the resolved tick over the metadata tick (binance 500ms, tealstreet-dex 2s).
type Control = { op: string; scale?: number | null; ceiling?: number };
const accounts: RustEngineExchange[] = [];

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  accounts.splice(0).forEach((account) => account.dispose());
  setTickIntervalMultipliers(undefined);
  vi.useRealTimers();
});

const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};
const ticker = {
  id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 99, ask: 101, last: 100, mark: 100, index: 100,
  percentage: 0, fundingRate: 0, volume: 0, quoteVolume: 0, openInterest: 0,
} as Ticker;

function fixture(name: ExchangeName = 'binance', options: Partial<ExchangeOptions> = {}) {
  const commands: Control[] = [];
  const events: string[] = [];
  const errors: unknown[] = [];
  let onControl = (_control: Control): void => {};
  let release!: () => void;
  const idle = () => '[]';
  // Startup is authorized by native market identity and ticker state, not just flags.
  const ready = () =>
    new Uint8Array(
      consumerFrame({
        markets: [market],
        tickers: [{ ...ticker }],
        loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
      }),
    );
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: vi.fn(() => {
      events.push('start');
      return '[]';
    }),
    command: (json) => {
      const command: Control = JSON.parse(json);
      commands.push(command);
      events.push(command.op);
      if (command.op === 'setPollScale') onControl(command);
      return '[]';
    },
    takeFrame: ready,
    snapshotFrame: ready,
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
  };
  const construct = vi.fn(function (_venue: string, _options: string) {
    return native;
  });
  const module = { WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] };
  setWasmEngineLoader(
    () =>
      new Promise((resolve) => {
        release = () => resolve(module);
      }),
  );
  const exchange = new RustEngineExchange(
    name,
    {
      key: 'fixture-key',
      secret: 'fixture-secret',
      publicDataOnly: true,
      ...options,
    },
    new DefaultStore(),
  );
  exchange.emitter.on('error', (error: unknown) => errors.push(error));
  accounts.push(exchange);
  const controls = () => commands.filter((command) => command.op === 'setPollScale');
  const start = async () => {
    const starting = exchange.start();
    release();
    await vi.advanceTimersByTimeAsync(250);
    await starting;
  };
  return {
    exchange,
    native,
    construct,
    commands,
    events,
    errors,
    release: () => release(),
    controls,
    start,
    onControl: (callback: typeof onControl) => {
      onControl = callback;
    },
  };
}

describe('Rust periodic polling policy propagation', () => {
  it.each(Object.keys(RUST_VENUES) as ExchangeName[])(
    '%s applies its resolved polling scale before native startup',
    async (name) => {
      setTickIntervalMultipliers({ '*': 3 });
      const r = fixture(name);
      await r.start();
      expect(r.events.slice(0, 2)).toEqual(['setPollScale', 'start']);
      expect(r.controls()).toEqual([{ op: 'setPollScale', scale: 3, ceiling: 3 * getExchangeMetadata(name).tickInterval! }]);
      expect(JSON.parse(r.construct.mock.calls[0][1])).not.toHaveProperty('tickInterval');
      expect(JSON.parse(r.construct.mock.calls[0][1])).not.toHaveProperty('pollingEnabled');
    },
  );

  it('pauses with explicit null and resumes after effective options change at the existing flush cadence', async () => {
    const r = fixture('tealstreet-dex', { extra: { tickInterval: Infinity } });
    await r.start();
    expect(r.controls()).toEqual([{ op: 'setPollScale', scale: null }]);
    r.exchange.options.extra!.tickInterval = 4000;
    await vi.advanceTimersByTimeAsync(250);
    expect(r.controls().at(-1)).toEqual({ op: 'setPollScale', scale: 2, ceiling: 4000 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.controls()).toHaveLength(2);
    r.exchange.options.extra!.tickInterval = Infinity;
    await vi.advanceTimersByTimeAsync(250);
    expect(r.controls().at(-1)).toEqual({ op: 'setPollScale', scale: null });
  });

  it('composes server slowdown before Base auth backoff and its cap, then reflects success', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    await r.start();
    setTickIntervalMultipliers({ '*': 3 });
    r.exchange.recordAuthFailure();
    expect(r.controls().at(-1)?.scale).toBe(12);
    for (let i = 0; i < 20; i++) r.exchange.recordAuthFailure();
    expect(r.controls().at(-1)?.scale).toBe(120);
    // The Base auth cap still bounds the slowest absolute cadence.
    expect(r.controls().at(-1)?.ceiling).toBe(60_000);
    const count = r.controls().length;
    r.exchange.recordAuthFailure();
    expect(r.controls()).toHaveLength(count);
    r.exchange.recordAuthSuccess();
    expect(r.controls().at(-1)?.scale).toBe(6);
    setTickIntervalMultipliers(undefined);
    expect(r.controls().at(-1)?.scale).toBe(2);
  });

  it('uses the latest pre-host multiplier, options and auth policy after a delayed loader', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    const starting = r.exchange.start();
    r.exchange.options.extra!.tickInterval = 2000;
    setTickIntervalMultipliers({ '*': 4 });
    r.exchange.recordAuthFailure();
    expect(r.commands).toEqual([]);
    r.release();
    await vi.advanceTimersByTimeAsync(250);
    await starting;
    expect(r.controls()).toEqual([{ op: 'setPollScale', scale: 32, ceiling: 16_000 }]);
  });

  it('does not duplicate unchanged policies or keep a disposed account subscribed when replacing it', async () => {
    const old = fixture('binance', { extra: { tickInterval: 1000 } });
    await old.start();
    const timers = vi.getTimerCount();
    setTickIntervalMultipliers({ '*': 1 });
    old.exchange.recordAuthSuccess();
    await vi.advanceTimersByTimeAsync(500);
    expect(old.controls()).toHaveLength(1);
    expect(vi.getTimerCount()).toBe(timers);
    old.exchange.dispose();
    const replacement = fixture('binance', { extra: { tickInterval: 1000 } });
    await replacement.start();
    setTickIntervalMultipliers({ '*': 2 });
    await vi.advanceTimersByTimeAsync(500);
    expect(old.controls()).toHaveLength(1);
    expect(replacement.controls().map((control) => control.scale)).toEqual([2, 4]);
  });

  it('finishes a newer reentrant initialization policy before native startup', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    r.onControl((control) => {
      if (control.scale === 2) setTickIntervalMultipliers({ '*': 2 });
    });
    await r.start();
    expect(r.events.slice(0, 3)).toEqual(['setPollScale', 'setPollScale', 'start']);
    expect(r.controls().map((control) => control.scale)).toEqual([2, 4]);
  });

  it('rejects failed initial policy delivery before native startup and wakes public reads', async () => {
    const r = fixture();
    const failure = new Error('policy rejected');
    r.onControl(() => {
      throw failure;
    });
    const starting = r.exchange.start();
    const read = r.exchange.fetchMarkets();
    const rejected = Promise.all([
      expect(starting).rejects.toBe(failure),
      expect(read).rejects.toThrow('policy rejected'),
    ]);
    r.release();
    await vi.advanceTimersByTimeAsync(0);
    await rejected;
    expect(r.native.start).not.toHaveBeenCalled();
    expect(r.exchange.isDisposed).toBe(true);
    expect(r.errors).toHaveLength(1);
  });

  it('reports failed updates once and retries unchanged policy without issuing a trading command', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    await r.start();
    let reject = true;
    r.onControl(() => {
      if (reject) {
        reject = false;
        throw new Error('policy update failed');
      }
    });
    setTickIntervalMultipliers({ '*': 2 });
    await vi.advanceTimersByTimeAsync(0);
    expect(r.errors).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(250);
    expect(r.controls().map((control) => control.scale)).toEqual([2, 4, 4]);
    await vi.advanceTimersByTimeAsync(500);
    expect(r.controls()).toHaveLength(3);
    expect(r.commands.every((command) => ['setPollScale', 'setTickerSymbols'].includes(command.op))).toBe(true);
  });

  it('preserves a newer reentrant policy when an older dispatch fails', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    await r.start();
    r.onControl((control) => {
      if (control.scale === 4) {
        setTickIntervalMultipliers({ '*': 3 });
        throw new Error('obsolete policy failed');
      }
    });
    setTickIntervalMultipliers({ '*': 2 });
    await vi.advanceTimersByTimeAsync(500);
    expect(r.controls().map((control) => control.scale)).toEqual([2, 4, 6]);
    expect(r.errors).toHaveLength(1);
  });
});
