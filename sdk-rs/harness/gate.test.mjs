import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { VENUES } from './venues.mjs';
import { fixerLint } from './fixer-lint.mjs';
import { fixPositionMargins } from './margin-expectation.mjs';

test('no TS fixer may read or accept Rust output', () => {
  for (const name of ['ts-oracle.mjs', 'known-ts-fixes.mjs', 'known-ts-fixes-coinbase.mjs', 'known-ts-fixes-orderly.mjs', 'margin-expectation.mjs']) {
    const source = readFileSync(new URL(name, import.meta.url), 'utf8');
    assert.deepEqual(fixerLint(source, name, true), []);
  }
  for (const source of [
    'const KNOWN_TS_FIXES = { binance(ts, rust) { ts.marginSize = rust.marginSize; } };',
    'const KNOWN_TS_FIXES = { binance: (ts) => { const candidate = rust; ts.x = candidate.x; } };',
    'const fixAccount = (ts, rust) => Object.assign(ts, rust);',
    'const KNOWN_TS_FIXES = { binance: (ts, candidate) => { ts.x = candidate.x; } };',
    'const KNOWN_TS_FIXES = { alias: (ts, rust) => KNOWN_TS_FIXES.binance(ts, rust) };',
    'export function fixFoo(ts, candidate) { ts.x = candidate.x; }',
    'const KNOWN_TS_FIXES = { binance(ts) { ts.x = results["rust"][0].x; } };',
    'const candidate = results["rust"][0]; const KNOWN_TS_FIXES = { binance(ts) { ts.x = candidate.x; } };',
  ]) assert.ok(fixerLint(source, 'fixture.mjs', true).length, source);
});

test('margin expectation uses native input and preserves unsupported cases', () => {
  const positions = [{ symbol: 'BTCUSDT', side: 'long', marginSize: 0 }, { symbol: 'ETHUSDT', side: 'short', marginSize: 0 }];
  const output = { account: { positions } };
  const records = [
    { type: 'http', status: 200, url: 'https://fapi.binance.com/fapi/v1/symbolConfig', body: '[{"symbol":"BTCUSDT","leverage":20}]' },
    { type: 'ws', text: JSON.stringify({ e: 'ACCOUNT_UPDATE', a: { P: [{ s: 'BTCUSDT', pa: '0.01', ep: '95000', up: '12.5', iw: '0' }] } }) },
    { type: 'ws', text: JSON.stringify({ e: 'ACCOUNT_UPDATE', a: { P: [{ s: 'ETHUSDT', ps: 'SHORT', pa: '-1', ep: '3000', up: '2', iw: '301.72' }] } }) },
  ];
  fixPositionMargins(output, records, 'binance');
  assert.deepEqual(positions.map((p) => p.marginSize), [48.125, 301.72]);
  positions[0].marginSize = 0;
  fixPositionMargins(output, [], 'binance');
  assert.equal(positions[0].marginSize, 0);
  fixPositionMargins(output, [{ type: 'ws', text: JSON.stringify({ topic: 'position', data: [{ symbol: 'BTCUSDT', side: 'Buy', positionIM: '95' }] }) }], 'bybit');
  assert.equal(positions[0].marginSize, 95);
  positions[0].marginSize = 7;
  fixPositionMargins(output, records, 'binance');
  assert.equal(positions[0].marginSize, 7);
});

// Exercise the actual CLI and comparison call sites. Stub only the expensive
// replay subprocess, supplying independently controlled TS and Rust results.
const gate = (venue, isPrivate = false, defect) => {
  const cwd = mkdtempSync(join(tmpdir(), 'safe-cex-gate-'));
  const tape = join(cwd, 'tape.jsonl');
  const records = [{ type: 'meta', private: isPrivate }];
  if (defect?.startsWith('live-smoke')) {
    records[0].liveSmoke = true;
    const commands = ['placeOrders', 'amendOrder', 'cancelOrders', 'cancelSymbolOrders', 'setLeverage'];
    records.push(...commands.map((op) => ({ type: 'command', op })),
      { type: 'live-smoke-verification', ok: true },
      { type: 'live-smoke-result', ok: true, cleanupComplete: true, authoritative: true, stage: 'complete', symbol: 'BTCUSDT', commands });
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
    // The gate must override an explicit parent opt-in for both implementations,
    // while retaining unrelated replay configuration in the child environment.
    process.env.ESBUILD_WORKER_THREADS = '1';
    process.env.HARNESS_TEST_INHERITED = 'retained';
    process.argv = ['node', 'gate.mjs', '--venue', venue, '--tape', ${JSON.stringify(tape)},
      '--books', 'BTCUSDT', '--runs', '2'];
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
