import type { ErrorEvent } from '@sentry/bun';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { redactDiagnosticData } from './diagnosticRedaction';

vi.mock('@sentry/bun', () => ({ init: vi.fn(), captureEvent: vi.fn(), captureException: vi.fn() }));
const Sentry = await import('@sentry/bun');
import { captureAlert, captureServiceInfo, resetCaptureAlertForTests } from './capture';
import { alertKey } from './alertKey';
import { initServiceSentry, resetServiceSentryForTests } from './init';

beforeEach(() => {
  vi.clearAllMocks();
  resetServiceSentryForTests();
  resetCaptureAlertForTests();
  initServiceSentry({ dsn: 'https://fake@o1.ingest.us.sentry.io/2', service: 'candle-server' });
});

const hooks = () => {
  const options = vi.mocked(Sentry.init).mock.calls[0]?.[0];
  if (!options?.beforeSend || !options.beforeBreadcrumb) throw new Error('missing final service hooks');
  return { beforeSend: options.beforeSend, beforeBreadcrumb: options.beforeBreadcrumb };
};

describe('one service Sentry role policy', () => {
  it('scrubs complete events, including parser projections and credential siblings', () => {
    const event: ErrorEvent = {
      type: undefined,
      message: 'parse failed',
      exception: { values: [{ type: 'SyntaxError', value: 'fake opaque parser input' }] },
      extra: { reason: 'fake opaque parser input', challenge: 'fake-short-challenge', session: 'fake-session' },
      tags: { auth: 'fake-auth', venue: 'bybit' },
      request: { url: 'https://fake-user:fake-pass@venue.invalid/public?symbol=BTCUSDT' },
      contexts: { client_cpu_pressure: { reportsThisSession: 2, maxReportsPerSession: 6, auth: 'fake-context-auth' } },
    };
    const safe = hooks().beforeSend(event, {});
    expect(safe).toEqual(redactDiagnosticData(event));
    expect(JSON.stringify(safe)).not.toMatch(
      /fake opaque parser input|fake-short-challenge|fake-session|fake-auth|fake-user|fake-pass|fake-context-auth/,
    );
    expect(safe).toMatchObject({
      tags: { venue: 'bybit' },
      contexts: { client_cpu_pressure: { reportsThisSession: 2, maxReportsPerSession: 6 } },
    });
    expect(event.extra?.reason).toBe('fake opaque parser input');
  });

  it('scrubs entire breadcrumbs so positional header roles are preserved', () => {
    const crumb = {
      category: 'test',
      data: { args: ['context', 'KC-API-PARTNER-SIGN', 'fake-short-proof'] },
      message: 'request failed',
    };
    const safe = hooks().beforeBreadcrumb(crumb, {});
    expect(safe).toEqual(redactDiagnosticData(crumb));
    expect(JSON.stringify(safe)).not.toContain('fake-short-proof');
    expect(crumb.data.args[2]).toBe('fake-short-proof');
  });

  it('uses the same policy for periodic info captures', () => {
    captureServiceInfo({
      key: 'summary',
      message: 'window complete',
      detail: { challenge: 'fake-challenge', session: 'fake-session', count: 3 },
    });
    const event = vi.mocked(Sentry.captureEvent).mock.calls[0]?.[0];
    if (!event) throw new Error('missing info event');
    expect(event.extra?.count).toBe(3);
    expect(JSON.stringify(event)).not.toMatch(/fake-challenge|fake-session/);
  });
});


describe('parser provenance across alert fingerprints', () => {
  it.each(['direct', 'wrapped'])('scrubs %s parser excerpts before fingerprint retention and final send', (shape) => {
    let parser: unknown;
    try { JSON.parse('v3!Q9x7M'); } catch (error) { parser = error; }
    if (!(parser instanceof SyntaxError)) throw new Error('Expected a synthetic parser failure');
    const error = shape === 'wrapped' ? new Error(parser.message, { cause: parser }) : parser;
    const original = error.message;
    captureAlert({ key: alertKey('unhandled-rejection', error), message: error.message, error, level: 'error', severity: 'low' });
    expect(Sentry.captureException).toHaveBeenCalledTimes(1);
    const call = vi.mocked(Sentry.captureException).mock.calls[0];
    if (!call) throw new Error('Missing exception capture');
    const context = call[1];
    if (!context || typeof context !== 'object' || !('fingerprint' in context)) throw new Error('Missing capture context');
    expect(JSON.stringify(context)).not.toContain('v3!Q9x7M');
    const safe = hooks().beforeSend({ type: undefined, ...context }, {});
    expect(safe).toMatchObject({ fingerprint: context.fingerprint });
    expect(JSON.stringify(safe)).not.toContain('v3!Q9x7M');
    expect(error.message).toBe(original);
    if (error.cause) expect(error.cause).toBe(parser);
  });

  it.each(['status code 401', 'status code 503', 'ECONNREFUSED'])('keeps ordinary grouping for %s', (message) => {
    const key = alertKey('process', new Error(message));
    captureAlert({ key, message, error: new Error(message), level: 'error', severity: 'low' });
    expect(Sentry.captureException).toHaveBeenCalledTimes(1);
    const context = vi.mocked(Sentry.captureException).mock.calls[0]?.[1];
    expect(context).toMatchObject({ fingerprint: [key], extra: { alert_key: key } });
  });
});


describe('parser fingerprints before normalization', () => {
  const wrapped = (kind: string, marker: string) => {
    let parser: unknown;
    if (kind === 'native newline') {
      try { JSON.parse(marker + '\n'); } catch (error) { parser = error; }
    } else if (kind === 'multiline') parser = new SyntaxError(`FAKE parser ${marker}\nsecond line`);
    else if (kind === 'clipped') parser = new SyntaxError(`FAKE parser ${marker} ${'ordinary context '.repeat(20)}`);
    else parser = new SyntaxError(`FAKE parser ${marker} request Ab1Cd2Ef3Gh4Ij5Kl6Mn`);
    if (!(parser instanceof SyntaxError)) throw new Error('Expected synthetic parser failure');
    return new Error(parser.message, { cause: parser });
  };

  it.each(['native newline', 'multiline', 'clipped', 'volatile token'])(
    'keeps %s parser excerpts out of the original bucket and final event', (kind) => {
      const marker = 'Q7!s9';
      const error = wrapped(kind, marker);
      const original = error.message;
      const key = alertKey('unhandled-rejection', error);
      captureAlert({ key, message: error.message, error, detail: { reason: error.message }, level: 'error', severity: 'low' });
      expect(Sentry.captureException).toHaveBeenCalledTimes(1);
      const call = vi.mocked(Sentry.captureException).mock.calls[0];
      if (!call || !(call[0] instanceof Error)) throw new Error('Missing exception capture');
      const context = call[1];
      if (!context || typeof context !== 'object' || !('fingerprint' in context)) throw new Error('Missing context');
      expect(key).not.toContain(marker);
      expect(JSON.stringify(context)).not.toContain(marker);
      const final = hooks().beforeSend({
        type: undefined, ...context,
        exception: { values: [{ type: call[0].name, value: call[0].message }] },
      }, {});
      expect(final).toMatchObject({ fingerprint: context.fingerprint });
      expect(JSON.stringify(final)).not.toContain(marker);
      expect(error.message).toBe(original);
      expect(error.cause).toBeInstanceOf(SyntaxError);
    },
  );

  it('uses one stable bucket for different real newline-parser excerpts', () => {
    const keys = [];
    for (const marker of ['Q7!s9', 'R8!t2']) {
      const error = wrapped('native newline', marker);
      const key = alertKey('unhandled-rejection', error);
      keys.push(key);
      captureAlert({ key, message: error.message, error, level: 'error', severity: 'low' });
    }
    expect(Sentry.captureException).toHaveBeenCalledTimes(1);
    expect(keys[0]).toBe(keys[1]);
  });
});


describe('whole-copy omission at service schemas', () => {
  const uninspectable = (kind: string) => kind === 'parser limit' ? '['.repeat(65) : new Proxy({}, {
    ownKeys() { throw new Error('Synthetic unreadable diagnostic'); },
  });
  const enable = (service: string) => {
    vi.mocked(Sentry.init).mockClear();
    resetServiceSentryForTests();
    initServiceSentry({ dsn: 'https://fake@o1.ingest.us.sentry.io/2', service });
  };

  for (const service of ['exchange-server', 'candle-server']) {
    it.each(['parser limit', 'host object'])(`${service} returns null for an omitted %s event`, (kind) => {
      enable(service);
      const event: ErrorEvent = { type: undefined, message: 'ordinary failure', extra: { detail: uninspectable(kind) } };
      expect(redactDiagnosticData(event)).toBe('[unserializable diagnostic]');
      expect(() => hooks().beforeSend(event, {})).not.toThrow();
      expect(hooks().beforeSend(event, {})).toBeNull();
      expect(event.message).toBe('ordinary failure');
    });

    it.each(['parser limit', 'host object'])(`${service} returns null for an omitted %s breadcrumb`, (kind) => {
      enable(service);
      const crumb = { category: 'test', message: 'ordinary failure', data: { detail: uninspectable(kind) } };
      expect(redactDiagnosticData(crumb)).toBe('[unserializable diagnostic]');
      expect(() => hooks().beforeBreadcrumb(crumb, {})).not.toThrow();
      expect(hooks().beforeBreadcrumb(crumb, {})).toBeNull();
      expect(crumb.message).toBe('ordinary failure');
    });
  }

  it.each(['parser limit', 'host object'])('drops omitted %s alerts before claiming any throttle bucket', (kind) => {
    for (const status of [401, 503]) {
      captureAlert({ key: `status:${status}`, message: `HTTP ${status}`, detail: { nested: uninspectable(kind) }, level: 'error', severity: 'low' });
    }
    expect(Sentry.captureEvent).not.toHaveBeenCalled();
    expect(Sentry.captureException).not.toHaveBeenCalled();
    for (const [key, message] of [['status:401', 'HTTP 401'], ['status:503', 'HTTP 503'], ['undefined', 'Literal undefined condition']]) {
      captureAlert({ key, message, detail: { count: 1 }, level: 'error', severity: 'low' });
    }
    const events = vi.mocked(Sentry.captureEvent).mock.calls.map(([event]) => event);
    expect(events).toHaveLength(3);
    expect(events.map(event => event.message)).toEqual(['HTTP 401', 'HTTP 503', 'Literal undefined condition']);
    expect(events.map(event => event.fingerprint)).toEqual([['status:401'], ['status:503'], ['undefined']]);
  });
});
