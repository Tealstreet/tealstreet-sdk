import { afterEach, expect, it, vi } from 'vitest';

import * as construction from '../../exchanges/rust/construction';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { MarginType, PositionMode } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});
const build = (name: 'phemex' | 'bybit', enabled: boolean) => {
  const policy = construction.deriveBridgeConstruction(name, { key: '', secret: '' });
  vi.spyOn(construction, 'deriveBridgeConstruction').mockReturnValue({
    ...policy,
    helpers: { resolveLeverageContext: enabled },
  });
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(name, { key: '', secret: '' }, store);
  exchanges.push(exchange);
  adoptEngineCapabilities(exchange, { leverageQuery: 'settings' });
  const command = vi.fn().mockResolvedValue({
    buyLeverage: 3,
    sellLeverage: 7,
    marginType: MarginType.Cross,
    positionMode: PositionMode.Hedged,
  });
  Object.defineProperty(exchange, 'command', { value: command });
  return { exchange, store, command };
};

it('exposes the resolver before startup when the declared capability enables it, regardless of venue name', async () => {
  const { exchange, command } = build('bybit', true);
  expect(exchange.hasStarted).toBe(false);
  expect(exchange.resolveLeverageContext).toBeTypeOf('function');
  await expect(exchange.resolveLeverageContext!('ADAUSDT')).resolves.toMatchObject({
    longLeverage: 3,
    shortLeverage: 7,
  });
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchLeverage', symbol: 'ADAUSDT' }, { report: false });
});

it('does not install the helper from a Phemex name when capability support is false', () => {
  const { exchange } = build('phemex', false);
  expect(exchange.resolveLeverageContext).toBeUndefined();
});

it('uses the declared capability for fresh leverage dispatch, independently of venue name', async () => {
  const { exchange, command } = build('bybit', true);
  await expect(exchange.fetchLeverageForSymbol('ADAUSDT', true)).resolves.toMatchObject({
    buyLeverage: 3,
    sellLeverage: 7,
  });
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchLeverage', symbol: 'ADAUSDT' }, { report: false });
});

it('keeps the default cached path when fresh-context support is undeclared, including on a Phemex name', async () => {
  const { exchange, store, command } = build('phemex', false);
  store.update({ fetchedLeverage: { ADAUSDT: { buyLeverage: 99, sellLeverage: 99 } } });
  await expect(exchange.fetchLeverageForSymbol('ADAUSDT', true)).resolves.toMatchObject({
    buyLeverage: 99,
    sellLeverage: 99,
  });
  expect(command).not.toHaveBeenCalled();
});

it('the shared construction declaration enables only the Phemex resolver until another venue declares a helper', () => {
  expect(construction.deriveBridgeConstruction('phemex', { key: '', secret: '' }).helpers).toEqual({
    resolveLeverageContext: true,
  });
  for (const name of ['bybit', 'okx', 'binance'] as const)
    expect(construction.deriveBridgeConstruction(name, { key: '', secret: '' }).helpers).toBeUndefined();
});
