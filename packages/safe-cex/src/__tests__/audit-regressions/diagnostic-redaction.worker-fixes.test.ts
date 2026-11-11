import { describe, expect, it } from 'vitest';

import { diagnosticText, redactDiagnosticData } from '../../utils/diagnosticRedaction';

const sentinel = 'synthetic-worker-diagnostic-material';

describe('auth text and diagnostic argument boundaries', () => {
  it.each([
    { type: 'auth', args: [sentinel] },
    { event: 'private.login', params: [sentinel] },
    { action: 'account:authenticate', params: [sentinel] },
    { TYPE: 'private-login', ARGS: [sentinel] },
  ])('redacts the same auth operation in structured, prefixed and truncated text: %j', (frame) => {
    expect(diagnosticText(frame)).not.toContain(sentinel);
    const serialized = JSON.stringify(frame);
    expect(diagnosticText(`worker received ${serialized}`)).not.toContain(sentinel);
    expect(diagnosticText(`worker received ${serialized.slice(0, -2)}`)).not.toContain(sentinel);
    expect(serialized).toContain(sentinel);
  });

  it('keeps positional auth context in a prefix from exposing a parsed array suffix', () => {
    expect(diagnosticText('worker type=auth params=' + JSON.stringify([sentinel]))).not.toContain(sentinel);
    expect(diagnosticText('worker type=subscribe params=["orders"]')).toBe('worker type=subscribe params=["orders"]');
  });

  it('preserves ordinary prefixed subscription fields', () => {
    const frame = { type: 'private.subscribe', args: ['orders', 'BTCUSDT'], requestId: 'r-1', timestamp: 123 };
    const prefix = 'worker received ';
    const result = diagnosticText(prefix + JSON.stringify(frame));
    expect(result.startsWith(prefix)).toBe(true);
    expect(JSON.parse(result.slice(prefix.length))).toEqual(frame);
  });

  it.each(['Invalid API-key, IP, or permissions for action', 'Authorization failed', 'jwt'])(
    'preserves error metadata after ordinary message %s',
    (message) => {
      const metadata = {
        sendToSentry: true,
        classified: { category: 'auth', terminal: true },
        sentryData: { tags: { worker_error_phase: 'startup', worker_error_family: 'auth' } },
      };
      const args = [message, metadata];
      const result = redactDiagnosticData(args);
      expect(result).toEqual(args);
      expect(result).not.toBe(args);
      expect(result[1]).not.toBe(metadata);
    },
  );

  it.each(['Authorization', 'X-MBX-APIKEY', 'jwt', 'Authent'])('still redacts credential header tuple %s', (key) => {
    const tuple = [key, sentinel];
    expect(redactDiagnosticData(tuple)).toEqual([key, '[REDACTED]']);
    expect(tuple[1]).toBe(sentinel);
  });

  it('preserves serialization and ordinary escaped fields in a prefixed JSON diagnostic', () => {
    const event = {
      code: 'bitget.close.diagnostic',
      fields: { nativeMsg: 'not enough position\n"native"', path: 'C:\\fixture', count: 2, status: 400 },
    };
    const prefix = '[bitget] ';
    const result = diagnosticText(prefix + JSON.stringify(event));
    expect(result.startsWith(prefix)).toBe(true);
    expect(JSON.parse(result.slice(prefix.length))).toEqual(event);
    expect(result.split('\n')).toHaveLength(1);
  });

  it('redacts nested secrets in prefixed JSON without changing ordinary fields or the input', () => {
    const event = { requestId: 'r-1', status: 400, error: { jwt: sentinel }, note: '"native"\nline' };
    const serialized = JSON.stringify(event);
    const result = diagnosticText('[bitget] ' + serialized);
    expect(result).not.toContain(sentinel);
    expect(JSON.parse(result.slice('[bitget] '.length))).toEqual({
      ...event,
      error: { jwt: '[REDACTED]' },
    });
    expect(event.error.jwt).toBe(sentinel);
  });
});
