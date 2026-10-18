import assert from 'node:assert/strict';
import { fork } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { gunzipSync } from 'node:zlib';
import { makeCA, recordingProxy } from '../../scripts/live-diff/recorder.mjs';
import { buildConnectHook, connectHookEnvironment, websocketRecorder } from '../../scripts/live-diff/ws-recorder.mjs';

const fixture = (name) => readFileSync(new URL(`./fixtures/live-public/${name}`, import.meta.url));
const root = fileURLToPath(new URL('../../../', import.meta.url));

test('Bitrue recorded gzip binary frames hydrate native tickers through the production addon', async (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'safe-cex-bitrue-gzip-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const ca = makeCA(directory);
  const hook = buildConnectHook(
    directory,
    fileURLToPath(new URL('../../scripts/live-diff/ws-connect.c', import.meta.url)),
  );
  const frames = JSON.parse(fixture('bitrue-gzip-prefix.ws.json'));
  assert.equal(frames.length, 1);
  for (const frame of frames) {
    assert.equal(frame.binary, true);
    const bytes = Buffer.from(frame.base64, 'base64');
    assert.deepEqual([...bytes.subarray(0, 2)], [0x1f, 0x8b]);
    assert.ok(JSON.parse(gunzipSync(bytes)).channel.endsWith('_ticker'));
  }
  const outputs = [];
  for (const implementation of ['ts', 'rust']) {
    const records = [];
    const cache = new Map([
      [
        'GET https://fapi.bitrue.com/fapi/v1/contracts\n',
        {
          status: 200,
          headers: { 'content-type': 'application/json' },
          bytes: fixture('bitrue-contracts.json'),
        },
      ],
    ]);
    const proxy = await recordingProxy({
      directory,
      ca,
      cache,
      records,
      implementation,
      allowHost: (host) => host === 'fapi.bitrue.com',
      replayOnly: true,
    });
    const socket = await websocketRecorder({
      directory,
      ca,
      records,
      implementation: 'rust',
      hosts: ['fmarket-ws.bitrue.com'],
      streams: new Map([
        [
          'wss://fmarket-ws.bitrue.com/kline-api/ws',
          frames.map(({ base64, ...frame }) => ({
            ...frame,
            bytes: Buffer.from(base64, 'base64'),
          })),
        ],
      ]),
    });
    const child = fork(
      fileURLToPath(new URL('../../scripts/live-diff/worker.mjs', import.meta.url)),
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
          ...connectHookEnvironment(hook, socket.ports),
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
      if (waiter) {
        clearTimeout(waiter.timer);
        pending.delete(message.requestId);
        if (message.failure) waiter.reject(new Error(JSON.stringify(message.failure)));
        else waiter.resolve(message.value);
      }
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
        exchange: 'bitrue',
        options: {
          key: '',
          secret: '',
          publicDataOnly: true,
          hostedMode: true,
          tealstreetProxy: false,
          tealstreetTunnel: false,
          tealstreetWsRoute: false,
          tuning: { consoleLogging: false },
        },
      });
      await call('$startPublicWS');
      await call('$waitLiveTickers', [1, 2000]);
      await call('$settle', [500]);
      const markets = await call('$markets'),
        tickers = await call('$tickers');
      assert.equal(markets.length, 779);
      assert.equal(tickers.length, 779);
      const receipts = records.filter((row) => row.protocol === 'websocket');
      assert.equal(receipts.length, 1);
      assert.equal(receipts[0].status, 101);
      assert.equal(receipts[0].inboundMessages, 1, `${implementation} receives the exact recorded frame`);
      assert.ok(!records.some((row) => row.replayMiss || row.blocked || row.failure));
      assert.equal(records.filter((row) => row.cacheHit && row.status === 200).length, 1);
      assert.equal(
        tickers.filter((row) => row.last > 0).length,
        1,
        `${implementation} must decode the recorded gzip ticker: ${stderr}`,
      );
      outputs.push({ markets, tickers });
    } finally {
      child.kill('SIGKILL');
      for (const waiter of pending.values()) clearTimeout(waiter.timer);
      await socket.close();
      await proxy.close();
    }
  }
  assert.equal(outputs[0].tickers.find((row) => row.symbol === 'E-BTC-USDT').last, 81378.5);
  assert.deepEqual(outputs[1], outputs[0]);
});
