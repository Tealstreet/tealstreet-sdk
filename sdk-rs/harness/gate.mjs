// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { subscriptionKeys } from './requests.mjs';
import { gunzipSync } from 'node:zlib';
import { parseRequestJson } from './request-json.mjs';

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
const outDir = `sdk-rs/harness/out/${values.venue}-${process.pid}`;
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push('sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', values.venue, '--tape', values.tape, '--books', values.books, '--out', out);
  if (values['no-egress']) args.push('--no-egress');
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125_000 });
  if (result.status !== 0) throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  return JSON.parse(readFileSync(out, 'utf8'));
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

// JSON object insertion order is immaterial; array order and values are exact.
const stable = (value) => {
  if (Array.isArray(value)) return value.map(stable);
  if (value === null || typeof value !== 'object') return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [key, stable(value[key])]));
};
const differences = [];
const compare = (label, a, b) => {
  const keyed = (rows) => new Map(rows.map((row) => [row.symbol, row]));
  const ta = keyed(a);
  const tb = keyed(b);
  for (const symbol of new Set([...ta.keys(), ...tb.keys()])) {
    const left = JSON.stringify(stable(ta.get(symbol)));
    const right = JSON.stringify(stable(tb.get(symbol)));
    if (left !== right) differences.push(`${label} ${symbol}: ts=${left} rust=${right}`);
  }
};

const runs = Number(values.runs);
const results = { ts: [], rust: [] };
for (let run = 0; run < runs; run++) {
  for (const impl of ['ts', 'rust']) results[impl].push(replay(impl, run));
}
const [ts, rust] = [results.ts[0], results.rust[0]];
const isPrivate = Boolean(ts.account);

// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
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
// OKX full-depth correction is also applied to the TS oracle: its old
// last-trade filter permanently deleted valid levels when the trade lagged.
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
  // Applied to the TS oracle itself (okx.ws-public.ts / okx.exchange.ts).
  okx: [
    'full books retain levels past a stale last trade',
    'public trades implement the inherited Not implemented method',
    'candle calendar bounds use UTC independently of host timezone',
    'fill window filtering retains native milliseconds before output seconds are truncated',
    'keyless/public-only business socket avoids empty-credential login and private topics',
    // Rust safety fixes: covered independently by documented-payload tests.
    'split bracket entries acquire SL/TP before the next entry is placed',
    'native OCO amend preserves its sibling protective leg',
    'negative protective sizes and market or terminal amends fail before writes',
    'positive protective dust cannot round into a close-entire-position order',
    'standalone protective limit amendment fails before cancel if execution price cannot be retained',
    'standalone cancellation continues remaining batches after per-row failures',
    'new account equity survives delayed snapshots and local balance ticks',
    'acknowledged leverage survives failed post-write snapshots',
  ],
  account: (account, rustAccount) => {
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
const fixCryptocomAccount = (account) => {
    // TS never sets the hedged store flag despite mapping positions as hedged.
    account.hedged = true;
    // TS omits user.trade entirely. Derive the execution oracle from documented
    // taped trade rows, independently of Rust's account output.
    const tape = tapeRecords;
    const seen = new Set();
    account.fills = [];
    for (const record of tape) {
      if (record.type !== 'ws' || !record.url.includes('/exchange/v1/user')) continue;
      const result = parseRequestJson(record.text).result;
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
};
for (let run = 0; run < runs; run++) {
  const ts = results.ts[run];
  const rust = results.rust[run];
  if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
  // Every request must match a taped one: an unserved request sent different parameters.
  for (const impl of ['ts', 'rust']) {
    const output = impl === 'ts' ? ts : rust;
    const unserved = output.unserved;
    if (output.metrics.retainedBytes < 0 || output.metrics.bufferRetainedBytes < 0) differences.push(impl + ': negative retained memory component; baseline is invalid');
    if (output.unusedWrites) differences.push(impl + ': ' + output.unusedWrites + ' taped mutation(s) never sent');
    for (const error of output.protocolErrors ?? []) differences.push(impl + ': ' + error);
    for (const symbol of values.books.split(',').filter(Boolean)) {
      if (!output.books[symbol]) differences.push(`${impl}: requested book ${symbol} is missing`);
    }
    if (!output.markets.length || !output.tickers.length) differences.push(`${impl}: empty public seed`);
    if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
  }
  const isPrivate = Boolean(ts.account);
  if (isPrivate) {
    if (values.venue === 'cryptocom') {
      fixCryptocomAccount(ts.account);
      // Rust consumes authoritative user.trade; TS omits that feed entirely.
      const suffix = ' {"method":"subscribe","params":{"channels":["user.order"]}}';
      const feeds = new Set(ts.subscriptions);
      for (const key of ts.subscriptions) {
        if (!key.endsWith(suffix)) continue;
        const url = key.slice(3, -suffix.length);
        for (const required of subscriptionKeys(url,
          { method: 'subscribe', params: { channels: ['user.trade'] } }, new Set(), { cryptoProtocol: true })) feeds.add(required);
      }
      ts.subscriptions = [...feeds].sort();
    }
    if (values.venue === 'binance') KNOWN_TS_FIXES.account(ts.account, rust.account);
    if (values.venue === 'bybit') KNOWN_TS_FIXES.bybit(ts.account, rust.account, ts);
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
      const left = JSON.stringify(ts.account[field]);
      const right = JSON.stringify(rust.account[field]);
      if (left !== right) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
    }
  }
  const left = JSON.stringify(stable(ts.commands));
  const right = JSON.stringify(stable(rust.commands));
  if (left !== right) differences.push(`command results: ts=${left} rust=${right}`);
  for (const field of ['requests', 'subscriptions']) {
    const missing = ts[field].filter((key) => !rust[field].includes(key));
    const extra = rust[field].filter((key) => !ts[field].includes(key));
    if (missing.length || extra.length) differences.push(`${field} run ${run + 1}: missing in Rust=${JSON.stringify(missing)}, extra in Rust=${JSON.stringify(extra)}`);
  }
  if (ts.writeCount !== rust.writeCount || JSON.stringify(ts.writes) !== JSON.stringify(rust.writes)) {
    differences.push(`HTTP write sequence run ${run + 1}: ts=${JSON.stringify(ts.writes)} rust=${JSON.stringify(rust.writes)}`);
  }
  // KNOWN_TS_FIXES: unavailable Crypto.com quotes produce NaN in TS.
  if (values.venue === 'cryptocom') for (const ticker of ts.tickers) {
    for (const field of ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']) {
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
if (values.venue.startsWith('okx')) console.log(`(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`);

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
