import type { AccountAuthResult } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { NativeFrameSession, type NativeFrameSessionOptions, type NativeFrameSocket } from './NativeFrameSession';

const PROTOCOL = 'safe-cex-frame.v1';
const ACCOUNT_AUTH_PROTOCOL = 'safe-cex-frame.account-auth';
const URL = 'wss://native.invalid/accounts/account-1';
const TOKEN = 'opaque-account-token';

class TestSocket extends EventTarget implements NativeFrameSocket {
  binaryType: BinaryType = 'blob';
  bufferedAmount = 0;
  readyState = 0;
  protocol = PROTOCOL;
  sent: string[] = [];
  close = vi.fn(() => { this.readyState = 3; });
  send = vi.fn((data: string) => { this.sent.push(data); });

  open(): void {
    this.readyState = 1;
    this.dispatchEvent(new Event('open'));
  }

  message(data: unknown): void {
    this.dispatchEvent(new MessageEvent('message', { data }));
  }
}

function frame(delta = false): ArrayBuffer {
  const buffer = new ArrayBuffer(delta ? 40 : 16);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, delta ? 1 : 0, true);
  view.setFloat64(8, 1, true);
  if (delta) {
    view.setUint16(16, 15, true);
    view.setUint32(20, 16, true);
  }
  return buffer;
}

function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function setup(overrides: Partial<NativeFrameSessionOptions> = {}) {
  const socket = new TestSocket();
  const sink = { onFrame: vi.fn(), onNotice: vi.fn(), onClose: vi.fn() };
  const socketFactory = vi.fn(() => socket);
  const promise = NativeFrameSession.connect({ url: URL, token: TOKEN, sink, socketFactory, ...overrides });
  // Tests intentionally terminate the connection before awaiting it.
  void promise.catch(() => {});
  return { socket, sink, socketFactory, promise };
}

async function connected(overrides: Partial<NativeFrameSessionOptions> = {}) {
  const setupResult = setup(overrides);
  setupResult.socket.open();
  setupResult.socket.message(frame());
  const session = await setupResult.promise;
  return { ...setupResult, session };
}

async function microtasks() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

afterEach(() => { vi.useRealTimers(); });

describe('NativeFrameSession', () => {
  it('delivers typed account auth before raw notices in serialized order without command ownership', async () => {
    const events: string[] = [];
    const wait = deferred();
    const onAccountAuth = vi.fn(async (result: AccountAuthResult) => {
      expect(Object.isFrozen(result)).toBe(true);
      events.push(`${result.credential}/${result.scope}/${result.outcome}`);
      await wait.promise;
    });
    const sink = {
      onFrame: vi.fn(), onClose: vi.fn(), onAccountAuth,
      onNotice: vi.fn((raw: string) => { events.push(JSON.parse(raw).type); }),
    };
    const { session, socket } = await connected({ sink });
    socket.message('{"type":"accountAuth","credential":"secondary","scope":"orders","outcome":"rejected"}');
    socket.message('{"type":"done","command":99,"result":"[]"}');
    await microtasks();
    expect(events).toEqual(['secondary/orders/rejected']);
    expect(sink.onNotice).not.toHaveBeenCalled();
    wait.resolve();
    await vi.waitFor(() => expect(events).toEqual(['secondary/orders/rejected', 'accountAuth', 'done']));
    expect(onAccountAuth).toHaveBeenCalledExactlyOnceWith({ credential: 'secondary', scope: 'orders', outcome: 'rejected' });
    expect(session.ready).toBe(true);
    expect(socket.sent).toEqual([]);
    session.close();
    socket.message('{"type":"accountAuth","credential":"secondary","scope":"orders","outcome":"accepted"}');
    await microtasks();
    expect(onAccountAuth).toHaveBeenCalledOnce();
  });

  it('keeps every signer/scope verdict on its owning connection and supports all enum values', async () => {
    const onAccountAuth = vi.fn();
    const siblingAuth = vi.fn();
    const own = await connected({ sink: { onFrame() {}, onNotice() {}, onClose() {}, onAccountAuth } });
    const sibling = await connected({ sink: { onFrame() {}, onNotice() {}, onClose() {}, onAccountAuth: siblingAuth } });
    const expected = [];
    for (const credential of ['primary', 'secondary']) {
      for (const scope of ['account', 'balance', 'orders', 'positions', 'settings']) {
        for (const outcome of ['accepted', 'rejected']) {
          const result = { credential, scope, outcome };
          expected.push(result);
          own.socket.message(JSON.stringify({ type: 'accountAuth', ...result }));
        }
      }
    }
    await vi.waitFor(() => expect(onAccountAuth).toHaveBeenCalledTimes(20));
    expect(onAccountAuth.mock.calls.map(([result]) => result)).toEqual(expected);
    expect(siblingAuth).not.toHaveBeenCalled();
    own.session.close();
    sibling.session.close();
  });

  it('contains a throwing typed account consumer using the existing terminal session boundary', async () => {
    const onClose = vi.fn();
    const onNotice = vi.fn();
    const { session, socket } = await connected({ sink: {
      onFrame() {}, onNotice, onClose,
      onAccountAuth() { throw new Error('private consumer detail'); },
    } });
    socket.message('{"type":"accountAuth","credential":"primary","scope":"settings","outcome":"accepted"}');
    await microtasks();
    expect(session.closed).toBe(true);
    expect(onNotice).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledOnce();
    expect(onClose.mock.calls[0][0].message).toBe('Native frame session closed');
  });
  it('authenticates with subprotocols, accepts preceding notices, and preserves raw frames and command IDs', async () => {
    const { socket, sink, socketFactory, promise } = setup();
    expect(socketFactory).toHaveBeenCalledWith(URL, [PROTOCOL, ACCOUNT_AUTH_PROTOCOL, `bearer.${TOKEN}`]);
    expect(socket.binaryType).toBe('arraybuffer');
    socket.open();
    const log = '{"type":"log","level":"info","message":"ready"}';
    socket.message(log);
    const snapshot = frame();
    socket.message(snapshot);
    const session = await promise;
    expect(sink.onNotice).toHaveBeenCalledExactlyOnceWith(log);
    expect(sink.onFrame).toHaveBeenCalledExactlyOnceWith(snapshot, { snapshot: true });
    expect(session.ready).toBe(true);
    const delta = frame(true);
    socket.message(delta);
    await microtasks();
    expect(sink.onFrame).toHaveBeenNthCalledWith(2, delta, { snapshot: false });
    const command = ' {"op":"fetchCandles", "command":4294967295,"symbol":"BTCUSDT","interval":"1m"} ';
    session.sendCommand(command);
    expect(socket.sent).toEqual([command]);
    const done = '{"type":"done","command":4294967295,"result":"[]"}';
    socket.message(done);
    await microtasks();
    expect(sink.onNotice).toHaveBeenLastCalledWith(done);
    const startupFailed = '{"type":"startupFailed","detail":{"message":"Bybit rejected the API key (10003): API key is invalid.","kind":"authInvalid"}}';
    socket.message(startupFailed);
    await microtasks();
    expect(sink.onNotice).toHaveBeenLastCalledWith(startupFailed);
    const failed = '{"type":"done","command":5,"error":"leg 2 rejected","detail":{"message":"leg 2 rejected","partial":{"orderIds":["a"],"total":2,"failedIndex":1}}}';
    socket.message(failed);
    await microtasks();
    expect(sink.onNotice).toHaveBeenLastCalledWith(failed);
    expect(session.ready).toBe(true);
    session.close();
  });

  it('uses the standard global WebSocket when no factory is supplied', async () => {
    const socket = new TestSocket();
    const constructor = vi.fn(function () { return socket; });
    vi.stubGlobal('WebSocket', constructor);
    try {
      const pending = NativeFrameSession.connect({ url: URL, token: TOKEN, sink: { onFrame() {}, onNotice() {}, onClose() {} } });
      socket.open();
      socket.message(frame());
      const session = await pending;
      expect(constructor).toHaveBeenCalledWith(URL, [PROTOCOL, ACCOUNT_AUTH_PROTOCOL, `bearer.${TOKEN}`]);
      session.close();
    } finally { vi.unstubAllGlobals(); }
  });

  it('copies the exact view range without exposing unrelated bytes', async () => {
    const { socket, sink, promise } = setup();
    socket.open();
    const padded = new Uint8Array(32);
    padded.set(new Uint8Array(frame()), 8);
    socket.message(new DataView(padded.buffer, 8, 16));
    padded.fill(255);
    const session = await promise;
    const received = sink.onFrame.mock.calls[0][0] as ArrayBuffer;
    expect(new Uint8Array(received)).toEqual(new Uint8Array(frame()));
    expect(received.byteLength).toBe(16);
    session.close();
  });

  it('serializes asynchronous notices, snapshot and deltas and gates ready on snapshot sink completion', async () => {
    const notice = deferred();
    const snapshot = deferred();
    const calls: string[] = [];
    const sink = {
      onNotice: vi.fn(async () => { calls.push('notice'); await notice.promise; }),
      onFrame: vi.fn(async (_buffer: ArrayBuffer, metadata: { snapshot: boolean }) => { calls.push(metadata.snapshot ? 'snapshot' : 'delta'); if (metadata.snapshot) await snapshot.promise; }),
      onClose: vi.fn(),
    };
    const { socket, promise } = setup({ sink });
    let resolved = false;
    void promise.then(() => { resolved = true; });
    socket.open();
    socket.message('{"type":"deactivate"}');
    socket.message(frame());
    socket.message(frame(true));
    await microtasks();
    expect(calls).toEqual(['notice']);
    expect(resolved).toBe(false);
    notice.resolve();
    await microtasks();
    expect(calls).toEqual(['notice', 'snapshot']);
    expect(resolved).toBe(false);
    snapshot.resolve();
    const session = await promise;
    await microtasks();
    expect(calls).toEqual(['notice', 'snapshot', 'delta']);
    session.close();
  });

  it('bounds message counts including an active sink, and never delivers a queued frame after overflow', async () => {
    const wait = deferred();
    const sink = { onFrame: vi.fn(() => wait.promise), onNotice: vi.fn(), onClose: vi.fn() };
    const { socket, promise } = setup({ sink, maxQueuedMessages: 2 });
    socket.open();
    socket.message(frame());
    socket.message(frame());
    socket.message(frame());
    await expect(promise).rejects.toThrow('Native frame session closed');
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    expect(socket.close).toHaveBeenCalledTimes(1);
    wait.resolve();
    await microtasks();
    expect(sink.onFrame).toHaveBeenCalledTimes(1);
  });

  it('bounds queued bytes including the active sink and notices', async () => {
    const wait = deferred();
    const sink = { onFrame: vi.fn(() => wait.promise), onNotice: vi.fn(), onClose: vi.fn() };
    const { socket, promise } = setup({ sink, maxQueuedBytes: 32 });
    socket.open();
    socket.message(frame());
    socket.message('{"type":"deactivate"}');
    await expect(promise).rejects.toThrow();
    expect(sink.onNotice).not.toHaveBeenCalled();
    wait.resolve();
  });

  it('accepts a queue exactly at its byte and count limits', async () => {
    const wait = deferred();
    const sink = { onFrame: vi.fn(() => wait.promise), onNotice: vi.fn(), onClose: vi.fn() };
    const { socket, promise } = setup({ sink, maxQueuedBytes: 32, maxQueuedMessages: 2 });
    socket.open();
    socket.message(frame());
    socket.message(frame());
    expect(socket.close).not.toHaveBeenCalled();
    wait.resolve();
    const session = await promise;
    await microtasks();
    expect(sink.onFrame).toHaveBeenCalledTimes(2);
    session.close();
  });

  it.each(['throw', 'reject'] as const)('fails closed on a sink %s without exposing its error', async (mode) => {
    const secret = new Error(`${URL}?token=${TOKEN}`);
    const sink = {
      onFrame: vi.fn(() => { if (mode === 'throw') throw secret; return Promise.reject(secret); }),
      onNotice: vi.fn(), onClose: vi.fn(),
    };
    const { socket, promise } = setup({ sink });
    socket.open();
    socket.message(frame());
    await expect(promise).rejects.toThrow('Native frame session closed');
    expect(sink.onClose.mock.calls[0][0].message).not.toContain(TOKEN);
    expect(sink.onClose.mock.calls[0][0].message).not.toContain(URL);
  });

  it('also fails closed on notice sink rejection', async () => {
    const { session, socket, sink } = await connected();
    sink.onNotice.mockRejectedValueOnce(new Error(TOKEN));
    socket.message('{"type":"deactivate"}');
    await microtasks();
    expect(session.closed).toBe(true);
    expect(sink.onClose).toHaveBeenCalledTimes(1);
  });

  it('does not become ready after a close during a pending snapshot sink', async () => {
    const wait = deferred();
    const sink = { onFrame: vi.fn(() => wait.promise), onNotice: vi.fn(), onClose: vi.fn() };
    const { socket, promise } = setup({ sink });
    socket.open();
    socket.message(frame());
    socket.dispatchEvent(new Event('close'));
    await expect(promise).rejects.toThrow();
    wait.resolve();
    await microtasks();
    expect(sink.onClose).toHaveBeenCalledTimes(1);
  });

  it('does not return an already closed session when the next queued sink fails', async () => {
    const wait = deferred();
    const sink = {
      onFrame: vi.fn(() => wait.promise),
      onNotice: vi.fn(() => { throw new Error('sink failure'); }),
      onClose: vi.fn(),
    };
    const { socket, promise } = setup({ sink });
    socket.open();
    socket.message(frame());
    socket.message('{"type":"deactivate"}');
    wait.resolve();
    await expect(promise).rejects.toThrow();
  });

  it('supports reentrant delivery and close without callbacks after teardown', async () => {
    const { session, socket, sink } = await connected();
    sink.onFrame.mockImplementationOnce(() => {
      socket.message(frame());
      session.close();
    });
    socket.message(frame());
    await microtasks();
    socket.message(frame());
    socket.dispatchEvent(new Event('error'));
    session.close();
    expect(sink.onFrame).toHaveBeenCalledTimes(2);
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    expect(socket.close).toHaveBeenCalledTimes(1);
    expect(session.ready).toBe(false);
    expect(() => session.sendCommand('{"op":"subscribeOrderBook","symbol":"BTCUSDT"}')).toThrow();
  });

  it('removes all listeners and absorbs throwing and rejecting onClose callbacks', async () => {
    for (const callback of [() => { throw new Error(TOKEN); }, () => Promise.reject(new Error(TOKEN))]) {
      const sink = { onFrame: vi.fn(), onNotice: vi.fn(), onClose: callback };
      const { session, socket } = await connected({ sink });
      const remove = vi.spyOn(socket, 'removeEventListener');
      expect(() => session.close()).not.toThrow();
      expect(remove.mock.calls.map(([type]) => type).sort()).toEqual(['close', 'error', 'message', 'open']);
      await microtasks();
    }
  });

  it.each(['open', 'sink'] as const)('times out waiting for %s and never retries', async (phase) => {
    vi.useFakeTimers();
    const wait = deferred();
    const sink = { onFrame: vi.fn(() => wait.promise), onNotice: vi.fn(), onClose: vi.fn() };
    const { socket, socketFactory, promise } = setup({ sink, connectTimeoutMs: 25 });
    if (phase === 'sink') { socket.open(); socket.message(frame()); }
    await vi.advanceTimersByTimeAsync(25);
    await expect(promise).rejects.toThrow();
    expect(socketFactory).toHaveBeenCalledTimes(1);
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    wait.resolve();
    await vi.advanceTimersByTimeAsync(100);
    expect(socketFactory).toHaveBeenCalledTimes(1);
  });

  it('clears the connection timeout only after snapshot delivery', async () => {
    vi.useFakeTimers();
    const { session, socket, sink } = await connected({ connectTimeoutMs: 10 });
    await vi.advanceTimersByTimeAsync(100);
    expect(session.ready).toBe(true);
    expect(socket.close).not.toHaveBeenCalled();
    expect(sink.onClose).not.toHaveBeenCalled();
    session.close();
  });

  it.each(['error', 'close'] as const)('terminates on socket %s before snapshot', async (event) => {
    const { socket, promise, sink } = setup();
    socket.dispatchEvent(new Event(event));
    await expect(promise).rejects.toThrow();
    expect(sink.onClose).toHaveBeenCalledTimes(1);
  });

  it('rejects a non-negotiated protocol and data before open', async () => {
    for (const beforeOpen of [false, true]) {
      const { socket, promise } = setup();
      if (beforeOpen) socket.message(frame());
      else { socket.protocol = ''; socket.open(); }
      await expect(promise).rejects.toThrow();
    }
  });

  it.each([
    new ArrayBuffer(0),
    new Uint8Array(16).buffer,
    (() => { const buffer = frame(); new DataView(buffer).setUint16(4, 2, true); return buffer; })(),
    (() => { const buffer = frame(); new DataView(buffer).setUint16(6, 1, true); return buffer; })(),
    (() => { const buffer = new Uint8Array(17); buffer.set(new Uint8Array(frame())); return buffer.buffer; })(),
    frame(true),
    new Blob([frame()]),
    42,
  ])('rejects a malformed snapshot or unsupported binary payload %#', async (payload) => {
    const { socket, sink, promise } = setup();
    socket.open();
    socket.message(payload);
    await expect(promise).rejects.toThrow();
    expect(sink.onFrame).not.toHaveBeenCalled();
  });

  it('rejects oversized frames and views before sink delivery', async () => {
    for (const payload of [frame(), new Uint8Array(frame())]) {
      const { socket, sink, promise } = setup({ maxFrameBytes: 15 });
      socket.open();
      socket.message(payload);
      await expect(promise).rejects.toThrow();
      expect(sink.onFrame).not.toHaveBeenCalled();
    }
  });

  it.each([
    'not-json', 'null', '[]', '{}', '{"type":"other"}',
    '{"type":"accountAuth","credential":"primary","scope":"orders"}',
    '{"type":"accountAuth","credential":"unknown","scope":"orders","outcome":"rejected"}',
    '{"type":"accountAuth","credential":["primary"],"scope":"orders","outcome":"rejected"}',
    '{"type":"accountAuth","credential":"primary","scope":"public","outcome":"accepted"}',
    '{"type":"accountAuth","credential":"primary","scope":["orders"],"outcome":"accepted"}',
    '{"type":"accountAuth","credential":"primary","scope":"orders","outcome":true}',
    '{"type":"accountAuth","credential":"primary","scope":"orders","outcome":"success"}',
    '{"type":"accountAuth","credential":"primary","scope":"orders","outcome":"rejected","command":1}',
    '{"type":"accountAuth","credential":"primary","scope":"orders","outcome":"rejected","account":"other"}',
    '{"type":"accountAuth","credential":"primary","scope":"orders","outcome":"rejected","key":"secret"}',
    '{"type":"done","command":1}',
    '{"type":"done","command":1,"result":"[]","error":"bad"}',
    '{"type":"done","command":-1,"result":"[]"}',
    '{"type":"done","command":4294967296,"result":"[]"}',
    '{"type":"done","command":"1","result":"[]"}',
    '{"type":"done","command":1,"result":[]}',
    '{"type":"done","command":1,"result":"[]","detail":{"message":"x"}}',
    '{"type":"done","command":1,"error":"x","detail":"x"}',
    '{"type":"startupFailed"}',
    '{"type":"startupFailed","detail":{}}',
    '{"type":"error","error":4}',
    '{"type":"error","command":null,"error":"bad"}',
    '{"type":"log","level":"warn","message":"bad"}',
    '{"type":"log","level":["info"],"message":"bad"}',
    '{"type":"log","level":"info","message":3}',
    '{"type":"event","level":"warning","code":"ws.closed","message":"bad"}',
    '{"type":"event","level":"warn","message":"no code"}',
    '{"type":"event","level":"warn","code":"ws.closed","message":"bad","fields":{"nested":{}}}',
    '{"type":"event","level":"warn","code":"ws.closed","message":"bad","extra":1}',
    '{"type":"deactivate","unexpected":"value"}',
    '{"type":"commandError","command":-1,"message":"failed"}',
    '{"type":"commandError","command":"1","message":"failed"}',
    '{"type":"commandError","command":1,"message":[]}',
    '{"type":"commandError","command":1,"message":"failed","extra":"value"}',
    '{"type":"commandProgress","command":4294967296,"receipts":[]}',
    '{"type":"commandProgress","command":1,"receipts":{}}',
    '{"type":"commandProgress","command":1,"receipts":[{"symbol":"ETH","kind":"clientOrderIndex","identity":9007199254740993}]}',
    '{"type":"commandProgress","command":1,"receipts":[{"symbol":"ETH","kind":"nonce","identity":"1"}]}',
    '{"type":"commandProgress","command":1,"receipts":[{"symbol":"ETH","kind":["clientOrderId"],"identity":"1"}]}',
    '{"type":"commandProgress","command":1,"receipts":[{"symbol":"ETH","kind":"clientOrderId","identity":"1","unexpected":"value"}]}',
  ])('rejects malformed native notices %#', async (notice) => {
    const { socket, promise, sink } = setup();
    socket.open();
    socket.message(notice);
    await expect(promise).rejects.toThrow();
    expect(sink.onNotice).not.toHaveBeenCalled();
  });

  it.each([
    '{"type":"accountAuth","credential":"primary","scope":"orders","outcome":"rejected"}',
    '{"type":"accountAuth","credential":"secondary","scope":"balance","outcome":"accepted"}',
    '{"type":"done","command":0,"error":"unsupported"}',
    '{"type":"error","error":"invalid command"}',
    '{"type":"error","command":3,"error":"invalid command"}',
    '{"type":"log","level":"warning","message":"retry"}',
    '{"type":"log","level":"error","message":"disconnected"}',
    '{"type":"event","level":"debug","code":"ws.closed","message":"WebSocket closed","fields":{"socket":2,"code":1006}}',
    '{"type":"event","level":"error","code":"auth.rejected","message":"rejected","fields":{"key":"[redacted]","retry":false,"at":null}}',
    '{"type":"event","level":"warn","code":"ws.reconnect","message":"reconnecting"}',
    '{"type":"deactivate"}',
    '{"type":"commandError","command":0,"message":"placement failed"}',
    '{"type":"commandError","command":4294967295,"message":"same error"}',
    '{"type":"commandProgress","command":4294967295,"receipts":[{"symbol":"ETH","kind":"clientOrderIndex","identity":"900719925474099312345"}]}',
    '{"type":"commandProgress","command":1,"receipts":[{"symbol":"BTCUSDT","kind":"clientOrderId","identity":"client-parent"},{"symbol":"ETHUSD","kind":"orderCode","identity":"code-001"}]}',
  ])('forwards valid native notices without treating venue errors as stream gaps %#', async (notice) => {
    const { session, socket, sink } = await connected();
    socket.message(notice);
    await microtasks();
    expect(sink.onNotice).toHaveBeenCalledWith(notice);
    expect(session.ready).toBe(true);
    session.close();
  });

  it.each(['bad', 'null', '[]', '{}', '{"op":2}', '{"op":""}'])('rejects locally malformed commands without sending or closing %#', async (command) => {
    const { session, socket } = await connected();
    expect(() => session.sendCommand(command)).toThrow('Invalid native frame command');
    expect(socket.send).not.toHaveBeenCalled();
    expect(session.ready).toBe(true);
    session.close();
  });

  it('leaves command schema/venue validation with the native typed decoder', async () => {
    const { session, socket } = await connected();
    const raw = '{"op":"fetchUnknown","command":"not-a-u32"}';
    session.sendCommand(raw);
    expect(socket.sent).toEqual([raw]);
    session.close();
  });

  it('uses UTF-8 command byte limits without closing on a local validation error', async () => {
    const command = '{"op":"é"}';
    const { session, socket } = await connected({ maxCommandBytes: command.length });
    expect(() => session.sendCommand(command)).toThrow('Invalid native frame command');
    expect(socket.send).not.toHaveBeenCalled();
    expect(session.ready).toBe(true);
    session.close();
  });

  it('closes on buffered output overflow without sending or retrying a write', async () => {
    const { session, socket } = await connected({ maxBufferedAmount: 100 });
    socket.bufferedAmount = 95;
    expect(() => session.sendCommand('{"op":"anything"}')).toThrow('Native frame session closed');
    expect(socket.send).not.toHaveBeenCalled();
    expect(session.closed).toBe(true);
  });

  it('permits output exactly at the buffer limit and sanitizes send exceptions', async () => {
    const command = '{"op":"anything"}';
    const { session, socket, sink } = await connected({ maxBufferedAmount: command.length });
    session.sendCommand(command);
    expect(socket.send).toHaveBeenCalledTimes(1);
    socket.send.mockImplementationOnce(() => { throw new Error(`${URL} ${TOKEN}`); });
    expect(() => session.sendCommand(command)).toThrow('Native frame session closed');
    expect(sink.onClose.mock.calls[0][0].message).not.toContain(TOKEN);
    expect(socket.send).toHaveBeenCalledTimes(2);
  });

  it.each([
    'https://native.invalid/accounts/id',
    'wss://name:password@native.invalid/accounts/id',
    'wss://native.invalid/accounts/id?token=secret',
    'wss://native.invalid/accounts/id?',
    'wss://native.invalid/accounts/id#secret',
    'wss://native.invalid/accounts/id#',
    'wss://native.invalid/accounts/',
    'wss://native.invalid/accounts/id/child',
    'wss://native.invalid/accounts/a%2Fb',
    `wss://native.invalid/accounts/${'a'.repeat(129)}`,
    'not-a-url',
  ])('rejects invalid configured endpoints without invoking the socket factory %#', async (url) => {
    const { promise, socketFactory } = setup({ url });
    await expect(promise).rejects.toThrow('Invalid native frame session configuration');
    expect(socketFactory).not.toHaveBeenCalled();
  });

  it.each(['', 'a b', 'a,b', 'a/b', 'ümlaut', 'a\nb'])('rejects invalid account tokens without exposing them %#', async (token) => {
    const { promise, socketFactory } = setup({ token });
    await expect(promise).rejects.toThrow('Invalid native frame session configuration');
    expect(socketFactory).not.toHaveBeenCalled();
  });

  it.each(['connectTimeoutMs', 'maxQueuedMessages', 'maxQueuedBytes', 'maxFrameBytes', 'maxCommandBytes', 'maxBufferedAmount'] as const)('validates %s bounds', async (name) => {
    for (const value of [0, -1, 0.1, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      const { promise, socketFactory } = setup({ [name]: value });
      await expect(promise).rejects.toThrow('Invalid native frame session configuration');
      expect(socketFactory).not.toHaveBeenCalled();
    }
  });

  it('rejects a timeout beyond the runtime timer range', async () => {
    const { promise, socketFactory } = setup({ connectTimeoutMs: 0x80000000 });
    await expect(promise).rejects.toThrow('Invalid native frame session configuration');
    expect(socketFactory).not.toHaveBeenCalled();
  });

  it('sanitizes factory errors and calls onClose once', async () => {
    const socketFactory = vi.fn(() => { throw new Error(`${URL} ${TOKEN}`); });
    const { promise, sink } = setup({ socketFactory });
    await expect(promise).rejects.toThrow('Native frame session closed');
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    expect(sink.onClose.mock.calls[0][0].message).toBe('Native frame session closed');
    expect(socketFactory).toHaveBeenCalledTimes(1);
  });

  it('rejects a preaborted signal before creating a socket or exposing its reason', async () => {
    const controller = new AbortController();
    controller.abort(new Error(`${URL} ${TOKEN}`));
    const { promise, socketFactory, sink } = setup({ signal: controller.signal });
    await expect(promise).rejects.toThrow('Native frame session closed');
    expect(socketFactory).not.toHaveBeenCalled();
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    expect(sink.onClose.mock.calls[0][0].message).not.toContain(TOKEN);
  });

  it('closes a socket returned by a factory that synchronously aborted the session', async () => {
    const controller = new AbortController();
    const socket = new TestSocket();
    const add = vi.spyOn(socket, 'addEventListener');
    const socketFactory = vi.fn(() => {
      controller.abort(new Error(TOKEN));
      return socket;
    });
    const { promise, sink } = setup({ socketFactory, signal: controller.signal });
    await expect(promise).rejects.toThrow('Native frame session closed');
    expect(socket.close).toHaveBeenCalledTimes(1);
    expect(add).not.toHaveBeenCalled();
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    socket.open();
    socket.message(frame());
    expect(sink.onFrame).not.toHaveBeenCalled();
  });

  it.each(['handshake', 'snapshot', 'connected'] as const)('aborts during %s and removes the abort listener', async (phase) => {
    const controller = new AbortController();
    const remove = vi.spyOn(controller.signal, 'removeEventListener');
    const wait = deferred();
    const sink = { onFrame: vi.fn(() => phase === 'snapshot' ? wait.promise : undefined), onNotice: vi.fn(), onClose: vi.fn() };
    const { socket, promise } = setup({ signal: controller.signal, sink });
    if (phase !== 'handshake') { socket.open(); socket.message(frame()); }
    const session = phase === 'connected' ? await promise : undefined;
    controller.abort(new Error(`${URL} ${TOKEN}`));
    if (phase !== 'connected') await expect(promise).rejects.toThrow('Native frame session closed');
    else { expect(session?.closed).toBe(true); expect(session?.ready).toBe(false); }
    expect(remove).toHaveBeenCalledWith('abort', expect.any(Function));
    expect(sink.onClose).toHaveBeenCalledTimes(1);
    expect(socket.close).toHaveBeenCalledTimes(1);
    wait.resolve();
    await microtasks();
    socket.message(frame());
    expect(sink.onFrame).toHaveBeenCalledTimes(phase === 'handshake' ? 0 : 1);
    expect(sink.onClose).toHaveBeenCalledTimes(1);
  });
});
