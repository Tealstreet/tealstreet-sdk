import { describe, expect, it, vi } from 'vitest';
import { diagnosticConsole, diagnosticText, redactDiagnosticData } from '../diagnostic-redaction';

describe('diagnostic copies', () => {
  it.each([' completed', '\ncompleted', ' {"count":19}'])(
    'discovers serialized parser records before suffix %s',
    (suffix) => {
      let failure: unknown;
      try { JSON.parse('v3!Q9x7M'); } catch (error) { failure = error; }
      if (!(failure instanceof SyntaxError)) throw new Error('Expected synthetic parser failure');
      const error = '[worker] ' + JSON.stringify({ name: 'SyntaxError', message: failure.message }) + suffix;
      for (const input of [
        { reason: failure.message, error, symbol: 'BTCUSDT', count: 19 },
        { error, reason: failure.message, symbol: 'BTCUSDT', count: 19 },
      ]) {
        const before = JSON.stringify(input);
        expect(redactDiagnosticData(input)).toMatchObject({ reason: '[REDACTED]', symbol: 'BTCUSDT', count: 19 });
        expect(JSON.stringify(input)).toBe(before);
      }
    },
  );

  it('discovers parser context after an ordinary complete JSON record', () => {
    const failure = new SyntaxError('FAKE second record excerpt');
    const error = '{"status":400} ' + JSON.stringify({ name: failure.name, message: failure.message }) + ' completed';
    expect(redactDiagnosticData({ reason: failure.message, error, symbol: 'BTCUSDT' }))
      .toMatchObject({ reason: '[REDACTED]', symbol: 'BTCUSDT' });
  });

  it('handles quoted brackets and escaped quotes inside a serialized parser message', () => {
    const failure = new SyntaxError('FAKE excerpt with "} [" and \\"nested quotes\\"');
    const error = '[worker] ' + JSON.stringify({ name: failure.name, message: failure.message }) + ' completed';
    const input = { reason: failure.message, error, symbol: 'BTCUSDT' };
    expect(redactDiagnosticData(input)).toMatchObject({ reason: '[REDACTED]', symbol: 'BTCUSDT' });
    expect(input.reason).toBe(failure.message);
  });

  it('does not spend extra parse attempts on children of one complete ordinary record', () => {
    const error = JSON.stringify({ rows: Array.from({ length: 65 }, () => ({ count: 19 })) });
    const input = { reason: 'ordinary unavailable', error, symbol: 'BTCUSDT' };
    expect(redactDiagnosticData(input)).toMatchObject({ reason: input.reason, symbol: 'BTCUSDT' });
  });

  it.each(['worker ', '[worker] ', 'metadata {event} '])('discovers parser records after prefix %s', (prefix) => {
    let failure: unknown;
    try { JSON.parse('p7!Z2q8X'); } catch (error) { failure = error; }
    if (!(failure instanceof SyntaxError)) throw new Error('Expected synthetic parser failure');
    const error = prefix + JSON.stringify({ name: 'SyntaxError', message: failure.message });
    for (const input of [
      { reason: failure.message, error, symbol: 'BTCUSDT' },
      { error, reason: failure.message, symbol: 'BTCUSDT' },
    ]) {
      expect(redactDiagnosticData(input)).toMatchObject({ reason: '[REDACTED]', symbol: 'BTCUSDT' });
      expect(JSON.stringify(redactDiagnosticData(input))).not.toContain('p7!Z2q8X');
      expect(input.reason).toBe(failure.message);
    }
  });

  it('counts every prefixed JSON candidate against the shared discovery budget', () => {
    const error = '[worker] '.repeat(65) + JSON.stringify({ name: 'SyntaxError', message: 'fake parser input' });
    expect(redactDiagnosticData({ reason: 'fake parser input', error })).toBe('[unserializable diagnostic]');
  });

  it.each(['cause', 'aggregate', 'nested'])('scrubs projections from non-enumerable %s parser containers', (container) => {
    let failure: unknown;
    try { JSON.parse('R7q!fake9M'); } catch (error) { failure = error; }
    if (!(failure instanceof SyntaxError)) throw new Error('Expected synthetic parser failure');
    const wrapped = container === 'cause'
      ? new Error('worker failed', { cause: failure })
      : container === 'aggregate'
        ? new AggregateError([failure], 'worker failed')
        : new Error('worker failed', { cause: new AggregateError([failure], 'worker failed') });
    for (const input of [
      { reason: failure.message, error: wrapped, symbol: 'BTCUSDT', count: 7 },
      { error: wrapped, reason: failure.message, symbol: 'BTCUSDT', count: 7 },
    ]) {
      const safe = redactDiagnosticData(input);
      expect(JSON.stringify(safe)).not.toContain('R7q!fake9M');
      expect(safe).toMatchObject({ reason: '[REDACTED]', symbol: 'BTCUSDT', count: 7 });
    }
    expect(failure.message).toContain('R7q!fake9M');
    if (container === 'cause') expect(wrapped.cause).toBe(failure);
  });

  it('retains ordinary metadata beside invalid or cyclic error containers', () => {
    const circular = new Error('ordinary failure');
    Object.defineProperty(circular, 'cause', { value: circular });
    const input = { reason: 'ordinary failure', error: circular, other: { cause: null, errors: false }, count: 7 };
    const safe = redactDiagnosticData(input);
    expect(safe).toMatchObject({ reason: 'ordinary failure', other: { cause: null, errors: false }, count: 7 });
    expect(circular.cause).toBe(circular);
  });

  it.each(['length', 'depth', 'count'])(
    'fails closed when serialized parser discovery exceeds its %s bound',
    (limit) => {
      let error: unknown;
      try { JSON.parse('Z8q!p2V6m4R'); } catch (failure) { error = failure; }
      if (!(error instanceof SyntaxError)) throw new Error('Expected synthetic parser failure');
      let serialized = JSON.stringify({ name: 'SyntaxError', message: error.message });
      if (limit === 'length') serialized = JSON.stringify({ padding: 'x'.repeat(65_536), error: serialized });
      if (limit === 'depth') for (let i = 0; i < 5; i++) serialized = JSON.stringify({ error: serialized });
      const preceding = limit === 'count' ? Array.from({ length: 65 }, (_, i) => JSON.stringify({ count: i })) : [];
      const input = { reason: error.message, preceding, error: serialized };
      expect(redactDiagnosticData(input)).toBe('[unserializable diagnostic]');
      expect(input.reason).toContain('Z8q!p2V6m4R');
    },
  );

  it('handles missing and invalid parser metadata without coercing fields', () => {
    const input = {
      malformed: { name: 42, type: null, message: false, value: [] },
      missing: {},
      named: { name: 'SyntaxError', message: 42, value: null },
      typed: { type: 'SyntaxError', message: false, value: { apiKey: 'fake-key' } },
      context: { symbol: 'BTCUSDT', count: 3 },
    };
    expect(redactDiagnosticData(input)).toEqual({
      malformed: input.malformed,
      missing: {},
      named: { name: 'SyntaxError', message: 'Invalid diagnostic syntax' },
      typed: {
        name: 'SyntaxError', message: 'Invalid diagnostic syntax',
        type: 'SyntaxError', value: 'Invalid diagnostic syntax',
      },
      context: input.context,
    });
    expect(input.typed.value.apiKey).toBe('fake-key');
  });

  it('preserves stack source coordinates while scrubbing credential values in every line', () => {
    const proof = '135ca5ee3780c9125ba4ed367fc8115aa3ec357ec71059a2eb347dc60f58a1ea';
    const error = new Error(`failed for ${proof}`);
    error.stack = `Error: failed for ${proof}\n    at sign (remote-signer.ts:62)\n    at call (/tmp/${proof}.ts:12:3)\nAuthorization: fake-short-value`;
    const safe = redactDiagnosticData(error);
    expect(safe.stack).toContain('at sign (remote-signer.ts:62)');
    expect(safe.stack).not.toContain(proof);
    expect(safe.stack).not.toContain('fake-short-value');
    expect(error.stack).toContain(proof);
  });

  it('does not treat credential assignments disguised as source coordinates as safe frames', () => {
    const error = new Error('failed');
    error.stack = 'Error: failed\n    at handler (apiKey=fake-frame-value.ts:1)\n    at handler (authorization:fake-frame-value.ts:1)\n    at handler (challenge=fake-frame-value.ts:1)';
    expect(redactDiagnosticData(error).stack).not.toContain('fake-frame-value');
  });

  it('removes projected parser messages beside their original error, regardless of property order', () => {
    let failure: unknown;
    try { JSON.parse('Q7x!r9L2v8M'); } catch (error) { failure = error; }
    if (!(failure instanceof SyntaxError)) throw new Error('missing parser failure');
    const input = { detail: { reason: failure.message, symbol: 'BTCUSDT' }, error: failure };
    const safe = redactDiagnosticData(input);
    expect(JSON.stringify(safe)).not.toContain('Q7x!r9L2');
    expect(safe.detail.symbol).toBe('BTCUSDT');
    expect(failure.message).toContain('Q7x!r9L2');
    expect(redactDiagnosticData(safe)).toEqual(safe);
  });

  it('removes projected excerpts beside serialized Sentry parser exceptions', () => {
    const input = {
      extra: { reason: 'fake opaque parse excerpt', count: 3 },
      exception: { values: [{ type: 'SyntaxError', value: 'fake opaque parse excerpt' }] },
    };
    const safe = redactDiagnosticData(input);
    expect(JSON.stringify(safe)).not.toContain('fake opaque parse excerpt');
    expect(safe.extra.count).toBe(3);
    expect(input.extra.reason).toBe('fake opaque parse excerpt');
  });

  it.each(['log', 'info', 'warn', 'error', 'debug'] as const)(
    'omits opaque parser excerpts through console.%s',
    (method) => {
      const marker = 'FAKE_PARSER_CREDENTIAL';
      let failure: unknown;
      try {
        JSON.parse(marker);
      } catch (error) {
        failure = error;
      }
      expect(failure).toBeInstanceOf(SyntaxError);
      if (!(failure instanceof SyntaxError)) throw new Error('Expected synthetic parser failure');
      expect(failure.message).toContain(marker.slice(0, 8));
      const spy = vi.spyOn(console, method === 'debug' ? 'log' : method).mockImplementation(() => {});
      try {
        diagnosticConsole[method](failure);
        const logged = JSON.stringify(spy.mock.calls, (_key, value) =>
          value instanceof Error ? { ...value, name: value.name, message: value.message, stack: value.stack } : value,
        );
        expect(logged).not.toContain(marker.slice(0, 8));
        expect(diagnosticText(failure)).not.toContain(marker.slice(0, 8));
        expect(failure.message).toContain(marker.slice(0, 8));
      } finally {
        spy.mockRestore();
      }
    },
  );

  it.each([
    'Authent',
    'AUTHENTICATION',
    'x-ts-grvt-session',
    'X-TS-GRVT-SESSION',
    'jwt',
    'JwtToken',
    'sessionId',
    'signed_challenge',
    'SIGNED_CHALLENGE',
    'signedChallenge',
  ])('scrubs exchange credential key %s in objects, tuples and serialized diagnostics', (key) => {
    const input = { headers: { [key]: 'fake-credential' }, tuple: [key, 'fake-credential'] };
    expect(diagnosticText(input)).not.toContain('fake-credential');
    expect(diagnosticText(JSON.stringify(input))).not.toContain('fake-credential');
    expect(diagnosticText(`request failed ${key}: fake-credential`)).not.toContain('fake-credential');
    expect(input.headers[key]).toBe('fake-credential');
  });

  it.each(['log', 'info', 'warn', 'error', 'debug'] as const)(
    'scrubs signed subscription proofs through console.%s',
    (method) => {
      const sink = method === 'debug' ? 'log' : method;
      const spy = vi.spyOn(console, sink).mockImplementation(() => {});
      const frame = { event: 'subscribe', feed: 'open_positions', signed_challenge: 'fake-kraken-proof' };
      try {
        diagnosticConsole[method](frame, JSON.stringify(frame), `received ${JSON.stringify(frame)}`);
        expect(JSON.stringify(spy.mock.calls)).not.toContain('fake-kraken-proof');
        expect(frame.signed_challenge).toBe('fake-kraken-proof');
        expect(redactDiagnosticData(frame)).toMatchObject({ event: 'subscribe', feed: 'open_positions' });
      } finally {
        spy.mockRestore();
      }
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

describe('credential roles independent of venue and field name', () => {
  const hex = '1a2b3c4d5e6f7890'.repeat(4);
  const base64 = btoa(String.fromCharCode(...Array.from({ length: 48 }, (_, n) => (n * 73 + 19) % 256)));
  it.each([
    hex,
    '0x' + hex + hex + '1b',
    base64,
    base64.replace(/\+/g, '-').replace(/\//g, '_'),
    base64 + '.' + base64 + '.' + base64,
    '-----BEGIN PRIVATE KEY-----\n' + base64 + '\n-----END PRIVATE KEY-----',
  ])('removes credential-shaped material even under an opaque field', (value) => {
    expect(diagnosticText({ opaque: value })).not.toContain(value);
  });
  it('keeps ordinary IDs, symbols, prices and timestamps', () => {
    const context = {
      id: '550e8400-e29b-41d4-a716-446655440000',
      orderId: '1502151477727584256',
      symbol: 'BTCUSDT',
      price: '12345.67',
      timestamp: 1700000000,
    };
    expect(redactDiagnosticData(context)).toEqual(context);
  });
});

describe('console positional credential roles', () => {
  it.each(['log', 'info', 'warn', 'error', 'debug'] as const)(
    'keeps a short credential role across console.%s arguments',
    (method) => {
      const spy = vi.spyOn(console, method === 'debug' ? 'log' : method).mockImplementation(() => {});
      try {
        diagnosticConsole[method]('worker context', 'OK-ACCESS-PASSPHRASE', 'Q7x!r9L2v8M');
        expect(JSON.stringify(spy.mock.calls)).not.toContain('Q7x!r9L2v8M');
        expect(JSON.stringify(spy.mock.calls)).toContain('worker context');
      } finally {
        spy.mockRestore();
      }
    },
  );
});

it('omits credential-shaped dynamic keys without collisions or source mutation', () => {
  const keyA = [
    '135ca5ee',
    '3780c912',
    '5ba4ed36',
    '7fc8115a',
    'a3ec357e',
    'c71059a2',
    'eb347dc6',
    '0f58a1ea',
  ].join('');
  const keyB = [
    '246db6ff',
    '4891da23',
    '6cb5fe47',
    '80d9226b',
    'b4fd468f',
    'd8216ab3',
    'fc458ed7',
    '1069b2fb',
  ].join('');
  const records = { [keyA]: { status: 'rejected' }, [keyB]: { status: 'failed' }, safe: { status: 'ok' } };
  expect(redactDiagnosticData(records)).toEqual({ safe: { status: 'ok' } });
  expect(Object.keys(records)).toEqual([keyA, keyB, 'safe']);
});

// These old controller fixtures matched the signature-shape policy. Keep the
// policy strict; ordinary capture/throttle tests must use non-sensitive context.
it('continues scrubbing proof-shaped paths and account labels used by old controller fixtures', () => {
  const path = 'https://simulator.invalid/v5/account/wallet-balance';
  expect(diagnosticText(path)).not.toContain('/v5/account/wallet-balance');
  expect(diagnosticText(path)).toContain('[REDACTED]');
  const label = (enabled: boolean) => `TestAccount0.1234-ws-diagnostic-disposed-${enabled}`;
  expect(diagnosticText(label(false))).toContain('[REDACTED]');
  expect(diagnosticText(label(false))).toBe(diagnosticText(label(true)));
});

// Already-clipped JSON must be safe even before final Sentry processing.
it.each(['apiKey', 'apiSecret', 'OK-ACCESS-PASSPHRASE'])('drops incomplete %s assignments at log sinks', (key) => {
  const marker = '9CREDENTIAL_PREFIX_CANARY';
  for (const quote of ['"', "'"]) {
    for (const length of [1, 6, 24, 199, 499, 500, 501, 700]) {
      const value = (marker + 'x'.repeat(700)).slice(0, length);
      const message = '{"headers":{' + quote + key + quote + ':' + quote + value;
      const safe = diagnosticText(message);
      expect(safe).not.toContain(value);
    }
  }
});

it('drops credential prefixes in actual console arguments', () => {
  const sink = vi.spyOn(console, 'error').mockImplementation(() => undefined);
  try {
    diagnosticConsole.error('{"apiKey":"9CREDENTIAL_PREFIX_CANARY');
    expect(JSON.stringify(sink.mock.calls)).not.toContain('9CRED');
  } finally { sink.mockRestore(); }
});

it.each([16, 32, 90])('drops unlabelled JWT/base64url JSON material clipped at %s', (length) => {
  const token = [
    Buffer.from(JSON.stringify({ alg: 'HS256' })).toString('base64url'),
    Buffer.from(JSON.stringify({ canary: '9PR9388_JWT_CANARY' })).toString('base64url'),
    Buffer.from('synthetic_signature').toString('base64url'),
  ].join('.').slice(0, length);
  expect(diagnosticText(`error ${token}`)).not.toContain('eyJhbGci');
  expect(diagnosticText({ diagnostic: token })).not.toContain('eyJhbGci');
});

it.each(['error:', 'error=', 'worker:error:'])('retains overlapping credential boundaries after %s', (prefix) => {
  expect(diagnosticText(`${prefix}apiKey="9OVERLAPPING_CANARY`)).not.toContain('9OVERLAPPING_CANARY');
});

it.each(['op', 'method', 'event', 'action', 'type'])(
  'uses the structured auth predicate for prefixed and clipped %s frames',
  (key) => {
    for (const operation of ['auth', 'user.auth', 'private:login', 'ws-authenticate']) {
      const frame = JSON.stringify({ [key]: operation, args: ['9AUTH_FRAME_CANARY'] });
      for (const text of [frame, `worker received ${frame}`, frame.slice(0, -3)]) {
        expect(diagnosticText(text)).not.toContain('9AUTH_FRAME_CANARY');
      }
    }
    expect(diagnosticText(`worker received {"${key}":"subscribe","args":["BTCUSDT"]}`)).toContain('BTCUSDT');
  },
);


it('preserves only exact validated refusal tags without encoded-key collisions', () => {
  const input = {
    tags: {
      passphrase_len: '0', '%70assphrase_len': 'fake-private-value',
      key_check_path: 'add', signing_mode: 'local',
    },
    extra: { key_check_path: 'add', signing_mode: 'local' },
  };
  expect(redactDiagnosticData(input)).toEqual({
    tags: { passphrase_len: '0', key_check_path: 'add', signing_mode: 'local' },
    extra: { key_check_path: '[REDACTED]', signing_mode: '[REDACTED]' },
  });
  expect(input.tags['%70assphrase_len']).toBe('fake-private-value');
  expect(redactDiagnosticData({ tags: { key_check_path: 'fake-private-value', signing_mode: 'fake-private-value' } }))
    .toEqual({ tags: { key_check_path: '[REDACTED]', signing_mode: '[REDACTED]' } });
});


it('keeps Sentry URL projections query-free without changes to live inputs', () => {
  const input = {
    exception: { values: [{ value: 'relay failed https://venue.invalid/orders?symbol=BTCUSDT&signature=fake-proof' }] },
    breadcrumbs: [{ message: '/private?sign=[REDACTED]' }],
    symbol: 'BTCUSDT',
  };
  expect(redactDiagnosticData(input)).toEqual({
    exception: { values: [{ value: 'relay failed https://venue.invalid/orders' }] },
    breadcrumbs: [{ message: '/private' }],
    symbol: 'BTCUSDT',
  });
  expect(input.breadcrumbs[0]?.message).toBe('/private?sign=[REDACTED]');
});
