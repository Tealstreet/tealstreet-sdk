import assert from 'node:assert/strict';
import { fork } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { makeCA, recordingProxy } from '../../scripts/live-diff/recorder.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const implementation = process.argv[2];
if (implementation) {
  console.log = console.warn = console.error = () => {};
  const options = {
    key: '',
    secret: '',
    publicDataOnly: true,
    hostedMode: true,
    tealstreetProxy: false,
    tealstreetTunnel: false,
    tealstreetWsRoute: false,
    tuning: { consoleLogging: false },
  };
  const updates = [];
  let instance, delivery, native, id;
  if (implementation === 'ts') {
    const { exchangeRegistry } = await import('../../../packages/safe-cex/src/exchanges/registry.ts');
    const { DefaultStore } = await import('../../../packages/safe-cex/src/store/store.base.ts');
    const { ExchangeWorker } = await import('../../../packages/safe-cex/src/webworker/ExchangeWorker.ts');
    const Exchange = await exchangeRegistry.binance();
    instance = new Exchange('binance', options, new DefaultStore());
    delivery = new ExchangeWorker(
      {
        postMessage: ({ op, data }) => {
          if (op === 'store') updates.push(structuredClone(data));
        },
      },
      'recorded-request-stats',
      'binance',
      options,
      { placeOrder: {} },
      { supportedTPSLTriggerTypes: [], privateProxyIps: [] },
      '',
    );
    delivery._exchange = instance;
    delivery._store = instance.store;
    delivery._initializeProcessorsAndHandlers();
    delivery._setupExchangeEventListeners();
  } else {
    native = createRequire(import.meta.url)(process.env.LIVE_DIFF_ADDON || '../dist/node/safe_cex_ffi.node');
    id = await native.nativeAccountOpen(
      { exchange: 'binance', options },
      () => {},
      (data) => updates.push(data),
    );
  }
  const call = (method, args = []) =>
    implementation === 'ts' ? instance[method](...args) : native.nativeAccountCallMethod(id, method, args);
  try {
    const candles = { symbol: 'BTCUSDT', interval: '1m', to: 1792334872923, limit: 1 };
    const initialPosts = updates.length;
    for (let i = 1; i < Number(process.argv[3]); i++) {
      assert.equal((await call('fetchOHLCV', [candles])).length, 1);
    }
    // Counters mutate in place. HTTP alone must not manufacture Store posts.
    await new Promise((resolve) => setTimeout(resolve, 350));
    assert.equal(updates.length, initialPosts);
    const changes = await call('refreshMarkets');
    assert.ok(changes.added.length > 500);
    await new Promise((resolve) => setTimeout(resolve, 350));
    assert.ok(updates.length > initialPosts, 'market commit ships the counters through real worker delivery');
    const stats = updates.at(-1).requestStats;
    process.send({ stats });
  } finally {
    if (implementation === 'ts') {
      delivery.dispose();
      await instance.dispose();
    } else await native.nativeAccountClose(id);
  }
} else {
  for (const count of [5, 6]) {
    test(
      `Binance ${count} recorded HTTP outcomes reach the production Store callback`,
      { timeout: 30000 },
      async (t) => {
        const directory = mkdtempSync(join(tmpdir(), 'safe-cex-request-stats-'));
        t.after(() => rmSync(directory, { recursive: true, force: true }));
        const ca = makeCA(directory);
        const outputs = [];
        for (const side of ['ts', 'rust']) {
          const records = [];
          const fixture = (name) => readFileSync(new URL(`./fixtures/live-public/${name}`, import.meta.url));
          const response = (name) => ({
            status: 200,
            headers: { 'content-type': 'application/json' },
            bytes: fixture(name),
          });
          const proxy = await recordingProxy({
            directory,
            ca,
            records,
            implementation: side,
            replayOnly: true,
            allowHost: (host) => host === 'fapi.binance.com',
            cache: new Map([
              ['GET https://fapi.binance.com/fapi/v1/exchangeInfo\n', response('binance-request-markets.json')],
              [
                'GET https://fapi.binance.com/fapi/v1/klines?endTime=1792334872923&interval=1m&limit=1&startTime=1792334812923&symbol=BTCUSDT\n',
                response('binance-request-candles.json'),
              ],
            ]),
          });
          try {
            outputs.push(
              await new Promise((resolve, reject) => {
                const child = fork(fileURLToPath(import.meta.url), [side, String(count)], {
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
                  },
                });
                let result,
                  stderr = '';
                const timer = setTimeout(() => {
                  child.kill('SIGKILL');
                  reject(new Error(`${side} exceeded the 12s recorded-response budget: ${stderr}`));
                }, 12000);
                child.stderr.on('data', (bytes) => {
                  stderr += bytes;
                });
                child.on('message', (value) => {
                  result = value;
                });
                child.on('error', reject);
                child.on('exit', (code) => {
                  clearTimeout(timer);
                  if (code === 0 && result) resolve(result.stats);
                  else reject(new Error(`${side} exited ${code}: ${stderr}`));
                });
              }),
            );
            assert.equal(records.length, count);
            assert.ok(records.every((row) => row.cacheHit && row.status === 200));
          } finally {
            await proxy.close();
          }
        }
        assert.deepEqual(outputs[0], { ok: count, rateLimit: 0, auth: 0, network: 0, other: 0 });
        assert.deepEqual(outputs[1], outputs[0], 'native factory must retain its owning Store');
      },
    );
  }
}
