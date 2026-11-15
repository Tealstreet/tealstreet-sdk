import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { BaseWebSocket } from '../../exchanges/base.ws';
import { ExchangeEmitter } from '../../exchanges/exchangeEmitter';
import { DefaultStore } from '../../store/store.base';
import { reportEvent, reportException, setTelemetrySink } from '../../telemetry';
import { LogSeverity } from '../../types';
import { diagnosticConsole, diagnosticText, redactDiagnosticData } from '../../utils/diagnosticRedaction';

const material = 'SYNTHETIC_REVIEW_3436_ONLY';
const credentialKeys = ['apiKey', 'jwt', 'token', 'Authorization'] as const;
const consoleMethods = ['log', 'info', 'warn', 'error', 'debug', 'trace'] as const;
const message = (key: string) => `request failed: ${key}="${material}"`;
const error = (key: string) => new Error(`${key}="${material}"`);
const inspect = (value: unknown): string =>
  JSON.stringify(value, (_key, child) =>
    child instanceof Error
      ? { ...child, name: child.name, message: child.message, stack: child.stack, cause: child.cause }
      : child,
  ) ?? '';
const exchanges: BaseExchange[] = [];
const exchange = () => {
  const value = new BaseExchange(
    'bybit',
    { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } },
    new DefaultStore(),
  );
  exchanges.push(value);
  return value;
};

beforeEach(() => {
  for (const method of consoleMethods) vi.spyOn(console, method).mockImplementation(() => {});
});
afterEach(() => {
  for (const value of exchanges.splice(0)) value.dispose();
  setTelemetrySink({});
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(credentialKeys)('colon-prefixed %s diagnostics', (key) => {
  it.each(['request failed: ', 'request failed:', 'connection: request failed: '])(
    'removes the material from diagnostic text after %s',
    (prefix) => {
      expect(diagnosticText(`${prefix}${key}="${material}"`)).not.toContain(material);
      expect(diagnosticText(`request failed ${key}="${material}"`)).not.toContain(material);
    },
  );

  it('removes material from copied Error messages, stacks and causes without mutating the source', () => {
    const original = new Error('ordinary failure', { cause: error(key) });
    const safe = redactDiagnosticData(original);
    expect(inspect(safe)).not.toContain(material);
    expect(safe).not.toBe(original);
    expect(safe.message).toBe('ordinary failure');
    expect(safe.cause).toBeInstanceOf(Error);
    expect(inspect(original)).toContain(material);
    const direct = redactDiagnosticData(error(key));
    expect(direct.message).not.toContain(material);
    expect(direct.stack).toBeTypeOf('string');
    expect(direct.stack).not.toContain(material);
    expect(Object.keys(direct)).toEqual([]);
    expect(JSON.stringify(direct)).toBe('{}');
  });

  it.each(consoleMethods)('never delivers material to the %s console sink', (method) => {
    diagnosticConsole[method](message(key), error(key), { reqId: 'request-3436', count: 2 });
    const sink = method === 'debug' ? 'log' : method;
    const calls = vi.mocked(console[sink]).mock.calls;
    expect(calls).toHaveLength(1);
    expect(inspect(calls)).not.toContain(material);
    expect(calls[0][2]).toEqual({ reqId: 'request-3436', count: 2 });
  });

  it('never delivers material to either telemetry receiver', () => {
    const captureException = vi.fn();
    const event = vi.fn();
    setTelemetrySink({ captureException, event });
    const original = error(key);
    const context = { tags: { phase: 'startup' }, extra: { message: message(key), reqId: 'request-3436' } };
    reportException(original, context);
    reportEvent('diagnostic.example', { message: message(key), reqId: 'request-3436', count: 2 });
    expect(captureException).toHaveBeenCalledOnce();
    expect(event).toHaveBeenCalledOnce();
    expect(inspect([captureException.mock.calls, event.mock.calls])).not.toContain(material);
    expect(captureException.mock.calls[0][1]).toMatchObject({ tags: { phase: 'startup' } });
    expect(event.mock.calls[0]).toEqual([
      'diagnostic.example',
      expect.objectContaining({ reqId: 'request-3436', count: 2 }),
    ]);
    expect(original.message).toContain(material);
    expect(context.extra.message).toBe(message(key));
  });

  it('never delivers material to the actual exec-log listener', () => {
    const emitter = new ExchangeEmitter('bybit');
    const listener = vi.fn();
    const payload = { message: message(key), error: error(key), reqId: 'request-3436', timestamp: 123 };
    emitter.on('exec-log', listener);
    emitter.emit('exec-log', payload);
    expect(listener).toHaveBeenCalledOnce();
    expect(inspect(listener.mock.calls)).not.toContain(material);
    expect(listener.mock.calls[0][0]).toMatchObject({ reqId: 'request-3436', timestamp: 123 });
    expect(listener.mock.calls[0][0]).not.toBe(payload);
    expect(payload.message).toBe(message(key));
    expect(payload.error.message).toContain(material);
  });

  it.each([false, true])('removes material at BaseExchange error sinks with worker=%s', (worker) => {
    if (worker) vi.stubGlobal('window', undefined);
    const current = exchange();
    const listener = vi.fn();
    const captureException = vi.fn();
    setTelemetrySink({ captureException });
    current.on('error', listener);
    const original = error(key);
    current.emitAndLogError(original);
    expect(console.error).toHaveBeenCalled();
    expect(listener).toHaveBeenCalledOnce();
    expect(inspect(vi.mocked(console.error).mock.calls)).not.toContain(material);
    expect(inspect(listener.mock.calls)).not.toContain(material);
    expect(inspect(captureException.mock.calls)).not.toContain(material);
    expect(captureException).toHaveBeenCalledTimes(worker ? 0 : 1);
    expect(original.message).toContain(material);
    if (worker) expect(listener.mock.calls[0][1].sendToSentry).toBe(true);
  });

  it('removes material from the disposed exchange console boundary', () => {
    const current = exchange();
    current.dispose();
    current.emitAndLogError(error(key));
    expect(console.warn).toHaveBeenCalled();
    expect(inspect(vi.mocked(console.warn).mock.calls)).not.toContain(material);
  });

  it('removes material before retained log and socket buffers receive it', () => {
    const current = exchange();
    const listener = vi.fn();
    current.on('log', listener);
    current.log(message(key), LogSeverity.Warning);
    const socket = new BaseWebSocket(current);
    socket.logLastLog(message(key));
    expect(listener).toHaveBeenCalledOnce();
    expect(inspect(listener.mock.calls)).not.toContain(material);
    expect(socket.lastLog.toArray().length).toBeGreaterThan(0);
    expect(inspect(socket.lastLog.toArray())).not.toContain(material);
  });

  it('removes listener-failure Error stacks before console delivery without starving later listeners', () => {
    const emitter = new ExchangeEmitter('bybit');
    const later = vi.fn();
    emitter.on('exec-log', () => {
      throw error(key);
    });
    emitter.on('exec-log', later);
    emitter.emit('exec-log', { message: 'ordinary diagnostic', reqId: 'request-3436' });
    expect(later).toHaveBeenCalledOnce();
    expect(console.error).toHaveBeenCalledOnce();
    expect(inspect(vi.mocked(console.error).mock.calls)).not.toContain(material);
  });
});

it.each(['op', 'method', 'event', 'action', 'type'])('does not skip a truncated auth %s after a colon label', (key) => {
  expect(diagnosticText(`request failed: ${key}=login args=["${material}"`)).not.toContain(material);
});

it('preserves ordinary colon context, Error fields and live-event identity', () => {
  const text = 'request failed: status=503 symbol=BTCUSDT';
  expect(diagnosticText(text)).toBe(text);
  const original = new Error(text);
  const safe = redactDiagnosticData(original);
  expect(safe.name).toBe(original.name);
  expect(safe.message).toBe(original.message);
  expect(safe.stack).toBe(original.stack);
  expect(JSON.stringify(redactDiagnosticData(new Error('x')))).toBe('{}');
  expect(Object.keys(redactDiagnosticData(new Error('x')))).toEqual([]);
  const emitter = new ExchangeEmitter('bybit');
  const listener = vi.fn();
  const payload = { message: message('token'), reqId: 'request-3436' };
  emitter.on('exec-result', listener);
  emitter.emit('exec-result', payload);
  expect(listener).toHaveBeenCalledWith(payload);
  expect(listener.mock.calls[0][0]).toBe(payload);
  expect(payload.message).toContain(material);
});
