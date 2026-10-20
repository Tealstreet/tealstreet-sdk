import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { RUST_VENUES, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { consumerFrame } from '../cutover/consumerFrame';

vi.mock('../../exchanges/bybit/bybit.types', async (original) => ({
  ...(await original<typeof import('../../exchanges/bybit/bybit.types')>()),
  HYROTRADER_SIMULATOR_URL: { livenet: 'https://hyro.fixture', testnet: 'https://hyro.fixture' },
  HYROTRADER_SIMULATOR_WS_URL: {
    public: { linear: { livenet: 'wss://hyro.fixture/public', testnet: 'wss://hyro.fixture/public' } },
    private: { livenet: 'wss://hyro.fixture/private', testnet: 'wss://hyro.fixture/private' },
  },
}));

type Control = { op: string; interval?: number | null };
const accounts: RustEngineExchange[] = [];

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  accounts.splice(0).forEach((account) => account.dispose());
  setTickIntervalMultipliers(undefined);
  vi.useRealTimers();
});

function fixture(name: ExchangeName = 'binance', options: Partial<ExchangeOptions> = {}) {
  const commands: Control[] = [];
  const events: string[] = [];
  const errors: unknown[] = [];
  let onControl = (_control: Control): void => {};
  let release!: () => void;
  const idle = () => '[]';
  const ready = () =>
    new Uint8Array(
      consumerFrame({
        loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
      }),
    );
  const native: WasmEngineLike = {
    start: vi.fn(() => {
      events.push('start');
      return '[]';
    }),
    command: (json) => {
      const command: Control = JSON.parse(json);
      commands.push(command);
      events.push(command.op);
      if (command.op === 'setTickInterval') onControl(command);
      return '[]';
    },
    takeFrame: ready,
    snapshotFrame: ready,
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle,
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
  const controls = () => commands.filter((command) => command.op === 'setTickInterval');
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
    '%s applies its resolved interval before native startup',
    async (name) => {
      setTickIntervalMultipliers({ '*': 3 });
      const r = fixture(name);
      await r.start();
      expect(r.events.slice(0, 2)).toEqual(['setTickInterval', 'start']);
      expect(r.controls()).toEqual([{ op: 'setTickInterval', interval: exchangeMetadata[name]!.tickInterval! * 3 }]);
      expect(JSON.parse(r.construct.mock.calls[0][1])).not.toHaveProperty('tickInterval');
      expect(JSON.parse(r.construct.mock.calls[0][1])).not.toHaveProperty('pollingEnabled');
    },
  );

  it('pauses with explicit null and resumes after effective options change at the existing flush cadence', async () => {
    const r = fixture('tealstreet-dex', { extra: { tickInterval: Infinity } });
    await r.start();
    expect(r.controls()).toEqual([{ op: 'setTickInterval', interval: null }]);
    r.exchange.options.extra!.tickInterval = 4000;
    await vi.advanceTimersByTimeAsync(250);
    expect(r.controls().at(-1)).toEqual({ op: 'setTickInterval', interval: 4000 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(r.controls()).toHaveLength(2);
    r.exchange.options.extra!.tickInterval = Infinity;
    await vi.advanceTimersByTimeAsync(250);
    expect(r.controls().at(-1)).toEqual({ op: 'setTickInterval', interval: null });
  });

  it('composes server slowdown before Base auth backoff and its cap, then reflects success', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    await r.start();
    setTickIntervalMultipliers({ '*': 3 });
    r.exchange.recordAuthFailure();
    expect(r.controls().at(-1)?.interval).toBe(6000);
    for (let i = 0; i < 20; i++) r.exchange.recordAuthFailure();
    expect(r.controls().at(-1)?.interval).toBe(60_000);
    const count = r.controls().length;
    r.exchange.recordAuthFailure();
    expect(r.controls()).toHaveLength(count);
    r.exchange.recordAuthSuccess();
    expect(r.controls().at(-1)?.interval).toBe(3000);
    setTickIntervalMultipliers(undefined);
    expect(r.controls().at(-1)?.interval).toBe(1000);
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
    expect(r.controls()).toEqual([{ op: 'setTickInterval', interval: 16_000 }]);
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
    expect(replacement.controls().map((control) => control.interval)).toEqual([1000, 2000]);
  });

  it('finishes a newer reentrant initialization policy before native startup', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    r.onControl((control) => {
      if (control.interval === 1000) setTickIntervalMultipliers({ '*': 2 });
    });
    await r.start();
    expect(r.events.slice(0, 3)).toEqual(['setTickInterval', 'setTickInterval', 'start']);
    expect(r.controls().map((control) => control.interval)).toEqual([1000, 2000]);
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
    expect(r.controls().map((control) => control.interval)).toEqual([1000, 2000, 2000]);
    await vi.advanceTimersByTimeAsync(500);
    expect(r.controls()).toHaveLength(3);
    expect(r.commands.every((command) => ['setTickInterval', 'setTickerSymbols'].includes(command.op))).toBe(true);
  });

  it('preserves a newer reentrant policy when an older dispatch fails', async () => {
    const r = fixture('binance', { extra: { tickInterval: 1000 } });
    await r.start();
    r.onControl((control) => {
      if (control.interval === 2000) {
        setTickIntervalMultipliers({ '*': 3 });
        throw new Error('obsolete policy failed');
      }
    });
    setTickIntervalMultipliers({ '*': 2 });
    await vi.advanceTimersByTimeAsync(500);
    expect(r.controls().map((control) => control.interval)).toEqual([1000, 2000, 3000]);
    expect(r.errors).toHaveLength(1);
  });
});
