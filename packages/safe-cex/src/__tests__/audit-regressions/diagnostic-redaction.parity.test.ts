// Fixture/expectation parity: tealstreet-next origin/master 6fde9450cf.
// Update both diagnostic contracts together; synthetic data only.
import { describe, expect, it, vi } from 'vitest';

import { diagnosticConsole, diagnosticText, redactDiagnosticData } from '../../utils/diagnosticRedaction';

describe('diagnostic copies', () => {
  it.each(['Authent', 'AUTHENTICATION', 'x-ts-grvt-session', 'X-TS-GRVT-SESSION', 'jwt', 'JwtToken', 'sessionId'])(
    'scrubs exchange credential key %s in objects, tuples and serialized diagnostics',
    (key) => {
      const input = { headers: { [key]: 'fake-credential' }, tuple: [key, 'fake-credential'] };
      expect(diagnosticText(input)).not.toContain('fake-credential');
      expect(diagnosticText(JSON.stringify(input))).not.toContain('fake-credential');
      expect(diagnosticText(`request failed ${key}: fake-credential`)).not.toContain('fake-credential');
      expect(input.headers[key]).toBe('fake-credential');
    },
  );

  it.each([
    { op: 'auth', args: ['fake-key', 1700000000000, 'fake-signature'] },
    { method: 'user.auth', params: ['API', 'fake-key', 'fake-signature', 1700000000] },
    { OP: 'AUTHENTICATE', ARGS: ['fake-key', 'fake-signature'] },
  ])('scrubs positional auth credentials in objects and serialized frames: %j', (input) => {
    expect(diagnosticText(input)).not.toContain('fake-');
    expect(diagnosticText(JSON.stringify(input))).not.toContain('fake-');
    expect(diagnosticText(`worker received ${JSON.stringify(input)}`)).not.toContain('fake-');
    expect(JSON.stringify(input)).toContain('fake-key');
  });

  it('scrubs encoded exchange header names and nested serialized auth frames', () => {
    const input = { headers: { 'A%75thent': 'fake-signature' }, message: JSON.stringify({ jwt: 'fake-jwt' }) };
    expect(diagnosticText(JSON.stringify(input))).not.toContain('fake-');
    expect(diagnosticText({ op: 'subscribe', args: ['orderbook', 'BTCUSDT'], timestamp: 123 })).toBe(
      '{"op":"subscribe","args":["orderbook","BTCUSDT"],"timestamp":123}',
    );
  });

  it('scrubs nested payloads, strings and header tuples without changing the source', () => {
    const input = {
      op: 'ws-diagnostic',
      id: 'request-1',
      timestamp: 123,
      data: {
        apiKey: 'fake-key',
        signature: 'fake-signature',
        token: 'fake-token',
        headers: [['Authorization', 'fake-bearer']],
        message: '{"apiKey":"fake-key","signature":"fake-signature","token":"fake-token"}',
      },
    };
    const result = redactDiagnosticData(input);
    expect(JSON.stringify(result)).not.toMatch(/fake-key|fake-signature|fake-token|fake-bearer/);
    expect(result).toMatchObject({ op: 'ws-diagnostic', id: 'request-1', timestamp: 123 });
    expect(input.data.apiKey).toBe('fake-key');
  });

  it('retains scrubbed Error fields and request/response context', () => {
    const error = Object.assign(new Error('request failed token="fake-token"'), {
      config: { headers: { apiKey: 'fake-key', signature: 'fake-signature' } },
    });
    const text = diagnosticText(error);
    expect(text).toContain('request failed');
    expect(text).not.toMatch(/fake-key|fake-signature|fake-token/);
  });

  it('removes credentials embedded in diagnostic URLs', () => {
    const input =
      'failed https://fake-user:fake-password@example.invalid/path?apiKey=fake-key&signature=fake-signature&token=fake-token';
    expect(diagnosticText(input)).not.toMatch(/fake-user|fake-password|fake-key|fake-signature|fake-token/);
    expect(diagnosticText(input)).toContain('example.invalid/path');
    expect(diagnosticText({ [input]: 'failed' })).not.toMatch(
      /fake-user|fake-password|fake-key|fake-signature|fake-token/,
    );
  });

  it('does not throw or leak when a diagnostic is circular', () => {
    const data: Record<string, unknown> = { token: 'fake-token' };
    data.self = data;
    expect(diagnosticText(data)).toBe('[unserializable diagnostic]');
  });

  it('fails closed when reading a host object throws', () => {
    const input = {
      get data(): unknown {
        throw new Error('fake-token');
      },
    };
    expect(diagnosticText(input)).toBe('[unserializable diagnostic]');
  });

  it('scrubs console error arguments before invoking the real sink', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    try {
      diagnosticConsole.error('worker failed', {
        apiKey: 'fake-key',
        signature: 'fake-signature',
        token: 'fake-token',
      });
      expect(JSON.stringify(spy.mock.calls)).not.toMatch(/fake-key|fake-signature|fake-token/);
    } finally {
      spy.mockRestore();
    }
  });

  it('omits binary bytes and prevents custom serializers from restoring raw values', () => {
    const input = { payload: new TextEncoder().encode('fake-token'), toJSON: () => ({ token: 'fake-token' }) };
    expect(diagnosticText(input)).not.toContain('fake-token');
    expect(diagnosticText(input)).toContain('binary omitted');
  });
});
