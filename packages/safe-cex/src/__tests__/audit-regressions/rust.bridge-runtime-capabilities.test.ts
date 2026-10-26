vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { EngineCapabilities } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName } from '../../types';
import { afterEach, expect, it, vi } from 'vitest';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});
const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100 };
const make = (name: ExchangeName, capabilities: Partial<EngineCapabilities>) => {
  const exchange = new RustEngineExchange(name, { key: 'fixture', secret: 'fixture' }, new DefaultStore());
  const command = vi.fn().mockResolvedValue([]);
  Object.assign(exchange, { host: { command, dispose: vi.fn() } });
  adoptEngineCapabilities(exchange, capabilities);
  running.push(exchange);
  return { exchange, command };
};

it.each(['mode', 'txflow'] as const)('%s pre-start operations reject visibly once with typed method-specific errors', async (name) => {
  vi.spyOn(console, 'trace').mockImplementation(() => {});
  for (const [method, call] of [
    ['fetchHistoricalOrders', (exchange: RustEngineExchange) => exchange.fetchHistoricalOrders({ orderId: '42' })],
    ['fetchHistoricalFills', (exchange: RustEngineExchange) => exchange.fetchHistoricalFills({ from: 1 })],
    ['fetchHistoricalPositions', (exchange: RustEngineExchange) => exchange.fetchHistoricalPositions({ account: 'fixture' })],
    ['fetchStpMode', (exchange: RustEngineExchange) => exchange.fetchStpMode()],
    ['changeStpMode', (exchange: RustEngineExchange) => exchange.changeStpMode('EXPIRE_TAKER')],
    ['placeOrders', (exchange: RustEngineExchange) => exchange.placeOrders([order])],
  ] as const) {
    const exchange = new RustEngineExchange(name, { key: '', secret: '' }, new DefaultStore());
    running.push(exchange);
    const errors = vi.fn();
    exchange.on('error', errors);
    const start = vi.spyOn(exchange, 'start');
    const load = vi.spyOn(exchange, 'loadCapabilities');
    const failure = await call(exchange).then(() => undefined, (error: unknown) => error);
    expect(failure).toBeInstanceOf(ExchangeError);
    expect(failure).toMatchObject({
      kind: 'invalidInput', outcome: 'notSent', errorReported: true,
      message: expect.stringContaining(`${name}.${method}: exchange has not started`),
    });
    expect(errors, method).toHaveBeenCalledOnce();
    expect(errors.mock.calls[0][0]).toBe((failure as Error).message);
    expect(errors.mock.calls[0][1]).toMatchObject({ classified: { kind: 'invalidInput', outcome: 'notSent' } });
    expect(start).not.toHaveBeenCalled();
    expect(load).not.toHaveBeenCalled();
    expect(exchange.engineCapabilities).toBeUndefined();
  }
});

it.each([
  ['asProvided', -1, 1_000_000_000_000, -1, 1_000_000_000_000],
  ['secondsOrMilliseconds', 1.25, 1_000_000_000_000, 1250, 1_000_000_000_000_000],
  ['nonnegativeSecondsOrMilliseconds', -1, 1_000_000_000_000, undefined, 1_000_000_000_000],
] as const)('uses %s history units independently of venue names', async (fillHistoryTime, from, to, expectedFrom, expectedTo) => {
  const { exchange, command } = make('binance', { fillHistoryTime });
  await exchange.fetchHistoricalFills({ from, to });
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchFills', from: expectedFrom, to: expectedTo });
});

it.each([['fillsAccount', false], ['fillsSymbol', true]] as const)(
  'selects the %s journal from the record', async (positionHistory, symbolScoped) => {
    const { exchange } = make('txflow', { positionHistory });
    const fold = vi.spyOn(
      exchange as unknown as { foldClosedPositionRecords: (...args: unknown[]) => Promise<unknown[]> },
      'foldClosedPositionRecords',
    ).mockResolvedValue([]);
    await exchange.fetchHistoricalPositions({ account: 'fixture' });
    expect(fold).toHaveBeenCalledExactlyOnceWith({ account: 'fixture' }, { symbolScoped });
  },
);

it('rejects unsupported history filters from the record before dispatch', async () => {
  const { exchange, command } = make('binance', { rejectOrderHistoryFilters: true });
  await expect(exchange.fetchHistoricalOrders({ orderId: '42' })).rejects.toThrow('does not support orderId or status filters');
  expect(command).not.toHaveBeenCalled();
});

it('enforces finite command numbers and history bounds from the record', async () => {
  const { exchange, command } = make('woo', { finiteCommandNumbers: true, finiteHistoryBounds: true });
  await expect(exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m', from: NaN })).rejects.toThrow('command numbers must be finite');
  await expect(exchange.fetchHistoricalFills({ from: Infinity })).rejects.toThrow('history bounds must be finite');
  expect(command).not.toHaveBeenCalled();
});

it('uses execution-control and client-slippage policies independently of the venue name', async () => {
  const refused = make('bybit', { bybitExecutionControls: false });
  const controlledOrder = { ...order, bybitBbo: {} };
  await expect(refused.exchange.placeOrders([controlledOrder])).rejects.toThrow('does not support Bybit BBO');
  expect(refused.command).not.toHaveBeenCalled();
  const strict = make('binance', { bybitExecutionControls: true, rejectClientSlippage: true });
  await expect(strict.exchange.placeOrders([{ ...order, maxSlippagePct: 0.1 }])).rejects.toThrow('HyroTrader does not support client-defined slippage');
  expect(strict.command).not.toHaveBeenCalled();
});

it('selects STP support from the record and retains the engine-reported version guard', async () => {
  const { exchange, command } = make('binance', { stpControls: true });
  Object.assign(exchange, { constructionRecord: { asterdexApiVersion: 'v3' } });
  await exchange.changeStpMode('EXPIRE_TAKER');
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'setStpMode', mode: 'EXPIRE_TAKER' });
});
