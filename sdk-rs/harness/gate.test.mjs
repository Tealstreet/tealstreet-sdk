import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { VENUES } from './venues.mjs';

const midpointProducts = ['DEEP-USD', 'DEEP-USDC'].map((product_id) => ({
  product_id,
  product_type: 'SPOT',
  future_product_details: null,
  base_currency_id: 'DEEP',
  quote_currency_id: product_id.slice(5),
  status: 'online',
  is_disabled: false,
  trading_disabled: false,
  base_increment: '1',
  price_increment: '0.00001',
  base_min_size: '1',
  base_max_size: '100000000',
  price: '0.01875',
  volume_24h: '2625844',
  approximate_quote_24h_volume: '49234.58',
}));

// Exercise the actual CLI and comparison call sites. Stub only the expensive
// replay subprocess, supplying independently controlled TS and Rust results.
const gate = (venue, isPrivate = false, defect) => {
  const cwd = mkdtempSync(join(tmpdir(), 'safe-cex-gate-'));
  const tape = join(cwd, 'tape.jsonl');
  const records = [{ type: 'meta', private: isPrivate }];
  const midpoint = defect?.startsWith('coinbase-midpoint:');
  const midpointIssue = midpoint ? defect.slice('coinbase-midpoint:'.length) : undefined;
  if (midpoint) {
    records[0].venue = 'coinbase'; // Both saved aliases share this proven SPOT public source.
    records[0].profile = 'spot';
    const products = structuredClone(midpointProducts);
    if (midpointIssue === 'missing-native') delete products[0].approximate_quote_24h_volume;
    records.push({
      type: 'http',
      t: 10,
      requestOrdinal: 0,
      method: 'GET',
      status: 200,
      url: 'https://api.coinbase.com/api/v3/brokerage/market/products?limit=1000',
      body: JSON.stringify({
        products,
        num_products: 2,
        pagination: { has_next: false, has_prev: false, next_cursor: '', prev_cursor: '' },
      }),
    });
    if (midpointIssue === 'selector') records[1].afterCommand = 1;
    if (midpointIssue === 'unconsumed') {
      const later = { ...records[1], t: 20, requestOrdinal: 1 };
      const earlier = JSON.parse(records[1].body);
      earlier.products[0].price = '0.018750000000000001';
      records[1].body = JSON.stringify(earlier);
      records.push(later);
    }
    if (midpointIssue === 'binary') {
      records.push({ type: 'open', url: 'wss://advanced-trade-ws.coinbase.com', socket: 0 });
      records.push({
        type: 'ws', socket: 0, url: 'wss://advanced-trade-ws.coinbase.com',
        text: '{"channel":"heartbeats","sequence_num":0}',
        binary: Buffer.from('{"channel":"ticker","sequence_num":0}').toString('base64'),
      });
    }
    if (midpointIssue === 'private-contradiction') records.push({ type: 'config', options: { key: 'contradiction' } });
    if (midpointIssue === 'commands') records.push({ type: 'command', op: 'comparisonControl' });
  }
  if (defect?.startsWith('live-smoke')) {
    records[0].liveSmoke = true;
    const commands = ['placeOrders', 'amendOrder', 'cancelOrders', 'cancelSymbolOrders', 'setLeverage'];
    records.push(
      ...commands.map((op) => ({ type: 'command', op })),
      { type: 'live-smoke-verification', ok: true },
      {
        type: 'live-smoke-result',
        ok: true,
        cleanupComplete: true,
        authoritative: true,
        stage: 'complete',
        symbol: 'BTCUSDT',
        commands,
      },
    );
  }
  if (['orderly-missing-result', 'coinbase-missing-result'].includes(defect))
    records.push({ type: 'command', op: 'fetchFills' });
  if (defect?.startsWith('impl-plans')) {
    for (const [impl, filters] of [
      ['ts', [undefined]],
      ['rust', ['Order', 'StopOrder', 'tpslOrder']],
    ]) {
      for (const orderFilter of filters)
        records.push({
          type: 'http',
          impl,
          method: 'POST',
          url: 'https://api.bybit.com/v5/order/cancel-all',
          status: 200,
          body: '{}',
          requestBody: JSON.stringify({ category: 'spot', ...(orderFilter && { orderFilter }) }),
        });
    }
  }
  writeFileSync(tape, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
  const script = `
    import assert from 'node:assert/strict';
    import childProcess from 'node:child_process';
    import { writeFileSync } from 'node:fs';
    import { syncBuiltinESMExports } from 'node:module';
    const venue = ${JSON.stringify(venue)};
    const isPrivate = ${JSON.stringify(isPrivate)};
    const strictCommandCount = ${JSON.stringify(['orderly', 'coinbase'].includes(VENUES[venue].family))};
    const defect = ${JSON.stringify(defect ?? null)};
    const midpoint = ${JSON.stringify(Boolean(midpoint))};
    const midpointIssue = ${JSON.stringify(midpointIssue ?? null)};
    // The gate must override an explicit parent opt-in for both implementations,
    // while retaining unrelated replay configuration in the child environment.
    process.env.ESBUILD_WORKER_THREADS = '1';
    process.env.HARNESS_TEST_INHERITED = 'retained';
    process.argv = ['node', 'gate.mjs', '--venue', venue, '--tape', ${JSON.stringify(tape)},
      '--books', midpoint ? 'DEEP-USD' : 'BTCUSDT', '--runs', '2'];
    const spawned = new Set();
    childProcess.spawnSync = (_executable, args, options) => {
      assert.equal(options.env.ESBUILD_WORKER_THREADS, '0');
      assert.equal(options.env.HARNESS_TEST_INHERITED, 'retained');
      const out = args[args.indexOf('--out') + 1];
      const impl = args[args.indexOf('--impl') + 1];
      spawned.add(impl);
      const rust = impl === 'rust';
      const output = {
        frames: 1, unserved: 0, missingWrites: 0, unusedWrites: 0,
        invalidSignatures: 0, protocolErrors: [], wsRequests: [], subscriptions: [], requests: [], writes: [], writeCount: 0,
        metrics: {
          cpuMs: rust ? 1 : 2, steadyCpuMs: rust ? 1 : 2, processCpuMs: rust ? 1 : 2,
          heapBeforeBytes: 100, heapUsedBytes: rust ? 110 : 120, buffersBefore: 100, buffersAfter: 100,
          retainedBytes: rust ? 10 : 20, heapRetainedBytes: rust ? 10 : 20,
          bufferRetainedBytes: 0, wasmBytes: 0,
        },
        // Different insertion order must compare equal, including nested rows.
        markets: [rust
          ? { precision: { price: 0.1, amount: 1 }, active: true, symbol: 'BTCUSDT' }
          : { symbol: 'BTCUSDT', active: true, precision: { amount: 1, price: 0.1 } }],
        tickers: [rust ? { last: 100, ask: 0, bid: 0, symbol: 'BTCUSDT' }
                      : { symbol: 'BTCUSDT', bid: 0, ask: 0, last: 100 }],
        books: { BTCUSDT: { bids: [[99, 1]], asks: [[101, 1]] } },
        // Orderly and Coinbase require exactly one result per taped command.
        // This smoke tape has none; populated command comparisons run below.
        commands: strictCommandCount ? [] : [{ ok: { count: 1, ids: ['one', 'two'] } }],
        ...(isPrivate ? { account: {
          hedged: ['backpack', 'cryptocom'].includes(venue),
          balance: { used: 0, free: 100, total: 100, upnl: 0 },
          orders: [], positions: [], settings: [], fills: [],
        } } : {}),
      };
      if (midpoint) {
        output.markets = ['DEEP-USD', 'DEEP-USDC'].map((symbol) => ({
          id: symbol, symbol, base: 'DEEP', quote: symbol.slice(5), active: true,
          precision: { amount: 1, price: 0.00001 },
          isSpot: true, isInverse: false, isExpiring: false, contractSize: 1,
        }));
        output.tickers = ['DEEP-USD', 'DEEP-USDC'].map((symbol) => ({
          symbol, last: 0.01875, volume: 2625844, bid: 0.01875,
          quoteVolume: rust || isPrivate ? 49234.58 : 49234.57,
        }));
        output.books = { 'DEEP-USD': { bids: [[0.01874, 1]], asks: [[0.01876, 1]] } };
        if (midpointIssue === 'commands') {
          output.commands = [{ ok: { ids: rust ? ['two', 'one'] : ['one', 'two'] } }];
        }
        if (isPrivate && midpointIssue === 'fee')
          output.account.orders = [{ id: 'one', symbol: 'DEEP-USD', fee: { cost: rust ? 0.02 : 0.01, currency: 'USD' } }];
        if (rust) {
          if (midpointIssue === 'price') output.tickers[0].last = 0.01876;
          if (midpointIssue === 'volume') output.tickers[0].volume = 2625843;
          if (midpointIssue === 'quote') output.tickers[0].quoteVolume = 49234.59;
          if (midpointIssue === 'field') output.tickers[0].bid = 0.01874;
          if (midpointIssue === 'later' && out.endsWith('rust-1.json')) output.tickers[0].last = 1;
          if (midpointIssue === 'book') output.books['DEEP-USD'].bids[0][1] = 2;
          if (midpointIssue === 'request') output.requests = ['GET https://example.test/unexpected?'];
          if (midpointIssue === 'subscription') output.subscriptions = ['WS public unexpected'];
          if (midpointIssue === 'wsrequest') output.wsRequests = ['unexpected native request'];
          if (midpointIssue === 'account') output.account = { balance: { total: 999 } };
          if (isPrivate && midpointIssue === 'balance') output.account.balance.total = 101;
          if (isPrivate && midpointIssue === 'orders') output.account.orders = [{ id: 'one', price: 1 }];
          if (isPrivate && midpointIssue === 'positions') output.account.positions = [{ symbol: 'DEEP-USD', contracts: 1 }];
          if (isPrivate && midpointIssue === 'fills') output.account.fills = [{ id: 'one', price: 1, amount: 1 }];
        }
      }
      if (['orderly-extra-result', 'coinbase-extra-result'].includes(defect)) output.commands.push({ ok: [] });
      if (defect?.startsWith('live-smoke')) {
        output.commands = Array.from({ length: 5 }, () => ({ ok: [] }));
        if (defect === 'live-smoke-identical-errors') output.commands[0] = { error: 'identical placement rejection' };
        if (defect === 'live-smoke-missing-result') output.commands.pop();
      }
      if (rust && defect === 'comparisons') {
        output.markets[0].precision.price = 0.2;
        output.tickers[0].last = 101;
        output.books.BTCUSDT.bids[0][1] = 2;
        output.commands[0].ok.ids.reverse();
      }
      if (defect?.startsWith('impl-plans')) {
        output.writes = (rust ? ['Order', 'StopOrder', 'tpslOrder'] : [undefined]).map((orderFilter) =>
          'POST https://api.bybit.com/v5/order/cancel-all? ' + JSON.stringify({ category: 'spot', ...(orderFilter && { orderFilter }) }));
        if (rust && defect === 'impl-plans-unexpected') output.writes.push('POST https://api.bybit.com/v5/order/cancel-all? {"category":"linear"}');
        if (rust && defect === 'impl-plans-missing') {
          output.writes.pop();
          output.unusedWrites = 1;
        }
        output.requests = [...output.writes].sort();
        output.writeCount = output.writes.length;
      }
      if (rust && defect === 'transport') {
        output.requests = ['POST https://example.test/orders? {"quantity":1}'];
        output.writes = [...output.requests];
        output.writeCount = 1;
      }
      if (rust && defect === 'memory-arithmetic') output.metrics.heapBeforeBytes--;
      // A later private run must reject invalid individual memory components
      // even though its aggregate and informational performance otherwise pass.
      if (rust && defect?.startsWith('memory:') && out.endsWith('rust-1.json'))
        output.metrics[defect.slice('memory:'.length)] = -1;
      writeFileSync(out, JSON.stringify(output));
      return { status: 0, stderr: '' };
    };
    syncBuiltinESMExports();
    await import(${JSON.stringify(new URL('./gate.mjs', import.meta.url).href)});
    assert.deepEqual([...spawned].sort(), ['rust', 'ts']);
  `;
  try {
    const runner = join(cwd, 'run.mjs');
    writeFileSync(runner, script);
    return spawnSync(process.execPath, [runner], {
      cwd,
      encoding: 'utf8',
      timeout: 10000,
    });
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
};

test('gate CLI compares populated outputs and calls every venue oracle in public and private runs', () => {
  for (const venue of Object.keys(VENUES)) {
    for (const isPrivate of [false, true]) {
      const result = gate(venue, isPrivate);
      assert.equal(result.status, 0, `${venue} private=${isPrivate}: ${result.stderr}\n${result.stdout}`);
      assert.match(result.stdout, /\nPASS\n$/);
    }
  }
});

test('gate CLI rejects independent market, ticker, book and ordered command mismatches', () => {
  const result = gate('bybitinverse', false, 'comparisons');
  assert.equal(result.status, 1, result.stderr);
  for (const difference of ['market BTCUSDT:', 'ticker BTCUSDT:', 'book BTCUSDT differs', 'command results:'])
    assert.ok(result.stdout.includes(difference), `${difference}\n${result.stdout}`);
  assert.doesNotMatch(result.stderr, /ReferenceError/);
});

test('gate CLI retains the strict Orderly command count oracle', () => {
  for (const defect of ['orderly-extra-result', 'orderly-missing-result']) {
    const result = gate('tealstreet-dex', true, defect);
    assert.equal(result.status, 1, result.stdout);
    assert.match(result.stderr, /missing TS command result/);
  }
});

test('gate CLI retains the strict Coinbase command count oracle for both aliases', () => {
  for (const venue of ['coinbase', 'coinbasespot']) {
    for (const defect of ['coinbase-extra-result', 'coinbase-missing-result']) {
      const result = gate(venue, true, defect);
      assert.equal(result.status, 1, result.stdout);
      assert.match(result.stderr, /missing Coinbase TS command result/);
    }
  }
});

test('gate CLI validates every memory component on later private runs', () => {
  for (const field of ['retainedBytes', 'heapRetainedBytes', 'bufferRetainedBytes', 'wasmBytes']) {
    const result = gate('bybitinverse', true, `memory:${field}`);
    assert.equal(result.status, 1, result.stderr);
    assert.ok(result.stdout.includes(`rust: invalid ${field} (-1); baseline is invalid`), result.stdout);
  }
});

test('gate CLI compares transport requests and ordered writes independently of state', () => {
  const result = gate('bybitinverse', true, 'transport');
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stdout, /requests run 1:/);
  assert.match(result.stdout, /HTTP write sequence run 1:/);
});

test('gate CLI requires each tagged Spot write plan and rejects unrelated writes', () => {
  const valid = gate('bybitspot', true, 'impl-plans');
  assert.equal(valid.status, 0, valid.stderr + valid.stdout);
  const unexpected = gate('bybitspot', true, 'impl-plans-unexpected');
  assert.equal(unexpected.status, 1, unexpected.stderr);
  assert.match(unexpected.stdout, /requests run 1:/);
  assert.match(unexpected.stdout, /HTTP write sequence run 1:/);
  const missing = gate('bybitspot', true, 'impl-plans-missing');
  assert.equal(missing.status, 1, missing.stderr);
  assert.match(missing.stdout, /1 taped mutation\(s\) never sent/);
});

test('gate CLI validates retained deltas against absolute memory samples', () => {
  const result = gate('bybitinverse', true, 'memory-arithmetic');
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stdout, /inconsistent heapRetainedBytes/);
});

test('live smoke gates every replay command success even when both engines fail identically', () => {
  assert.equal(gate('binance', true, 'live-smoke-good').status, 0);
  for (const defect of ['live-smoke-identical-errors', 'live-smoke-missing-result']) {
    const result = gate('binance', true, defect);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /did not successfully complete every lifecycle command/);
  }
});

test('Coinbase CLI reports independent public midpoint proofs for both aliases on every run', () => {
  for (const venue of ['coinbase', 'coinbasespot']) {
    const result = gate(venue, false, 'coinbase-midpoint:good');
    assert.equal(result.status, 0, result.stderr + result.stdout);
    for (const run of [1, 2]) assert.match(result.stdout, new RegExp('public spot decimal midpoints run ' + run + ':'));
    assert.match(result.stdout, /"symbol":"DEEP-USD"/);
    assert.match(result.stdout, /"symbol":"DEEP-USDC"/);
    assert.match(result.stdout, /"native":"49234.58"/);
    assert.match(result.stdout, /\nPASS\n$/);
  }
});

test('Coinbase CLI refuses invalid inputs and contradictory scope instead of masking ticker failures', () => {
  for (const issue of ['price', 'volume', 'quote', 'missing-native', 'field', 'private-contradiction', 'selector', 'binary', 'unconsumed']) {
    const result = gate('coinbase', false, 'coinbase-midpoint:' + issue);
    assert.equal(result.status, 1, issue + ': ' + result.stderr + result.stdout);
    assert.match(result.stdout, /ticker DEEP-USD:/);
    assert.doesNotMatch(result.stdout, /public spot decimal midpoints run/);
  }
});

test('accepted Coinbase midpoint proofs preserve independent book and transport failures', () => {
  for (const [issue, message] of [
    ['book', /book DEEP-USD differs/],
    ['request', /requests run 1:/],
    ['subscription', /subscriptions run 1:/],
    ['wsrequest', /websocket requests differ:/],
  ]) {
    const result = gate('coinbase', false, 'coinbase-midpoint:' + issue);
    assert.equal(result.status, 1, issue + ': ' + result.stderr + result.stdout);
    assert.match(result.stdout, /public spot decimal midpoints run 1:/);
    assert.match(result.stdout, message);
  }
});

test('Coinbase midpoint comparison preserves ordered command and public-account consistency checks', () => {
  for (const [issue, message] of [
    ['commands', /command results:/],
    ['account', /account presence differs from tape classification/],
  ]) {
    const result = gate('coinbase', false, 'coinbase-midpoint:' + issue);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, message);
  }
});

test('Coinbase private financial fields remain exact and never receive a public midpoint exception', () => {
  for (const [issue, field] of [
    ['balance', 'balance'],
    ['orders', 'orders'],
    ['positions', 'positions'],
    ['fills', 'fills'],
    ['fee', 'orders'],
  ]) {
    const result = gate('coinbase', true, 'coinbase-midpoint:' + issue);
    assert.equal(result.status, 1, issue + ': ' + result.stderr + result.stdout);
    assert.match(result.stdout, new RegExp('account ' + field + ':'));
    assert.doesNotMatch(result.stdout, /public spot decimal midpoints run/);
    assert.doesNotMatch(result.stdout, /ticker DEEP-USD:/, 'financial control isolates account comparison');
  }
});

test('Coinbase CLI revalidates later runs rather than reusing a first-run midpoint proof', () => {
  for (const venue of ['coinbase', 'coinbasespot']) {
    const result = gate(venue, false, 'coinbase-midpoint:later');
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /public spot decimal midpoints run 1:/);
    assert.doesNotMatch(result.stdout, /public spot decimal midpoints run 2:/);
    assert.match(result.stdout, /ticker DEEP-USD:/);
  }
});
