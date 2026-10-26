import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { VENUES } from './venues.mjs';
import { fixerLint } from './fixer-lint.mjs';
import { fixPositionMargins } from './margin-expectation.mjs';

test('no TS fixer may read or accept Rust output', () => {
  for (const name of ['ts-oracle.mjs', 'known-ts-fixes.mjs', 'known-ts-fixes-coinbase.mjs', 'known-ts-fixes-orderly.mjs', 'margin-expectation.mjs', 'legacy-fill-oracle.mjs']) {
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
const gate = (venue, isPrivate = false, defect, correctnessOnly = false) => {
  const cwd = mkdtempSync(join(tmpdir(), 'safe-cex-gate-'));
  const tape = join(cwd, 'tape.jsonl');
  const reportPath = join(cwd, 'gate.json');
  const records = [{ type: 'meta', private: isPrivate }];
  const midpoint = defect?.startsWith('midpoint:');
  const midpointIssue = midpoint ? defect.slice('midpoint:'.length) : undefined;
  if (midpoint) {
    const products = ['DEEP-USD', 'DEEP-USDC'].map((product_id) => ({
      product_id, product_type: 'SPOT', future_product_details: null,
      base_currency_id: 'DEEP', quote_currency_id: product_id.slice(5),
      status: 'online', is_disabled: false, trading_disabled: false,
      base_increment: '1', price_increment: '0.00001', base_min_size: '1', base_max_size: '100000000',
      price: '0.01875', volume_24h: '2625844', approximate_quote_24h_volume: '49234.58',
    }));
    records.push({ type: 'http', t: 10, requestOrdinal: 0, method: 'GET', status: 200,
      url: 'https://api.coinbase.com/api/v3/brokerage/market/products?limit=1000',
      body: JSON.stringify({ products, num_products: 2,
        pagination: { has_next: false, has_prev: false, next_cursor: '', prev_cursor: '' } }),
    });
    if (midpointIssue === 'commands') records.push({ type: 'command', t: 20, op: 'comparisonControl' });
    if (isPrivate && ['fills', 'fee'].includes(midpointIssue)) {
      // The legacy oracle must independently corroborate the TS execution.
      // Mutations below change only the candidate price/fee, never this input.
      const url = 'wss://advanced-trade-ws-user.coinbase.com';
      records.push({ type: 'ws', t: 20, url, text: JSON.stringify({ channel: 'user', sequence_num: 0,
        events: [{ type: 'snapshot', orders: [] }] }) });
      records.push({ type: 'ws', t: 30, url, text: JSON.stringify({ channel: 'user', sequence_num: 1,
        events: [{ type: 'update', orders: [{ order_id: 'one', product_id: 'DEEP-USD', order_side: 'BUY',
          status: 'FILLED', cumulative_quantity: '1', avg_price: '0.01875' }] }] }) });
    }

  }
  if (defect?.startsWith('live-smoke')) {
    records[0].liveSmoke = true;
    const commands = ['placeOrders', 'amendOrder', 'cancelOrders', 'cancelSymbolOrders', 'setLeverage'];
    records.push(...commands.map((op) => ({ type: 'command', op })),
      { type: 'live-smoke-verification', ok: true },
      { type: 'live-smoke-result', ok: true, cleanupComplete: true, authoritative: true, stage: 'complete', symbol: 'BTCUSDT', commands });
  }
  if (defect?.startsWith('checkpoint-coverage-'))
    records.push(...Array.from({ length: 8 }, (_, index) => ({ type: index === 2 ? 'command' : 'wait', t: index * 10, ...(index === 2 ? { op: 'getAccount' } : {}) })));
  if (['orderly-missing-result', 'coinbase-missing-result'].includes(defect))
    records.push({ type: 'command', op: 'fetchFills' });
  if (['command-timeout', 'runner-crash'].includes(defect)) records.push({ type: 'command', op: 'getAccount' });
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
    import { readFileSync, writeFileSync } from 'node:fs';
    import { createHash } from 'node:crypto';
    import { syncBuiltinESMExports } from 'node:module';
    import { expectedCheckpointPlan } from ${JSON.stringify(new URL('./tape.mjs', import.meta.url).href)};
    import { venueOf } from ${JSON.stringify(new URL('./venues.mjs', import.meta.url).href)};
    const records = ${JSON.stringify(records)};
    const venue = ${JSON.stringify(venue)};
    const isPrivate = ${JSON.stringify(isPrivate)};
    const correctnessOnly = ${JSON.stringify(correctnessOnly)};
    const strictCommandCount = ${JSON.stringify(['orderly', 'coinbase'].includes(VENUES[venue].family))};
    const defect = ${JSON.stringify(defect ?? null)};
    const midpoint = ${JSON.stringify(Boolean(midpoint))};
    const midpointIssue = ${JSON.stringify(midpointIssue ?? null)};
    const midpointOutputs = {};
    const { compareCoinbaseSpotMidpoints } = await import(${JSON.stringify(new URL('./coinbase-spot-midpoints.mjs', import.meta.url).href)});
    // The gate must override an explicit parent opt-in for both implementations,
    // while retaining unrelated replay configuration in the child environment.
    process.env.ESBUILD_WORKER_THREADS = '1';
    process.env.HARNESS_TEST_INHERITED = 'retained';
    process.argv = ['node', 'gate.mjs', '--venue', venue, '--tape', ${JSON.stringify(tape)},
      '--books', midpoint ? 'DEEP-USD' : 'BTCUSDT',
      ...(correctnessOnly ? ['--correctness-only'] : ['--runs', isPrivate ? '2' : '7']), '--report', ${JSON.stringify(reportPath)}];
    const spawned = new Set();
    const calls = [];
    childProcess.spawnSync = (_executable, args, options) => {
      assert.equal(options.env.ESBUILD_WORKER_THREADS, '0');
      assert.equal(options.env.HARNESS_TEST_INHERITED, 'retained');
      const out = args[args.indexOf('--out') + 1];
      const impl = args[args.indexOf('--impl') + 1];
      spawned.add(impl);
      calls.push([args.includes('--performance') ? 'performance' : 'correctness', impl]);
      const rust = impl === 'rust';
      if (rust && defect === 'command-timeout') return { status: 1, stderr: 'ReplayTimeout [Error]: replay timeout: command getAccount\\n' };
      if (rust && defect === 'runner-crash') return { status: 1, stderr: 'Error: loader failure\\n' };
      const emptyAccount = () => ({
        hedged: ['backpack', 'cryptocom'].includes(venue),
        balance: { used: 0, free: 100, total: 100, upnl: 0 },
        orders: [], positions: [], settings: [], fills: [], flatPositionPnl: [],
      });
      const output = {
        impl, frames: 1, unserved: 0, missingWrites: 0, unusedWrites: 0,
        invalidSignatures: 0, protocolErrors: [], wsRequests: [], subscriptions: [], requests: [], writes: [], writeCount: 0,
        metrics: {
          baselineKind: 'idle-loaded', measuredMode: args.includes('--performance') ? 'performance' : 'correctness',
          cpuMs: rust ? 1 : 2, steadyCpuMs: rust ? 1 : 2, processCpuMs: rust ? 1 : 2,
          idleLoadCpuMs: 0, idleLoadProcessCpuMs: 0, idleLoadWallMs: 0,
          runtimeCpuMs: rust ? 1 : 2, runtimeProcessCpuMs: rust ? 1 : 2,
          runtimeWallMs: 1, wallMs: 1,
          heapBeforeBytes: 100, heapUsedBytes: rust ? 110 : 120, buffersBefore: 100, buffersAfter: 100,
          retainedBytes: rust ? 10 : 20, heapRetainedBytes: rust ? 10 : 20,
          bufferRetainedBytes: 0, wasmBytes: 0, wasmBeforeBytes: 0, wasmRetainedBytes: 0,
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
        checkpoints: args.includes('--performance') ? [] : expectedCheckpointPlan(records, venueOf(venue), impl).map((point) => ({ ...point, markets: [], tickers: [], books: {}, ...(isPrivate ? { account: emptyAccount() } : {}) })),
        transportReceipt: {},
        ...(isPrivate ? { account: emptyAccount() } : {}),
      };
      if (midpoint) {
        output.markets = ['DEEP-USD', 'DEEP-USDC'].map((symbol) => ({
          id: symbol, symbol, base: 'DEEP', quote: symbol.slice(5), active: true,
          isSpot: true, isInverse: false, isExpiring: false, contractSize: 1,
          precision: { amount: 1, price: 0.00001 },
        }));
        output.tickers = ['DEEP-USD', 'DEEP-USDC'].map((symbol) => ({ symbol,
          last: 0.01875, volume: 2625844, bid: 0.01875,
          quoteVolume: rust || isPrivate || midpointIssue === 'equal' ? 49234.58 : 49234.57,
        }));
        output.books = { 'DEEP-USD': { bids: [[0.01874, 1]], asks: [[0.01876, 1]] } };
        if (midpointIssue === 'commands') output.commands = [{ ok: { ids: rust ? ['two', 'one'] : ['one', 'two'] } }];
        if (rust) {
          if (midpointIssue === 'price') output.tickers[0].last = 0.01876;
          if (midpointIssue === 'volume') output.tickers[0].volume--;
          if (midpointIssue === 'bid') output.tickers[0].bid = 0.01874;
          if (midpointIssue === 'book') output.books['DEEP-USD'].bids[0][1] = 2;
          if (midpointIssue === 'request') output.requests = ['GET https://example.test/unexpected?'];
          if (midpointIssue === 'subscription') output.subscriptions = ['WS public unexpected'];
          if (midpointIssue === 'wsrequest') output.wsRequests = ['unexpected native request'];
          if (midpointIssue === 'account') output.account = emptyAccount();
        }
        if (isPrivate) {
          if (midpointIssue === 'balance' && rust) output.account.balance.total++;
          if (midpointIssue === 'orders') output.account.orders = [{ id: 'one', clientOrderId: null, symbol: 'DEEP-USD',
            status: 'open', type: 'limit', side: 'buy', price: rust ? 0.01876 : 0.01875, amount: 1,
            filled: 0, remaining: 1, timeInForce: null, reduceOnly: false, close: false,
            trigger: null, takeProfit: null, stopLoss: null, conditionalOrderPrice: null, parentId: null,
            takeProfitTrigger: null, stopLossTrigger: null, rejectReason: null }];
          if (midpointIssue === 'positions') output.account.positions = [{ symbol: 'DEEP-USD', side: 'long',
            entryPrice: rust ? 0.01876 : 0.01875, contracts: 1, notional: 0.01875, leverage: 1,
            unrealizedPnl: 0, liquidationPrice: null, marginType: 'cross', positionMode: 'oneway', marginSize: 1, realizedPnl: 0 }];
          if (['fills', 'fee'].includes(midpointIssue)) output.account.fills = [{ id: 'one', symbol: 'DEEP-USD', side: 'buy',
            amount: 1, price: rust && midpointIssue === 'fills' ? 0.01876 : 0.01875,
            fee: rust && midpointIssue === 'fee' ? 0.02 : 0.01, timestamp: 1710000000000, executionId: 'exec-one' }];
        }
        if (midpointIssue === 'good') {
          midpointOutputs[impl] = structuredClone(output);
          if (midpointOutputs.ts && midpointOutputs.rust)
            assert.equal(compareCoinbaseSpotMidpoints({ venue, records, ...midpointOutputs }).equivalent, true,
              'control must qualify for the independent standalone midpoint proof');
        }
        if (rust && midpointIssue === 'later' && args.includes('--performance') && out.endsWith('-6.json'))
          output.tickers[0].quoteVolume = 49234.59;
        for (const point of output.checkpoints) {
          point.markets = structuredClone(output.markets);
          point.tickers = structuredClone(output.tickers);
          point.books = structuredClone(output.books);
          if (output.account) point.account = structuredClone(output.account);
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
        output.tickers[0].ask = 102;
        output.books.BTCUSDT.bids[0][1] = 2;
        output.commands[0].ok.ids.reverse();
      }
      if (defect?.startsWith('failure-order:')) {
        const symbols = Array.from({ length: 43 }, (_, index) => 'FAIL' + String(index).padStart(2, '0'));
        if (defect.endsWith(':reverse')) symbols.reverse();
        for (const symbol of symbols)
          output.books[symbol] = { bids: [[99, rust ? 2 : 1]], asks: [[101, 1]] };
        if (rust) {
          output.protocolErrors = ['duplicate evidence', 'duplicate evidence'];
          if (args.includes('--performance'))
            output.metrics.processCpuMs = output.metrics.runtimeProcessCpuMs = 3;
        }
      }
      if (defect === 'duplicate-symbols') output.markets.push({ ...output.markets[0] });
      if (defect === 'filtered-duplicates') output.duplicateSymbols = ['duplicate markets symbol SPOT at rows 0 and 1'];
      if (rust && !args.includes('--performance') && defect === 'checkpoint-transient') output.checkpoints[0].tickers.push({ symbol: 'BTCUSDT', last: 999 });
      if (defect?.startsWith('checkpoint-coverage-') && !args.includes('--performance')) {
        if (defect === 'checkpoint-coverage-drop') output.checkpoints.splice(1, 1);
        if (defect === 'checkpoint-coverage-drop-command') output.checkpoints = output.checkpoints.filter((point) => point.ordinal !== 3);
        if (defect === 'checkpoint-coverage-startup-only') output.checkpoints.splice(1);
        if (defect === 'checkpoint-coverage-duplicate') output.checkpoints[1] = structuredClone(output.checkpoints[0]);
        if (defect === 'checkpoint-coverage-reorder') [output.checkpoints[1], output.checkpoints[2]] = [output.checkpoints[2], output.checkpoints[1]];
        if (defect === 'checkpoint-coverage-retime') output.checkpoints[1].t++;
        if (defect === 'checkpoint-coverage-erase-account') for (const point of output.checkpoints) delete point.account;
        if (defect === 'checkpoint-coverage-erase-books') for (const point of output.checkpoints) delete point.books;
        if (defect === 'checkpoint-coverage-erase-final-books') delete output.books;
        if (defect.startsWith('checkpoint-coverage-erase-section:'))
          for (const point of output.checkpoints) delete point.account[defect.split(':')[1]];
        if (defect.startsWith('checkpoint-coverage-erase-balance:'))
          for (const point of output.checkpoints) delete point.account.balance[defect.split(':')[1]];
        if (defect.startsWith('checkpoint-coverage-erase-row:') || defect.startsWith('checkpoint-coverage-erase-final-row:')) {
          const [, section, field] = defect.split(':');
          const complete = {
            orders: { id: 'order', symbol: 'BTCUSDT', status: 'open', type: 'limit', side: 'buy', price: 100, amount: 1,
              filled: 0, remaining: 1, reduceOnly: false, close: false, trigger: null, rejectReason: null, timeInForce: 'GTC',
              clientOrderId: null, takeProfit: null, stopLoss: null, conditionalOrderPrice: null, parentId: null, takeProfitTrigger: null, stopLossTrigger: null },
            positions: { symbol: 'BTCUSDT', side: 'long', entryPrice: 100, contracts: 1, notional: 100, leverage: 10,
              unrealizedPnl: 0, liquidationPrice: null, marginType: 'cross', positionMode: null, marginSize: 10, realizedPnl: 0 },
            settings: { symbol: 'BTCUSDT', marginType: 'cross', leverage: 10 },
            fills: { id: 'order', symbol: 'BTCUSDT', side: 'buy', amount: 1, price: 100, fee: null, timestamp: null, executionId: null },
            flatPositionPnl: { symbol: 'BTCUSDT', side: 'long', positionMode: null, realizedPnl: 0 },
          };
          for (const point of defect.startsWith('checkpoint-coverage-erase-final-row:') ? [output] : output.checkpoints) {
            point.account[section] = [structuredClone(complete[section])];
            delete point.account[section][0][field];
          }
        }
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
      if (rust && args.includes('--performance')) {
        if (defect === 'process-cpu') output.metrics.processCpuMs = output.metrics.runtimeProcessCpuMs = 3;
        if (defect === 'marginal-cpu') output.metrics.cpuMs = output.metrics.runtimeCpuMs = 3.001; // TS 2 ms: just past the sub-10 ms 1 ms allowance
        if (defect === 'late-invalid-cpu' && out.endsWith('-6.json')) output.metrics.cpuMs = null;
        if (defect === 'incomplete-perf' && out.endsWith('-6.json')) output.frames = 0;
        if (defect === 'dropped-perf-output' && out.endsWith('-6.json')) output.tickers[0].last = 0;
        if (defect === 'changed-perf-transport' && out.endsWith('-6.json')) output.transportReceipt = { changed: true };
      }
      // A later private run must reject invalid individual memory components
      // even though its aggregate and informational performance otherwise pass.
      if (rust && defect?.startsWith('memory:') && out.endsWith('rust-audit-1.json'))
        output.metrics[defect.slice('memory:'.length)] = -1;
      writeFileSync(out, JSON.stringify(output));
      return { status: 0, stderr: '' };
    };
    syncBuiltinESMExports();
    await import(${JSON.stringify(new URL('./gate.mjs', import.meta.url).href)});
    if (defect?.startsWith('recorded-')) {
      const original = JSON.parse(readFileSync(${JSON.stringify(reportPath)}, 'utf8'));
      const raw = original.artifacts.map(({ impl, mode, run, path }) => {
        let bytes = readFileSync(path, 'utf8');
        if (defect === 'recorded-corrupt' && impl === 'rust' && mode === 'performance' && run === 8) {
          const changed = JSON.parse(bytes); changed.tickers[0].last = 7;
          bytes = JSON.stringify(changed); writeFileSync(path, bytes);
        }
        return { impl, mode, run, bytes, sha256: createHash('sha256').update(bytes).digest('hex') };
      });
      if (defect === 'recorded-missing') raw.pop();
      const recordedPath = ${JSON.stringify(join(cwd, 'recorded.json'))};
      const offlinePath = ${JSON.stringify(join(cwd, 'offline.json'))};
      writeFileSync(recordedPath, JSON.stringify({ report: original, raw }));
      childProcess.spawnSync = () => { throw new Error('FORBIDDEN REPLAY FALLBACK'); };
      syncBuiltinESMExports();
      process.argv = ['node', 'gate.mjs', '--venue', venue, '--tape', ${JSON.stringify(tape)},
        '--books', 'BTCUSDT', '--runs', '9', '--report', offlinePath, '--recorded-evidence', recordedPath];
      await import(${JSON.stringify(new URL('./gate.mjs', import.meta.url).href + '?recorded-control')});
      const { compareRecordedReport } = await import(${JSON.stringify(new URL('./recorded-replay.mjs', import.meta.url).href)});
      assert.deepEqual(compareRecordedReport(original, JSON.parse(readFileSync(offlinePath, 'utf8'))), []);
    }
    assert.deepEqual([...spawned].sort(), ['rust', 'ts']);
    if (correctnessOnly) assert.deepEqual(calls, [['correctness', 'ts'], ['correctness', 'rust']]);
    else if (!isPrivate) {
      assert.deepEqual(calls.slice(0, 2), [['correctness', 'ts'], ['correctness', 'rust']]);
      // Stub CPU is a few ms, so the small-window policy extends seven pairs to nine.
      assert.equal(calls.length, 20);
      for (let run = 0; run < 9; run++)
        assert.deepEqual(calls.slice(2 + run * 2, 4 + run * 2), (run % 2 ? ['rust', 'ts'] : ['ts', 'rust']).map((impl) => ['performance', impl]));
    } else assert.equal(calls.length, 4);
  `;
  try {
    const runner = join(cwd, 'run.mjs');
    writeFileSync(runner, script);
    const result = spawnSync(process.execPath, [runner], {
      cwd,
      encoding: 'utf8',
      timeout: 10000,
    });
    return { ...result, report: existsSync(reportPath) ? JSON.parse(readFileSync(reportPath, 'utf8')) : null };
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
};

test('PR correctness mode executes exactly one audited pair and retains all output refusals', () => {
  for (const isPrivate of [false, true]) {
    const result = gate('bybitinverse', isPrivate, 'process-cpu', true);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.report.correctnessOnly, true);
    assert.equal(result.report.runs, 0); assert.equal(result.report.correctnessRuns, 1);
    assert.equal(result.report.artifacts.length, 2);
    assert.deepEqual(result.report.coverage, { correctness: 'complete', performance: 'not-run' });
    const bad = gate('bybitinverse', isPrivate, 'comparisons', true);
    assert.equal(bad.status, 1, bad.stderr);
    assert.match(bad.stdout, /ticker BTCUSDT.last:/);
  }
});

test('gate CLI orders complete failures deterministically before report and console truncation', () => {
  const expected = [
    ...Array.from({ length: 43 }, (_, index) => `book FAIL${String(index).padStart(2, '0')}.bids[0][1]: ts=1 rust=2`),
    'performance process CPU: Rust must be at least 10% below TS (ts=2, rust=3)',
    'rust: duplicate evidence',
    'rust: duplicate evidence',
  ];
  const results = ['forward', 'reverse'].map((order) => gate('bybitinverse', false, `failure-order:${order}`));
  for (const result of results) {
    assert.equal(result.status, 1, result.stderr);
    assert.equal(result.report.status, 'FAIL');
    assert.equal(result.report.failures.filter((failure) => failure === 'rust: duplicate evidence').length, 2);
    assert.deepEqual([...result.report.failures].sort(), expected, 'sorting must preserve the complete failure multiset');
    assert.deepEqual(result.report.failures, expected);
    assert.deepEqual(result.stdout.split('\nFAIL (46)\n')[1]?.trimEnd().split('\n'), expected.slice(0, 40).map((failure) => `  ${failure}`));
  }
  const semanticReport = (report) => ({
    schema: report.schema, venue: report.venue, tapeSha256: report.tapeSha256,
    private: report.private, books: report.books, runs: report.runs, correctnessRuns: report.correctnessRuns,
    requiredImprovement: report.requiredImprovement, status: report.status,
    summary: report.summary, samples: report.samples, fixers: report.fixers,
    // Each child has a different temporary tape path and PID in raw paths.
    tapeName: report.tape.split('/').at(-1),
    artifactSlots: report.artifacts.map(({ impl, mode, run }) => ({ impl, mode, run })),
  });
  assert.deepEqual(semanticReport(results[0].report), semanticReport(results[1].report));
});

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
  for (const difference of ['market BTCUSDT.precision.price:', 'ticker BTCUSDT.last:', 'ticker BTCUSDT.ask:', 'book BTCUSDT.bids[0][1]:', 'command results[0].ok.ids[0]:'])
    assert.ok(result.stdout.includes(difference), `${difference}\n${result.stdout}`);
  assert.doesNotMatch(result.stderr, /ReferenceError/);
});

test('gate CLI rejects duplicates and transient checkpoint divergences', () => {
  for (const [defect, message] of [['duplicate-symbols', /duplicate symbol BTCUSDT/], ['filtered-duplicates', /duplicate markets symbol SPOT/], ['checkpoint-transient', /checkpoint 0 tickers/]]) {
    const result = gate('bybitinverse', false, defect);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stdout, message);
  }
});

test('gate CLI independently enforces process CPU, ten percent margin and every performance sample', () => {
  for (const [defect, message] of [
    ['process-cpu', /performance process CPU/], ['marginal-cpu', /performance main-thread CPU/],
    ['late-invalid-cpu', /rust performance run 7: invalid cpuMs/], ['incomplete-perf', /incomplete replay workload/],
    ['dropped-perf-output', /rust performance run 7 tickers/], ['changed-perf-transport', /transport workload differs/],
  ]) {
    const result = gate('bybitinverse', false, defect);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stdout, message);
  }
});

test('gate CLI reports command timeouts as incomplete failures and keeps runner crashes fatal', () => {
  const timeout = gate('bybitinverse', true, 'command-timeout');
  assert.equal(timeout.status, 1);
  assert.match(timeout.stdout, /FAIL \(incomplete replay\)/);
  assert.match(timeout.stdout, /command getAccount did not complete/);
  const crash = gate('bybitinverse', true, 'runner-crash');
  assert.equal(crash.status, 1);
  assert.match(crash.stderr, /rust replay failed/);
  assert.doesNotMatch(crash.stdout, /incomplete replay/);
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
    const category = field === 'wasmBytes' ? 'invalid' : 'inconsistent';
    assert.ok(result.stdout.includes(`rust: ${category} ${field} (-1`), result.stdout);
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


test('gate CLI independently enforces checkpoint coverage when both implementations omit or alter the same evidence', () => {
  for (const defect of ['drop', 'drop-command', 'startup-only', 'duplicate', 'reorder', 'retime', 'erase-account', 'erase-books']) {
    const result = gate('binance', true, `checkpoint-coverage-${defect}`);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /checkpoint coverage ts(?: point \d+| final)?:/, result.stderr);
    assert.match(result.stdout, /checkpoint coverage rust(?: point \d+| final)?:/);
  }
});

test('gate CLI rejects shared private financial-section and widened-row omissions before final convergence', () => {
  const defects = [
    ...['orders', 'positions', 'settings', 'fills', 'flatPositionPnl', 'hedged', 'balance'].map((field) => 'erase-section:' + field),
    ...['used', 'free', 'total', 'upnl'].map((field) => 'erase-balance:' + field),
    ...['clientOrderId', 'takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId', 'takeProfitTrigger', 'stopLossTrigger'].map((field) => 'erase-row:orders:' + field),
    'erase-row:positions:realizedPnl',
    ...['price', 'fee', 'timestamp', 'executionId'].map((field) => 'erase-row:fills:' + field),
    'erase-row:flatPositionPnl:realizedPnl',
  ];
  for (const defect of defects) {
    const result = gate('binance', true, 'checkpoint-coverage-' + defect);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /checkpoint coverage ts point \d+:/, defect);
    assert.match(result.stdout, /checkpoint coverage rust point \d+:/, defect);
  }
});

test('gate CLI rejects shared base normalized row-key erasure at checkpoints and final output', () => {
  const fields = {
    orders: ['id', 'symbol', 'status', 'type', 'side', 'price', 'amount', 'filled', 'remaining', 'reduceOnly', 'close', 'trigger', 'rejectReason', 'timeInForce'],
    positions: ['symbol', 'side', 'entryPrice', 'contracts', 'notional', 'leverage', 'unrealizedPnl', 'liquidationPrice', 'marginType', 'positionMode', 'marginSize'],
    settings: ['symbol', 'marginType', 'leverage'],
    fills: ['id', 'symbol', 'side', 'amount'],
  };
  for (const [section, keys] of Object.entries(fields)) for (const field of keys) {
    const result = gate('binance', true, 'checkpoint-coverage-erase-row:' + section + ':' + field);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /checkpoint coverage ts point \d+:/, `${section}.${field}`);
    assert.match(result.stdout, /checkpoint coverage rust point \d+:/, `${section}.${field}`);
  }
  for (const [section, field] of [['orders', 'amount'], ['positions', 'contracts'], ['settings', 'leverage'], ['fills', 'amount']]) {
    const result = gate('binance', true, 'checkpoint-coverage-erase-final-row:' + section + ':' + field);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /checkpoint coverage ts final:/, `${section}.${field}`);
    assert.match(result.stdout, /checkpoint coverage rust final:/, `${section}.${field}`);
  }
});


test('Coinbase midpoint proofs cannot excuse strict raw or terminal ticker differences', () => {
  for (const venue of ['coinbase', 'coinbasespot']) {
    const result = gate(venue, false, 'midpoint:good');
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, /checkpoint \d+ tickers\[0\]\.quoteVolume:/);
    assert.match(result.stdout, /ticker DEEP-USD\.quoteVolume:/);
    assert.doesNotMatch(result.stdout, /public spot decimal midpoints/);
  }
});

test('equal Coinbase midpoint values pass the complete seven-pair CLI control', () => {
  for (const venue of ['coinbase', 'coinbasespot']) {
    const result = gate(venue, false, 'midpoint:equal');
    assert.equal(result.status, 0, result.stderr + result.stdout);
    assert.match(result.stdout, /\nPASS\n/);
  }
});

test('Coinbase midpoint inputs retain every exact ticker field', () => {
  for (const [issue, field] of [['price', 'last'], ['volume', 'volume'], ['bid', 'bid']]) {
    const result = gate('coinbase', false, 'midpoint:' + issue);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, new RegExp('ticker DEEP-USD\\.' + field + ':'));
  }
});

test('Coinbase midpoint differences preserve independent book and transport failures', () => {
  for (const [issue, diagnostic] of [
    ['book', /book DEEP-USD/], ['request', /requests/], ['subscription', /subscriptions/], ['wsrequest', /websocket requests/],
    ['commands', /command results/], ['account', /account presence/],
  ]) {
    const result = gate('coinbase', false, 'midpoint:' + issue);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, diagnostic);
    assert.match(result.stdout, /quoteVolume:/);
  }
});

test('Coinbase private financial controls remain exact at equal midpoint values', () => {
  for (const issue of ['balance', 'orders', 'positions', 'fills', 'fee']) {
    const result = gate('coinbase', true, 'midpoint:' + issue);
    assert.equal(result.status, 1, result.stderr + result.stdout);
    assert.match(result.stdout, new RegExp('account[ .]' + (issue === 'fee' ? 'fills' : issue)));
    assert.doesNotMatch(result.stdout, /ticker DEEP-USD\.quoteVolume:/);
  }
});

test('Coinbase midpoint changes in the last measured run fail own-audit validation', () => {
  const result = gate('coinbase', false, 'midpoint:later');
  assert.equal(result.status, 1, result.stderr + result.stdout);
  assert.match(result.stdout, /rust performance run 7.*quoteVolume/);
});


test('offline gate reuses every strict validator and never spawns another replay', () => {
  const original = gate('bybitinverse', false, 'recorded-control');
  assert.equal(original.status, 0, original.stderr);
  assert.equal(original.stdout.split('\nPASS\n').length, 3, 'both original and offline validation pass');
  const corrupt = gate('bybitinverse', false, 'recorded-corrupt');
  assert.equal(corrupt.status, 1, corrupt.stderr);
  assert.match(corrupt.stdout, /performance run 9.*ticker|performance run 9.*tickers/);
  const missing = gate('bybitinverse', false, 'recorded-missing');
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /recorded public case\/inventory/);
  for (const result of [original, corrupt, missing]) assert.doesNotMatch(result.stderr, /FORBIDDEN REPLAY FALLBACK/);
});
