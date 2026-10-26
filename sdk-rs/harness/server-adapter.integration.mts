// Real native WebSocket -> server session adapter -> SCXF decoder -> SDK books.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { EventEmitter, once } from 'node:events';
import { createInterface } from 'node:readline';
import test from 'node:test';
import { NativeFrameSession, type NativeFrameSessionSink, type NativeFrameSocketFactory } from '../../packages/safe-cex-server/src/frameSession/NativeFrameSession.ts';
import { ServerSessionFactory } from '../../packages/safe-cex-server/src/frameSession/ServerSessionFactory.ts';
import { FrameApplier, type FrameBook } from '../../packages/safe-cex/src/store/frameApplier.ts';
import { DefaultStore } from '../../packages/safe-cex/src/store/store.base.ts';
import { Frame } from '../hosts/wasm/js/wire.mjs';

type Received = { frame: Frame; snapshot: boolean; books: FrameBook[] };
type Notice = { type: string; command?: number; result?: string; error?: string };

function consumer() {
  const changed = new EventEmitter();
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  const frames: Received[] = [];
  const notices: Notice[] = [];
  const wireFrames: Uint8Array[] = [];
  let closes = 0;
  const sink: NativeFrameSessionSink = {
    onFrame(buffer, { snapshot }) {
      // Capture at the real socket boundary and compare after adapter forwarding.
      assert.deepEqual(new Uint8Array(buffer), wireFrames.shift());
      const frame = new Frame(buffer);
      applier.apply(frame);
      frames.push({ frame, snapshot, books: applier.books(frame) });
      changed.emit('change');
    },
    onNotice(text) { notices.push(JSON.parse(text) as Notice); changed.emit('change'); },
    onClose() { closes++; changed.emit('change'); },
  };
  const socketFactory: NativeFrameSocketFactory = (url, protocols) => {
    const socket = new WebSocket(url, protocols);
    socket.binaryType = 'arraybuffer';
    socket.addEventListener('message', (event) => {
      if (event.data instanceof ArrayBuffer) wireFrames.push(new Uint8Array(event.data).slice());
    });
    return socket;
  };
  async function until<T>(read: () => T | undefined): Promise<T> {
    for (;;) {
      const result = read();
      if (result !== undefined) return result;
      assert.equal(closes, 0, 'session closed before expected delivery');
      await once(changed, 'change', { signal: AbortSignal.timeout(5000) });
    }
  }
  return {
    sink, socketFactory, store, frames, notices,
    get closes() { return closes; },
    frameAfter(index: number, predicate: (frame: Received) => boolean) {
      return until(() => frames.slice(index).find(predicate));
    },
    done(command: number) { return until(() => notices.find((notice) => notice.type === 'done' && notice.command === command)); },
  };
}

function levels(book: FrameBook) {
  return {
    bids: book.orderbook.bids.map(({ price, amount, total }) => [price, amount, total]),
    asks: book.orderbook.asks.map(({ price, amount, total }) => [price, amount, total]),
  };
}
function expected(amount: number) {
  return { bids: [[99, amount, amount], [97, amount + 1, amount * 2 + 1]], asks: [[102, amount + 2, amount + 2]] };
}
function result(notice: Notice) {
  assert.equal(notice.error, undefined);
  assert.equal(typeof notice.result, 'string');
  return JSON.parse(notice.result!);
}

test('server sessions forward native snapshots, book deltas, commands and fresh reconnect baselines', async () => {
  const cargoArgs = ['--quiet', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-native', '--example', 'session-adapter-fixture'];
  const build = spawnSync('cargo', ['build', ...cargoArgs], { encoding: 'utf8', timeout: 120_000 });
  assert.equal(build.status, 0, build.stderr || build.error?.message);
  const child = spawn('cargo', ['run', ...cargoArgs], { stdio: ['pipe', 'pipe', 'pipe'] });
  const exited = once(child, 'exit');
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', (text: string) => { stderr += text; });
  const sessions: NativeFrameSession[] = [];
  const lines = createInterface({ input: child.stdout });
  try {
    const [line] = await once(lines, 'line', { signal: AbortSignal.timeout(10_000) });
    const { url } = JSON.parse(line as string) as { url: string };
    assert.match(url, /^ws:\/\/127\.0\.0\.1:\d+\/accounts\/fixture$/);
    const first = consumer();
    const resolved: unknown[] = [];
    const factory = new ServerSessionFactory({
      publicExchangeFactory: { get() { assert.fail('Rust sessions must not construct a TypeScript exchange'); } },
      async resolveNativeAccount(request) {
        resolved.push({ exchange: request.exchange, accountId: request.accountId });
        assert.equal(request.signal?.aborted, false);
        return { url, token: 'offline-test-token' };
      },
      nativeOptions: { socketFactory: first.socketFactory },
    });
    const session = await factory.open({ engine: 'rust', exchange: 'binance', accountId: 'fixture', sink: first.sink });
    sessions.push(session);
    assert.deepEqual(resolved, [{ exchange: 'binance', accountId: 'fixture' }]);
    assert.equal(session.ready, true);
    assert.equal(first.frames.length, 1);
    const snapshot = first.frames[0];
    assert.equal(snapshot.snapshot, true);
    assert.ok(snapshot.frame.markets());
    assert.equal([...snapshot.frame.bookDeltas()].length, 0);
    assert.deepEqual(levels(snapshot.books[0]), { bids: [[99, 1, 1], [98, 2, 3]], asks: [[101, 1, 1]] });

    let boundary = first.frames.length;
    session.sendCommand('{"op":"subscribeOrderBook","symbol":"BTCUSDT"}');
    await first.frameAfter(boundary, ({ frame }) => [...frame.books()].length === 1);
    boundary = first.frames.length;
    session.sendCommand('{"op":"setLeverage","command":41,"symbol":"BTCUSDT","leverage":2}');
    assert.deepEqual(result(await first.done(41)), { symbol: 'BTCUSDT', writes: 1, timerFirings: 1 });
    const delta = await first.frameAfter(boundary, ({ frame }) => [...frame.bookDeltas()].length === 1);
    assert.equal(delta.snapshot, false);
    assert.equal([...delta.frame.books()].length, 0);
    assert.deepEqual(levels(delta.books[0]), expected(2));
    // Full and delta consumers must retain older delivered book values.
    assert.deepEqual(levels(snapshot.books[0]), { bids: [[99, 1, 1], [98, 2, 3]], asks: [[101, 1, 1]] });

    const peer = consumer();
    const peerSession = await NativeFrameSession.connect({ url, token: 'offline-test-token', sink: peer.sink, socketFactory: peer.socketFactory });
    sessions.push(peerSession);
    assert.equal(peer.frames[0].snapshot, true);
    assert.deepEqual(levels(peer.frames[0].books[0]), expected(2));
    // Identical caller IDs are isolated by the native server, preserved by the adapter.
    session.sendCommand('{"op":"fetchLeverage","command":77,"symbol":"first"}');
    peerSession.sendCommand('{"op":"fetchLeverage","command":77,"symbol":"peer"}');
    assert.deepEqual(result(await first.done(77)), { symbol: 'first', writes: 1, timerFirings: 1 });
    assert.deepEqual(result(await peer.done(77)), { symbol: 'peer', writes: 1, timerFirings: 1 });
    assert.equal(first.notices.filter((notice) => notice.command === 77).length, 1);
    assert.equal(peer.notices.filter((notice) => notice.command === 77).length, 1);

    boundary = first.frames.length;
    session.sendCommand('{"op":"subscribeOrderBook","symbol":"BTCUSDT"}');
    const resync = await first.frameAfter(boundary, ({ frame }) => [...frame.books()].length === 1);
    assert.deepEqual(levels(resync.books[0]), expected(2));
    boundary = first.frames.length;
    session.sendCommand('{"op":"setLeverage","command":42,"symbol":"BTCUSDT","leverage":3}');
    assert.deepEqual(result(await first.done(42)), { symbol: 'BTCUSDT', writes: 2, timerFirings: 2 });
    const secondDelta = await first.frameAfter(boundary, ({ frame }) => [...frame.bookDeltas()].length === 1);
    assert.deepEqual(levels(secondDelta.books[0]), expected(3));
    assert.deepEqual(levels(delta.books[0]), expected(2));
    assert.equal(peer.notices.some((notice) => notice.command === 42), false);
    session.close();
    assert.equal(session.closed, true);
    assert.equal(first.closes, 1);
    assert.throws(() => session.sendCommand('{"op":"fetchLeverage","command":99,"symbol":"closed"}'));

    const reconnect = consumer();
    const fresh = await NativeFrameSession.connect({ url, token: 'offline-test-token', sink: reconnect.sink, socketFactory: reconnect.socketFactory });
    sessions.push(fresh);
    assert.equal(reconnect.frames.length, 1);
    assert.equal(reconnect.frames[0].snapshot, true);
    assert.ok(reconnect.frames[0].frame.markets());
    assert.equal([...reconnect.frames[0].frame.bookDeltas()].length, 0);
    assert.deepEqual(levels(reconnect.frames[0].books[0]), expected(3));
    fresh.sendCommand('{"op":"fetchLeverage","command":41,"symbol":"reconnected"}');
    assert.deepEqual(result(await reconnect.done(41)), { symbol: 'reconnected', writes: 2, timerFirings: 2 });
    assert.equal(first.notices.filter((notice) => notice.command === 41).length, 1);
  } finally {
    for (const session of sessions) session.close();
    lines.close();
    child.stdin.end();
    try {
      const [code, signal] = await Promise.race([exited, new Promise<never>((_, reject) => {
        const timer = setTimeout(() => reject(new Error('native fixture did not stop')), 5000);
        timer.unref();
      })]);
      assert.equal(signal, null);
      assert.equal(code, 0, stderr);
    } finally {
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    }
  }
});
