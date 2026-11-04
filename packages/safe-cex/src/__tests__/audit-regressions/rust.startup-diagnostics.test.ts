import { afterEach, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { startupDiagnosticTags } from '../../exchanges/rust/startupDiagnostics';
import { DefaultStore } from '../../store/store.base';

vi.mock('../../utils/virtualClock', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});

const accounts: RustEngineExchange[] = [];
afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  vi.unstubAllGlobals();
});

it('carries startup stage, elapsed/deadline and 429 count through the real error telemetry boundary', () => {
  vi.stubGlobal('window', undefined);
  const exchange = new RustEngineExchange(
    'hyperliquid',
    { key: '', secret: '', publicDataOnly: true },
    new DefaultStore(),
  );
  accounts.push(exchange);
  const errors = vi.fn();
  exchange.on('error', errors);
  (exchange as any).receiveEvent({
    code: 'startup.incomplete',
    level: 'error',
    message: 'Startup incomplete: still waiting for orders',
    fields: {
      stage: 'frontendOpenOrders',
      elapsedMs: 60_123,
      deadlineMs: 60_000,
      readTimeoutMs: 20_000,
      http429Count: 3,
    },
  });
  expect(errors).toHaveBeenCalledTimes(1);
  expect(errors.mock.calls[0][1].sentryData.tags).toMatchObject({
    startup_stage: 'frontendOpenOrders',
    startup_elapsed_ms: '60123',
    startup_deadline_ms: '60000',
    startup_read_timeout_ms: '20000',
    startup_http_429_count: '3',
  });
});

it('does not turn arbitrary stage text or invalid counters into Sentry tags', () => {
  expect(
    startupDiagnosticTags({
      code: 'startup.incomplete',
      level: 'error',
      message: '',
      fields: {
        stage: 'wallet-or-request-data',
        elapsedMs: NaN,
        deadlineMs: -1,
        readTimeoutMs: Infinity,
        http429Count: 'untrusted',
      },
    }),
  ).toEqual({});
  expect(
    startupDiagnosticTags({
      code: 'request.failed',
      level: 'error',
      message: '',
      fields: { stage: 'frontendOpenOrders', http429Count: 3 },
    }),
  ).toEqual({});
});

it('forwards bounded request failure facts with the actual venue and no additional error', () => {
  vi.stubGlobal('window', undefined);
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  accounts.push(exchange);
  const events = vi.fn(),
    errors = vi.fn();
  exchange.on('engine-event', events);
  exchange.on('error', errors);
  (exchange as any).receiveEvent({
    code: 'http.request_failed',
    level: 'debug',
    message: 'HTTP request failed',
    fields: { method: 'GET', path: '/v5/market/instruments-info', kind: 'timeout', phase: 'preparing' },
  });
  expect(events).toHaveBeenCalledWith(
    expect.objectContaining({
      fields: {
        venue: 'bybit',
        method: 'GET',
        path: '/v5/market/instruments-info',
        kind: 'timeout',
        phase: 'preparing',
      },
    }),
  );
  expect(errors).not.toHaveBeenCalled();
});

it('tags only host-templated deadline metadata on the original read error', () => {
  const event = {
    code: 'request.failed',
    level: 'error' as const,
    message: 'HTTP request timed out',
    fields: { httpMethod: 'GET', httpPath: '/v5/order/:segment', httpPhase: 'released' },
  };
  expect(startupDiagnosticTags(event)).toEqual({
    http_request_method: 'GET',
    http_request_path: '/v5/order/:segment',
    http_request_phase: 'released',
  });
  for (const path of ['/v5/order/SECRET', '/v5/order?signature=SECRET', 'https://api.bybit.com/v5/order'])
    expect(startupDiagnosticTags({ ...event, fields: { ...event.fields, httpPath: path } })).toEqual({});
});
