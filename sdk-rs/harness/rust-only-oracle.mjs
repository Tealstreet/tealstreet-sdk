// A tape whose TS implementation is retired declares `meta.oracle: 'rust-only'` and
// carries its expected state in `meta.expected`. The gate replays Rust alone, keeps
// every per-implementation integrity check, and compares Rust to that state. The
// strict result is never PASS: RUST_ONLY_FAILURE must be a declared known failure.
import { memoryMetricErrors } from './memory.mjs';
import { performanceMetricErrors } from './performance-policy.mjs';
import { checkpointCoverageErrors, duplicateSymbols, snapshotCoverageErrors, valueDifferences } from './snapshot-compare.mjs';

export const RUST_ONLY = 'rust-only';
export const RUST_ONLY_FAILURE = 'retired TS oracle: Rust replayed alone against the tape expected state';
const ACCOUNT_FIELDS = ['hedged', 'balance', 'orders', 'positions', 'flatPositionPnl', 'settings', 'fills'];

export function rustOnlyOracle(records, isPrivate) {
  const meta = records.find((record) => record.type === 'meta');
  if (meta?.oracle === undefined) return false;
  if (meta.oracle !== RUST_ONLY) throw new Error(`unknown replay oracle ${meta.oracle}`);
  const expected = meta.expected;
  if (!isPrivate || !expected || typeof expected !== 'object' || !expected.account || !Array.isArray(expected.commands)
      || ACCOUNT_FIELDS.some((field) => !Object.hasOwn(expected.account, field)))
    throw new Error('a rust-only tape must be private and declare its complete expected account and command results');
  return true;
}

export function rustOnlyDifferences(rust, { tapeRecords, checkpointPlan, books, run, venue }) {
  const expected = tapeRecords.find((record) => record.type === 'meta').expected;
  const found = [
    ...snapshotCoverageErrors(rust, 'rust final', { isPrivate: true, venue }),
    ...checkpointCoverageErrors(rust.checkpoints, checkpointPlan, 'rust', { isPrivate: true, venue }),
    ...['markets', 'tickers'].flatMap((field) => duplicateSymbols(rust[field], `rust ${field}`)),
    ...(rust.duplicateSymbols ?? []).map((error) => `rust: ${error}`),
  ];
  if (!rust.account) found.push('rust: account presence differs from tape classification');
  for (const error of [...memoryMetricErrors(rust.metrics), ...performanceMetricErrors(rust.metrics, 'correctness')])
    found.push(`rust: ${error}; baseline is invalid`);
  if (rust.unusedWrites) found.push(`rust: ${rust.unusedWrites} taped mutation(s) never sent`);
  for (const error of rust.protocolErrors ?? []) found.push(`rust: ${error}`);
  for (const symbol of books.split(',').filter(Boolean)) if (!rust.books[symbol]) found.push(`rust: requested book ${symbol} is missing`);
  if (!rust.markets.length || !rust.tickers.length) found.push('rust: empty public seed');
  if (rust.unserved) found.push(`rust: ${rust.unserved} request(s) matched nothing on the tape`);
  if (rust.missingWrites) found.push(`rust: ${rust.missingWrites} required taped write(s) never sent`);
  if (rust.invalidSignatures) found.push(`rust: ${rust.invalidSignatures} invalid signatures`);
  if (rust.account) for (const field of ACCOUNT_FIELDS)
    found.push(...valueDifferences(expected.account[field], rust.account[field], `expected account ${field}`));
  found.push(...valueDifferences(expected.commands, rust.commands, 'expected command results'));
  return found.map((difference) => (run ? `run ${run + 1}: ${difference}` : difference));
}
