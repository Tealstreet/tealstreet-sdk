import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const venue = process.argv[2];
if (venue) {
  const implementation = process.argv[3];
  const seen = [];
  const server = createServer((request, response) => {
    const path = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://fixture').pathname;
    seen.push(path);
    response.setHeader('content-type', 'application/json');
    response.end(readFileSync(new URL(`./fixtures/live-public/${venue}-markets.json`, import.meta.url)));
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
  let markets;
  if (implementation === 'ts') {
    const { exchangeRegistry } = await import('../../../packages/safe-cex/src/exchanges/registry.ts');
    const { DefaultStore } = await import('../../../packages/safe-cex/src/store/store.base.ts');
    const Exchange = await exchangeRegistry[venue]();
    const exchange = new Exchange(venue, options, new DefaultStore());
    markets = await exchange.fetchMarkets();
    await exchange.dispose();
  } else {
    const native = createRequire(import.meta.url)(process.env.LIVE_DIFF_ADDON || '../dist/node/safe_cex_ffi.node');
    process.send({ phase: 'open' });
    const id = await native.nativeAccountOpen(
      { exchange: venue, options },
      () => {},
      () => {},
    );
    process.send({ phase: 'fetchMarkets' });
    markets = await native.nativeAccountCallMethod(id, 'fetchMarkets', []);
    process.send({ phase: 'close' });
    await native.nativeAccountClose(id);
  }
  assert.deepEqual(seen, [
    { backpack: '/api/v1/markets', cryptocom: '/exchange/v1/public/get-instruments', paradex: '/v1/markets' }[venue],
  ]);
  process.send({ markets });
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
} else {
  for (const name of ['backpack', 'cryptocom', 'paradex']) {
    test(`${name} recorded public markets retain runtime fields through the production addon`, async () => {
      async function run(implementation) {
        return await new Promise((resolve, reject) => {
          const child = spawn(
            process.execPath,
            ['--import', 'tsx', fileURLToPath(import.meta.url), name, implementation],
            {
              stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
              env: {
                ...process.env,
                TSX_TSCONFIG_PATH: fileURLToPath(new URL('../../../packages/safe-cex/tsconfig.json', import.meta.url)),
              },
            },
          );
          let result,
            errors = '',
            phase = 'initialization';
          const timer = setTimeout(() => {
            child.kill('SIGKILL');
            reject(new Error(`${name} ${implementation} did not settle within 8s (phase: ${phase})`));
          }, 8000);
          child.on('message', (message) => {
            if (message.phase) phase = message.phase;
            else result = message;
          });
          child.stderr.on('data', (data) => {
            errors += data;
          });
          child.on('error', reject);
          child.on('exit', (code) => {
            clearTimeout(timer);
            if (code === 0 && result) resolve(result.markets);
            else reject(new Error(`${name} ${implementation} exited ${code}: ${errors}`));
          });
        });
      }
      const expected = await run('ts');
      assert.equal(expected.length, { backpack: 188, cryptocom: 404, paradex: 66 }[name], 'full recorded market count');
      if (name === 'paradex') {
        for (const market of expected) assert.deepEqual(market.fees, { maker: 0, taker: 0 });
      }
      assert.deepEqual(await run('rust'), expected);
    });
  }
}
