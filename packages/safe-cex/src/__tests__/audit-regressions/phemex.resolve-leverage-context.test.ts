import type { Position } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { WORKER_WRITE_METHODS } from '../../exchanges/base';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode, PositionSide } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { PhemexExchange } from '../reference/phemex/phemex.exchange';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));
const build = (workerMirror = false) => {
  const store = new DefaultStore();
  const exchange = new RustEngineExchange('phemex', { key: '', secret: '', workerMirror }, store);
  exchanges.push(exchange);
  adoptEngineCapabilities(exchange);
  const command = vi.fn().mockResolvedValue({
    buyLeverage: 10,
    sellLeverage: 7,
    marginType: MarginType.Cross,
    positionMode: PositionMode.Hedged,
  });
  Object.defineProperty(exchange, 'command', { value: command });
  return { exchange, store, command };
};
const stored = (side = PositionSide.Long) =>
  ({
    id: side,
    symbol: 'ADAUSDT',
    side,
    leverage: side === PositionSide.Long ? -3 : -5,
    marginType: MarginType.Isolated,
    positionMode: PositionMode.OneWay,
  }) as Position;
const resolve = (exchange: RustEngineExchange, symbol = 'ADAUSDT') => (exchange as any).resolveLeverageContext(symbol);

describe('Rust Phemex leverage context contract', () => {
  it('exposes the flat-symbol helper and reads fresh native context rather than cached rails', async () => {
    const { exchange, store, command } = build();
    store.update({ fetchedLeverage: { ADAUSDT: { buyLeverage: 99, sellLeverage: 99 } } });
    expect(store.loaded.positions).toBe(false);
    expect(store.positions).toEqual([]);
    await expect(resolve(exchange)).resolves.toEqual({
      longLeverage: 10,
      shortLeverage: 7,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
    });
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchLeverage', symbol: 'ADAUSDT' }, { report: false });
  });

  it('prefers the current raw read over stale stored positions', async () => {
    const { exchange, store } = build();
    store.update({ positions: [stored()] });
    await expect(resolve(exchange)).resolves.toMatchObject({ marginType: MarginType.Cross, longLeverage: 10 });
  });

  it.each(['empty', 'failed'] as const)('falls back to complete stored legs on an %s read', async (kind) => {
    const { exchange, store, command } = build();
    if (kind === 'empty') command.mockResolvedValue(null);
    else command.mockRejectedValue(new Error('fixture read failed'));
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined);
    store.update({ positions: [stored(PositionSide.Short), stored()] });
    await expect(resolve(exchange)).resolves.toEqual({
      longLeverage: 3,
      shortLeverage: 5,
      marginType: MarginType.Isolated,
      positionMode: PositionMode.OneWay,
    });
  });

  it.each(['none', 'margin', 'mode'] as const)('never guesses missing %s stored context', async (missing) => {
    const { exchange, store, command } = build();
    command.mockResolvedValue(null);
    const position = stored();
    if (missing === 'margin') Reflect.deleteProperty(position, 'marginType');
    if (missing === 'mode') Reflect.deleteProperty(position, 'positionMode');
    store.update({ positions: missing === 'none' ? [] : [position] });
    await expect(resolve(exchange)).resolves.toBeUndefined();
  });

  it('uses the first symbol leg for a missing side, as the TS helper does', async () => {
    const { exchange, store, command } = build();
    command.mockResolvedValue(null);
    store.update({ positions: [stored(PositionSide.Short)] });
    await expect(resolve(exchange)).resolves.toMatchObject({ longLeverage: 5, shortLeverage: 5 });
  });

  it('uses the existing hosted/browser worker read without starting a page engine', async () => {
    const loader = vi.fn(() => new Promise<never>(() => {}));
    setWasmEngineLoader(loader);
    const { exchange, command } = build(true);
    const result = {
      marginType: MarginType.Cross,
      positionMode: PositionMode.OneWay,
      longLeverage: 9,
      shortLeverage: 9,
    };
    exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      expect(event.method).toBe('fetchLeverageForSymbol');
      expect(event.args).toEqual(['ADAUSDT', true]);
      exchange.emitter.emit('callMethod:done', {
        requestId: event.requestId,
        success: true,
        result: { marginType: result.marginType, positionMode: result.positionMode, buyLeverage: 9, sellLeverage: 9 },
      });
    });
    await expect(resolve(exchange)).resolves.toEqual(result);
    expect(command).not.toHaveBeenCalled();
    expect(loader).not.toHaveBeenCalled();

    expect(WORKER_WRITE_METHODS.has('resolveLeverageContext')).toBe(false);
  });

  it('drops an owner A read queued before the bridge binds owner B', async () => {
    const { exchange, command } = build(true);
    let owner = { ownerId: 'owner-a', generation: 1 };
    exchange.setWorkerOwner(() => owner);
    const pending = resolve(exchange);
    const rejected = expect(pending).rejects.toMatchObject({ code: 'WORKER_STOPPED' });
    owner = { ownerId: 'owner-b', generation: 2 };
    const dispatch = vi.fn();
    exchange.on('callMethod', dispatch);
    await rejected;
    expect(dispatch).not.toHaveBeenCalled();
    expect(command).not.toHaveBeenCalled();
  });

  it('lets the worker fresh-read path bypass Phemex cached settings without mutating them', async () => {
    const { exchange, store, command } = build();
    const cached = { buyLeverage: 99, sellLeverage: 99 };
    store.update({ fetchedLeverage: { ADAUSDT: cached } });
    await expect((exchange.fetchLeverageForSymbol as any)('ADAUSDT', true)).resolves.toMatchObject({
      buyLeverage: 10,
      sellLeverage: 7,
    });
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchLeverage', symbol: 'ADAUSDT' }, { report: false });
    expect(store.fetchedLeverage.ADAUSDT).toEqual(cached);
  });

  it('drops owner A metadata delivered after the bridge has switched to owner B', async () => {
    const { exchange, store } = build(true);
    let owner = { ownerId: 'owner-a', generation: 1 };
    exchange.setWorkerOwner(() => owner);
    const dispatched = Promise.withResolvers<string>();
    exchange.on('callMethod', (event: { requestId: string }) => dispatched.resolve(event.requestId));
    const pending = resolve(exchange);
    const rejected = expect(pending).rejects.toMatchObject({ code: 'WORKER_STOPPED' });
    const requestId = await dispatched.promise;
    owner = { ownerId: 'owner-b', generation: 2 };
    store.update({ positions: [stored()] });
    exchange.emitter.emit('callMethod:done', {
      requestId,
      success: true,
      result: { buyLeverage: 10, sellLeverage: 10, marginType: MarginType.Cross, positionMode: PositionMode.OneWay },
    });
    await rejected;
  });

  it.each([
    ['OneWay', -10, -10, false, MarginType.Cross, PositionMode.OneWay],
    ['Hedged', 3, 7, false, MarginType.Isolated, PositionMode.Hedged],
    ['Hedged', 5, 9, true, MarginType.Cross, PositionMode.Hedged],
  ] as const)(
    'matches the actual TS helper for %s raw legs %s/%s',
    async (mode, long, short, cross, marginType, positionMode) => {
      const { exchange, command } = build();
      const reference = new PhemexExchange('phemex', { key: '', secret: '' }, new DefaultStore());
      reference.xhr = {
        get: vi.fn().mockResolvedValue({
          data: {
            data: {
              positions: [
                { symbol: 'OTHERUSDT', leverageRr: '99' },
                {
                  symbol: 'ADAUSDT',
                  posSide: 'Short',
                  posMode: mode,
                  leverageRr: String(short),
                  crossMargin: cross,
                  size: '0',
                },
                {
                  symbol: 'ADAUSDT',
                  posSide: 'Long',
                  posMode: mode,
                  leverageRr: String(long),
                  crossMargin: cross,
                  size: '0',
                },
              ],
            },
          },
        }),
      } as any;
      command.mockResolvedValue({
        buyLeverage: Math.abs(long),
        sellLeverage: Math.abs(short),
        marginType,
        positionMode,
      });
      try {
        await expect(resolve(exchange)).resolves.toEqual(await (reference as any).resolveLeverageContext('ADAUSDT'));
      } finally {
        reference.dispose();
      }
    },
  );

  it('keeps the helper absent on other Rust venues', () => {
    const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    exchanges.push(exchange);
    expect((exchange as any).resolveLeverageContext).toBeUndefined();
  });
});
