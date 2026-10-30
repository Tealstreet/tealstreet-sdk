import type { HttpPacket, HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';
import type { RequestsLogEvent } from '../../transport/requestsLog';

import { afterEach, expect, it, vi } from 'vitest';

import { withRequestsLog } from '../../transport/requestsLog';
import { setWsDiagnosticsEnabled } from '../../utils/wsDiagnostics';

afterEach(() => setWsDiagnosticsEnabled(false));

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

it('logs nothing and builds nothing while the Requests Log toggle is off', async () => {
  const { transport, events, sockets } = harness();
  await transport.request(signedRequest());
  const socket = transport.connect('wss://private', { channel: 'private', onOpen() {}, onMessage() {}, onClose() {} });
  socket.send('{"op":"subscribe"}');
  sockets[0].handlers.onMessage('{"e":"ORDER_TRADE_UPDATE"}');
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
  expect(events.map(([event]) => event)).toEqual(['grafana-http-request', 'grafana-http-response']);
  expect(events[0][1]).toMatch(/^Request POST https:\/\/api\.binance\.com\/api\/v3\/order\?symbol=BTCUSDT/);
  expect(events[1][1]).toMatch(/^Response POST .* 200 \{"status":200,"ms":\d+,"body":/);
  expect(events[1][1]).toContain('orderId');
  expect(leaked(events)).toEqual([]);
  for (const header of ['X-MBX-APIKEY', 'Authorization', 'OK-ACCESS-PASSPHRASE']) {
    expect(events.some(([, message]) => message.includes(header))).toBe(false);
  }
});

it('logs a streamed body once it completes, and a managed request at dispatch', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, events } = harness(() => ({
    status: 400,
    body: (async function* () {
      yield new TextEncoder().encode('{"code":-1022,');
      yield new TextEncoder().encode('"msg":"Signature for this request is not valid."}');
    })(),
  }));
  const response = await transport.request(signedRequest());
  expect(events).toHaveLength(1);
  let body = '';
  for await (const chunk of response.body as AsyncIterable<Uint8Array>) body += new TextDecoder().decode(chunk);
  expect(body).toContain('-1022');
  expect(events[1][1]).toContain('-1022');

  const prepared = await transport.prepareHttp!({ ...signedRequest(), intent: 'primary' });
  const bound = prepared.bind(signedRequest());
  expect(events).toHaveLength(2);
  const managed = await bound.dispatch();
  for await (const _ of managed.body as AsyncIterable<Uint8Array>) {
    // The entry is written once the body has been read.
  }
  expect(events.slice(2).map(([event]) => event)).toEqual(['grafana-http-request', 'grafana-http-response']);
  expect(leaked(events)).toEqual([]);
});

it('logs private socket frames redacted, skips heartbeats, and leaves public sockets unwrapped', () => {
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
  expect(events.map(([event]) => event)).toEqual(['grafana-pws-sub', 'grafana-pws']);
  expect(events[1][1]).toContain('ORDER_TRADE_UPDATE');
  expect(sockets[0].sent).toHaveLength(1);
  expect(leaked(events)).toEqual([]);

  const handlers = { channel: 'public' as const, onOpen() {}, onMessage() {}, onClose() {} };
  transport.connect('wss://public', handlers);
  expect(sockets[1].handlers).toBe(handlers);
});
