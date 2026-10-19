// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { subscriptionKeys } from './requests.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    runs: { type: 'string', default: '3' },
    'no-egress': { type: 'boolean', default: false },
  },
});
const outDir = 'sdk-rs/harness/out';
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push('sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', values.venue, '--tape', values.tape, '--books', values.books, '--out', out);
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`${impl} replay failed:\n${result.stderr}`);
  return JSON.parse(readFileSync(out, 'utf8'));
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

const differences = [];
const compare = (label, a, b) => {
  const keyed = (rows) => new Map(rows.map((row) => [row.symbol, row]));
  const ta = keyed(a);
  const tb = keyed(b);
  for (const symbol of new Set([...ta.keys(), ...tb.keys()])) {
    const left = JSON.stringify(ta.get(symbol));
    const right = JSON.stringify(tb.get(symbol));
    if (left !== right) differences.push(`${label} ${symbol}: ts=${left} rust=${right}`);
  }
};

const runs = Number(values.runs);
const results = { ts: [], rust: [] };
for (let run = 0; run < runs; run++) {
  for (const impl of ['ts', 'rust']) results[impl].push(replay(impl, run));
}
const [ts, rust] = [results.ts[0], results.rust[0]];

for (let run = 0; run < runs; run++) {
const ts = results.ts[run];
const rust = results.rust[run];
if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
// Every request must match a taped one: an unserved request sent different parameters.
for (const impl of ['ts', 'rust']) {
  const unserved = results[impl][run].unserved;
  if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
}
const isPrivate = Boolean(ts.account);
// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
const KNOWN_TS_FIXES = {
  hyperliquid() {
    // Native command regressions pin these deliberate TS fixes:
    // - Validate every cancel acknowledgement before cancel+replace continues.
    // - Resolve queued leverage/margin settings when the step executes.
    // - Keep each HIP-3 bracket leg's own metadata in pending/optimistic state;
    //   TS assigns entry metadata to every returned oid in placeLimitOrder.
    // - History keeps distinct tid executions and signed rebates, uses vault
    //   identity and caller ranges, and rejects failed or saturated reads.
    // - Execution limits sort raw milliseconds before serializing seconds.
    // - Income honors the requested kind; fills support all-symbol queries.
    // The tape compares converged state: no account values are rewritten here.
  },
  binance(account, rustAccount) {
  for (const position of account.positions) {
    const fixed = rustAccount.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
    if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
  }
  },
  bybit(account, rustAccount, tsResult) {
    // TS's optional API-key/VIP UI metadata probe has no engine-frame output.
    // Rust uses the conservative non-VIP batch limit; this is an intentional
    // metadata omission, not a TS bug. All trading requests remain exact.
    tsResult.requests = tsResult.requests.filter((key) => key !== 'GET https://api.bybit.com/v5/user/query-api?');
    // TS drops EC_PostOnlyWillTakeLiquidity; Rust reports PostOnly.
    for (const order of account.orders) {
      const fixed = rustAccount.orders.find((row) => row.id === order.id);
      if (order.rejectReason === null && fixed?.rejectReason === 'PostOnly' && order.status === 'canceled') order.rejectReason = 'PostOnly';
    }
    for (const position of account.positions) {
      const fixed = rustAccount.positions.find((row) => row.symbol === position.symbol && row.side === position.side);
      if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
    }
    // Rust subscribes execution/wallet for authoritative fills/balance; TS only
    // requests order/position. Require these extra feeds rather than ignoring them.
    const orderSuffix = ' {"args":["order"],"op":"subscribe"}';
    const feeds = new Set(tsResult.subscriptions);
    for (const key of tsResult.subscriptions) {
      if (!key.endsWith(orderSuffix)) continue;
      const url = key.slice(3, -orderSuffix.length);
      if (new URL(url).pathname !== '/v5/private') continue;
      for (const required of subscriptionKeys(url, { op: 'subscribe', args: ['execution', 'wallet'] })) feeds.add(required);
    }
    tsResult.subscriptions = [...feeds].sort();
    // Other native regressions pin intentional Bybit fixes without rewrites:
    // split quantities use maximum-size lots and the exact remainder;
    // ambiguous failed batch writes never fall back to duplicate single creates;
    // REST ticker seeds install before the initial socket snapshot;
    // Partial-stop amend keeps its native ID when no mark price is available.
  },
};
if (isPrivate) {
  KNOWN_TS_FIXES[values.venue]?.(ts.account, rust.account, ts);
  for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
    const left = JSON.stringify(ts.account[field]);
    const right = JSON.stringify(rust.account[field]);
    if (left !== right) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
  }
  const left = JSON.stringify(ts.commands);
  const right = JSON.stringify(rust.commands);
  if (left !== right) differences.push(`command results: ts=${left} rust=${right}`);
}
for (const field of ['requests', 'subscriptions']) {
  const missing = ts[field].filter((key) => !rust[field].includes(key));
  const extra = rust[field].filter((key) => !ts[field].includes(key));
  if (missing.length || extra.length) differences.push(`${field} run ${run + 1}: missing in Rust=${JSON.stringify(missing)}, extra in Rust=${JSON.stringify(extra)}`);
}
compare('market', ts.markets, rust.markets);
compare('ticker', ts.tickers, rust.tickers);
for (const symbol of new Set([...Object.keys(ts.books), ...Object.keys(rust.books)])) {
  if (JSON.stringify(ts.books[symbol]) !== JSON.stringify(rust.books[symbol])) {
    differences.push(`book ${symbol} differs`);
  }
}

}

const summary = (impl) => ({
  cpuMs: median(results[impl].map((r) => r.metrics.cpuMs)),
  steadyCpuMs: median(results[impl].map((r) => r.metrics.steadyCpuMs)),
  processCpuMs: median(results[impl].map((r) => r.metrics.processCpuMs)),
  retainedMB: median(results[impl].map((r) => r.metrics.retainedBytes)) / 1e6,
  wasmMB: median(results[impl].map((r) => r.metrics.wasmBytes ?? 0)) / 1e6,
});
const t = summary('ts');
const r = summary('rust');
console.log(`frames ${ts.frames}, markets ${ts.markets.length}, tickers ${ts.tickers.length}, books ${Object.keys(ts.books).length}`);
const row = (label, a, b, unit = 'ms') =>
  console.log(`${label.padEnd(16)} ts ${a.toFixed(1).padStart(8)} ${unit}   rust ${b.toFixed(1).padStart(8)} ${unit}   ${(a / b).toFixed(2)}x`);
row('main-thread cpu', t.cpuMs, r.cpuMs);
row('steady cpu', t.steadyCpuMs, r.steadyCpuMs);
row('process cpu', t.processCpuMs, r.processCpuMs);
row('retained memory', t.retainedMB, r.retainedMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB)`);
if (ts.account) console.log('(private tape: paced in real time, so it gates correctness only)');

const failures = [...differences];
// A private tape is paced in real time, so it gates correctness only.
if (!ts.account) {
  if (!(r.cpuMs < t.cpuMs)) failures.push('Rust main-thread CPU is not below TS');
  if (!(r.steadyCpuMs < t.steadyCpuMs)) failures.push('Rust steady-state CPU is not below TS');
  if (!(r.retainedMB < t.retainedMB)) failures.push('Rust retained memory is not below TS');
}
if (failures.length) {
  console.log(`FAIL (${failures.length})`);
  for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
  process.exit(1);
}
console.log('PASS');
