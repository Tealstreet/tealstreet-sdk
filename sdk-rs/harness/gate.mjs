// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { validateCiWasm, validatePairedCiWasm } from './ci-wasm.mjs';
import { verifyRuntime } from './cpu-runtime.mjs';
import { observeCpu } from './cpu-observation.mjs';
import { resolve } from 'node:path';
import { createRecordedReplay } from './recorded-replay.mjs';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { sameCommandResults, expectedCheckpointPlan } from './tape.mjs';
import { replayVenueOf } from './venues.mjs';
import { RUST_ONLY_FAILURE, rustOnlyOracle, rustOnlyDifferences } from './rust-only-oracle.mjs';
import { COINBASE_TS_FIXES } from './known-ts-fixes-coinbase.mjs';
import { ORDERLY_TS_FIXES } from './known-ts-fixes-orderly.mjs';
import { BITRUE_TS_FIXES } from './known-ts-fixes-bitrue.mjs';
import { memoryMetricErrors } from './memory.mjs';
import { requestKey, callerIdsOf } from './requests.mjs';
import { assertLiveSmokeComplete } from './live-smoke-proof.mjs';
import { runAuditedFixer } from './oracle-audit.mjs';
import { createTsFixes } from './ts-oracle.mjs';
import { duplicateSymbols, compareCheckpoints, compareSnapshots, valueDifferences, checkpointCoverageErrors, snapshotCoverageErrors } from './snapshot-compare.mjs';
import { writeCpuAnnotations } from './cpu-annotations.mjs';
import { PUBLIC_REPLAY_PAIRS, cpuSlower, median, performanceMetricErrors, performanceOrder, performanceVerdict, remeasuredVerdict, replayRunCount } from './performance-policy.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    runs: { type: 'string' },
    report: { type: 'string' },
    'ci-wasm': { type: 'string' },
    'paired-source': { type: 'string' },
    'recorded-evidence': { type: 'string' },
    'runtime-capsule': { type: 'string' },
    'no-egress': { type: 'boolean', default: false },
    'correctness-only': { type: 'boolean', default: false },
  },
});
if (values['correctness-only'] && (values['paired-source'] || values['runtime-capsule'] || values['recorded-evidence'] || values.runs))
  throw new Error('Correctness-only replay cannot supply CPU measurement evidence');
// Validate before spawning either implementation; a bad artifact has no fallback.
if (values['paired-source'] !== undefined && (!values['ci-wasm'] || values.venue !== 'bitget' ||
    values.runs !== String(PUBLIC_REPLAY_PAIRS) || values['runtime-capsule'] || values['recorded-evidence'] || values['no-egress']))
  throw new Error('Paired source is only a public Bitget fixed-pair measurement module contract');
const ciWasm = values['ci-wasm'] === undefined ? null : values['paired-source'] === undefined
  ? validateCiWasm({ directory: values['ci-wasm'], venue: values.venue })
  : validatePairedCiWasm({ directory: values['ci-wasm'], venue: values.venue, sourceHead: values['paired-source'] });
if (values['runtime-capsule'] && values['ci-wasm']) throw new Error('select one explicit replay module contract');
const cpuRuntime = values['runtime-capsule'] ? verifyRuntime({ root: process.cwd(), directory: resolve(values['runtime-capsule']) }) : null;
if (cpuRuntime && resolve(process.execPath) !== cpuRuntime.nodePath) throw new Error('CPU replay must use its sealed Node executable');
const sealedWasm = cpuRuntime ? resolve('sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm.js') : null;
if (cpuRuntime && !cpuRuntime.manifest.entries.some((entry) => entry.path === 'sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm.js' && entry.type === 'file'))
  throw new Error('sealed replay module is missing; no fallback');
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
const recorded = values['recorded-evidence'] === undefined ? null : createRecordedReplay({
  evidence: JSON.parse(readFileSync(values['recorded-evidence'], 'utf8')),
  venue: values.venue, tape: values.tape, books: values.books,
});
if (recorded && values.runs !== String(PUBLIC_REPLAY_PAIRS)) throw new Error('recorded public verification requires the complete fixed balanced original pairs');
const artifacts = [];
const sampleOrder = [];
const cpuObservation = recorded ? undefined : observeCpu();
const retainMeasurement = () => {
  if (!recorded) writeFileSync(`${outDir}/measurement.json`, JSON.stringify({ cpu: cpuObservation, sampleOrder }, null, 2));
};
retainMeasurement();
const fixerReports = [];
class CommandReplayTimeout extends Error {
  constructor(impl, command, stderrPath) {
    super(`${impl} replay: command ${command} did not complete`);
    this.failure = { kind: 'command-timeout', impl, command, stderrPath };
  }
}

/** Every non-CPU check of one performance set against the audited correctness pair. */
function performanceRunDifferences(sets, audits, audited) {
  const found = [];
  for (const impl of ['ts', 'rust']) for (const [run, output] of sets[impl].entries()) {
    for (const error of [...memoryMetricErrors(output.metrics), ...performanceMetricErrors(output.metrics, 'performance')])
      found.push(`${impl} performance run ${run + 1}: ${error}`);
    if (output.frames !== audits[impl][0].frames || output.commands.length !== audits[impl][0].commands.length ||
        output.unserved || output.missingWrites || output.unusedWrites)
      found.push(`${impl} performance run ${run + 1}: incomplete replay workload`);
    found.push(...compareSnapshots(audited[impl], output, `${impl} performance run ${run + 1}`));
    if (!sameCommandResults(audited[impl].commands, output.commands))
      found.push(`${impl} performance run ${run + 1}: command workload differs from audited replay`);
    if (!output.transportReceipt || !isDeepStrictEqual(audited[impl].transportReceipt, output.transportReceipt))
      found.push(`${impl} performance run ${run + 1}: transport workload differs from audited replay`);
  }
  return found;
}

const replay = (impl, run, performanceRun = false) => {
  const out = `${outDir}/${impl}-${performanceRun ? "perf" : "audit"}-${run}.json`;
  artifacts.push({ impl, mode: performanceRun ? 'performance' : 'correctness', run, path: out });
  if (recorded) return recorded.read(impl, run, performanceRun);
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
  if (impl === 'rust' && (ciWasm || sealedWasm)) args.push('--wasm', sealedWasm ?? ciWasm.path);
  if (values['no-egress']) args.push('--no-egress');
  if (performanceRun) args.push('--performance');
  // TSX's compiler worker retains shared buffers independently of engine work.
  const observed = performanceRun ? observeCpu() : undefined;
  const sampleStarted = performance.now();
  const result = spawnSync(process.execPath, args, {
    encoding: 'utf8',
    timeout: 125000,
    env: { ...process.env, ESBUILD_WORKER_THREADS: '0' },
  });
  if (performanceRun) {
    sampleOrder.push({ pair: run, impl, ordinal: sampleOrder.length, before: observed, after: observeCpu(), elapsedMs: performance.now() - sampleStarted });
    retainMeasurement();
  }
  if (result.status !== 0) {
    const timeout = /^ReplayTimeout \[Error\]: replay timeout: command ([A-Za-z][A-Za-z0-9]*)$/m.exec(result.stderr ?? '');
    if (result.status === 1 && !performanceRun && timeout && tapeRecords.some((record) => record.type === 'command' && record.op === timeout[1])) {
      const stderrPath = `${out}.stderr`;
      writeFileSync(stderrPath, result.stderr);
      throw new CommandReplayTimeout(impl, timeout[1], stderrPath);
    }
    throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  }
  const output = JSON.parse(readFileSync(out, 'utf8'));
  if (smokeResult && (!Array.isArray(output.commands) || output.commands.length !== smokeResult.commands.length ||
      output.commands.some((result) => !result || !Object.hasOwn(result, 'ok') || Object.hasOwn(result, 'error'))))
    throw new Error(`${impl} live-smoke replay did not successfully complete every lifecycle command`);
  return output;
};

const differences = [];
const compare = (label, a, b) => {
  const keyed = (rows) => new Map(rows.map((row) => [row.symbol, row]));
  const ta = keyed(a);
  const tb = keyed(b);
  for (const symbol of new Set([...ta.keys(), ...tb.keys()])) {
    differences.push(...valueDifferences(ta.get(symbol), tb.get(symbol), `${label} ${symbol}`));
  }
};

const venue = replayVenueOf(values.venue);
const isPrivate =
  tapeRecords.find((record) => record.type === 'config')?.private ??
  tapeRecords.find((record) => record.type === 'meta')?.private ??
  tapeRecords.some((record) => venue.isPrivateRecord ? venue.isPrivateRecord(record)
    : Boolean(venue.privateSocket && record.url?.includes(venue.privateSocket)));
// A retired TS oracle (meta.oracle) replays Rust alone against the tape's expected state.
const rustOnly = rustOnlyOracle(tapeRecords, isPrivate);
const impls = rustOnly ? ['rust'] : ['ts', 'rust'];
let runs = values['correctness-only'] ? 0 : replayRunCount(values.runs ?? (isPrivate ? undefined : process.env.RUST_REPLAY_PUBLIC_RUNS), isPrivate);
const correctnessRuns = values['correctness-only'] ? 1 : isPrivate ? runs : 1;
const checkpointPlans = Object.fromEntries(impls.map((impl) => [impl, expectedCheckpointPlan(tapeRecords, venue, impl)]));
const results = { ts: [], rust: [] };
const performanceResults = { ts: [], rust: [] };
const metricsOf = (impl) => performanceResults[impl].map((output) => output.metrics);
let remeasure;
try {
for (let run = 0; run < correctnessRuns; run++) {
  for (const impl of impls) results[impl].push(replay(impl, run));
}
if (!isPrivate && !values['correctness-only']) {
  const measure = () => {
    for (let run = 0; run < runs; run++)
      for (const impl of performanceOrder(run)) performanceResults[impl].push(replay(impl, run, true));
  };
  measure();
  // One re-measure on this runner before exceeding the venue CPU bound fails. Only the
  // final set is reported and retained, so recorded revalidation never re-measures.
  const first = recorded ? undefined : performanceVerdict(metricsOf('ts'), metricsOf('rust'), values.venue);
  if (first && cpuSlower(first.failures)) {
    const audited = { ts: structuredClone(results.ts[0]), rust: structuredClone(results.rust[0]) };
    remeasure = { first, firstDifferences: performanceRunDifferences(performanceResults, results, audited) };
    for (const impl of ['ts', 'rust']) performanceResults[impl].length = 0;
    for (let index = artifacts.length - 1; index >= 0; index--) if (artifacts[index].mode === 'performance') artifacts.splice(index, 1);
    sampleOrder.length = 0;
    measure();
  }
}
} catch (error) {
  if (!(error instanceof CommandReplayTimeout)) throw error;
  // A declared venue command that never settles is a strict failure with no
  // completed parity/performance evidence, distinct from a runner crash.
  writeFileSync(values.report ?? `${outDir}/gate.json`, JSON.stringify({
    schema: 1, venue: values.venue, tape: values.tape, books: values.books,
    ...(ciWasm ? { ciWasm: ciWasm.metadata } : {}),
    tapeSha256: createHash('sha256').update(tapeBytes).digest('hex'),
    private: isPrivate, status: 'FAIL', failures: [error.message], replayFailure: error.failure,
    ...(rustOnly ? { rustOnly: true } : {}),
    ...(values['correctness-only'] ? { correctnessOnly: true } : {}),
    coverage: { correctness: 'incomplete', performance: 'not-run' },
    ...(recorded ? {} : { measurement: { cpu: cpuObservation, sampleOrder } }),
    runs: 0, correctnessRuns, artifacts, fixers: [], samples: { ts: [], rust: [] },
  }, null, 2) + '\n');
  console.log(`FAIL (incomplete replay)\n  ${error.message}`);
  process.exit(1);
}
const [ts, rust] = [results.ts[0], results.rust[0]];
const rawAudited = rustOnly ? undefined : { ts: structuredClone(ts), rust: structuredClone(rust) };
if (rustOnly) for (let run = 0; run < correctnessRuns; run++)
  differences.push(...rustOnlyDifferences(results.rust[run], { tapeRecords, checkpointPlan: checkpointPlans.rust, books: values.books, run, venue: values.venue }));

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
//    the execution's own price (`L`). The stricter gate reports the divergence.
//  - Bybit: TS drops the reject reason of a post-only order cancelled for taking
//    liquidity (`EC_PostOnlyWillTakeLiquidity`); Rust reports it as PostOnly.
//  - Bybit: TS can round split quantities away from the requested total;
//    Rust uses maximum-size lots and the exact remainder.
//  - Bybit: TS falls back from failed batch writes to single creates, even
//    after ambiguous HTTP/transport failures; Rust never resends those writes.
//  - Bybit: TS may drop the initial ticker snapshot before a slow REST seed;
//    Rust installs the seed before connecting. These cases have native regression
//    coverage; the committed replay tapes require no normalization for them.
//  - Bybit Partial TP/SL now follows the owner-selected TS replacement and
//    missing-mark fallbacks; the former amend-in-place divergence is retired.
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
// Audit corrections are exercised by native interleaving regressions, never
// rewritten into the replay result: Kraken Spot verifies raw-decimal CRC32;
// WOO validates prevTs and clears invalid books during recovery; Backpack
// preserves positions after an unqualified 404 rather than committing [].
const { KNOWN_TS_FIXES, applyTsCorrections } = createTsFixes(values, tapeRecords);
for (let run = 0; run < (rustOnly ? 0 : correctnessRuns); run++) {
  const ts = results.ts[run];
  const rust = results.rust[run];

  // Inspect original arrays before a fixer or Map can erase duplicate rows.
  for (const [impl, output] of [['ts', ts], ['rust', rust]])
    {
      differences.push(...snapshotCoverageErrors(output, `${impl} final`, { isPrivate, venue: values.venue }));
      differences.push(...checkpointCoverageErrors(output.checkpoints, checkpointPlans[impl], impl, { isPrivate, venue: values.venue }));
      for (const field of ['markets', 'tickers']) differences.push(...duplicateSymbols(output[field], `${impl} ${field}`));
      differences.push(...(output.duplicateSymbols ?? []).map((error) => `${impl}: ${error}`));
    }
  // Checkpoints compare the actual receiver output without final-state fixers.
  differences.push(...compareCheckpoints(ts.checkpoints, rust.checkpoints));

  if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
  // Every request must match a taped one: an unserved request sent different parameters.
  for (const impl of ['ts', 'rust']) {
    const output = results[impl][run];
    if (Boolean(output.account) !== isPrivate)
      differences.push(impl + ': account presence differs from tape classification');
    // Invalid baselines fail on every run, including informational private metrics.
    for (const error of [...memoryMetricErrors(output.metrics), ...performanceMetricErrors(output.metrics, 'correctness')]) differences.push(`${impl}: ${error}; baseline is invalid`);
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
  if (JSON.stringify(ts.wsRequests) !== JSON.stringify(rust.wsRequests))
    differences.push(
      `websocket requests differ: ts=${JSON.stringify(ts.wsRequests)} rust=${JSON.stringify(rust.wsRequests)}`,
    );
  const candidateBefore = structuredClone(rust);
  const overwritten = runAuditedFixer(values.venue, ts, applyTsCorrections);
  fixerReports.push({ run, ...overwritten });
  differences.push(...overwritten.errors);
  for (const [field, count] of Object.entries(overwritten.counts))
    console.log(`fixer ${values.venue} ${field}: ${count} overwritten field(s)`);
  console.log(`fixer ${values.venue}: ${overwritten.total} overwritten field(s)`);
  if (!isDeepStrictEqual(candidateBefore, rust)) differences.push('fixer modified Rust candidate output');
  if (isPrivate && ts.account && rust.account) {
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'flatPositionPnl', 'settings', 'fills']) {
      differences.push(...valueDifferences(ts.account[field], rust.account[field], `account ${field}`));
    }
  }
  differences.push(...valueDifferences(ts.commands, rust.commands, 'command results'));
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
    // Lighter closes a socket sending too many messages (30009), so Rust paces its
    // ticker subscribes; a Lighter unit test proves every market still gets one.
    // Kraken has no ping frame, so the Rust futures private socket keeps alive on the heartbeat feed.
    // Paradex caps a socket at 200 subscriptions, so Rust takes trades only for charted markets.
    // Binance futures: Rust streams !bookTicker for public-data-only accounts too (#2899); TS
    // skipped it there, leaving a guest the startup REST bid and ask.
    // Hyperliquid: Rust streams trades for each watched coin as its last; TS used the mid.
    const paced = (key) => field === 'subscriptions' && ((values.venue.startsWith('lighter') && key.includes('"channel":"ticker/')) ||
      (values.venue.startsWith('hyperliquid') && key.includes('"type":"trades"')) ||
      (values.venue === 'kraken' && key.includes('"feed":"heartbeat"')) ||
      (values.venue === 'paradex' && key.includes('"channel":"trades.')) ||
      (values.venue.startsWith('binance') && key.includes('"!bookTicker"')));
    const left = ts[field]?.filter((key) => !paced(key) && (field !== 'requests' || !implementationPlans.ts.has(key)));
    const right = rust[field]?.filter((key) => !paced(key) && (field !== 'requests' || !implementationPlans.rust.has(key)));
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
    differences.push(...valueDifferences(ts.books[symbol], rust.books[symbol], `book ${symbol}`));
  }
}

// Performance artifacts are separate from the fully audited correctness pair.
// A fast but incomplete workload cannot substitute for the measured tape.
if (!isPrivate) differences.push(...performanceRunDifferences(performanceResults, results, rawAudited));
// A re-measure decides only the CPU verdict: the discarded set's non-CPU failures stay.
if (remeasure) differences.push(...remeasure.firstDifferences);
const measuredResults = isPrivate || values['correctness-only'] ? results : performanceResults;
const summary = (impl) => ({
  cpuMs: median(measuredResults[impl].map((r) => r.metrics.cpuMs)),
  idleLoadCpuMs: median(measuredResults[impl].map((r) => r.metrics.idleLoadCpuMs)),
  runtimeCpuMs: median(measuredResults[impl].map((r) => r.metrics.runtimeCpuMs)),
  steadyCpuMs: median(measuredResults[impl].map((r) => r.metrics.steadyCpuMs)),
  processCpuMs: median(measuredResults[impl].map((r) => r.metrics.processCpuMs)),
  retainedMB: median(measuredResults[impl].map((r) => r.metrics.retainedBytes)) / 1e6,
  wasmMB: median(measuredResults[impl].map((r) => r.metrics.wasmBytes ?? 0)) / 1e6,
});
const t = summary('ts');
const r = summary('rust');
const shown = rustOnly ? rust : ts;
console.log(
  `frames ${shown.frames}, markets ${shown.markets.length}, tickers ${shown.tickers.length}, books ${Object.keys(shown.books).length}`,
);
const row = (label, a, b, unit = 'ms') =>
  console.log(
    `${label.padEnd(16)} ts ${a.toFixed(1).padStart(8)} ${unit}   rust ${b.toFixed(1).padStart(8)} ${unit}   ${(a / b).toFixed(2)}x`,
  );
row('main-thread cpu', t.cpuMs, r.cpuMs);
row('cold-load cpu', t.idleLoadCpuMs, r.idleLoadCpuMs);
row('runtime cpu', t.runtimeCpuMs, r.runtimeCpuMs);
row('steady cpu', t.steadyCpuMs, r.steadyCpuMs);
row('process cpu', t.processCpuMs, r.processCpuMs);
row('retained memory', t.retainedMB, r.retainedMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB; idle-loaded baseline; ${isPrivate ? 0 : runs} interleaved performance pairs; required improvement 10%)`);
console.log('(CPU includes cold implementation load and replay transport; audit snapshots, validators and baseline GC are excluded from performance runs)');
if (isPrivate) console.log('(private tape: correctness only; CPU and memory are informational)');
if (rustOnly) console.log('(retired TS oracle: Rust replayed alone against the tape\'s expected state)');
if (venue.family === 'orderly')
  console.log(
    `(documented Orderly corrections: ${ORDERLY_TS_FIXES.length}; native protocol and independent oracles checked)`,
  );
if (values.venue === 'bitrue' && isPrivate)
  console.log(`(documented Bitrue corrections: ${BITRUE_TS_FIXES.length}; exact fixture and TS baseline checked)`);
if (values.venue.startsWith('okx'))
  console.log(
    `(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`,
  );

if (venue.family === 'coinbase')
  console.log(
    `(documented Coinbase corrections: ${COINBASE_TS_FIXES.length}; native JWT and independent oracles checked)`,
  );

recorded?.finish();
// Never parity: the declared known failure keeps this strict result FAIL.
const failures = [...differences, ...(rustOnly ? [RUST_ONLY_FAILURE] : [])];
const warnings = [];
// A private tape is paced in real time, so it gates correctness only.
if (!isPrivate && !values['correctness-only']) {
  const measured = performanceVerdict(metricsOf('ts'), metricsOf('rust'), values.venue);
  const verdict = remeasure ? remeasuredVerdict(remeasure.first, measured) : measured;
  failures.push(...verdict.failures);
  warnings.push(...verdict.warnings);
}
warnings.sort();
// Report order must not depend on the arrival order of keyed replay rows.
failures.sort();
writeFileSync(values.report ?? `${outDir}/gate.json`, JSON.stringify({
  schema: 1, venue: values.venue, tape: values.tape,
  ...(ciWasm ? { ciWasm: ciWasm.metadata } : {}),
  tapeSha256: createHash('sha256').update(tapeBytes).digest('hex'),
  private: isPrivate, books: values.books, runs: isPrivate ? 0 : runs, correctnessRuns, requiredImprovement: 0.10,
  ...(rustOnly ? { rustOnly: true } : {}),
  ...(values['correctness-only'] ? { correctnessOnly: true, coverage: { correctness: 'complete', performance: 'not-run' } } : {}),
  status: failures.length ? 'FAIL' : 'PASS', failures, artifacts, fixers: fixerReports,
  ...(warnings.length ? { warnings } : {}), ...(remeasure ? { remeasure: { first: remeasure.first.failures, firstDifferences: remeasure.firstDifferences } } : {}),
  ...(recorded ? {} : { measurement: { cpu: cpuObservation, sampleOrder } }),
  summary: { ts: t, rust: r },
  samples: { ts: measuredResults.ts.map((output) => output.metrics), rust: measuredResults.rust.map((output) => output.metrics) },
}, null, 2) + '\n');
if (!recorded) writeCpuAnnotations({ venue: values.venue, tape: values.tape, failures, warnings,
  ...(!isPrivate && !values['correctness-only'] ? { samples: { ts: metricsOf('ts'), rust: metricsOf('rust') } } : {}),
  remeasure: remeasure && { first: remeasure.first.failures } });
if (failures.length) {
  console.log(`FAIL (${failures.length})`);
  for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
  process.exit(1);
}
console.log('PASS');
