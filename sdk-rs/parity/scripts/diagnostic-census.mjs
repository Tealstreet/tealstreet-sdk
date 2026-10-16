import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const classes = [
  ['unmapped-public', /^invention: \S+: public item has no manifest rs or exemption$/],
  ['rename-target', /^status: .+ \([^)]+\): Rust item path must be .+: \S+; a rename note must name this exact Rust target$/],
  ['undeclared-file', /^status: .+ \([^)]+\): Rust source file is not declared for \S+: \S+; declare the exact crate\/file in \[file\]\.rust_files$/],
  ['unresolved-target', /^status: .+ \([^)]+\): rs does not resolve: \S+$/],
  ['missing-layer', /^status: .+ \([^)]+\): nonempty rs requires layer$/],
  ['unused-file', /^status: \S+: declared Rust source file has no ported or divergent row mapping: \S+; remove unused \[file\]\.rust_files declaration$/],
  ['stale-exemption', /^invention: \S+: exemption does not name a public Rust item$/],
];

export function diagnosticCensus(output) {
  const counts = Object.fromEntries(classes.map(([name]) => [name, 0]));
  const families = { status: 0, invention: 0 };
  let diagnostics = 0;
  for (const [index, line] of output.split(/\r?\n/).entries()) {
    if (!line.trim()) continue;
    const matches = classes.filter(([, pattern]) => pattern.test(line));
    if (matches.length !== 1) {
      throw new Error(`Unclassified or ambiguous canonical output at line ${index + 1}: ${line}`);
    }
    counts[matches[0][0]] += 1;
    families[line.startsWith('status: ') ? 'status' : 'invention'] += 1;
    diagnostics += 1;
  }
  if (diagnostics === 0) throw new Error('No canonical diagnostics found; this tool requires a failing checker log');
  return { diagnostics, families, classes: counts };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  try {
    if (process.argv.length !== 3) throw new Error('Usage: node diagnostic-census.mjs <direct-checker-log>');
    console.log(JSON.stringify(diagnosticCensus(readFileSync(process.argv[2], 'utf8')), null, 2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
