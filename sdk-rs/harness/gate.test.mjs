import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { VENUES } from './venues.mjs';

// Exercise the actual CLI and comparison call sites. Stub only the expensive
// replay subprocess, supplying independently controlled TS and Rust results.
const gate = (venue, isPrivate = false, defect) => {
  const cwd = mkdtempSync(join(tmpdir(), 'safe-cex-gate-'));
  const tape = join(cwd, 'tape.jsonl');
  writeFileSync(tape, JSON.stringify({ type: 'meta', private: isPrivate }) + '\n');
  const script = `
    import assert from 'node:assert/strict';
    import childProcess from 'node:child_process';
    import { writeFileSync } from 'node:fs';
    import { syncBuiltinESMExports } from 'node:module';
    const venue = ${JSON.stringify(venue)};
    const isPrivate = ${JSON.stringify(isPrivate)};
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
        invalidSignatures: 0, protocolErrors: [], wsRequests: [], subscriptions: [],
        metrics: {
          cpuMs: rust ? 1 : 2, steadyCpuMs: rust ? 1 : 2, processCpuMs: rust ? 1 : 2,
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
        commands: [{ ok: { count: 1, ids: ['one', 'two'] } }],
        ...(isPrivate ? { account: {
          hedged: ['backpack', 'cryptocom'].includes(venue),
          balance: { used: 0, free: 100, total: 100, upnl: 0 },
          orders: [], positions: [], settings: [], fills: [],
        } } : {}),
      };
      if (rust && defect === 'comparisons') {
        output.markets[0].precision.price = 0.2;
        output.tickers[0].last = 101;
        output.books.BTCUSDT.bids[0][1] = 2;
        output.commands[0].ok.ids.reverse();
      }
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

test('gate CLI validates every memory component on later private runs', () => {
  for (const field of ['retainedBytes', 'heapRetainedBytes', 'bufferRetainedBytes', 'wasmBytes']) {
    const result = gate('bybitinverse', true, `memory:${field}`);
    assert.equal(result.status, 1, result.stderr);
    assert.ok(result.stdout.includes(`rust: invalid ${field} (-1); baseline is invalid`), result.stdout);
  }
});
