vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions } from '../../types';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { OrderSide, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const running: RustEngineExchange[] = [];
const fixture = (options: Partial<ExchangeOptions> = {}, startupActions: unknown[] = [], loaded = true) => {
  const commands: Array<Record<string, unknown>> = [];
  const noActions = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('breakoutprop'),
    start: () => JSON.stringify(startupActions),
    command(json) { commands.push(JSON.parse(json)); return '[]'; },
    takeFrame: () => new Uint8Array(consumerFrame({
      loaded: {
        markets: loaded, tickers: loaded,
        balance: loaded && !options.publicDataOnly,
        orders: loaded && !options.publicDataOnly,
        positions: loaded && !options.publicDataOnly,
      },
    })),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions, wsMessage: noActions, wsClose: noActions, wsRouteExhausted: noActions,
    httpStart: noActions, httpChunk: noActions, httpEnd: noActions, httpFailed: noActions, timer: noActions,
  };
  const construct = vi.fn(function (_venue: string, _options: string) { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  if (loaded) store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange('breakoutprop', { key: 'username', secret: 'password', ...options }, store);
  exchange.emitter.on('error', () => {});
  running.push(exchange);
  return { exchange, construct, commands };
};

afterEach(() => { setTickIntervalMultipliers(undefined); for (const exchange of running.splice(0)) exchange.dispose(); vi.restoreAllMocks(); });

describe('BreakoutProp Rust wrapper session and account boundary', () => {
  it('registers the venue and preserves account selection, pacing and timeout options', async () => {
    const { exchange, construct, commands } = fixture({ subaccount: '00002', extra: { tickInterval: 5000, recvWindow: 9000 } });
    expect(isRustVenue('breakoutprop')).toBe(false);
    await exchange.start();
    expect(construct.mock.calls[0][0]).toBe('breakoutprop');
    expect(JSON.parse(construct.mock.calls[0][1])).toMatchObject({ key: 'username', secret: 'password', subaccount: '00002', recvWindow: 9000 });
    expect(commands[0]).toEqual({ op: 'setPollScale', scale: 2.5, ceiling: 5000 });
    expect(commands).toContainEqual({ op: 'setTickerSymbols', symbols: [] });
  });
  it('forwards initial and live incident slowdowns and unsubscribes on disposal', async () => {
    setTickIntervalMultipliers({ '*': 4 });
    const { exchange, commands } = fixture({ extra: { tickInterval: 5000 } });
    await exchange.start();
    expect(commands[0]).toEqual({ op: 'setPollScale', scale: 10, ceiling: 20_000 });
    setTickIntervalMultipliers({ '*': 4, breakoutprop: 2 });
    expect(commands.at(-1)).toEqual({ op: 'setPollScale', scale: 5, ceiling: 10_000 });
    setTickIntervalMultipliers(undefined);
    expect(commands.at(-1)).toEqual({ op: 'setPollScale', scale: 2.5, ceiling: 5000 });
    exchange.dispose();
    const count = commands.length;
    setTickIntervalMultipliers({ '*': 3 });
    expect(commands).toHaveLength(count);
  });
  it('preserves an explicitly paused polling scale across JSON', async () => {
    const { exchange, construct, commands } = fixture({ extra: { tickInterval: Number.POSITIVE_INFINITY } });
    await exchange.start();
    expect(commands[0]).toEqual({ op: 'setPollScale', scale: null });
    expect(JSON.parse(construct.mock.calls[0][1])).not.toHaveProperty('pollingEnabled');
  });
  it('settles account selection startup failures without waiting forever', async () => {
    const message = 'DXtrade startup failed: Multiple accounts found';
    const { exchange } = fixture({}, [{ op: 'event', code: 'test.diagnostic', level: 'error', message }, { op: 'startupFailed', detail: { message } }], false);
    await expect(exchange.start()).rejects.toThrow('Multiple accounts found');
    expect(exchange.isDisposed).toBe(true);
  });
  it('treats an error log alone as a toast, not a startup failure', async () => {
    const { exchange } = fixture({}, [{ op: 'event', code: 'test.diagnostic', level: 'error', message: 'DXtrade startup failed: Multiple accounts found' }], false);
    const outcome = await Promise.race([exchange.start().then(() => 'started', () => 'rejected'), new Promise((resolve) => setTimeout(() => resolve('pending'), 20))]);
    expect(outcome).toBe('pending');
  });
  it('keeps the venue reason when a startup failure precedes deactivation', async () => {
    const message = 'Bybit rejected the API key (10003): API key is invalid.';
    const { exchange } = fixture({}, [{ op: 'startupFailed', detail: { message, kind: 'authInvalid', venueCode: '10003' } }, { op: 'deactivate' }], false);
    const deactivated = vi.fn();
    exchange.emitter.on('deactivate', deactivated);
    await expect(exchange.start()).rejects.toThrow(message);
    expect(deactivated).toHaveBeenCalledOnce();
  });
  it('keeps the startup failure kind for start() and validateAccount', async () => {
    const message = 'Bybit rejected the API key (10003): API key is invalid.';
    const detail = { message, kind: 'authInvalid', venueCode: '10003' };
    const started = fixture({}, [{ op: 'startupFailed', detail }], false);
    await expect(started.exchange.start()).rejects.toMatchObject({ message, kind: 'authInvalid', venueCode: '10003' });
    // Venues log the reason just before StartupFailed in the same batch.
    const validated = fixture({}, [{ op: 'event', code: 'test.diagnostic', level: 'error', message }, { op: 'startupFailed', detail }], false);
    await expect(validated.exchange.validateAccount()).resolves.toEqual({ error: message, kind: 'authInvalid' });
  });
  it('settles definitive login rejection through the shared deactivation contract', async () => {
    const { exchange } = fixture({}, [{ op: 'deactivate' }], false);
    await expect(exchange.start()).rejects.toThrow('credentials were rejected');
    expect(exchange.isDisposed).toBe(true);
  });
  it('does not advertise the market maximum as a writable account leverage', async () => {
    const { exchange } = fixture();
    await exchange.start();
    await expect(exchange.fetchLeverageForSymbol('ETHUSD')).resolves.toBeNull();
    await expect(exchange.fetchSymbolMaxLeverage('ETHUSD')).rejects.toThrow('Not implemented');
  });
  it('brands empty history capabilities once the engine reports them, without I/O', async () => {
    const { exchange, commands } = fixture();
    await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
    const issued = commands.length;
    expect(exchange.capabilities).toMatchObject({
      fillHistory: false,
      orderHistory: false,
      incomeHistory: false,
      incomeRange: false,
    });
    await expect(exchange.fetchHistoricalFills({})).resolves.toEqual([]);
    await expect(exchange.fetchHistoricalOrders()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistory()).resolves.toEqual([]);
    await expect(exchange.fetchIncomeHistoryRange(0, 1000)).resolves.toEqual([]);
    expect(commands).toHaveLength(issued);
  });
  it('validates a loaded public-only legacy session without private portfolio data', async () => {
    const { exchange } = fixture({ publicDataOnly: true });
    exchange.store.update({ loaded: { markets: true, tickers: true, balance: false, positions: false, orders: false } });
    await expect(exchange.validateAccount()).resolves.toMatchObject({ error: '' });
  });
  it('rejects caller client IDs before any batch write', async () => {
    const { exchange, commands } = fixture();
    await exchange.start();
    await expect(exchange.placeOrders([
      { symbol: 'ETHUSD', side: OrderSide.Buy, type: OrderType.Market, amount: 1 },
      { symbol: 'ETHUSD', side: OrderSide.Buy, type: OrderType.Market, amount: 1, clientOrderId: 'caller' },
    ])).rejects.toThrow(/clientOrderId/i);
    expect(commands.every(({ op }) => op === 'setTickerSymbols' || op === 'setPollScale')).toBe(true);
  });
});
