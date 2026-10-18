import assert from 'node:assert/strict';
import { fork } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { hash, makeCA, recordingProxy } from '../../scripts/live-diff/recorder.mjs';
import { streamRelay } from '../../scripts/live-diff/stream-relay.mjs';
import { buildConnectHook, connectHookEnvironment } from '../../scripts/live-diff/ws-recorder.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const fixture = (name) => readFileSync(new URL(`./fixtures/live-public/${name}`, import.meta.url));
const recording = JSON.parse(fixture('binance-nested-socket.json'));

test('Binance nested market socket keeps its production parent grouping', { timeout: 60000 }, async (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'safe-cex-binance-nested-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const ca = makeCA(directory);
  const hook = buildConnectHook(
    directory,
    fileURLToPath(new URL('../../scripts/live-diff/ws-connect.c', import.meta.url)),
  );
  const outputs = [];
  for (const implementation of ['ts', 'rust']) {
    const requests = [],
      sockets = [];
    const relay = await streamRelay({
      directory,
      ca,
      hosts: ['fstream.binance.com'],
      mode: 'replay',
      tape: recording.tape,
      records: sockets,
    });
    const cache = new Map(
      recording.rest.map(({ key, file, sha256 }) => {
        const bytes = fixture(file);
        assert.equal(hash(bytes), sha256, 'recorded HTTP bytes are unchanged');
        return [key, { status: 200, headers: { 'content-type': 'application/json' }, bytes }];
      }),
    );
    const proxy = await recordingProxy({
      directory,
      ca,
      cache,
      records: requests,
      implementation,
      allowHost: (host) => host === 'fapi.binance.com',
      replayOnly: true,
    });
    const child = fork(
      fileURLToPath(new URL('../../scripts/live-diff/stream-worker.mjs', import.meta.url)),
      [implementation],
      {
        cwd: root,
        execArgv: ['--import', 'tsx'],
        serialization: 'advanced',
        stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
        env: {
          ...process.env,
          HTTPS_PROXY: proxy.url,
          HTTP_PROXY: proxy.url,
          ALL_PROXY: proxy.url,
          https_proxy: proxy.url,
          http_proxy: proxy.url,
          all_proxy: proxy.url,
          NO_PROXY: '127.0.0.1,localhost',
          no_proxy: '127.0.0.1,localhost',
          SSL_CERT_FILE: ca.cert,
          NODE_EXTRA_CA_CERTS: ca.cert,
          TSX_TSCONFIG_PATH: join(root, 'packages/safe-cex/tsconfig.json'),
          LIVE_DIFF_ADDON:
            process.env.LIVE_DIFF_ADDON || fileURLToPath(new URL('../dist/node/safe_cex_ffi.node', import.meta.url)),
          ...connectHookEnvironment(hook, relay.ports),
        },
      },
    );
    let sequence = 0,
      stderr = '';
    const pending = new Map();
    child.stderr.on('data', (bytes) => {
      stderr += bytes;
    });
    child.on('message', (message) => {
      const waiter = pending.get(message.requestId);
      if (!waiter) return;
      clearTimeout(waiter.timer);
      pending.delete(message.requestId);
      if (message.failure) waiter.reject(new Error(JSON.stringify(message.failure)));
      else waiter.resolve(message.value);
    });
    const call = (method, args = [], extra = {}) =>
      new Promise((resolve, reject) => {
        const requestId = ++sequence;
        const timer = setTimeout(() => {
          pending.delete(requestId);
          reject(new Error(`${implementation} ${method} exceeded 15s: ${stderr}`));
        }, 15000);
        pending.set(requestId, { resolve, reject, timer });
        child.send({ requestId, method, args, ...extra });
      });
    try {
      await call('open', [], {
        exchange: 'binance',
        replayEpoch: recording.epoch,
        options: {
          key: '',
          secret: '',
          publicDataOnly: true,
          hostedMode: true,
          tealstreetProxy: false,
          tealstreetTunnel: false,
          tealstreetWsRoute: false,
          tuning: { consoleLogging: false, activeTickerSymbols: ['BTCUSDT'] },
        },
      });
      await call('start');
      await call('subscribe', ['BTCUSDT', '1m', recording.epoch]);
      assert.ok(await relay.ready(), 'both public and nested market sockets open');
      await call('barrier');
      await call('begin');
      const receipt = await relay.playControlled({
        advance: (at) => call('advance', [at]),
        receipt: () => call('receipt'),
        timeoutMs: 15000,
      });
      assert.equal(receipt.delivered, 4);
      assert.equal(receipt.inputHash, receipt.expectedInputHash);
      assert.equal(receipt.applicationDrainVerified, true, 'all four venue handlers finish');
      if (implementation === 'rust') assert.equal(receipt.nativeTransportVerified, true);
      assert.ok(receipt.processed.sockets.every((socket) => socket.errors === 0));
      await call('barrier');
      const result = await call('drain');
      assert.ok(
        result.outputs.some(({ kind }) => kind === 'candles'),
        'nested kline handler remains live',
      );
      assert.equal(sockets.length, 2);
      assert.ok(sockets.every((socket) => socket.status === 101 && socket.dropped === 0 && !socket.failure));
      assert.equal(requests.length, 6);
      assert.ok(requests.every((request) => request.cacheHit && request.status === 200));
      outputs.push({ ...result.state, candles: result.outputs.filter(({ kind }) => kind === 'candles') });
      await call('close');
    } finally {
      child.kill('SIGKILL');
      for (const waiter of pending.values()) clearTimeout(waiter.timer);
      await relay.close();
      await proxy.close();
    }
  }
  assert.deepEqual(outputs[1].candles, outputs[0].candles, 'nested candle values retain TS parity');
  await t.test('only the source top-level public handler registry is delivered', () => {
    assert.ok(Object.keys(outputs[0].messageHandlerStats.public).length > 0);
    for (const topic of ['markPriceUpdate', 'kline', '24hrTicker']) {
      assert.equal(Object.hasOwn(outputs[0].messageHandlerStats.public, topic), false);
    }
    assert.deepEqual(outputs[1].messageHandlerStats, outputs[0].messageHandlerStats);
  });
  await t.test('public health timestamp excludes the nested market socket', () => {
    assert.ok(outputs[0].socketHealth.publicLastMessage > recording.epoch);
    assert.equal(outputs[1].socketHealth.publicLastMessage, outputs[0].socketHealth.publicLastMessage);
  });
});
