// Scoped source census using the maintained extractor; this is not a package typecheck.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import ts from 'typescript';
import { extractFile } from '../../parity/extract-ts/src/extract.ts';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const files = ['kraken.types', 'krakenspot.types', 'kraken.fills', 'kraken.income', 'kraken.api', 'kraken.exchange', 'krakenspot.exchange'].map(name => `packages/safe-cex/src/exchanges/kraken/${name}.ts`);
// One Python process parses the entire manifest corpus; never spawn per source file.
const manifestPaths = files.map(file => path.join(root, 'sdk-rs/parity/manifest/exchanges/kraken', path.basename(file) + '.toml'));
const manifests = JSON.parse(execFileSync('python3.12', [
  '-c', 'import json,sys,tomllib; print(json.dumps([tomllib.load(open(p, "rb")) for p in sys.argv[1:]]))',
  ...manifestPaths,
], { encoding: 'utf8' }));
assert.equal(manifests.length, files.length, 'manifest corpus length');
const program = ts.createProgram(files.map(file => path.join(root, file)), {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, skipLibCheck: true,
});
const checker = program.getTypeChecker();
let count = 0;
for (const [index, file] of files.entries()) {
  const source = program.getSourceFile(path.join(root, file));
  assert.ok(source, file);
  const extracted = extractFile(source, checker, path.join(root, 'packages/safe-cex'));
  const manifest = manifests[index];
  assert.deepEqual(manifest.symbol.map(({ts, kind}) => ({ts, kind})), extracted, `scoped manifest census ${file}`);
  count += extracted.length;
}
console.log(`Scoped maintained extractor: ${count} symbols across ${files.length} slice files; no package typecheck claim`);
