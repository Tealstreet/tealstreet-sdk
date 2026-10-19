import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { fork } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const implementation = process.argv[2];
if (implementation) {
  const seen = [];
  const server = createServer((request, response) => {
    const path = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://fixture').pathname;
    seen.push(path);
    const name = { '/v1/public/info': 'woo-markets', '/v1/public/futures': 'woo-futures' }[path];
    assert.ok(name, `unexpected request ${path}`);
    response.setHeader('content-type', 'application/json');
    const bytes = readFileSync(new URL(`./fixtures/live-public/${name}.json`, import.meta.url));
    // Admit the one affected market; keep all 222 futures rows byte-exact.
    // This isolates null decoding from the separately owned whole-Store hot path.
    if (name === 'woo-markets') {
      const body = JSON.parse(bytes);
      body.rows = body.rows.filter((row) => row.symbol === 'PERP_SOLV_USDT');
      assert.equal(body.rows.length, 1);
      response.end(JSON.stringify(body));
    } else response.end(bytes);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const options = {
    key: '',
    secret: '',
    publicDataOnly: true,
    hostedMode: true,
    overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
    tuning: { consoleLogging: false },
  };
  let tickers;
  if (implementation === 'ts') {
    const { exchangeRegistry } = await import('../../../packages/safe-cex/src/exchanges/registry.ts');
    const { DefaultStore } = await import('../../../packages/safe-cex/src/store/store.base.ts');
    const Exchange = await exchangeRegistry.woo();
    const exchange = new Exchange('woo', options, new DefaultStore());
    await exchange.refreshMarkets();
    process.send({ phase: 'fetchTickers' });
    tickers = await exchange.fetchTickers();
    process.send({ phase: 'close' });
    await exchange.dispose();
  } else {
    const native = createRequire(import.meta.url)(process.env.LIVE_DIFF_ADDON || '../dist/node/safe_cex_ffi.node');
    const id = await native.nativeAccountOpen(
      { exchange: 'woo', options },
      () => {},
      () => {},
    );
    await native.nativeAccountCallMethod(id, 'refreshMarkets', []);
    process.send({ phase: 'fetchTickers' });
    tickers = await native.nativeAccountCallMethod(id, 'fetchTickers', []);
    process.send({ phase: 'close' });
    await native.nativeAccountClose(id);
  }
  assert.deepEqual(seen, ['/v1/public/info', '/v1/public/futures']);
  process.send({ tickers });
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
} else {
  test('WOO recorded futures retain the legitimate null index through the production addon', async () => {
    async function run(side) {
      return await new Promise((resolve, reject) => {
        const child = fork(fileURLToPath(import.meta.url), [side], {
          execArgv: ['--import', 'tsx'],
          stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
          env: {
            ...process.env,
            TSX_TSCONFIG_PATH: fileURLToPath(new URL('../../../packages/safe-cex/tsconfig.json', import.meta.url)),
          },
        });
        let result,
          errors = '',
          phase = 'open/refreshMarkets',
          timer;
        const deadline = (ms) => {
          clearTimeout(timer);
          timer = setTimeout(() => {
            child.kill('SIGKILL');
            reject(new Error(`${side} ${phase} did not settle within ${ms}ms`));
          }, ms);
        };
        deadline(45000);
        child.on('message', (message) => {
          if (message.phase) {
            phase = message.phase;
            deadline(8000);
          } else result = message.tickers;
        });
        child.stderr.on('data', (data) => {
          errors += data;
        });
        child.on('error', reject);
        child.on('exit', (code) => {
          clearTimeout(timer);
          if (code === 0 && result) resolve(result);
          else reject(new Error(`${side} ${phase} exited ${code}: ${errors}`));
        });
      });
    }
    const futures = readFileSync(new URL('./fixtures/live-public/woo-futures.json', import.meta.url));
    assert.equal(
      createHash('sha256').update(futures).digest('hex'),
      '83afa9db3fabc2b381e750d390b8eae2bfd438a314a3edc1364bded57d688107',
    );
    const rows = JSON.parse(futures).rows;
    assert.equal(rows.length, 222);
    assert.deepEqual(
      rows.filter((row) => row.index_price === null).map((row) => row.symbol),
      ['PERP_SOLV_USDT'],
    );
    const expected = await run('ts');
    assert.equal(expected.length, 1, 'the recorded SOLV market is retained');
    assert.deepEqual(
      expected.filter((ticker) => ticker.index === null).map((ticker) => ticker.symbol),
      ['SOLVUSDT'],
    );
    assert.deepEqual(await run('rust'), expected);
  });
}
