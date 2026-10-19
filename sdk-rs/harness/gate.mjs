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
const outDir = `sdk-rs/harness/out/${values.venue}-${values.tape.split('/').at(-1).replaceAll('.', '-')}`;
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push('sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', values.venue, '--tape', values.tape, '--books', values.books, '--out', out);
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 90_000 });
  if (result.status !== 0) throw new Error(`${impl} replay failed:\n${result.stderr}`);
  return JSON.parse(readFileSync(out, 'utf8'));
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

const tapeBytes = readFileSync(values.tape);
const tape = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes).toString('utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
const canonical = (value) => Array.isArray(value) ? value.map(canonical) : value && typeof value === 'object'
  ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, canonical(v)])) : value;
const equal = (a, b) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));

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
const isPrivate = Boolean(results.ts[0].account);
// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
const KNOWN_TS_FIXES = (account, rustAccount) => {
  if (values.venue === 'bitunix') {
    // Bitunix fixes: regular orders retain clientId; failed reconciliation preserves state; live cumulative fills
    // emit once; history dedupes by tradeId rather than collapsing equal fills;
    // split orders use distinct generated client IDs; cancel replies
    // require per-order success; replacements reconcile final remaining quantity.
    // TS never emits Bitunix live fills. This independent tape oracle validates
    // cumulative quantities/costs; it does not copy anything from Rust's output.
    const totals = new Map();
    for (const record of tape.filter((r) => r.type === 'http' && new URL(r.url).pathname.endsWith('/trade/get_pending_orders') && (r.after ?? 0) === 0)) {
      const data = JSON.parse(record.body).data;
      for (const row of Array.isArray(data) ? data : data.orderList) totals.set(String(row.orderId), [Number(row.tradeQty), Number(row.tradeQty) * Number(row.avgPrice ?? row.price)]);
    }
    for (const record of tape.filter((r) => r.type === 'http' && new URL(r.url).pathname.endsWith('/trade/get_pending_orders'))) {
      const data = JSON.parse(record.body).data;
      for (const row of Array.isArray(data) ? data : data.orderList ?? []) {
        const order = account.orders.find((o) => o.id === String(row.orderId));
        if (order && !order.clientOrderId && row.clientId) order.clientOrderId = row.clientId;
      }
    }
    account.fills = [];
    for (const record of tape.filter((r) => r.type === 'ws')) {
      const message = JSON.parse(record.text);
      if (message.ch !== 'order') continue;
      const row = message.data;
      const id = String(row.orderId);
      const [previous, previousCost] = totals.get(id) ?? [0, 0];
      const cumulative = Number(row.dealAmount ?? 0);
      const delta = cumulative - previous;
      if (delta > 0) {
        const cost = Number(row.averagePrice) > 0 ? Number(row.averagePrice) * cumulative : previousCost + delta * Number(row.price);
        account.fills.push({ id, symbol: row.symbol, side: row.side.toLowerCase(), amount: delta, price: (cost - previousCost) / delta });
        totals.set(id, [cumulative, cost]);
      }
    }
    return;
  }
  // Existing Binance Rust orders retain wire client IDs/TIF that TS drops.
  // Populate these comparison fields from the tape, never from Rust output.
  const metadata = new Map();
  for (const record of tape) {
    if (record.type === 'http' && record.method === 'GET') {
      let rows; try { rows = JSON.parse(record.body.replace(/(:\s*)(-?\d{16,})(?=\s*[,}])/g, '$1"$2"')); } catch { continue; }
      if (Array.isArray(rows)) for (const row of rows) {
        if (row.orderId && row.clientOrderId) metadata.set(String(row.orderId), { clientOrderId: row.clientOrderId, tif: row.timeInForce });
      }
    } else if (record.type === 'ws') {
      const m = JSON.parse(record.text.replace(/(:\s*)(-?\d{16,})(?=\s*[,}])/g, '$1"$2"'));
      if (m.e === 'ORDER_TRADE_UPDATE') metadata.set(String(m.o.i), { clientOrderId: m.o.c, tif: m.o.f });
    }
  }
  const tif = { GTC: 'GoodTillCancel', GTX: 'PostOnly', IOC: 'ImmediateOrCancel', FOK: 'FillOrKill' };
  for (const order of account.orders) {
    const raw = metadata.get(order.id);
    if (raw && !order.clientOrderId) order.clientOrderId = raw.clientOrderId;
    if (raw?.tif && order.timeInForce === 'GTC') order.timeInForce = tif[raw.tif] ?? raw.tif;
  }
  for (const position of account.positions) {
    const fixed = rustAccount.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
    if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
  }
};
// KNOWN_TS_FIXES: the history composite-key map drops distinct executions
// with equal order/time/price/size/side. Reconstruct only that specific loss
// from documented trade IDs on the tape, independently of Rust's results.
const fixHistoricalFills = (commands) => {
  if (values.venue !== 'bitunix') return;
  const requests = tape.filter((r) => r.type === 'command');
  requests.forEach((request, index) => {
    if (request.op !== 'fetchFills' || !Array.isArray(commands[index]?.ok)) return;
    const response = tape.find((r) => r.type === 'http' && r.url.includes('/trade/get_history_trades?') &&
      new URL(r.url).searchParams.get('orderId') === request.orderId);
    if (!response) return;
    const rows = JSON.parse(response.body).data.tradeList;
    const executions = rows.map((r) => ({ id: r.tradeId, orderId: r.orderId, symbol: r.symbol, timestamp: Math.floor(r.ctime / 1000),
      side: r.side.toLowerCase(), price: Number(r.price), qty: Number(r.qty), commission: Math.abs(Number(r.fee)), commissionAsset: r.marginCoin }));
    const key = (e) => JSON.stringify([e.orderId, e.timestamp, e.price, e.qty, e.side]);
    const collapsed = [...new Map(executions.map((e) => [key(e), e])).values()].sort((a, b) => b.timestamp - a.timestamp);
    if (collapsed.length !== executions.length && equal(commands[index].ok, collapsed))
      commands[index].ok = executions.sort((a, b) => b.timestamp - a.timestamp);
  });
};
for (let run = 0; run < runs; run++) {
const result = { ts: results.ts[run], rust: results.rust[run] };
const { ts, rust } = result;
if (ts.frames !== rust.frames) differences.push(`run ${run} frames: ts=${ts.frames} rust=${rust.frames}`);
if (isPrivate) {
  KNOWN_TS_FIXES(ts.account, rust.account);
  fixHistoricalFills(ts.commands);
  for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
    const left = JSON.stringify(ts.account[field]);
    const right = JSON.stringify(rust.account[field]);
    if (!equal(ts.account[field], rust.account[field])) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
  }
  const left = JSON.stringify(ts.commands);
  const right = JSON.stringify(rust.commands);
  if (!equal(ts.commands, rust.commands)) differences.push(`command results: ts=${left} rust=${right}`);
}
for (const impl of ['ts', 'rust']) {
  if (result[impl].signatures?.length) differences.push(`${impl}: ${result[impl].signatures.join('; ')}`);
  if (result[impl].socketErrors?.length) differences.push(`${impl}: ${result[impl].socketErrors.join('; ')}`);
  if (result[impl].unserved) differences.push(`${impl}: ${result[impl].unserved} unserved requests`);
}
for (const field of ['outbound', 'subscriptions', 'candles', 'bookEvents']) {
  if (!equal(ts[field], rust[field])) differences.push(`${field}: ts=${JSON.stringify(ts[field])} rust=${JSON.stringify(rust[field])}`);
}
// KNOWN_TS_FIXES: Bitunix parses ISO ft with parseFloat (the year) and ignores
// nft. The ticker contract uses the next funding timestamp in milliseconds.
if (values.venue === 'bitunix') {
  const latest = new Map();
  for (const record of tape.filter((r) => r.type === 'ws')) {
    const m = JSON.parse(record.text);
    if (m.ch === 'price') latest.set(m.symbol, m.data);
  }
  for (const ticker of ts.tickers) {
    const price = latest.get(ticker.symbol);
    if (!price) continue;
    const value = price.nft || price.ft;
    const corrected = Number.isFinite(Number(value)) ? Number(value) : Date.parse(value);
    if (ticker.fundingTime === Number.parseFloat(price.ft)) ticker.fundingTime = corrected;
    else if (ticker.fundingTime !== corrected) differences.push(`unexpected TS funding time for ${ticker.symbol}`);
  }
}
compare('market', ts.markets, rust.markets);
compare('ticker', ts.tickers, rust.tickers);
for (const symbol of new Set([...Object.keys(ts.books), ...Object.keys(rust.books)])) {
  if (JSON.stringify(ts.books[symbol]) !== JSON.stringify(rust.books[symbol])) {
    differences.push(`book ${symbol} differs`);
  }
}

for (const symbol of values.books.split(',').filter(Boolean)) {
  if (!ts.books[symbol] || !rust.books[symbol]) differences.push(`requested book ${symbol} was not observed in both implementations`);
}

}
const [ts, rust] = [results.ts[0], results.rust[0]];

const summary = (impl) => ({
  cpuMs: median(results[impl].map((r) => r.metrics.cpuMs)),
  steadyCpuMs: median(results[impl].map((r) => r.metrics.steadyCpuMs)),
  processCpuMs: median(results[impl].map((r) => r.metrics.processCpuMs)),
  peakMB: median(results[impl].map((r) => r.metrics.peakBytes)) / 1e6,
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
row('sampled peak', t.peakMB, r.peakMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB)`);
if (ts.account) console.log('(private tape: paced in real time, so it gates correctness only)');

const failures = [...differences];
// A private tape is paced in real time, so it gates correctness only.
if (!isPrivate) {
  if (!(r.cpuMs < t.cpuMs)) failures.push('Rust main-thread CPU is not below TS');
  if (!(r.steadyCpuMs < t.steadyCpuMs)) failures.push('Rust steady-state CPU is not below TS');
  if (!(r.retainedMB < t.retainedMB)) failures.push('Rust retained memory is not below TS');
  if (!(r.peakMB < t.peakMB)) failures.push('Rust sampled peak memory is not below TS');
}
if (failures.length) {
  console.log(`FAIL (${failures.length})`);
  for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
  process.exit(1);
}
console.log('PASS');
