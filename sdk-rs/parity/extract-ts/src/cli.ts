import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { extractSources, kinds, manifestPathForSource, type SourceSymbols } from './extract.ts';
import { reconcile, type ReconcileResult } from './manifest.ts';

export function summary(sources: SourceSymbols[]): string {
  const byKind = Object.fromEntries(kinds.map((kind) => [kind, 0]));
  const byDirectory = new Map<string, number>();
  for (const source of sources) {
    const path = manifestPathForSource(source.ts).slice(0, -5);
    const directory = path.includes('/') ? path.split('/')[0] : '(root)';
    byDirectory.set(directory, (byDirectory.get(directory) ?? 0) + source.symbols.length);
    for (const symbol of source.symbols) byKind[symbol.kind]++;
  }
  return [
    `Symbols: ${sources.reduce((total, source) => total + source.symbols.length, 0)} across ${sources.length} source files`,
    'By kind:',
    ...Object.entries(byKind).map(([kind, count]) => `  ${kind}: ${count}`),
    'By top-level directory:',
    ...[...byDirectory.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
      .map(([directory, count]) => `  ${directory}: ${count}`),
  ].join('\n');
}

export function coverageExitCode(check: boolean, result: ReconcileResult): number {
  // Symbol coverage is independent of whether serialization changes a file.
  return check && (result.added.length || result.orphaned.length || result.changed.length) ? 1 : 0;
}

export function main(args: string[]): number {
  let check = false;
  let repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..');
  for (let index = 0; index < args.length; index++) {
    if (args[index] === '--check') check = true;
    else if (args[index] === '--repo-root' && args[index + 1] && !args[index + 1].startsWith('--')) {
      repoRoot = resolve(args[++index]);
    } else throw new Error(`Unknown or incomplete argument: ${args[index]}`);
  }
  const sources = extractSources(repoRoot);
  const result = reconcile(sources, resolve(repoRoot, 'sdk-rs/parity/manifest'), check);
  for (const symbol of result.added) console.log(`added: ${symbol}`);
  for (const symbol of result.orphaned) console.log(`orphaned: ${symbol}`);
  if (check) for (const file of result.changed) console.log(`changed: ${file}`);
  console.log(summary(sources));
  console.log(`${check ? 'Would update' : 'Updated'} ${result.changed.length} manifests; ${result.added.length} added, ${result.orphaned.length} orphaned symbols.`);
  return coverageExitCode(check, result);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
