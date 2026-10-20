import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { VENUES } from './venues.mjs';
import { requestKey } from './requests.mjs';

// Exercise the actual CLI and comparison call sites. Stub only the expensive
// replay subprocess, supplying independently controlled TS and Rust results.
const gate = (venue, isPrivate = false, defect) => {
  const cwd = mkdtempSync(join(tmpdir(), 'safe-cex-gate-'));
  const tape = join(cwd, 'tape.jsonl');
  const records = [{ type: 'meta', private: isPrivate }];
  if (defect === 'orderly-missing-result') records.push({ type: 'command', op: 'fetchFills' });
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
    const isOrderly = ${JSON.stringify(VENUES[venue].family === 'orderly')};
    const defect = ${JSON.stringify(defect ?? null)};
    const planKeys = ${JSON.stringify(Object.fromEntries(['ts','rust'].map(impl => [impl, records.filter(record => record.type === 'http' && record.impl === impl).map(record => requestKey(record.method, record.url, record.requestBody, new Set(), VENUES[venue]))])))};
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
          bufferRetainedBytes: 0, wasmBytes: 0, peakBytes: rust ? 15 : 25,
        },
        // Different insertion order must compare equal, including nested rows.
        markets: [rust
          ? { precision: { price: 0.1, amount: 1 }, active: true, symbol: 'BTCUSDT' }
          : { symbol: 'BTCUSDT', active: true, precision: { amount: 1, price: 0.1 } }],
        tickers: [rust ? { last: 100, ask: 0, bid: 0, symbol: 'BTCUSDT' }
                      : { symbol: 'BTCUSDT', bid: 0, ask: 0, last: 100 }],
        books: { BTCUSDT: { bids: [[99, 1]], asks: [[101, 1]] } },
        // Orderly's strict oracle requires exactly one result per taped command.
        // This smoke tape has none; populated command comparisons run below.
        commands: isOrderly || venue === 'kucoin' ? [] : [{ ok: { count: 1, ids: ['one', 'two'] } }],
        ...(isPrivate ? { account: {
          hedged: ['backpack', 'cryptocom'].includes(venue),
          balance: { used: 0, free: 100, total: 100, upnl: 0 },
          orders: [], positions: [], settings: [], fills: [],
        } } : {}),
      };
      if (defect === 'orderly-extra-result') output.commands.push({ ok: [] });
      if (rust && defect === 'comparisons') {
        output.markets[0].precision.price = 0.2;
        output.tickers[0].last = 101;
        output.books.BTCUSDT.bids[0][1] = 2;
        output.commands[0].ok.ids.reverse();
      }
      if (defect?.startsWith('impl-plans')) {
        output.writes = [...planKeys[impl]];
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
      if (rust && defect === 'memory-signed') Object.assign(output.metrics, {
        heapBeforeBytes: 110, heapUsedBytes: 100, heapRetainedBytes: -10,
        buffersBefore: 100, buffersAfter: 120, bufferRetainedBytes: 20,
        wasmBytes: 5, retainedBytes: 15,
      });
      if (defect === 'book-events') output.bookEvents = [[100, 'BTCUSDT', rust ? 'wrong-intermediate-hash' : 'reference-hash']];
      if (rust && defect === 'peak') output.metrics.peakBytes = 26;
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

test('gate CLI rejects negative absolute and net memory counters on later private runs', () => {
  for (const field of ['retainedBytes', 'wasmBytes']) {
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


test('gate CLI preserves signed component deltas in an exact nonnegative net total', () => {
  const result = gate('bybitinverse', true, 'memory-signed');
  assert.equal(result.status, 0, result.stderr + result.stdout);
});

test('gate CLI retains the strict public sampled peak comparison', () => {
  const result = gate('bybitinverse', false, 'peak');
  assert.equal(result.status, 1, result.stderr + result.stdout);
  assert.match(result.stdout, /Rust sampled peak memory is not below TS/);
});


test('Kraken gate rejects wrong intermediate hashes while its final book matches', () => {
  const result=gate('kraken',false,'book-events');
  assert.equal(result.status,1,result.stderr);
  assert.match(result.stdout,/bookEvents:/);
  assert.doesNotMatch(result.stdout,/book BTCUSDT differs/);
});
