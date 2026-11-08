import type { HttpPacket, HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { RequestsLogEvent, WsDiagnosticEvent } from '../../transport/requestsLog';

import { afterEach, expect, it, vi } from 'vitest';

import { withRequestsLog } from '../../transport/requestsLog';
import { setWsDiagnosticsEnabled } from '../../utils/wsDiagnostics';

// Lets a test make redaction or the toggle check itself throw.
const faults = vi.hoisted(() => ({ redact: false, toggle: false }));
vi.mock('../../utils/wsDiagnostics', async (original) => {
  const actual = await original<typeof import('../../utils/wsDiagnostics')>();
  return {
    ...actual,
    redactWsDiagnosticPayload: (message: string) => {
      if (faults.redact) throw new Error('redaction failed');
      return actual.redactWsDiagnosticPayload(message);
    },
    isWsDiagnosticsEnabled: () => {
      if (faults.toggle) throw new Error('toggle unreadable');
      return actual.isWsDiagnosticsEnabled();
    },
  };
});

afterEach(() => {
  setWsDiagnosticsEnabled(false);
  faults.redact = false;
  faults.toggle = false;
});

// One value per secret kind the Requests Log must never show.
const SECRETS = {
  apiKey: 'APIKEYVALUE123',
  secret: 'SECRETVALUE456',
  signature: 'SIGNATUREVALUE789',
  passphrase: 'PASSPHRASEVALUE',
  proxyToken: 'PROXYTOKENVALUE',
  authHeader: 'BEARERVALUE',
  sig: 'WALLETSIGVALUE',
};

const signedRequest = (): HttpPacket => ({
  request: 1,
  method: 'post',
  url: `https://api.binance.com/api/v3/order?symbol=BTCUSDT&apiKey=${SECRETS.apiKey}&signature=${SECRETS.signature}&token=${SECRETS.proxyToken}`,
  headers: {
    'X-MBX-APIKEY': SECRETS.apiKey,
    Authorization: `Bearer ${SECRETS.authHeader}`,
    'OK-ACCESS-PASSPHRASE': SECRETS.passphrase,
  },
  body: JSON.stringify({
    symbol: 'BTCUSDT',
    secret: SECRETS.secret,
    passphrase: SECRETS.passphrase,
    signature: { r: SECRETS.signature },
    sig: SECRETS.sig,
    proxyToken: SECRETS.proxyToken,
  }),
});

const harness = (
  response: () => HttpResponse = () => ({ status: 200, body: new TextEncoder().encode('{"ok":1}') }),
  label = 'binance',
) => {
  const events: Array<[RequestsLogEvent, string]> = [];
  const diagnostics: WsDiagnosticEvent[] = [];
  const sockets: Array<{ handlers: Parameters<Transport['connect']>[1]; sent: string[] }> = [];
  const inner: Transport = {
    now: () => 0,
    request: vi.fn(async () => response()),
    prepareHttp: () => ({
      bind: () => ({ ready: () => true, dispatch: async () => response() }),
    }),
    connect: (_url, handlers) => {
      const socket = { handlers, sent: [] as string[] };
      sockets.push(socket);
      return { send: (text) => socket.sent.push(text), close: () => {} };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  const transport = withRequestsLog(
    inner,
    label,
    (event, message) => events.push([event, message]),
    (event) => diagnostics.push(event),
  );
  return { inner, transport, events, sockets, diagnostics };
};

const leaked = (events: Array<[RequestsLogEvent, string]>) =>
  Object.values(SECRETS).filter((value) => events.some(([, message]) => message.includes(value)));

/** Logging runs off the caller's path, one timer tick later. */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

it('logs nothing and builds nothing while the Requests Log toggle is off', async () => {
  const { inner, transport, events, sockets, diagnostics } = harness();
  const schedule = vi.spyOn(inner, 'setTimeout');
  await transport.request(signedRequest());
  const socket = transport.connect('wss://private', { channel: 'private', onOpen() {}, onMessage() {}, onClose() {} });
  socket.send('{"op":"subscribe"}');
  sockets[0].handlers.onMessage('{"e":"ORDER_TRADE_UPDATE"}');
  await tick();
  expect(events).toEqual([]);
  expect(diagnostics).toEqual([]);
  expect(schedule).not.toHaveBeenCalled();
});

it('logs each HTTP request and response with status and timing, every secret redacted and no header', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, events } = harness(() => ({
    status: 200,
    body: new TextEncoder().encode(JSON.stringify({ orderId: 7, listenKey: SECRETS.proxyToken })),
  }));
  const response = await transport.request(signedRequest());
  expect(new TextDecoder().decode(response.body as Uint8Array)).toContain('orderId');
  await tick();
  expect(events.map(([event]) => event)).toEqual(['grafana-http-request', 'grafana-http-response']);
  expect(events[0][1]).toMatch(/^Request POST https:\/\/api\.binance\.com\/api\/v3\/order\?symbol=BTCUSDT/);
  expect(events[1][1]).toMatch(/^Response POST .* 200 \{"status":200,"ms":\d+,"body":/);
  expect(events[1][1]).toContain('orderId');
  expect(leaked(events)).toEqual([]);
  for (const header of ['X-MBX-APIKEY', 'Authorization', 'OK-ACCESS-PASSPHRASE']) {
    expect(events.some(([, message]) => message.includes(header))).toBe(false);
  }
});

it("hands the caller the transport's own promise, response and stream, and never reads a streamed body", async () => {
  setWsDiagnosticsEnabled(true);
  const stream = (async function* () {
    yield new TextEncoder().encode('{"code":-1022}');
  })();
  const answer: HttpResponse = { status: 400, body: stream };
  const { inner, transport, events } = harness(() => answer);
  const pending = Promise.resolve(answer);
  inner.request = () => pending;
  const promise = transport.request(signedRequest());
  expect(promise).toBe(pending);
  const response = await promise;
  expect(response).toBe(answer);
  expect(response.body).toBe(stream);
  await tick();
  expect(events.map(([event]) => event)).toEqual(['grafana-http-request', 'grafana-http-response']);
  expect(events[1][1]).toContain('(streamed body, not logged)');
  let body = '';
  for await (const chunk of response.body as AsyncIterable<Uint8Array>) body += new TextDecoder().decode(chunk);
  expect(body).toBe('{"code":-1022}');

  const prepared = await transport.prepareHttp!({ ...signedRequest(), intent: 'primary' });
  await prepared.bind(signedRequest()).dispatch();
  await tick();
  expect(events.slice(2).map(([event]) => event)).toEqual(['grafana-http-request', 'grafana-http-response']);
  expect(leaked(events)).toEqual([]);
});

it('logs private socket frames redacted after delivering them and skips heartbeats', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, events, sockets } = harness();
  const delivered: string[] = [];
  const socket = transport.connect('wss://private', {
    channel: 'private',
    onOpen() {},
    onMessage: (text) => delivered.push(text),
    onClose() {},
  });
  socket.send(
    JSON.stringify({ method: 'session.logon', params: { apiKey: SECRETS.apiKey, signature: SECRETS.signature } }),
  );
  sockets[0].handlers.onMessage('{"e":"ORDER_TRADE_UPDATE","o":{"i":7}}');
  sockets[0].handlers.onMessage('pong');
  expect(delivered).toEqual(['{"e":"ORDER_TRADE_UPDATE","o":{"i":7}}', 'pong']);
  expect(sockets[0].sent).toHaveLength(1);
  expect(events).toEqual([]);
  await tick();
  expect(events.map(([event]) => event)).toEqual(['grafana-pws-sub', 'grafana-pws']);
  expect(events[1][1]).toContain('ORDER_TRADE_UPDATE');
  expect(leaked(events)).toEqual([]);
});

it.each(['hyrotrader', 'binance', 'okx'])('opts %s into diagnostics for every socket channel', async (label) => {
  const { transport, sockets, diagnostics } = harness(undefined, label);
  for (const channel of ['public', 'private', 'business'] as const) {
    const received = vi.fn();
    const socket = transport.connect('wss://fixture.invalid/', {
      channel,
      onOpen() {},
      onMessage: received,
      onClose() {},
    });
    const wire = '{"op":"subscribe","args":["tickers.BTCUSDT"]}';
    socket.send(wire);
    sockets.at(-1)!.handlers.onMessage('{"topic":"tickers.BTCUSDT","data":{"lastPrice":"50000"}}');
    expect(received).toHaveBeenCalledOnce();
    expect(sockets.at(-1)!.sent).toEqual([wire]);
  }
  await tick();
  expect(diagnostics).toEqual([]);
  setWsDiagnosticsEnabled(true);
  for (const socket of sockets) {
    socket.handlers.onMessage('{"topic":"tickers.BTCUSDT","data":{"lastPrice":"50001"}}');
  }
  await tick();
  expect(diagnostics.map(({ venue, channel, direction }) => ({ venue, channel, direction }))).toEqual(
    ['public', 'private', 'business'].map((channel) => ({ venue: label, channel, direction: 'receive' })),
  );
  expect(diagnostics.every(({ message }) => message.includes('50001'))).toBe(true);
});

it('redacts auth payloads, keys, signatures and listen keys in both WS directions', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, sockets, diagnostics, events } = harness(undefined, 'hyrotrader');
  const received: string[] = [];
  const socket = transport.connect('wss://fixture.invalid/private?listenKey=FIXTURE_URL_KEY', {
    channel: 'private',
    onOpen() {},
    onMessage: (message) => received.push(message),
    onClose() {},
  });
  const auth = JSON.stringify({
    op: 'auth',
    args: ['FIXTURE_KEY', 123456789, 'FIXTURE_SIGN'],
    payload: 'FIXTURE_AUTH',
  });
  const payload = JSON.stringify({
    topic: 'order',
    data: {
      api_key: 'FIXTURE_KEY',
      'X-BAPI-SIGN': 'FIXTURE_SIGN',
      listen_key: 'FIXTURE_LISTEN',
      auth_payload: { opaque: 'FIXTURE_OTHER_AUTH' },
      credentials: ['FIXTURE_CREDENTIAL'],
      wsAuth: 'FIXTURE_WS_AUTH',
      cookie: 'FIXTURE_COOKIE',
      opaque: 'f01df00daabbccdd0123456789abcdef0123456789abcdef',
      orderId: 'order-1',
    },
  });
  socket.send(auth);
  socket.send(payload);
  sockets[0].handlers.onMessage(auth);
  sockets[0].handlers.onMessage(payload);
  expect(received).toEqual([auth, payload]);
  expect(sockets[0].sent).toEqual([auth, payload]);
  await tick();
  expect(diagnostics.map(({ direction }) => direction)).toEqual(['send', 'send', 'receive', 'receive']);
  const logged = JSON.stringify({ diagnostics, events });
  for (const secret of [
    'FIXTURE_KEY',
    '123456789',
    'FIXTURE_SIGN',
    'FIXTURE_AUTH',
    'FIXTURE_LISTEN',
    'FIXTURE_URL_KEY',
    'FIXTURE_OTHER_AUTH',
    'FIXTURE_CREDENTIAL',
    'FIXTURE_WS_AUTH',
    'FIXTURE_COOKIE',
    'f01d',
  ]) {
    expect(logged).not.toContain(secret);
  }
  expect(logged).toContain('order-1');
  expect(logged).toContain('[redacted]');
});

it('withholds malformed, oversized, binary and deeply nested frame contents and drops queued entries when disabled', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, sockets, diagnostics } = harness();
  const binary = vi.fn();
  const socket = transport.connect('wss://fixture.invalid/', {
    onOpen() {},
    onMessage() {},
    onBinary: binary,
    onClose() {},
  });
  let nested: unknown = { opaque: 'FIXTURE_DEEP_SECRET' };
  for (let depth = 0; depth < 20; depth++) nested = { inner: nested };
  for (const frame of [
    '{"op":"auth","args":["FIXTURE_TRUNCATED_SECRET"',
    'FIXTURE_OPAQUE_SECRET',
    JSON.stringify({ data: nested }),
    JSON.stringify({ data: 'x'.repeat(20_000), opaque: 'FIXTURE_LARGE_SECRET' }),
    JSON.stringify({ echo: '{"op":"auth","args":["FIXTURE_EMBEDDED_SECRET"]}' }),
    JSON.stringify([0, 'auth', 'FIXTURE_POSITIONAL_SECRET']),
  ])
    socket.send(frame);
  const bytes = new TextEncoder().encode('FIXTURE_BINARY_SECRET');
  sockets[0].handlers.onBinary!(bytes);
  expect(binary).toHaveBeenCalledWith(bytes);
  await tick();
  expect(diagnostics).toHaveLength(7);
  expect(JSON.stringify(diagnostics)).not.toContain('FIXTURE_');
  expect(diagnostics.at(-1)?.message).toBe(`binary ${bytes.byteLength} bytes`);
  socket.send('{"op":"subscribe","args":["orders"]}');
  setWsDiagnosticsEnabled(false);
  await tick();
  expect(diagnostics).toHaveLength(7);
});

it('redacts token-shaped keys, query auth, any JWT, and secrets nested in a body longer than the log cap', async () => {
  setWsDiagnosticsEnabled(true);
  const jwt = 'eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJwYXJhZGV4In0.c2lnbmF0dXJlLXZhbHVl';
  const long = JSON.stringify({
    rows: Array.from({ length: 200 }, (_, index) => ({ index, note: 'x'.repeat(20) })),
    nested: { deeper: { refreshToken: 'REFRESHVALUE' } },
  });
  const { transport, events } = harness(() => ({
    status: 200,
    body: new TextEncoder().encode(JSON.stringify({ jwt_token: jwt, access_token: 'ACCESSVALUE' })),
  }));
  await transport.request({
    method: 'get',
    url: `https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders?account_index=1&auth=LIGHTERAUTHVALUE`,
    headers: {},
    body: long,
  });
  await tick();
  const logged = events.map(([, message]) => message).join('\n');
  for (const secret of [jwt, 'ACCESSVALUE', 'LIGHTERAUTHVALUE', 'REFRESHVALUE']) {
    expect(logged).not.toContain(secret);
  }
  expect(long.length).toBeGreaterThan(4096);
  expect(logged).toContain('account_index=1');
  expect(logged).toContain('truncated');
});

/** A Uint8Array whose copy throws: the log's only touch of the caller's bytes. */
class UncopyableBytes extends Uint8Array {
  slice(): never {
    throw new Error('cannot copy');
  }
}

it('a throw at any logging step leaves what the caller gets identical to the bare transport', async () => {
  setWsDiagnosticsEnabled(true);
  const bytes = new TextEncoder().encode('{"orderId":7}');
  const failure = new Error('venue unreachable');
  const decoder = globalThis.TextDecoder;
  const steps: Array<{
    name: string;
    emit?: () => void;
    request?: () => HttpPacket;
    answer?: () => HttpResponse;
    timer?: () => never;
    before?: () => void;
  }> = [
    {
      name: 'emit throws',
      emit: () => {
        throw new Error('log sink down');
      },
    },
    {
      name: 'reading the request throws',
      request: () =>
        Object.defineProperty(signedRequest(), 'body', {
          get: () => {
            throw new Error('getter');
          },
        }),
    },
    {
      name: 'reading the status throws',
      answer: () =>
        Object.defineProperty({ body: bytes } as HttpResponse, 'status', {
          get: () => {
            throw new Error('getter');
          },
        }),
    },
    { name: 'copying the body throws', answer: () => ({ status: 200, body: new UncopyableBytes(bytes) }) },
    {
      name: 'redaction throws',
      before: () => {
        faults.redact = true;
      },
    },
    {
      name: 'the toggle check throws',
      before: () => {
        faults.toggle = true;
      },
    },
    { name: 'no TextDecoder', before: () => Reflect.deleteProperty(globalThis, 'TextDecoder') },
    {
      name: 'TextDecoder throws',
      before: () => {
        globalThis.TextDecoder = class {
          constructor() {
            throw new Error('no codec');
          }
        } as unknown as typeof TextDecoder;
      },
    },
    {
      name: 'the timer throws',
      timer: () => {
        throw new Error('no timers');
      },
    },
  ];
  for (const step of steps) {
    try {
      step.before?.();
      for (const outcome of ['answer', 'reject'] as const) {
        const answer = step.answer?.() ?? { status: 200, body: bytes };
        const inner = harness().inner;
        inner.request = () => (outcome === 'answer' ? Promise.resolve(answer) : Promise.reject(failure));
        if (step.timer) inner.setTimeout = step.timer;
        const logged = withRequestsLog(inner, 'binance', step.emit ?? (() => {}));
        const request = step.request?.() ?? signedRequest();
        const settle = (promise: Promise<HttpResponse>) =>
          promise.then(
            (value) => ({ value }),
            (error: unknown) => ({ error }),
          );
        const bare = await settle(inner.request(request));
        const viaLog = await settle(logged.request(request));
        expect(viaLog, step.name).toEqual(bare);
        if ('value' in viaLog) expect(viaLog.value, step.name).toBe(answer);
        else expect(viaLog.error, step.name).toBe(failure);
      }
      await tick();
    } finally {
      globalThis.TextDecoder = decoder;
      faults.redact = false;
      faults.toggle = false;
    }
  }

  // Sockets: the handler runs and the frame is sent whatever the log does.
  faults.redact = true;
  for (const emit of [
    () => {},
    () => {
      throw new Error('log sink down');
    },
  ]) {
    const { inner, sockets } = harness();
    const delivered: string[] = [];
    const socket = withRequestsLog(inner, 'binance', emit).connect('wss://private', {
      channel: 'private',
      onOpen() {},
      onMessage: (text) => delivered.push(text),
      onBinary: (data) => delivered.push(`binary ${data.byteLength}`),
      onClose() {},
    });
    socket.send('{"op":"subscribe"}');
    sockets[0].handlers.onMessage('{"e":"ORDER_TRADE_UPDATE"}');
    sockets[0].handlers.onBinary!(new Uint8Array(3));
    expect(sockets[0].sent).toEqual(['{"op":"subscribe"}']);
    expect(delivered).toEqual(['{"e":"ORDER_TRADE_UPDATE"}', 'binary 3']);
    await tick();
  }
});
