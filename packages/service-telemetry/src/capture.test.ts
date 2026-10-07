import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@sentry/bun', () => ({
  captureEvent: vi.fn(),
  captureException: vi.fn(),
  init: vi.fn(),
  flush: vi.fn(async () => true),
}));

const Sentry = await import('@sentry/bun');

import { captureAlert, resetCaptureAlertForTests } from './capture';
import { initServiceSentry, isServiceSentryEnabled, resetServiceSentryForTests } from './init';

const DSN = 'https://abc123@o815347.ingest.us.sentry.io/4512049750933504';

const enable = () => initServiceSentry({ dsn: DSN, service: 'exchange-server' });
const lastEvent = () => vi.mocked(Sentry.captureEvent).mock.calls.at(-1)?.[0] as Record<string, any>;
const lastException = () => vi.mocked(Sentry.captureException).mock.calls.at(-1) as [Error, Record<string, any>];

beforeEach(() => {
  vi.clearAllMocks();
  resetServiceSentryForTests();
  resetCaptureAlertForTests();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('no DSN', () => {
  it('does not initialise', () => {
    initServiceSentry({ dsn: undefined, service: 'exchange-server' });
    expect(isServiceSentryEnabled()).toBe(false);
    expect(Sentry.init).not.toHaveBeenCalled();
  });

  it('makes captureAlert a no-op rather than a crash', () => {
    expect(() => captureAlert({ key: 'k', message: 'm', level: 'error', severity: 'low' })).not.toThrow();
    expect(Sentry.captureEvent).not.toHaveBeenCalled();
  });
});

describe('captureAlert', () => {
  beforeEach(enable);

  it('fingerprints on the alert key, so one condition is one issue', () => {
    captureAlert({
      key: 'unhandled-rejection:Error:boom',
      message: 'unhandled rejection',
      level: 'error',
      severity: 'low',
    });
    expect(lastEvent().fingerprint).toEqual(['unhandled-rejection:Error:boom']);
  });

  it('tags severity and whether Slack saw it', () => {
    captureAlert({ key: 'k', message: 'm', level: 'error', severity: 'low', slackSuppressed: true });
    expect(lastEvent().tags).toEqual({ severity: 'low', slack_suppressed: 'true' });
  });

  it('maps warn to Sentry warning', () => {
    captureAlert({ key: 'k', message: 'm', level: 'warn', severity: 'low' });
    expect(lastEvent().level).toBe('warning');
  });

  /**
   * Sentry bills events, not issues. Capturing sits BEFORE the Slack filters, so
   * without its own window it would re-ingest exactly the flood those filters
   * exist to stop — at one event per occurrence, billed.
   */
  it('sends one event per condition per window', () => {
    for (let i = 0; i < 20; i++) {
      captureAlert({ key: 'same', message: 'm', level: 'error', severity: 'low' });
    }
    expect(Sentry.captureEvent).toHaveBeenCalledOnce();
  });

  it('reports how many it suppressed when the window reopens', () => {
    vi.useFakeTimers();
    for (let i = 0; i < 5; i++) {
      captureAlert({ key: 'same', message: 'm', level: 'error', severity: 'low' });
    }
    vi.advanceTimersByTime(6 * 60 * 1000);
    captureAlert({ key: 'same', message: 'm', level: 'error', severity: 'low' });

    expect(Sentry.captureEvent).toHaveBeenCalledTimes(2);
    expect(lastEvent().extra.suppressed_since_last_event).toBe(4);
  });

  it('does not let one condition throttle another', () => {
    captureAlert({ key: 'a', message: 'm', level: 'error', severity: 'low' });
    captureAlert({ key: 'b', message: 'm', level: 'error', severity: 'low' });
    expect(Sentry.captureEvent).toHaveBeenCalledTimes(2);
  });
});

/**
 * The key is as untrusted as the message: exchange-server's `alertKey` embeds up
 * to 160 characters of raw venue error text, and it becomes the fingerprint.
 */
describe('captureAlert scrubs and bounds what it sends', () => {
  beforeEach(enable);

  it('scrubs a key-shaped value out of the fingerprint', () => {
    const key = 'unhandled-rejection:Error:bad key AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV';
    captureAlert({ key, message: 'm', level: 'error', severity: 'low' });
    expect(lastEvent().fingerprint[0]).toBe('unhandled-rejection:Error:bad key [redacted]');
  });

  it('caps fingerprint length, so issue cardinality is bounded', () => {
    // Spaced words, so the scrubber leaves it and only the cap can shorten it.
    const long = `k:${Array.from({ length: 200 }, (_, i) => `w${i}`).join(' ')}`;
    captureAlert({ key: long, message: 'm', level: 'error', severity: 'low' });
    expect(lastEvent().fingerprint[0].length).toBe(120);
  });

  it('scrubs the message', () => {
    captureAlert({
      key: 'k',
      message: 'failed with AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV',
      level: 'error',
      severity: 'low',
    });
    expect(lastEvent().message).toBe('failed with [redacted]');
  });

  it('scrubs detail by key name', () => {
    captureAlert({ key: 'k', message: 'm', detail: { apiKey: 'x', venue: 'bybit' }, level: 'error', severity: 'low' });
    expect(lastEvent().extra).toMatchObject({ apiKey: '[redacted]', venue: 'bybit' });
  });

  it('never lets a Sentry failure break the caller', () => {
    vi.mocked(Sentry.captureEvent).mockImplementationOnce(() => {
      throw new Error('sentry is down');
    });
    expect(() => captureAlert({ key: 'k', message: 'm', level: 'error', severity: 'low' })).not.toThrow();
  });
});

/**
 * The default integration set is the leak, not the events: Console breadcrumbs
 * would carry the alert path's own unscrubbed log line, and Http/NodeFetch/Fetch
 * ones carry signed venue REST URLs whose query string holds the API key and
 * signature. `tracesSampleRate: 0` does not turn breadcrumbs off.
 */
describe('init hardening', () => {
  const initOptions = () => vi.mocked(Sentry.init).mock.calls[0][0] as Record<string, any>;

  beforeEach(enable);

  it('drops the integrations that breadcrumb credentials', () => {
    const named = (name: string) => ({ name, setupOnce: () => {} });
    const kept = initOptions().integrations([
      named('Console'),
      named('Http'),
      named('NodeFetch'),
      named('Fetch'),
      named('RequestData'),
      named('BunHttpServer'),
      named('LocalVariables'),
    ]);
    expect(kept.map((i: { name: string }) => i.name)).toEqual(['LocalVariables']);
  });

  /**
   * Sentry's own global handlers send one event per rejection, outside
   * `captureAlert` and therefore outside its window. Both services register
   * their own handlers and route them through `alert()`, so leaving these on
   * means a second UNTHROTTLED event for the highest-volume condition there is.
   */
  it('drops the global handlers, which would bypass the throttle entirely', () => {
    const named = (name: string) => ({ name, setupOnce: () => {} });
    const kept = initOptions().integrations([named('OnUncaughtException'), named('OnUnhandledRejection')]);
    expect(kept).toEqual([]);
  });

  it('sends no PII, rich request context, or traces', () => {
    expect(initOptions().dataCollection).toEqual({
      userInfo: false,
      cookies: false,
      httpHeaders: {
        request: { deny: ['forwarded', '-ip', 'remote-', 'via', '-user'] },
        response: { deny: ['forwarded', '-ip', 'remote-', 'via', '-user'] },
      },
      httpBodies: [],
      urlQueryParams: { deny: ['forwarded', '-ip', 'remote-', 'via', '-user'] },
      genAI: { inputs: false, outputs: false },
      databaseQueryData: false,
      graphQL: { document: false, variables: false },
      queues: false,
      stackFrameVariables: false,
    });
    expect(initOptions().tracesSampleRate).toBe(0);
  });

  it('scrubs breadcrumbs that survive anyway', () => {
    const crumb = initOptions().beforeBreadcrumb({
      message: 'GET ?api_key=AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV',
      data: { secret: 'x' },
    });
    expect(crumb.message).toBe('GET ?api_key=[redacted]');
    expect(crumb.data).toEqual({ secret: '[redacted]' });
  });

  it('strips the container hostname from every event', () => {
    expect(initOptions().beforeSend({ server_name: 'shard-apac-2' }).server_name).toBeUndefined();
  });

  it('is idempotent — two entry points in one process must not double-init', () => {
    enable();
    expect(Sentry.init).toHaveBeenCalledOnce();
  });
});

/**
 * These processes run for weeks. An earlier cut pruned only EXPIRED buckets, so
 * a burst of distinct live conditions grew the map without limit.
 */
describe('the throttle map is bounded', () => {
  beforeEach(enable);

  it('stays bounded even when every key is still inside its window', () => {
    for (let i = 0; i < 700; i++) {
      captureAlert({ key: `distinct-${i}`, message: 'm', level: 'error', severity: 'low' });
    }
    // Every one sent (they are distinct conditions), but the map did not keep
    // all 700 alive.
    expect(Sentry.captureEvent).toHaveBeenCalledTimes(700);

    // A key evicted as oldest is allowed to send again; one still tracked is not.
    vi.clearAllMocks();
    captureAlert({ key: 'distinct-0', message: 'm', level: 'error', severity: 'low' });
    captureAlert({ key: 'distinct-699', message: 'm', level: 'error', severity: 'low' });
    expect(Sentry.captureEvent).toHaveBeenCalledOnce();
  });
});

/**
 * 37 `unhandled rejection` events arrived over two days on 2026-09-08/09 naming
 * a signing timeout, and not one of them carried a stack — so the timeout was
 * visible and the code that failed to catch it was not.
 */
describe('an Error is sent as an exception so the stack survives', () => {
  beforeEach(enable);

  it('captures the exception rather than a bare message', () => {
    const err = new Error('Signing request abc timed out after 15000ms');
    captureAlert({ key: 'unhandled-rejection:x', message: 'unhandled rejection', level: 'error', severity: 'low', error: err });

    expect(Sentry.captureEvent).not.toHaveBeenCalled();
    const [sent] = lastException();
    expect(sent.message).toBe('Signing request abc timed out after 15000ms');
    expect(sent.name).toBe('Error');
  });

  it('keeps the alert key as the fingerprint, so grouping does not change', () => {
    captureAlert({ key: 'unhandled-rejection:x', message: 'm', level: 'error', severity: 'low', error: new Error('boom') });
    const [, context] = lastException();
    expect(context.fingerprint).toEqual(['unhandled-rejection:x']);
    expect(context.tags.severity).toBe('low');
    expect(context.extra.alert_key).toBe('unhandled-rejection:x');
    expect(context.extra.alert_message).toBe('m');
  });

  it('scrubs the message AND the stack, which Sentry sends verbatim', () => {
    const err = new Error('auth failed for AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV');
    err.stack = 'Error: auth failed for AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV\n    at sign (remote-signer.ts:62)';
    captureAlert({ key: 'k', message: 'm', level: 'error', severity: 'low', error: err });

    const [sent] = lastException();
    expect(sent.message).toBe('auth failed for [redacted]');
    expect(sent.stack).toContain('[redacted]');
    expect(sent.stack).not.toContain('AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV');
    expect(sent.stack).toContain('at sign (remote-signer.ts:62)');
  });

  it('leaves the original error untouched for whoever else handles it', () => {
    const err = new Error('auth failed for AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV');
    captureAlert({ key: 'k', message: 'm', level: 'error', severity: 'low', error: err });
    expect(err.message).toBe('auth failed for AKIA1234567890ABCDEFGHIJKLMNOPQRSTUV');
  });

  it('falls back to a message event when the rejection is not an Error', () => {
    captureAlert({ key: 'k', message: 'unhandled rejection', level: 'error', severity: 'low', error: 'just a string' });
    expect(Sentry.captureException).not.toHaveBeenCalled();
    expect(lastEvent().message).toBe('unhandled rejection');
  });

  it('still throttles: an Error does not bypass the window', () => {
    const send = () => captureAlert({ key: 'k', message: 'm', level: 'error', severity: 'low', error: new Error('boom') });
    send();
    send();
    expect(vi.mocked(Sentry.captureException)).toHaveBeenCalledTimes(1);
  });
});
