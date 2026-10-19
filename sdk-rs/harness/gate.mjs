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
const outDir = 'sdk-rs/harness/out';
mkdirSync(outDir, { recursive: true });
const tapeBytes = readFileSync(values.tape);
const records = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString().split('\n').filter(Boolean).map(JSON.parse);

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

if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
// Every request must match a taped one: an unserved request sent different parameters.
for (const impl of ['ts', 'rust']) {
  const unserved = results[impl][0].unserved;
  if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
}
const isPrivate = Boolean(ts.account);
// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
// BingX additionally fixes write/read defects covered by native contract tests:
// - closePosition=true STOP_MARKET/TAKE_PROFIT_MARKET requires quantity.
// - A rejected/ambiguous entry must stop its remaining bracket placement jobs.
// - fillHistory uses startTs/endTs/pageSize/pageIndex, not TS's ignored params.
// - Contradictory placement IDs or rejected ACK status must not advance jobs.
//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
const KNOWN_TS_FIXES = (account, rustAccount) => {
  if (values.venue === 'bingx') {
    // BingX TS emits a terminal cumulative fill, ignores partial executions,
    // and drops its terminal watermark so TRADE_UPDATE can repeat that fill.
    // Use the taped per-execution oracle; Rust is still compared against it.
    const watermark = new Map();
    const fills = [];
    for (const record of records) {
      if (record.type !== 'ws' || !record.url?.includes('?listenKey=')) continue;
      let event;
      try { event = JSON.parse(record.text, (_key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^-?\d+$/.test(context.source) ? context.source : value); } catch { continue; }
      if (!['ORDER_TRADE_UPDATE', 'TRADE_UPDATE'].includes(event.e)) continue;
      const o = event.o;
      if (!['PARTIALLY_FILLED', 'FILLED'].includes(o.X)) continue;
      const id = String(o.i);
      const cumulative = Number(o.z);
      const previous = watermark.get(id) ?? 0;
      const amount = Math.max(0, cumulative - previous);
      watermark.set(id, Math.max(previous, cumulative));
      if (amount > 0) fills.push({ id, symbol: o.s.replaceAll('-', ''), side: o.S === 'BUY' ? 'buy' : 'sell', amount: Number(o.l) > 0 ? Math.min(amount, Number(o.l)) : amount });
    }
    account.fills = fills;
    // TS's private mapper reads closePosition but misses the native cp alias.
    for (const record of records) {
      if (record.type !== 'ws') continue;
      let event; try { event = JSON.parse(record.text, (_key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) && context?.source && /^-?\d+$/.test(context.source) ? context.source : value); } catch { continue; }
      const row = account.orders.find((o) => o.id === String(event.o?.i));
      if (row && event.o.cp !== undefined) row.close = event.o.cp === true || event.o.cp === 'true';
    }
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
// KNOWN_TS_FIXES: BingX turns precision=0 into 0.1 instead of the native
// integer step 1. Correct that precision and its synthetic ticker touch only.
if (values.venue === 'bingx') {
  const contracts = new Map();
  for (const record of records) {
    if (record.type !== 'http' || new URL(record.url).pathname !== '/openApi/swap/v2/quote/contracts') continue;
    const response = JSON.parse(record.body);
    if (response.code !== 0 || !Array.isArray(response.data)) continue;
    for (const contract of response.data) contracts.set(contract.symbol, contract);
  }
  const zeroPrecision = (value) => (typeof value === 'number' || typeof value === 'string' && value.trim() !== '') && Number(value) === 0;
  const prices = new Set();
  for (const market of ts.markets) {
    const source = contracts.get(market.id);
    for (const [field, nativeField] of [['amount', 'quantityPrecision'], ['price', 'pricePrecision']]) {
      if (market.precision[field] === 0.1 && zeroPrecision(source?.[nativeField])) {
        market.precision[field] = 1;
        if (field === 'price') prices.add(market.symbol);
      }
    }
  }
  // TS parses non-trading '-' placeholders into NaN; Rust stores finite zeros.
  for (const ticker of ts.tickers) {
    for (const field of ['percentage', 'volume', 'quoteVolume']) if (ticker[field] === null) ticker[field] = 0;
    if (!prices.has(ticker.symbol)) continue;
    ticker.bid = Math.max(ticker.last - 1, 0);
    ticker.ask = ticker.last + 1;
  }
}
compare('market', ts.markets, rust.markets);
compare('ticker', ts.tickers, rust.tickers);
// KNOWN_TS_FIXES: BingX's internal feed sends asks descending; TS forwards
// that order. Consumers require best ask first, so Rust sorts full snapshots.
if (values.venue === 'bingx') {
  for (const book of Object.values(ts.books)) {
    book.bids.sort((a,b) => b[0]-a[0]);
    book.asks.sort((a,b) => a[0]-b[0]);
  }
}
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
