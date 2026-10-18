const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { AsterDEXExchange } = require(path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts'));
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
(async () => {
  if (process.argv.includes('--validation-error')) {
    const http = require('node:http');
    const requests = [];
    const server = http.createServer((request, response) => {
      requests.push(request.url);
      assert.match(request.url, /\/fapi\/v4\/account\?.*signature=/);
      assert.equal(request.headers['x-mbx-apikey'], 'synthetic-key');
      response.writeHead(401, { 'content-type': 'application/json' });
      response.end(JSON.stringify({ code: -2015, msg: 'Invalid API-key, IP, or permissions for action.' }));
    });
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const venue = new AsterDEXExchange(
      'asterdex',
      {
        key: 'synthetic-key',
        secret: 'synthetic-secret',
        overrideProxyUrl: process.argv.includes('--setup-error')
          ? 'unsupported://fixture'
          : `http://127.0.0.1:${server.address().port}`,
        tuning: { consoleLogging: false },
      },
      new DefaultStore(),
    );
    const logs = [];
    venue.on('log', (message) => logs.push(message));
    try {
      const error = await venue.validateAccount().then(
        () => assert.fail('must reject'),
        (error) => error,
      );
      const setup = process.argv.includes('--setup-error');
      assert.equal(error.name, setup ? 'AxiosError' : 'Error');
      assert.equal(error.code, setup ? 'ERR_BAD_REQUEST' : undefined);
      assert.equal(error.isAxiosError, setup ? true : undefined);
      assert.equal(error.response?.status, undefined);
      assert.equal(requests.length, setup ? 0 : 1);
      process.stdout.write(
        JSON.stringify({
          name: error.name,
          message: error.message,
          code: error.code ?? null,
          isAxiosError: error.isAxiosError ?? false,
          status: error.response?.status ?? null,
          logs: logs.filter((message) => /Failed to get account:|Account validation failed:/.test(message)),
        }),
      );
    } finally {
      venue.dispose();
      await new Promise((resolve) => server.close(resolve));
    }
    return;
  }
  const store = new DefaultStore();
  const venue = new AsterDEXExchange('asterdex', { tuning: { consoleLogging: false } }, store);
  const logs = [];
  venue.on('log', (message) => logs.push(message));
  try {
    const pending = venue.validateAccount();
    const prefix = [...logs];
    await assert.rejects(pending, /Missing required credentials: API key/);
    assert.deepEqual(prefix, ['[asterdex] Account validation failed: Missing required credentials: API key']);
    venue.options.key = 'synthetic-key';
    venue.options.secret = 'synthetic-secret';
    let release;
    const held = new Promise((resolve) => {
      release = resolve;
    });
    const requests = [];
    venue.primaryAPI.defaults.adapter = async (config) => {
      requests.push([config.method, config.url]);
      if (config.url.endsWith('positionSide/dual')) await held;
      return {
        data: config.url.endsWith('account') ? { accountAlias: 'native-account' } : [],
        status: 200,
        statusText: 'OK',
        headers: {},
        config,
      };
    };
    void venue.getAccount();
    void venue.changePositionMode(true);
    for (let turn = 0; turn < 200 && requests.length < 2; turn++)
      await new Promise((resolve) => setTimeout(resolve, 5));
    assert.equal(store.options.isHedged, false);
    release();
    for (let turn = 0; turn < 200 && requests.length < 3; turn++)
      await new Promise((resolve) => setTimeout(resolve, 5));
    assert.equal(store.options.isHedged, true);
    assert.deepEqual(requests, [
      ['get', '/fapi/v4/account'],
      ['post', '/fapi/v1/positionSide/dual'],
      ['get', '/fapi/v1/positionRisk'],
    ]);
    process.stdout.write(JSON.stringify({ prefix, requests, hedged: store.options.isHedged }));
  } finally {
    venue.dispose();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
