import type { HttpPacket, HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { RequestsLogEvent } from '../../transport/requestsLog';

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
) => {
  const events: Array<[RequestsLogEvent, string]> = [];
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
  const transport = withRequestsLog(inner, 'binance', (event, message) => events.push([event, message]));
  return { inner, transport, events, sockets };
};

const leaked = (events: Array<[RequestsLogEvent, string]>) =>
  Object.values(SECRETS).filter((value) => events.some(([, message]) => message.includes(value)));

/** Logging runs off the caller's path, one timer tick later. */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

it('logs nothing and builds nothing while the Requests Log toggle is off', async () => {
  const { transport, events, sockets } = harness();
  await transport.request(signedRequest());
  const socket = transport.connect('wss://private', { channel: 'private', onOpen() {}, onMessage() {}, onClose() {} });
  socket.send('{"op":"subscribe"}');
  sockets[0].handlers.onMessage('{"e":"ORDER_TRADE_UPDATE"}');
  await tick();
  expect(events).toEqual([]);
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

it('logs private socket frames redacted after delivering them, skips heartbeats, and leaves public sockets unwrapped', async () => {
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

  const handlers = { channel: 'public' as const, onOpen() {}, onMessage() {}, onClose() {} };
  transport.connect('wss://public', handlers);
  expect(sockets[1].handlers).toBe(handlers);
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
