// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { sameCommandResults, stable } from './tape.mjs';
import { normalizeTsBybitSubscriptions } from './bybit-subscriptions.mjs';
import { venueOf } from './venues.mjs';
import { COINBASE_TS_FIXES } from './known-ts-fixes-coinbase.mjs';
import { ORDERLY_TS_FIXES } from './known-ts-fixes-orderly.mjs';
import { memoryMetricErrors } from './memory.mjs';
import { applyKucoinRequestContracts } from './kucoin-request-contracts.mjs';
import { requestKey, callerIdsOf, subscriptionKeys } from './requests.mjs';
import { assertLiveSmokeComplete } from './live-smoke-proof.mjs';
import { createTsFixes } from './ts-oracle.mjs';

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
const tapeRecords = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .trim()
  .split('\n')
  .map(JSON.parse);
const smokeResult = assertLiveSmokeComplete(tapeRecords);
// The runner subscribes exactly its selected market. Reproduce that feed set
// rather than the public benchmark's default three symbols.
if (smokeResult) values.books = smokeResult.symbol;
const outDir = `sdk-rs/harness/out/${values.venue}-${process.pid}`;
mkdirSync(outDir, { recursive: true });
console.log(`Replay output: ${outDir}`);

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
  // TSX's compiler worker retains shared buffers independently of engine work.
  const result = spawnSync(process.execPath, args, {
    encoding: 'utf8',
    timeout: 125000,
    env: { ...process.env, ESBUILD_WORKER_THREADS: '0' },
  });
  if (result.status !== 0)
    throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  const output = JSON.parse(readFileSync(out, 'utf8'));
  if (smokeResult && (!Array.isArray(output.commands) || output.commands.length !== smokeResult.commands.length ||
      output.commands.some((result) => !result || !Object.hasOwn(result, 'ok') || Object.hasOwn(result, 'error'))))
    throw new Error(`${impl} live-smoke replay did not successfully complete every lifecycle command`);
  return output;
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

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

const venue = venueOf(values.venue);
const isPrivate =
  tapeRecords.find((record) => record.type === 'config')?.private ??
  tapeRecords.find((record) => record.type === 'meta')?.private ??
  tapeRecords.some((record) =>
    venue.isPrivateRecord
      ? venue.isPrivateRecord(record)
      : Boolean(venue.privateSocket && record.url?.includes(venue.privateSocket)),
  );
const records = tapeRecords;
const instrumentRows = tapeRecords
  .filter((r) => r.type === 'http' && new URL(r.url).pathname === '/v5/market/instruments-info')
  .flatMap((r) => JSON.parse(r.body).result?.list ?? []);
const instruments = new Map(instrumentRows.map((r) => [r.symbol, r]));

// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
// BingX additionally fixes write/read defects covered by native contract tests:
// - closePosition=true STOP_MARKET/TAKE_PROFIT_MARKET requires quantity.
// - A rejected/ambiguous entry must stop its remaining bracket placement jobs.
// - fillHistory uses startTs/endTs/pageSize/pageIndex, not TS's ignored params.
// - Contradictory placement IDs or rejected ACK status must not advance jobs.
// - Delayed placement/replacement ACKs must not reopen newer private state.
// - Trigger replacement checks its selected source; TS always checks mark.
// - Attached SL/TP keeps per-leg workingType; TS spreads the untyped entry.

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
//  - Bybit inverse: TS clamps USD contracts before splitting, silently shrinking
//    large requests; Rust preserves the requested total in max lots + remainder.
//  - Bybit inverse: TS sums coin PnL into the USD wallet; Rust converts using
//    the position's USD notional/base contracts while preserving coin position PnL.
//  - Bybit spot: TS reads absent qtyStep and deprecated maxOrderQty; Rust reads
//    basePrecision and the conservative minimum of documented limit/market maxima.
//    TS also sends invalid spot instrument pagination parameters; Rust omits them.
//  - Bybit spot: documented tpslOrder rows become protective orders; TS has no
//    mapping for that spelling. PartiallyFilledCanceled events now retire orders
//    instead of leaving them open after their remaining quantity was cancelled.
//  - Bybit spot: Rust sends marketUnit=baseCoin for market buys, uses native
//    spot conditional/amend/cancel families, and rejects unsupported trailing
//    orders instead of calling derivative position/trading-stop endpoints.
//  - Bybit spot: TS cancel-all omits orderFilter and leaves trigger families
//    live; Rust cancels Order, StopOrder and tpslOrder independently. Documented
//    private tapes select each implementation's exact expected write plan.
//  - Hyro: seed unseen partial fills from accepted account snapshots; TS can
//    report pre-snapshot cumulative executions again when the order stream resumes.
//  - Hyro: TS keys ambiguous-create recovery by a re-rendered wire payload;
//    ticker/position changes can allocate a new identity for the same retry.
//    Rust resumes the original caller intent before rendering any new payload.
// OKX full-depth correction is also applied to the TS oracle: its old
// last-trade filter permanently deleted valid levels when the trade lagged.
// TS bugs intentionally corrected in Rust. Backpack normalization derives
// expected fields only from the input tape, never from Rust's answer.
// Binance paired TS/Rust fix: EXPIRED_IN_MATCH maps to canceled, like EXPIRED,
// rather than the historical mapper's fallback Closed. Status/lookup regressions
// cover this native STP expiration; no replay answer is normalized.
// OKX paired TS/Rust fix: definitive client-ID lookup uses the primary account
// even when ordinary GETs use a secondary key; a different account's 51603
// cannot establish that the primary order was never placed. Regression tests
// exercise routing directly; no replay result is rewritten for this fix.
// Binance/OKX also bypass cached client-ID hits with secondary credentials:
// that cache may contain another account's orders. TS and Rust both query
// the primary account; native tests retain PM's canonical client-ID identity.
const { KNOWN_TS_FIXES, normalizeBingxPublic, fixCryptocomAccount } = createTsFixes(values, tapeRecords);
for (let run = 0; run < runs; run++) {
  const ts = results.ts[run];
  const rust = results.rust[run];

  if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
  // Every request must match a taped one: an unserved request sent different parameters.
  for (const impl of ['ts', 'rust']) {
    const output = results[impl][run];
    if (Boolean(output.account) !== isPrivate)
      differences.push(impl + ': account presence differs from tape classification');
    // Invalid baselines fail on every run, including informational private metrics.
    for (const error of memoryMetricErrors(output.metrics)) differences.push(`${impl}: ${error}; baseline is invalid`);
    if (output.unusedWrites) differences.push(impl + ': ' + output.unusedWrites + ' taped mutation(s) never sent');
    for (const error of output.protocolErrors ?? []) differences.push(impl + ': ' + error);
    for (const symbol of values.books.split(',').filter(Boolean)) {
      if (!output.books[symbol]) differences.push(`${impl}: requested book ${symbol} is missing`);
    }
    if (!output.markets.length || !output.tickers.length) differences.push(`${impl}: empty public seed`);
    const unserved = output.unserved;
    if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
    const { missingWrites, invalidSignatures } = results[impl][run];
    if (missingWrites) differences.push(`${impl}: ${missingWrites} required taped write(s) never sent`);
    if (invalidSignatures) differences.push(`${impl}: ${invalidSignatures} invalid signatures`);
  }
  const fix =
    KNOWN_TS_FIXES[values.venue] ??
    (['bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].includes(values.venue)
      ? KNOWN_TS_FIXES.bybit
      : undefined);
  // A fixer receives only TS and input records, never the candidate answer.
  if (typeof fix === 'function') fix(ts);
  if (values.venue === 'kraken') KNOWN_TS_FIXES.kraken.output(ts, tapeRecords);
  if (JSON.stringify(ts.wsRequests) !== JSON.stringify(rust.wsRequests))
    differences.push(
      `websocket requests differ: ts=${JSON.stringify(ts.wsRequests)} rust=${JSON.stringify(rust.wsRequests)}`,
    );
  if (values.venue === 'bybitspot') {
    for (const market of ts.markets) {
      const lot = instruments.get(market.symbol)?.lotSizeFilter;
      if (!lot) continue;
      market.precision.amount = Number(lot.qtyStep ?? lot.basePrecision);
      const maxima = [lot.maxLimitOrderQty, lot.maxMarketOrderQty, lot.maxOrderQty]
        .map(Number)
        .filter((n) => Number.isFinite(n) && n > 0);
      if (maxima.length) market.limits.amount.max = Math.min(...maxima);
    }
  }
  if (isPrivate && ts.account && rust.account && values.venue === 'bybitinverse') {
    const upnl = ts.account.positions.reduce(
      (sum, p) => sum + (p.contracts > 0 ? (p.unrealizedPnl * p.notional) / p.contracts : 0),
      0,
    );
    ts.account.balance.total = Math.round((ts.account.balance.total + ts.account.balance.upnl - upnl) * 1e8) / 1e8;
    ts.account.balance.upnl = upnl;
  }
  if (['bybitinverse', 'bybitspot'].includes(values.venue)) {
    const privateRecord = tapeRecords.find(
      (record) => record.type === 'ws' && new URL(record.url).pathname === '/v5/private',
    );
    const privateUrl = privateRecord && new URL(privateRecord.url);
    ts.subscriptions = normalizeTsBybitSubscriptions(
      values.venue,
      ts.subscriptions,
      privateUrl && `${privateUrl.origin}${privateUrl.pathname}`,
    );
  }
  if (isPrivate && ts.account && rust.account) {
    if (values.venue === 'cryptocom') {
      fixCryptocomAccount(ts.account);
      const feeds = new Set(ts.subscriptions);
      const suffix = ' {"method":"subscribe","params":{"channels":["user.order"]}}';
      for (const key of ts.subscriptions) {
        if (!key.endsWith(suffix)) continue;
        for (const required of subscriptionKeys(
          key.slice(3, -suffix.length),
          { method: 'subscribe', params: { channels: ['user.trade'] } },
          new Set(),
          { cryptoProtocol: true },
        ))
          feeds.add(required);
      }
      ts.subscriptions = [...feeds].sort();
    }
    if (['lighter', 'lighter-robinhood'].includes(values.venue)) {
      KNOWN_TS_FIXES.lighter.fills(ts.account, tapeRecords);
      KNOWN_TS_FIXES.lighter.commands(ts.commands, tapeRecords);
    }
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
      const left = JSON.stringify(stable(ts.account[field]));
      const right = JSON.stringify(stable(rust.account[field]));
      if (left !== right) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
    }
  }
  // KNOWN_TS_FIXES: KuCoin TS has no futures history implementations. The
  // synthetic tape declares independent documented query expectations; never
  // derive this reference from the Rust result.
  if (values.venue === 'kucoin')
    tapeRecords
      .filter((r) => r.type === 'command')
      .forEach((record, index) => {
        if (record.expected !== undefined) {
          if (!['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(record.op))
            throw new Error('KuCoin expected oracle is restricted to missing TS query methods');
          ts.commands[index] = { ok: record.expected };
        }
      });
  if (!sameCommandResults(ts.commands, rust.commands))
    differences.push(`command results: ts=${JSON.stringify(ts.commands)} rust=${JSON.stringify(rust.commands)}`);
  normalizeBingxPublic(ts);
  if (values.venue === 'kucoin') {
    const requests = applyKucoinRequestContracts(tapeRecords, ts.requests, rust.requests);
    ts.requests = requests.tsRequests;
    rust.requests = requests.rustRequests;
  }
  // KNOWN_TS_FIXES: unavailable Crypto.com quotes produce NaN in TS.
  if (values.venue === 'cryptocom')
    for (const ticker of ts.tickers) {
      for (const field of ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']) {
        if (ticker[field] === null) ticker[field] = 0;
      }
    }
  const selectedOnly = (impl) =>
    new Set(
      tapeRecords
        .filter((record) => record.type === 'http' && record.impl === impl)
        .map((record) => requestKey(record.method, record.url, record.requestBody, callerIdsOf(tapeRecords), venue)),
    );
  const implementationPlans = { ts: selectedOnly('ts'), rust: selectedOnly('rust') };
  // Tagged fixture writes are independently required by each transport's
  // consumption audit; compare shared requests and their shared write sequence.
  for (const field of ['requests', 'subscriptions']) {
    const left = ts[field]?.filter((key) => field !== 'requests' || !implementationPlans.ts.has(key));
    const right = rust[field]?.filter((key) => field !== 'requests' || !implementationPlans.rust.has(key));
    if (!Array.isArray(left) || !Array.isArray(right) || JSON.stringify(left) !== JSON.stringify(right))
      differences.push(`${field} run ${run + 1}: ts=${JSON.stringify(left)} rust=${JSON.stringify(right)}`);
  }
  const tsWrites = ts.writes?.filter((key) => !implementationPlans.ts.has(key));
  const rustWrites = rust.writes?.filter((key) => !implementationPlans.rust.has(key));
  if (
    !Array.isArray(tsWrites) ||
    !Array.isArray(rustWrites) ||
    ts.writeCount !== ts.writes.length ||
    rust.writeCount !== rust.writes.length ||
    JSON.stringify(tsWrites) !== JSON.stringify(rustWrites)
  )
    differences.push(
      `HTTP write sequence run ${run + 1}: ts=${JSON.stringify(tsWrites)} rust=${JSON.stringify(rustWrites)}`,
    );
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
if (isPrivate) console.log('(private tape: correctness only; CPU and memory are informational)');
if (venue.family === 'orderly')
  console.log(
    `(documented Orderly corrections: ${ORDERLY_TS_FIXES.length}; native protocol and independent oracles checked)`,
  );
if (values.venue.startsWith('okx'))
  console.log(
    `(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`,
  );

if (venue.family === 'coinbase')
  console.log(
    `(documented Coinbase corrections: ${COINBASE_TS_FIXES.length}; native JWT and independent oracles checked)`,
  );

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
