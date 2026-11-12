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
  diagnosticEvents = true,
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
      const socket: (typeof sockets)[number] = { handlers, sent: [] };
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
    diagnosticEvents ? (event) => diagnostics.push(event) : undefined,
  );
  return { inner, transport, events, sockets, diagnostics };
};

const leaked = (events: Array<[RequestsLogEvent, string]>) =>
  Object.values(SECRETS).filter((value) => events.some(([, message]) => message.includes(value)));

/** Logging runs off the caller's path, one timer tick later. */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

it.each([false, true])('withholds Kraken signed-challenge subscriptions with diagnostics=%s', async (optIn) => {
  setWsDiagnosticsEnabled(optIn);
  const { transport, sockets, diagnostics, events } = harness(undefined, 'kraken');
  const challenge = '00000000-0000-0000-0000-000000000001';
  const signature = Buffer.alloc(64, 7).toString('base64');
  const subscribe = JSON.stringify({
    event: 'subscribe',
    feed: 'open_orders',
    api_key: 'fixture-kraken-key',
    original_challenge: challenge,
    signed_challenge: signature,
  });
  const received: string[] = [];
  const socket = transport.connect('wss://fixture.invalid/ws/v1', {
    channel: 'private',
    onOpen() {},
    onMessage: (message) => received.push(message),
    onClose() {},
  });
  socket.send(subscribe);
  sockets[0].handlers.onMessage(subscribe);
  await tick();
  expect(sockets[0].sent).toEqual([subscribe]);
  expect(received).toEqual([subscribe]);
  if (optIn) {
    expect(diagnostics.map(({ direction }) => direction)).toEqual(['send', 'receive']);
    expect(events.map(([event]) => event)).toEqual(['grafana-pws-sub', 'grafana-pws']);
    expect(diagnostics.every(({ message }) => message.includes('[redacted]'))).toBe(true);
    for (const message of [...diagnostics.map((event) => event.message), ...events.map((event) => event[1])]) {
      expect(message).not.toContain(challenge);
      expect(message).not.toContain(signature);
      expect(message).not.toContain('open_orders');
      expect(message).not.toContain('fixture-kraken-key');
    }
  } else {
    expect(diagnostics).toEqual([]);
    expect(events).toEqual([]);
  }
});

const authMaterials = {
  key: 'fixture-socket-key',
  signature: Buffer.alloc(64, 11).toString('base64'),
  challenge: '00000000-0000-0000-0000-000000000002',
  token: 'fixture-socket-token',
};

const diagnosticMarkers = ['fixture-challenge-marker', 'fixture-signature-marker'].flatMap((marker) => [
  marker,
  Buffer.from(marker).toString('hex'),
  Buffer.from(marker).toString('base64'),
]);

const encodedEchoMarkers = ['fixture-challenge:/+?=\u00ff', 'fixture-signature:/+?=\u00ff'].flatMap((marker, material) => [
  { material, encoding: 'raw', value: marker },
  { material, encoding: 'hex', value: Buffer.from(marker).toString('hex') },
  { material, encoding: 'base64', value: Buffer.from(marker).toString('base64') },
  { material, encoding: 'base64url', value: Buffer.from(marker).toString('base64url') },
  { material, encoding: 'url', value: encodeURIComponent(marker) },
  { material, encoding: 'json', value: Array.from(marker, (char) => `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`).join('') },
]);
const echoShapes: Array<{ name: string; wrap: (leaf: Record<string, unknown>) => Record<string, unknown> }> = [
  { name: 'top', wrap: (leaf) => leaf },
  { name: 'result', wrap: (leaf) => ({ result: leaf }) },
  { name: 'nested', wrap: (leaf) => ({ data: { result: leaf } }) },
  { name: 'array', wrap: (leaf) => ({ data: [{ result: [leaf] }] }) },
];
const retainedEchoCases = encodedEchoMarkers.flatMap(({ material, encoding, value }) =>
  ['id', 'code', 'data', 'topic', 'message', 'orderId', 'count'].flatMap((field) =>
    echoShapes.flatMap(({ name: shape, wrap }) => ['error', 'subscribe'].map((event) => ({
      material, encoding, field, shape, event,
      wire: JSON.stringify({ event, ...wrap({ [field]: `signed_challenge=${value};original_challenge=${value}` }) }),
    })))));

it.each(retainedEchoCases)(
  'withholds retained-field echoes ($material/$encoding/$field/$shape/$event) in serialized outputs',
  async ({ wire }) => {
    setWsDiagnosticsEnabled(true);
    const { transport, sockets, diagnostics, events } = harness(undefined, 'kraken');
    const received: string[] = [];
    const socket = transport.connect('wss://fixture.invalid/', {
      channel: 'private', onOpen() {}, onMessage: (message) => received.push(message), onClose() {},
    });
    socket.send(wire);
    sockets[0].handlers.onMessage(wire);
    await tick();
    expect(sockets[0].sent).toEqual([wire]);
    expect(received).toEqual([wire]);
    expect(diagnostics.map(({ direction }) => direction)).toEqual(['send', 'receive']);
    expect(events.map(([event]) => event)).toEqual(['grafana-pws-sub', 'grafana-pws']);
    const output = JSON.stringify({ diagnostics, events });
    for (const { value } of encodedEchoMarkers) {
      expect(output.includes(value)).toBe(false);
      expect(output.includes(JSON.stringify(value).slice(1, -1))).toBe(false);
    }
  },
);

it('retains closed protocol codes and bounded counts while withholding free-form values', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, sockets, diagnostics, events } = harness();
  transport.connect('wss://fixture.invalid/', {
    channel: 'private', onOpen() {}, onMessage() {}, onClose() {},
  });
  sockets[0].handlers.onMessage(JSON.stringify({
    event: 'error', code: 'AUTH_FAILED', message: 'untrusted echo',
    result: { code: 401, count: 3, success: false, id: 'untrusted-id' },
  }));
  await tick();
  const expected = { event: 'error', code: 'AUTH_FAILED', redacted: '[redacted]',
    result: { code: 401, count: 3, success: false, id: '[redacted]' } };
  expect(diagnostics).toHaveLength(1);
  expect(events).toHaveLength(1);
  expect(diagnostics[0].message).toBe(JSON.stringify(expected));
  expect(events[0][1]).toBe(diagnostics[0].message);
});

it('uses the same value-safe sanitizer for legacy private logs without a diagnostic event listener', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, sockets, diagnostics, events } = harness(undefined, 'kraken', false);
  const socket = transport.connect('wss://fixture.invalid/', {
    channel: 'private', onOpen() {}, onMessage() {}, onClose() {},
  });
  for (const { wire } of retainedEchoCases.filter(({ shape, event }) => shape === 'nested' && event === 'subscribe')) {
    socket.send(wire);
    sockets[0].handlers.onMessage(wire);
  }
  await tick();
  expect(diagnostics).toEqual([]);
  expect(events).toHaveLength(encodedEchoMarkers.length * 7 * 2);
  const output = JSON.stringify(events);
  for (const { value } of encodedEchoMarkers) {
    expect(output.includes(value)).toBe(false);
    expect(output.includes(JSON.stringify(value).slice(1, -1))).toBe(false);
  }
});

it('withholds unknown enum strings, positional scalars and invalid numeric summaries', async () => {
  setWsDiagnosticsEnabled(true);
  const { transport, sockets, diagnostics, events } = harness();
  transport.connect('wss://fixture.invalid/', {
    channel: 'private', onOpen() {}, onMessage() {}, onClose() {},
  });
  const frame = { op: 'subscribe-echo', event: 'arbitrary-event', code: 'UNKNOWN_CODE',
    count: -1, length: 0.5, ts: 2_147_483_648, id: Number.MAX_SAFE_INTEGER,
    data: [42, true, 'AUTH_FAILED', { code: 401.5, count: 2_147_483_647 }] };
  sockets[0].handlers.onMessage(JSON.stringify(frame));
  await tick();
  expect(diagnostics[0].message).toBe(JSON.stringify({ op: '[redacted]', event: '[redacted]', code: '[redacted]',
    count: '[redacted]', length: '[redacted]', ts: '[redacted]', id: '[redacted]',
    data: ['[redacted]', '[redacted]', '[redacted]', { code: '[redacted]', count: 2_147_483_647 }] }));
  expect(events[0][1]).toBe(diagnostics[0].message);
});

it.each(diagnosticMarkers)('withholds auth-error and echo material in every diagnostic sink: %s', async (marker) => {
  setWsDiagnosticsEnabled(true);
  const { transport, sockets, diagnostics, events } = harness(undefined, 'kraken');
  const received: string[] = [];
  const socket = transport.connect('wss://fixture.invalid/', {
    channel: 'private', onOpen() {}, onMessage: (message) => received.push(message), onClose() {},
  });
  const frames = [
    { event: 'error', message: `signed_challenge=${marker}` },
    { event: 'error', message: `original_challenge=${marker}` },
    { event: 'error', message: marker, code: 401 },
    { event: 'subscribe', echo: marker },
    { event: 'subscribe', data: { message: marker, echo: marker, unexpected: marker } },
    { event: 'subscribe', data: [marker] },
    { event: 'subscribe', [marker]: marker },
  ].map((frame) => JSON.stringify(frame));
  for (const frame of frames) {
    socket.send(frame);
    sockets[0].handlers.onMessage(frame);
  }
  await tick();
  expect(received).toEqual(frames);
  expect(sockets[0].sent).toEqual(frames);
  expect(diagnostics).toHaveLength(frames.length * 2);
  expect(events).toHaveLength(frames.length * 2);
  for (const emitted of [...diagnostics.map(({ message }) => message), ...events.map(([, message]) => message)]) {
    for (const material of diagnosticMarkers) expect(emitted).not.toContain(material);
  }
});

const subscriptionAuthCases: Array<{ venue: string; frame: unknown }> = [
  { venue: 'kraken-challenge', frame: { event: 'challenge', message: authMaterials.challenge } },
  { venue: 'kraken-signature-field', frame: { event: 'subscribe', signed_challenge: authMaterials.signature } },
  { venue: 'kraken-challenge-field', frame: { event: 'subscribe', original_challenge: authMaterials.challenge } },
  {
    venue: 'krakenspot',
    frame: { method: 'subscribe', params: { channel: 'executions', token: authMaterials.token } },
  },
  {
    venue: 'backpack',
    frame: {
      method: 'SUBSCRIBE',
      params: ['account.orderUpdate'],
      signature: [authMaterials.key, authMaterials.signature, '1000', '5000'],
    },
  },
  { venue: 'coinbase', frame: { type: 'subscribe', channel: 'user', jwt: authMaterials.token } },
  { venue: 'coinbasespot', frame: { type: 'subscribe', channel: 'user', jwt: authMaterials.token } },
  { venue: 'lighter', frame: { type: 'subscribe', channel: 'account_all_orders/1', auth: authMaterials.token } },
  {
    venue: 'lighter-robinhood',
    frame: { type: 'subscribe', channel: 'account_all_positions/1', auth: authMaterials.token },
  },
  {
    venue: 'polymarket',
    frame: {
      type: 'user',
      auth: { apiKey: authMaterials.key, secret: authMaterials.token, passphrase: authMaterials.signature },
    },
  },
  {
    venue: 'grvt',
    frame: { type: 'auth', session_cookie: authMaterials.token, account_id: 'fixture-account', testnet: true },
  },
  { venue: 'paradex', frame: { jsonrpc: '2.0', method: 'auth', params: { bearer: authMaterials.token }, id: 1 } },
  {
    venue: 'binancespot',
    frame: {
      method: 'userDataStream.subscribe.signature',
      params: { apiKey: authMaterials.key, timestamp: 1000, signature: authMaterials.signature },
    },
  },
];

it.each(subscriptionAuthCases)(
  'withholds $venue authentication material in both event paths',
  async ({ venue, frame }) => {
    const { transport, sockets, diagnostics, events } = harness(undefined, venue);
    const socket = transport.connect('wss://fixture.invalid/', {
      channel: 'private',
      onOpen() {},
      onMessage() {},
      onClose() {},
    });
    const wire = JSON.stringify(frame);
    socket.send(wire);
    sockets[0].handlers.onMessage(wire);
    await tick();
    expect(diagnostics).toEqual([]);
    expect(events).toEqual([]);
    setWsDiagnosticsEnabled(true);
    socket.send(wire);
    sockets[0].handlers.onMessage(wire);
    await tick();
    expect(diagnostics).toHaveLength(2);
    expect(events).toHaveLength(2);
    for (const entry of [...diagnostics.map(({ message }) => message), ...events.map((event) => event[1])]) {
      expect(entry).toContain('[redacted]');
      for (const material of Object.values(authMaterials)) expect(entry).not.toContain(material);
    }
    expect(sockets[0].sent).toEqual([wire, wire]);
  },
);

const privateSocketLocations: Array<{ venue: string; path: string; tokenInHeaders?: boolean }> = [
  { venue: 'binance', path: '/private/ws?listenKey=' },
  { venue: 'binanceinverse', path: '/ws/' },
  { venue: 'binancepm', path: '/pm/ws/' },
  { venue: 'asterdex', path: '/ws/' },
  { venue: 'bingx', path: '/swap-market?listenKey=' },
  { venue: 'bitrue', path: '/stream?listenKey=' },
  { venue: 'kucoin', path: '/endpoint?token=' },
  { venue: 'kucoinspot', path: '/endpoint?token=' },
  { venue: 'grvt', path: '/ws', tokenInHeaders: true },
  { venue: 'weex', path: '/v3/ws/private', tokenInHeaders: true },
];

it.each(privateSocketLocations)(
  'omits $venue authenticated socket locations and headers from both event paths',
  async ({ venue, path, tokenInHeaders }) => {
    const { transport, sockets, diagnostics, events } = harness(undefined, venue);
    const endpoint = `wss://fixture.invalid${path}${tokenInHeaders ? '' : authMaterials.token}`;
    const headers = {
      Cookie: authMaterials.token,
      Authorization: authMaterials.signature,
      'ACCESS-KEY': authMaterials.key,
      'ACCESS-PASSPHRASE': authMaterials.token,
      'ACCESS-SIGN': authMaterials.signature,
    };
    const wire = JSON.stringify({ topic: 'orders', data: { orderId: 'fixture-order', endpoint } });
    const socket = transport.connect(endpoint, {
      channel: 'private',
      headers,
      onOpen() {},
      onMessage() {},
      onClose() {},
    });
    socket.send(wire);
    sockets[0].handlers.onMessage(wire);
    await tick();
    expect(diagnostics).toEqual([]);
    expect(events).toEqual([]);
    setWsDiagnosticsEnabled(true);
    socket.send(wire);
    sockets[0].handlers.onMessage(wire);
    await tick();
    expect(diagnostics).toHaveLength(2);
    expect(events).toHaveLength(2);
    const logged = JSON.stringify({ diagnostics, events });
    for (const value of [endpoint, ...Object.values(authMaterials), ...Object.keys(headers)])
      expect(logged).not.toContain(value);
    expect(logged).not.toContain('fixture-order');
    expect(diagnostics.every(({ message }) => message.includes('"orderId":"[redacted]"'))).toBe(true);
    expect(sockets[0].sent).toEqual([wire, wire]);
    expect(sockets[0].handlers.headers).toEqual(headers);
  },
);

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
  const channels: WsDiagnosticEvent['channel'][] = ['public', 'private', 'business'];
  for (const channel of channels) {
    const received = vi.fn();
    const socket = transport.connect('wss://fixture.invalid/', {
      channel,
      onOpen() {},
      onMessage: received,
      onClose() {},
    });
    const connected = sockets.at(-1);
    if (!connected) throw new Error('transport did not create a socket');
    const wire = '{"op":"subscribe","args":["tickers.BTCUSDT"]}';
    socket.send(wire);
    connected.handlers.onMessage('{"topic":"tickers.BTCUSDT","data":{"lastPrice":"50000"}}');
    expect(received).toHaveBeenCalledOnce();
    expect(connected.sent).toEqual([wire]);
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
  expect(diagnostics.map(({ message }) => message)).toEqual(
    sockets.map(() => JSON.stringify({ topic: '[redacted]', data: { lastPrice: '[redacted]' } })),
  );
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
  expect(logged).not.toContain('order-1');
  expect(diagnostics[1].message).toContain('"orderId":"[redacted]"');
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
  const onBinary = sockets[0].handlers.onBinary;
  if (!onBinary) throw new Error('transport did not install a binary handler');
  onBinary(bytes);
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

const toggleChannels: WsDiagnosticEvent['channel'][] = ['public', 'private', 'business'];
const toggleDirections: WsDiagnosticEvent['direction'][] = ['send', 'receive'];
const toggleTransitions = [
  { transition: 'stays enabled', change: () => setWsDiagnosticsEnabled(true), expected: 1 },
  { transition: 'stays disabled', change: () => setWsDiagnosticsEnabled(false), expected: 0 },
  {
    transition: 'disabled then re-enabled',
    change: () => { setWsDiagnosticsEnabled(false); setWsDiagnosticsEnabled(true); },
    expected: 0,
  },
];

it.each(toggleChannels.flatMap((channel) => toggleDirections.flatMap((direction) =>
  toggleTransitions.map((transition) => ({ channel, direction, ...transition })),
)))('queued $channel $direction diagnostics: $transition', ({ channel, direction, change, expected }) => {
  setWsDiagnosticsEnabled(true);
  const { inner, transport, sockets, diagnostics, events } = harness(undefined, 'hyrotrader');
  const queued: Array<() => void> = [];
  inner.setTimeout = (callback) => { queued.push(callback); return 0; };
  const received = vi.fn();
  const socket = transport.connect('wss://fixture.invalid/', {
    channel, onOpen() {}, onMessage: received, onClose() {},
  });
  const wire = '{"op":"subscribe"}';
  const deliver = () => direction === 'send' ? socket.send(wire) : sockets[0].handlers.onMessage(wire);
  deliver();
  expect(queued).toHaveLength(1);
  expect(diagnostics).toEqual([]);
  change();
  for (const callback of queued.splice(0)) callback();
  expect(diagnostics).toHaveLength(expected);
  expect(events).toHaveLength(channel === 'public' ? 0 : expected);
  expect(sockets[0].sent).toEqual(direction === 'send' ? [wire] : []);
  expect(received).toHaveBeenCalledTimes(direction === 'receive' ? 1 : 0);

  // A retired entry stays gone, while fresh work on the same socket can resume.
  setWsDiagnosticsEnabled(true);
  deliver();
  for (const callback of queued.splice(0)) callback();
  expect(diagnostics).toHaveLength(expected + 1);
  expect(diagnostics.at(-1)).toEqual({ venue: 'hyrotrader', channel, direction, message: wire });
  expect(events).toHaveLength(channel === 'public' ? 0 : expected + 1);
  expect(sockets[0].sent).toEqual(direction === 'send' ? [wire, wire] : []);
  expect(received).toHaveBeenCalledTimes(direction === 'receive' ? 2 : 0);
});

it.each(toggleChannels)('queued $channel binary diagnostics stay retired after re-enabling', (channel) => {
  setWsDiagnosticsEnabled(true);
  const { inner, transport, sockets, diagnostics, events } = harness();
  const queued: Array<() => void> = [];
  inner.setTimeout = (callback) => { queued.push(callback); return 0; };
  const received = vi.fn();
  transport.connect('wss://fixture.invalid/', {
    channel, onOpen() {}, onMessage() {}, onBinary: received, onClose() {},
  });
  const bytes = new Uint8Array([1, 2, 3]);
  const onBinary = sockets[0].handlers.onBinary;
  if (!onBinary) throw new Error('transport did not install a binary handler');
  onBinary(bytes);
  setWsDiagnosticsEnabled(false);
  setWsDiagnosticsEnabled(true);
  for (const callback of queued.splice(0)) callback();
  expect(received).toHaveBeenCalledExactlyOnceWith(bytes);
  expect(diagnostics).toEqual([]);
  expect(events).toEqual([]);
});

it('a toggle retires the old transport queue without re-attributing it to the replacement', () => {
  setWsDiagnosticsEnabled(true);
  const old = harness(undefined, 'hyrotrader');
  const replacement = harness(undefined, 'bybit');
  const queued: Array<() => void> = [];
  for (const { inner } of [old, replacement]) {
    inner.setTimeout = (callback) => { queued.push(callback); return 0; };
  }
  const handlers: Parameters<Transport['connect']>[1] = { channel: 'public', onOpen() {}, onMessage() {}, onClose() {} };
  const oldSocket = old.transport.connect('wss://old-fixture.invalid/', handlers);
  oldSocket.send('{"op":"subscribe"}');
  setWsDiagnosticsEnabled(false);
  setWsDiagnosticsEnabled(true);
  const newSocket = replacement.transport.connect('wss://new-fixture.invalid/', handlers);
  newSocket.send('{"op":"unsubscribe"}');
  for (const callback of queued.splice(0)) callback();
  expect(old.diagnostics).toEqual([]);
  expect(replacement.diagnostics).toEqual([
    { venue: 'bybit', channel: 'public', direction: 'send', message: '{"op":"unsubscribe"}' },
  ]);
  expect(old.sockets[0].sent).toEqual(['{"op":"subscribe"}']);
  expect(replacement.sockets[0].sent).toEqual(['{"op":"unsubscribe"}']);
});

it('disabling inside a structured callback retires its pending legacy sibling', () => {
  setWsDiagnosticsEnabled(true);
  const { inner, sockets } = harness();
  const queued: Array<() => void> = [];
  inner.setTimeout = (callback) => { queued.push(callback); return 0; };
  const legacy = vi.fn();
  const structured = vi.fn(() => { setWsDiagnosticsEnabled(false); setWsDiagnosticsEnabled(true); });
  const transport = withRequestsLog(inner, 'hyrotrader', legacy, structured);
  const socket = transport.connect('wss://fixture.invalid/', {
    channel: 'private', onOpen() {}, onMessage() {}, onClose() {},
  });
  socket.send('{"op":"subscribe"}');
  for (const callback of queued.splice(0)) callback();
  expect(structured).toHaveBeenCalledOnce();
  expect(legacy).not.toHaveBeenCalled();
  expect(sockets[0].sent).toEqual(['{"op":"subscribe"}']);
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
