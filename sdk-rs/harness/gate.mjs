// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    runs: { type: 'string', default: '3' },
    'no-egress': { type: 'boolean', default: false },
  },
});
const outDir = `sdk-rs/harness/out/${values.venue}-${process.pid}`;
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push(
    'sdk-rs/harness/replay.mjs',
    '--impl',
    impl,
    '--venue',
    values.venue,
    '--tape',
    values.tape,
    '--books',
    values.books,
    '--out',
    out,
  );
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

if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
// Every request must match a taped one: an unserved request sent different parameters.
for (const impl of ['ts', 'rust']) {
  const unserved = results[impl][0].unserved;
  if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
  const unused = results[impl][0].unusedWrites;
  if (unused) differences.push(`${impl}: ${unused} recorded mutation(s) were never sent`);
}
if (JSON.stringify(ts.wsRequests) !== JSON.stringify(rust.wsRequests))
  differences.push(
    `websocket requests differ: ts=${JSON.stringify(ts.wsRequests)} rust=${JSON.stringify(rust.wsRequests)}`,
  );
const isPrivate = Boolean(ts.account);
// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
// BloFin normal placement batches partition by instrument and margin, respecting
// venue error 152007; legacy TS chunks incompatible rows together. Cancel-replace
// verifies final canceled-order detail and aborts on execution progress/unknown
// terminal outcomes, while TS can duplicate fills with stale replacement size.
// BloFin keeps terminal status monotonic on late live/partial frames and rejects
// replacement/merging of terminal rows; TS can reopen/resurrect these rows.
// BloFin cancel-all now filters terminal store rows in both implementations;
// TS formerly resent filled/canceled orders to the venue. Terminal statuses
// remain in the store until reconciliation, matching the shared engine contract.
// BloFin history walks older fill windows despite ignored begin/end, never
// fabricates executions from order totals, and rejects incomplete income walks.
// Valid documented query payloads need no result rewrite for these TS fixes.
// BloFin trading validates per-row cancels and requested identities even under
// code 0; rejects rounded-to-zero partial stops and unsupported TIF; and splits
// in integral lots without losing quantity. Valid command tapes need no rewrite.
// BloFin also rejects malformed successful margin/position-mode responses
// instead of silently choosing cross/one-way, and clears used/upnl after an
// empty position push. Valid payloads need no mode normalization.
// BloFin books5 formerly overwrote N array slots, corrupting ordering when the
// top moved. Both TS and Rust now merge by price boundary; full books are still
// compared without normalization or exclusions for this corrected TS defect.
//  - Bybit: TS drops the reject reason of a post-only order cancelled for taking
//    liquidity (`EC_PostOnlyWillTakeLiquidity`); Rust reports it as PostOnly.
//  - Bybit: TS can round split quantities away from the requested total;
//    Rust uses maximum-size lots and the exact remainder.
//  - Bybit: TS falls back from failed batch writes to single creates, even
//    after ambiguous HTTP/transport failures; Rust never resends those writes.
//  - Bybit: TS may drop the initial ticker snapshot before a slow REST seed;
//    Rust installs the seed before connecting. These cases have native regression
//    coverage; the committed replay tapes require no normalization for them.
//  - Bybit: with no mark price, TS can add a second Partial stop on amend;
//    Rust amends the existing native order ID.
const KNOWN_TS_FIXES = (account, rustAccount) => {
  if (values.venue === 'blofin' && account.positions.length === 0) {
    account.balance.used = 0;
    account.balance.upnl = 0;
  }
  for (const position of account.positions) {
    const fixed = rustAccount.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
    if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
  }
  for (const order of account.orders) {
    const fixed = rustAccount.orders.find((o) => o.id === order.id);
    if (order.rejectReason === null && fixed?.rejectReason === 'PostOnly' && order.status === 'canceled')
      order.rejectReason = 'PostOnly';
  }
};
if (isPrivate) {
  KNOWN_TS_FIXES(ts.account, rust.account);
  for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
    const left = JSON.stringify(ts.account[field]);
    const right = JSON.stringify(rust.account[field]);
    if (left !== right) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
  }
  const left = JSON.stringify(ts.commands);
  const right = JSON.stringify(rust.commands);
  if (left !== right) differences.push(`command results: ts=${left} rust=${right}`);
}
compare('market', ts.markets, rust.markets);
compare('ticker', ts.tickers, rust.tickers);
for (const symbol of new Set([...Object.keys(ts.books), ...Object.keys(rust.books)])) {
  if (JSON.stringify(ts.books[symbol]) !== JSON.stringify(rust.books[symbol])) {
    differences.push(`book ${symbol} differs`);
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
console.log(
  `frames ${ts.frames}, markets ${ts.markets.length}, tickers ${ts.tickers.length}, books ${Object.keys(ts.books).length}`,
);
const row = (label, a, b, unit = 'ms') =>
  console.log(
    `${label.padEnd(16)} ts ${a.toFixed(1).padStart(8)} ${unit}   rust ${b.toFixed(1).padStart(8)} ${unit}   ${(a / b).toFixed(2)}x`,
  );
row('main-thread cpu', t.cpuMs, r.cpuMs);
row('steady cpu', t.steadyCpuMs, r.steadyCpuMs);
row('process cpu', t.processCpuMs, r.processCpuMs);
row('retained memory', t.retainedMB, r.retainedMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB)`);
if (ts.account) console.log('(private tape: paced in real time, so it gates correctness only)');

const failures = [...differences];
// A private tape is paced in real time, so it gates correctness only.
if (!isPrivate) {
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
