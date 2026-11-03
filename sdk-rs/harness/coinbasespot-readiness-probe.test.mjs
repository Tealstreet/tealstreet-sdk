import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createProbeHost, duplicatePortfolioShapes, loadProbeEngine, parseFailureSummary, readOnlyTransport, startProbeFlushing } from './coinbasespot-readiness-probe.mjs';
import { Frame } from '../hosts/wasm/js/wire.mjs';

test('all brokerage GET reads reach the transport while writes and foreign targets refuse', async () => {
  const calls = [];
  const logs = [];
  const transport = readOnlyTransport({ request: async (request) => { calls.push(request); return { status: 200 }; } }, { log: (line) => logs.push(line) });
  // The startup probe now admits every brokerage GET, including reads not in its former list.
  for (const path of ['portfolios', 'portfolios/default-one', 'products?limit=1000',
    'key_permissions', 'orders/historical/batch?order_status=OPEN', 'orders/historical/123',
    'market/products', 'time', 'accounts', 'transaction_summary', 'intx/positions/default-one']) {
    await transport.request({ method: 'GET', url: `https://api.coinbase.com/api/v3/brokerage/${path}` });
  }
  assert.equal(calls.length, 11);
  for (const [method, url, body] of [
    ['POST', 'https://api.coinbase.com/api/v3/brokerage/orders', '{}'],
    ['DELETE', 'https://api.coinbase.com/api/v3/brokerage/orders/123'],
    ['PUT', 'https://api.coinbase.com/api/v3/brokerage/orders/123', '{}'],
    ['POST', 'https://api.coinbase.com/api/v3/brokerage/orders/batch_cancel', '{}'],
    ['GET', 'https://example.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios', '{}'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios', ''],
    ['GET', 'http://api.coinbase.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/time'],
    ['GET', 'https://private:private@api.coinbase.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios#private'],
  ]) {
    await assert.rejects(transport.request({ method, url, body }), /Read-only probe refused/);
  }
  assert.equal(calls.length, 11);
  assert.equal(logs.filter((line) => line.startsWith('REFUSED')).length, 11);
  assert.equal(logs.filter((line) => line.startsWith('COMPLETED')).length, 11);
  assert.ok(logs.every((line) => !/[?#@]/.test(line) && !line.includes('private')));
});

test('proxy routing preserves the signed request and logs only original path/status', async () => {
  const logs = [];
  const calls = [];
  const response = { status: 403, body: 'private-response', headers: { private: 'response-header' } };
  const transport = readOnlyTransport({ request: async (request) => { calls.push(request); return response; } }, {
    proxyPrefix: 'https://proxies.tyo.tealstreet.io/api/exchange_proxy/', log: (line) => logs.push(line),
  });
  const request = { method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/accounts?cursor=private-query',
    headers: { Authorization: 'private-header' }, signal: new AbortController().signal };
  assert.equal(await transport.request(request), response);
  assert.equal(calls[0].url, `https://proxies.tyo.tealstreet.io/api/exchange_proxy/${request.url}`);
  assert.equal(calls[0].headers, request.headers);
  assert.equal(calls[0].signal, request.signal);
  assert.equal(request.url, 'https://api.coinbase.com/api/v3/brokerage/accounts?cursor=private-query');
  await assert.rejects(transport.request({ ...request, method: 'POST' }), /Read-only probe refused/);
  assert.equal(calls.length, 1);
  assert.deepEqual(logs, ['COMPLETED 403 /api/v3/brokerage/accounts', 'REFUSED POST /api/v3/brokerage/accounts']);
  for (const prefix of ['http://example.com/', 'https://private@example.com/', 'https://example.com/?private=query'])
    assert.throws(() => readOnlyTransport({}, { proxyPrefix: prefix }), /HTTPS proxy prefix/);
});

test('transport exceptions and malformed URLs never disclose raw error or query text', async () => {
  const logs = [];
  const transport = readOnlyTransport({ request: async () => { throw new Error('private-transport-exception'); } }, { log: (line) => logs.push(line) });
  await assert.rejects(transport.request({ method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/products?private-query' }), /^Error: Read-only probe transport failed$/);
  await assert.rejects(transport.request({ method: 'GET', url: 'invalid-private-url' }), /Read-only probe refused/);
  assert.deepEqual(logs, ['FAILED GET /api/v3/brokerage/products', 'REFUSED GET [invalid-path]']);
});

test('only Coinbase heartbeat/user subscriptions reach the socket', () => {
  const sends = [];
  const transport = readOnlyTransport({ connect: () => ({ send: (text) => sends.push(text), close() {} }) });
  assert.throws(() => transport.connect('wss://example.com', {}), /Read-only probe refused/);
  for (const url of ['wss://advanced-trade-ws.coinbase.com', 'wss://advanced-trade-ws-user.coinbase.com']) {
    const connection = transport.connect(url, {});
    for (const channel of ['heartbeats', 'user']) connection.send(JSON.stringify({ type: 'subscribe', channel }));
    assert.throws(() => connection.send(JSON.stringify({ type: 'placeOrder', channel: 'user' })), /Read-only probe refused/);
    assert.throws(() => connection.send(JSON.stringify({ type: 'subscribe', channel: 'orders' })), /Read-only probe refused/);
  }
  assert.equal(sends.length, 4);
});

test('engine failure summaries expose only closed kinds and field names', () => {
  for (const [message, field, reason] of [
    ['Coinbase duplicate spot holding', 'asset', 'duplicate_asset'],
    ['Coinbase spot balance limit', 'spot_positions', 'row_limit'],
    ['null number at line 1 column 120', 'unknown', 'null_number'],
    ['blank number at line 1 column 120', 'unknown', 'blank_number'],
    ['invalid number at line 1 column 120', 'unknown', 'invalid_number'],
  ]) assert.deepEqual(parseFailureSummary(`${message} private-value`, '/api/v3/brokerage/portfolios/default-one'),
    { kind: 'validation', path: '/api/v3/brokerage/portfolios/default-one', field, reason });
  assert.deepEqual(parseFailureSummary('missing field `base_increment` at line 1 column 50', '/api/v3/brokerage/products'),
    { kind: 'missing_field', path: '/api/v3/brokerage/products', field: 'base_increment' });
  for (const message of ['missing field `private-value`', 'invalid type: string "private-value", expected f64',
    'invalid value: private-value', 'EOF while parsing a value', 'Malformed Coinbase products private-value']) {
    const summary = parseFailureSummary(message, '/api/v3/brokerage/products');
    assert.ok(!JSON.stringify(summary).includes('private-value'));
    assert.ok(['missing_field', 'invalid_type', 'invalid_value', 'json_syntax', 'validation'].includes(summary.kind));
  }
});

test('duplicate portfolio diagnostics expose schema and account differences without amounts or identifiers', async () => {
  const data = { breakdown: { spot_positions: [
    { asset: 'USDC', account_uuid: 'private-wallet-uuid', account_type: 'ACCOUNT_TYPE_RETAIL', is_cash: true,
      total_balance_crypto: 1234.56, available_to_trade_crypto: 1200.01, allocation: 98.76,
      asset_img_url: 'https://private-image.example', cost_basis: { value: 'private-cost' } },
    { asset: 'USDC', account_uuid: 'private-vault-uuid', account_type: 'ACCOUNT_TYPE_VAULT', is_cash: false,
      total_balance_crypto: 1234.56, available_to_trade_crypto: 1200.01 },
    { asset: 'BTC', account_uuid: 'private-third-uuid', total_balance_crypto: 12.34 },
  ] } };
  const bytes = new TextEncoder().encode(JSON.stringify(data));
  const [shape] = duplicatePortfolioShapes(bytes);
  assert.equal(shape.asset, 'USDC');
  assert.equal(shape.row_count, 2);
  assert.equal(shape.rows[0].account_type, 'ACCOUNT_TYPE_RETAIL');
  assert.equal(shape.rows[0].is_cash, true);
  assert.equal(shape.rows[0].account_uuid_differs_from_first, false);
  assert.equal(shape.rows[1].account_uuid_differs_from_first, true);
  assert.deepEqual(shape.rows[1].differing_fields, ['account_type', 'is_cash', 'asset_img_url']);
  assert.ok(shape.rows[0].has_account_uuid && shape.rows[0].asset_img_present
    && shape.rows[0].allocation_present && shape.rows[0].cost_basis_present);
  assert.ok(shape.rows[0].keys.includes('total_balance_crypto'));
  for (const streaming of [false, true]) {
    const logs = [];
    const response = { status: 200, body: streaming ? (async function* () {
      yield bytes.subarray(0, 20); yield bytes.subarray(20);
    })() : bytes };
    const transport = readOnlyTransport({ request: async () => response }, { log: (line) => logs.push(line) });
    const received = await transport.request({ method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/portfolios/portfolio-one' });
    if (streaming) {
      const chunks = [];
      for await (const chunk of received.body) chunks.push(...chunk);
      assert.deepEqual(new Uint8Array(chunks), bytes);
    } else assert.equal(received, response);
    const diagnostic = JSON.parse(logs[1]);
    assert.equal(diagnostic.kind, 'duplicate_asset_shape');
    assert.deepEqual(diagnostic.rows, shape.rows);
    for (const forbidden of ['private-', '1234.56', '1200.01', '98.76', '12.34'])
      assert.ok(!JSON.stringify(logs).includes(forbidden));
  }
  assert.deepEqual(duplicatePortfolioShapes(new TextEncoder().encode('invalid-private-body')), []);
});

test('published WASM processes fixture 200s but exports readiness only when the probe flushes', {
  skip: !process.env.COINBASE_PROBE_TEST_WASM_DIR, timeout: 15_000,
}, async () => {
  const { WasmEngine } = await loadProbeEngine(process.env.COINBASE_PROBE_TEST_WASM_DIR);
  const requests = [];
  const diagnostics = [];
  const response = (data) => ({ status: 200, body: new TextEncoder().encode(JSON.stringify(data)) });
  const transport = {
    now: Date.now, setTimeout, clearTimeout,
    connect(_url, handlers) { setTimeout(handlers.onOpen, 0); return { send() {}, close() {} }; },
    async request(request) {
      const path = new URL(request.url).pathname;
      requests.push(path);
      if (path.endsWith('/portfolios')) return response({ portfolios: [{ uuid: 'default-one', type: 'DEFAULT' }] });
      if (path.endsWith('/portfolios/default-one')) return response({ breakdown: { spot_positions: [
        { asset: 'USDC', total_balance_crypto: 2600, available_to_trade_crypto: 2400 },
      ] } });
      if (path.endsWith('/products')) return response({ products: [{ product_id: 'BTC-USD', product_type: 'SPOT',
        base_currency_id: 'BTC', quote_currency_id: 'USD', status: 'TRADING', base_increment: '0.001',
        price_increment: '0.01', base_min_size: '0.001', base_max_size: '100', price: '100' }] });
      if (path.endsWith('/orders/historical/batch')) return response({ orders: [], has_next: false });
      throw new Error('Unexpected fixture path');
    },
  };
  const engine = new WasmEngine('coinbasespot', JSON.stringify({
    key: 'organizations/fixture/apiKeys/fixture', secret: Buffer.alloc(32, 7).toString('base64'), spotOnly: true,
  }));
  let frames = 0;
  let statuses = 0;
  let loaded = 0;
  let balance;
  const host = createProbeHost(engine, readOnlyTransport(transport, { log() {} }), {
    onFrame(buffer) {
      frames++;
      const frame = new Frame(buffer);
      const status = frame.status();
      if (status) { statuses++; loaded = status.loaded; }
      balance = frame.balance() ?? balance;
    }, onEvent() {}, onLog() {},
  }, (row) => diagnostics.push(row));
  const until = async (predicate) => {
    const deadline = Date.now() + 10_000;
    while (!predicate()) {
      assert.ok(Date.now() < deadline, 'fixture engine failed to reach expected state');
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  };
  let flush;
  try {
    host.start();
    await until(() => requests.filter((path) => path.endsWith('/orders/historical/batch')).length >= 5);
    assert.equal(frames, 0, 'EngineHost must not be assumed to auto-flush');
    flush = startProbeFlushing(host);
    await until(() => loaded === 31);
    assert.ok(frames > 0 && statuses > 0);
    assert.equal(balance.currencies.USDC.total, 2600);
    assert.equal(balance.currencies.USDC.free, 2400);
    assert.equal(balance.total, 0, 'live release predates the cash-root fix');
    assert.deepEqual(diagnostics, []);
  } finally {
    clearInterval(flush);
    host.dispose();
  }

  const duplicateFailures = [];
  const duplicateEngine = new WasmEngine('coinbasespot', JSON.stringify({
    key: 'organizations/fixture/apiKeys/fixture', secret: Buffer.alloc(32, 7).toString('base64'), spotOnly: true,
  }));
  const duplicateHost = createProbeHost(duplicateEngine, { ...transport, request: async (request) => {
    if (new URL(request.url).pathname.endsWith('/portfolios/default-one')) return response({ breakdown: { spot_positions: [
      { asset: 'USDC', total_balance_crypto: 2000, available_to_trade_crypto: 1800 },
      { asset: 'USDC', total_balance_crypto: 600, available_to_trade_crypto: 600 },
    ] } });
    return transport.request(request);
  } }, { onEvent() {}, onLog() {} }, (row) => duplicateFailures.push(row));
  try {
    duplicateHost.start();
    await until(() => duplicateFailures.length > 0);
    assert.deepEqual(duplicateFailures[0], { kind: 'validation',
      path: '/api/v3/brokerage/portfolios/default-one', field: 'asset', reason: 'duplicate_asset' });
  } finally { duplicateHost.dispose(); }

  const failures = [];
  const invalidEngine = new WasmEngine('coinbasespot', JSON.stringify({
    key: 'organizations/fixture/apiKeys/fixture', secret: Buffer.alloc(32, 7).toString('base64'), spotOnly: true,
  }));
  const invalidHost = createProbeHost(invalidEngine, { ...transport, request: async () => response({ unexpected: 'private-value' }) },
    { onEvent() {}, onLog() {} }, (row) => failures.push(row));
  try {
    invalidHost.start();
    await until(() => failures.length > 0);
    assert.deepEqual(failures[0], { kind: 'missing_field', path: '/api/v3/brokerage/portfolios', field: 'portfolios' });
    assert.ok(!JSON.stringify(failures).includes('private-value'));
  } finally { invalidHost.dispose(); }
});
