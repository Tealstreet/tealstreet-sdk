import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { BaseWebSocket } from '../../exchanges/base.ws';
import { DefaultStore } from '../../store/store.base';
import { reportEvent, setTelemetrySink } from '../../telemetry';
import { LogSeverity } from '../../types';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { redactWsDiagnosticPayload } from '../../utils/wsDiagnostics';

const material = 'synthetic-only-material-0123456789';
const diagnostic = () =>
  Object.assign(new Error(`request failed apiKey="${material}"`), {
    config: { headers: { secret: material, signature: material, passphrase: material, jwt: material } },
  });
const inspect = (value: unknown) =>
  JSON.stringify(value, (_key, child) =>
    child instanceof Error ? { ...child, name: child.name, message: child.message, stack: child.stack } : child,
  );
const makeExchange = (consoleLogging: boolean) =>
  new BaseExchange(
    'bybit',
    { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging } },
    new DefaultStore(),
  );
const exchanges: BaseExchange[] = [];

beforeEach(() => {
  for (const method of ['log', 'info', 'warn', 'error', 'debug', 'trace'] as const)
    vi.spyOn(console, method).mockImplementation(() => {});
});
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  setTelemetrySink({});
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const create = (consoleLogging = true) => {
  const exchange = makeExchange(consoleLogging);
  exchanges.push(exchange);
  return exchange;
};

describe('exchange diagnostic boundaries', () => {
  it.each([false, true])('redacts console diagnostics with disposed=%s', (disposed) => {
    const exchange = create();
    const error = diagnostic();
    if (disposed) exchange.dispose();
    exchange.emitAndLogError(error);
    const calls = disposed ? vi.mocked(console.warn).mock.calls : vi.mocked(console.error).mock.calls;
    expect(calls.length).toBeGreaterThan(0);
    expect(inspect(calls)).not.toContain(material);
    expect(error.config.headers.secret).toBe(material);
  });

  it('redacts retained log diagnostics even when console logging is off', () => {
    const exchange = create(false);
    const listener = vi.fn();
    exchange.on('log', listener);
    exchange.log(`request failed apiKey="${material}"`, LogSeverity.Warning);
    expect(listener).toHaveBeenCalledOnce();
    expect(inspect(listener.mock.calls)).not.toContain(material);
    expect(console.log).not.toHaveBeenCalled();
  });

  it('redacts telemetry diagnostic copies', () => {
    const exchange = create();
    const captureException = vi.fn();
    const event = vi.fn();
    setTelemetrySink({ captureException, event });
    exchange.emitAndLogError(diagnostic(), undefined, { sentryExtra: { passphrase: material } });
    reportEvent('diagnostic', { jwt: material, timestamp: 123 });
    expect(captureException).toHaveBeenCalledOnce();
    expect(event).toHaveBeenCalledOnce();
    expect(inspect([captureException.mock.calls, event.mock.calls])).not.toContain(material);
    expect(event.mock.calls[0][1]).toMatchObject({ timestamp: 123 });
  });

  it('redacts worker error diagnostics before forwarding them', () => {
    vi.stubGlobal('window', undefined);
    const exchange = create();
    const listener = vi.fn();
    exchange.on('error', listener);
    exchange.emitAndLogError(diagnostic(), undefined, { sentryExtra: { jwt: material } });
    expect(listener).toHaveBeenCalledOnce();
    expect(inspect(listener.mock.calls)).not.toContain(material);
  });

  it('redacts structured log diagnostics without changing the source', () => {
    const exchange = create();
    const listener = vi.fn();
    const event = {
      code: 'diagnostic.example',
      level: 'warn' as const,
      message: 'Request failed',
      fields: { jwt: material, count: 3 },
    };
    exchange.on('log', listener);
    exchange.log(event.message, LogSeverity.Warning, event);
    expect(inspect(listener.mock.calls)).not.toContain(material);
    expect(listener.mock.calls[0][2].fields.count).toBe(3);
    expect(event.fields.jwt).toBe(material);
  });

  it('redacts listener failure diagnostics without starving later listeners', () => {
    const exchange = create();
    const later = vi.fn();
    exchange.on('log', () => {
      throw diagnostic();
    });
    exchange.on('log', later);
    exchange.log('Markets loaded', LogSeverity.Info);
    expect(later).toHaveBeenCalledOnce();
    expect(console.error).toHaveBeenCalled();
    expect(inspect(vi.mocked(console.error).mock.calls)).not.toContain(material);
  });

  it.each(['key', 'secret', 'signature', 'passphrase', 'jwt'])('redacts diagnostic values for %s', (field) => {
    const exchange = create();
    const listener = vi.fn();
    exchange.on('log', listener);
    for (const value of [material, material.slice(0, 12), material.slice(-12), `[REDACTED]${material}`]) {
      for (const message of [
        JSON.stringify({ [field]: value }),
        `request failed ${field}="${value}"`,
        `request failed {"${field}":"${value}`,
      ]) {
        exchange.log(message, LogSeverity.Warning);
        expect(inspect(listener.mock.calls.at(-1))).not.toContain(value);
        expect(inspect(vi.mocked(console.log).mock.calls.at(-1))).not.toContain(value);
      }
    }
  });

  it('redacts retained socket diagnostics', () => {
    const exchange = create();
    const socket = new BaseWebSocket(exchange);
    socket.logLastLog(`request failed jwt="${material}"`);
    expect(inspect(socket.lastLog.toArray())).not.toContain(material);
    const value = '0123456789abcdef'.repeat(4);
    expect(redactWsDiagnosticPayload(value)).toBe('[redacted]');
  });

  it('keeps live event payloads separate from diagnostic copies', () => {
    const exchange = create();
    const listener = vi.fn();
    const payload = { key: material };
    exchange.on('update', listener);
    exchange.emitter.emit('update', payload);
    expect(listener).toHaveBeenCalledWith(payload);
    expect(listener.mock.calls[0][0]).toBe(payload);
    expect(payload.key).toBe(material);
  });

  it.each(['log', 'info', 'warn', 'error', 'debug', 'trace'] as const)('redacts the %s console boundary', (method) => {
    diagnosticConsole[method]({ jwt: material, count: 3 });
    const sink = method === 'debug' ? 'log' : method;
    expect(console[sink]).toHaveBeenCalledOnce();
    expect(inspect(vi.mocked(console[sink]).mock.calls)).not.toContain(material);
    expect(vi.mocked(console[sink]).mock.calls[0][0]).toMatchObject({ count: 3 });
  });

  it('retains diagnostic error context through the console boundary', () => {
    const exchange = create();
    exchange.emitAndLogError(new Error('request failed', { cause: new Error(`request failed jwt="${material}"`) }));
    const captured = vi.mocked(console.error).mock.calls[0][1] as Error;
    expect(captured.message).toBe('request failed');
    expect(captured.cause).toBeInstanceOf(Error);
    expect(inspect(captured)).not.toContain(material);
  });

  it('keeps ordinary console and log diagnostics useful', () => {
    const exchange = create();
    const listener = vi.fn();
    exchange.on('log', listener);
    exchange.log('Markets loaded', LogSeverity.Info);
    expect(console.log).toHaveBeenCalledWith('Markets loaded', LogSeverity.Info);
    expect(listener).toHaveBeenCalledWith('[bybit] Markets loaded', LogSeverity.Info);
  });
});
