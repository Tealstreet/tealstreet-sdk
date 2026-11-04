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
