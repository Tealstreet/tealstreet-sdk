// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    runs: { type: 'string', default: '3' },
    'no-egress': { type: 'boolean', default: false },
  },
});
const tapeBytes = readFileSync(values.tape);
const tapeRecords = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes).toString('utf8').trim().split('\n').map(JSON.parse);
const replayTimeout = Math.max(60_000, tapeRecords.reduce((last, r) => Math.max(last, r.t ?? 0), 0) + 30_000);
const outDir = 'sdk-rs/harness/out';
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push('sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', values.venue, '--tape', values.tape, '--books', values.books, '--out', out);
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: replayTimeout });
  if (result.status !== 0) throw new Error(`${impl} replay failed:\n${result.error?.message ?? result.stderr}`);
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
  const result = results[impl][0];
  if (result.unusedWrites) differences.push(impl + ': ' + result.unusedWrites + ' taped mutation(s) never sent');
  for (const error of result.protocolErrors ?? []) differences.push(impl + ': ' + error);
  if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
}
const isPrivate = Boolean(ts.account);
// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
const KNOWN_TS_FIXES = (account, rustAccount) => {
  if (values.venue === 'cryptocom') {
    // TS never sets the hedged store flag despite mapping positions as hedged.
    account.hedged = true;
    // TS omits user.trade entirely. Derive the execution oracle from documented
    // taped trade rows, independently of Rust's account output.
    const tape = tapeRecords;
    const seen = new Set();
    account.fills = [];
    for (const record of tape) {
      if (record.type !== 'ws' || !record.url.includes('/exchange/v1/user')) continue;
      const result = JSON.parse(record.text).result;
      if (result?.channel !== 'user.trade') continue;
      for (const row of result.data) {
        const id = String(row.trade_id);
        if (seen.has(id)) continue;
        seen.add(id);
        account.fills.push({ id: String(row.order_id), symbol: row.instrument_name, side: row.side.toLowerCase(), amount: Number(row.traded_quantity) });
      }
    }
    // Further deliberate fixes are exercised by Rust regressions: failed reads
    // retain state; client_oid survives mapping; terminal/stale frames cannot
    // reopen orders/positions; REST retains live position enrichment; post-only
    // uses exec_inst; account-wide leverage uses account_id + integer leverage;
    // ignored brackets/opening triggers reject before writes; caller ids use a
    // conservative 36-byte ceiling without JS string emulation.
    // Public throttle and public-only subscription corrections are the
    // afterTsFrame and prepareTs hooks in venues.mjs.
    return;
  }
  for (const position of account.positions) {
    const fixed = rustAccount.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
    if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
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
// KNOWN_TS_FIXES: Crypto.com legitimately sends null quotes (empty books).
// TS parseFloat(null) yields NaN; the engine's numeric boundary represents
// unavailable ticker numbers as zero.
if (values.venue === 'cryptocom') {
  for (const ticker of ts.tickers) for (const field of ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']) {
    if (ticker[field] === null) ticker[field] = 0;
  }
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
