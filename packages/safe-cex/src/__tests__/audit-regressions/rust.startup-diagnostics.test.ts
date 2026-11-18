import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

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
        lastFailedPrivateReadKind: 'wallet-or-request-data',
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

it.each(['state', 'spot', 'orders', 'role', 'mode', 'discovery'])('allows the closed failed-read enum %s', (kind) => {
  expect(
    startupDiagnosticTags({
      code: 'startup.incomplete',
      level: 'warn',
      message: '',
      fields: { lastFailedPrivateReadKind: kind },
    }),
  ).toEqual({ startup_last_failed_private_read_kind: kind });
});

it('accepts failure enums before the watchdog, without inferring a kind from error text', () => {
  const exchange = new RustEngineExchange(
    'hyperliquid',
    { key: '', secret: '', publicDataOnly: true },
    new DefaultStore(),
  );
  accounts.push(exchange);
  exchange.on('error', vi.fn());
  const emit = (fields: Record<string, unknown>, message = 'role orders mode') =>
    (exchange as any).receiveEvent({ code: 'request.failed', level: 'error', message, fields });
  emit({ lastFailedPrivateReadKind: 'spot' });
  expect(exchange.lastFailedPrivateReadKind).toBe('spot');
  expect(exchange.startupDiagnostics.startup_last_failed_private_read_kind).toBe('spot');
  emit({ lastFailedPrivateReadKind: 'wallet-or-request-data' });
  emit({});
  expect(exchange.lastFailedPrivateReadKind).toBe('spot');
});

it('invalidates cached catalogues only on typed discovery failures', () => {
  const exchange = new RustEngineExchange(
    'hyperliquid',
    { key: '', secret: '', publicDataOnly: true },
    new DefaultStore(),
  );
  accounts.push(exchange);
  exchange.on('error', vi.fn());
  const invalidate = vi.fn();
  (exchange as any).invalidateInfoCatalogue = invalidate;
  const emit = (code: string, kind?: string) =>
    (exchange as any).receiveEvent({
      code,
      level: 'warning',
      message: 'discovery failed',
      fields: { lastFailedPrivateReadKind: kind },
    });
  emit('request.failed');
  emit('request.failed', 'orders');
  emit('startup.incomplete', 'discovery');
  expect(invalidate).not.toHaveBeenCalled();
  emit('request.failed', 'discovery');
  expect(invalidate).toHaveBeenCalledTimes(1);
});

it('derives all five startup causes from a hosted-shaped event at the BaseExchange boundary', () => {
  vi.stubGlobal('window', undefined);
  vi.stubGlobal('self', {});
  const exchange = new RustEngineExchange('bitget', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  accounts.push(exchange);
  const errors = vi.fn();
  exchange.on('error', errors);
  const fields: Record<string, string | number> = {};
  for (const section of ['Markets', 'Tickers', 'Balance', 'Orders', 'Positions']) {
    fields[`startup${section}State`] = 'timedOut';
    fields[`startup${section}Kind`] = 'timeout';
    fields[`startup${section}VenueCode`] = '10006';
    fields[`startup${section}HttpStatus`] = 429;
  }
  const message = 'Startup incomplete: still waiting for markets, tickers, balance, orders, positions';
  exchange.emitAndLogError(message, undefined, {
    engineEvent: { code: 'startup.incomplete', level: 'error', message, fields },
  });
  expect(errors).toHaveBeenCalledTimes(1);
  const [text, opts] = errors.mock.calls[0];
  expect(text).toBe(`Exchange Error: ${message}`);
  for (const section of ['markets', 'tickers', 'balance', 'orders', 'positions']) {
    expect(opts.sentryData.tags).toMatchObject({
      [`startup_${section}_state`]: 'timedOut',
      [`startup_${section}_kind`]: 'timeout',
      [`startup_${section}_venue_code`]: '10006',
      [`startup_${section}_http_status`]: '429',
    });
  }
  expect(Object.keys(opts.sentryData.tags).every((key) => key.length <= 32)).toBe(true);
  expect(exchange.startupDiagnostics).not.toHaveProperty('startup_orders_state');
});

it('rejects unbounded, untyped and sensitive startup cause fields', () => {
  expect(
    startupDiagnosticTags({
      code: 'startup.incomplete',
      level: 'error',
      message: '',
      fields: {
        startupMarketsState: 'wallet-secret',
        startupMarketsVenueCode: 'secret',
        startupTickersState: 'started',
        startupTickersKind: 'body text',
        startupTickersVenueCode: 'https://secret.invalid?key=secret',
        startupTickersHttpStatus: 600,
        startupBalanceState: 'failed',
        startupBalanceHttpStatus: 400.5,
        startupOrdersState: 'refused',
        startupOrdersVenueCode: 'x'.repeat(33),
        startupPositionsState: 'neverStarted',
        startupPositionsHttpStatus: NaN,
        account: 'secret',
        url: 'secret',
        body: 'secret',
      },
    }),
  ).toEqual({
    startup_tickers_state: 'started',
    startup_balance_state: 'failed',
    startup_orders_state: 'refused',
    startup_positions_state: 'neverStarted',
  });
});

it.each([{ state: ['timedOut'] }, { state: { toString: () => 'failed' } }, { state: null }, { state: 500 }])(
  'does not coerce a non-string startup state',
  ({ state }) => {
    expect(
      startupDiagnosticTags({
        code: 'startup.incomplete',
        level: 'error',
        message: '',
        fields: {
          startupMarketsState: state,
        } as unknown as EngineEvent['fields'],
      }),
    ).toEqual({});
  },
);
