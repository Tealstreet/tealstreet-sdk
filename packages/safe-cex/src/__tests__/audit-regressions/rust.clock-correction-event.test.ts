import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

vi.mock('../../utils/virtualClock', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});

const accounts: RustEngineExchange[] = [];
afterEach(() => accounts.splice(0).forEach((exchange) => exchange.dispose()));

const account = () => {
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  accounts.push(exchange);
  return exchange;
};

it('lets an explicit clock-advice listener replace the generic info log', () => {
  const exchange = account();
  const events = vi.fn();
  exchange.on('engine-event', events);
  const advice = vi.fn();
  exchange.on('clock-advice', advice);
  const log = vi.spyOn(exchange, 'log').mockImplementation(() => {});
  const event = {
    code: 'bybit.clock.corrected',
    level: 'info',
    message: 'Bybit protocol clock corrected',
    fields: { offsetMs: -713_451 },
  };
  (exchange as any).receiveEvent(event);
  expect(events).toHaveBeenCalledWith(event);
  expect(advice).toHaveBeenCalledWith(event);
  expect(log).not.toHaveBeenCalled();
});

it('keeps ordinary information and real errors on their existing routes', () => {
  const exchange = account();
  const errors = vi.fn();
  exchange.on('error', errors);
  const log = vi.spyOn(exchange, 'log').mockImplementation(() => {});
  (exchange as any).receiveEvent({ code: 'request.retry', level: 'info', message: 'normal retry', fields: {} });
  expect(log).toHaveBeenCalledOnce();
  (exchange as any).receiveEvent({ code: 'bybit.clock.corrected', level: 'error', message: 'real error', fields: {} });
  expect(errors).toHaveBeenCalledOnce();
});

it('keeps the clock info log when the host has no advice listener, even with a telemetry observer', () => {
  const exchange = account();
  const events = vi.fn();
  exchange.on('engine-event', events);
  const log = vi.spyOn(exchange, 'log').mockImplementation(() => {});
  const event = {
    code: 'bybit.clock.corrected',
    level: 'info',
    message: 'Bybit protocol clock corrected',
    fields: { offsetMs: -713_451 },
  };
  (exchange as any).receiveEvent(event);
  expect(events).toHaveBeenCalledWith(event);
  expect(log).toHaveBeenCalledOnce();
  expect(log.mock.calls[0][2]).toBe(event);
});
