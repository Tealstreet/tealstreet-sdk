import assert from 'node:assert/strict';
import { test } from 'node:test';
import { diagnosticCensus } from './diagnostic-census.mjs';

const examples = [
  'invention: sample/src/lib.rs::Extra: public item has no manifest rs or exemption',
  'status: packages/source.ts::call (function): Rust item path must be call: sample/src/lib.rs::renamed; a rename note must name this exact Rust target',
  'status: packages/source.ts::call (function): Rust source file is not declared for packages/source.ts: sample/src/lib.rs::call; declare the exact crate/file in [file].rust_files',
  'status: packages/source.ts::call (function): rs does not resolve: sample/src/lib.rs::missing',
  'status: packages/source.ts::call (function): nonempty rs requires layer',
  'status: packages/source.ts: declared Rust source file has no ported or divergent row mapping: sample/src/unused.rs; remove unused [file].rust_files declaration',
  'invention: sample/src/lib.rs::Private: exemption does not name a public Rust item',
];

test('every diagnostic contributes once, including repeated diagnostics', () => {
  assert.deepEqual(diagnosticCensus([...examples, examples[1]].join('\r\n')), {
    diagnostics: 8,
    families: { status: 6, invention: 2 },
    classes: {
      'unmapped-public': 1,
      'rename-target': 2,
      'undeclared-file': 1,
      'unresolved-target': 1,
      'missing-layer': 1,
      'unused-file': 1,
      'stale-exemption': 1,
    },
  }, 'DIAGNOSTIC independently expected class counts');
});

test('unknown diagnostics and non-diagnostic output fail rather than disappear', () => {
  for (const extra of ['status: source: a new checker diagnostic', 'Finished build', 'invention: malformed']) {
    assert.throws(() => diagnosticCensus(`${examples[0]}\n${extra}`), /Unclassified or ambiguous/);
  }
  assert.throws(() => diagnosticCensus(' \n'), /No canonical diagnostics/);
});
