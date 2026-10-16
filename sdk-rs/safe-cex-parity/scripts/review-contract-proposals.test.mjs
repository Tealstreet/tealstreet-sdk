import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { compilerNames, normalizeCompiler, proposalKind, sourceEvidence, targetProposals, main } from './review-contract-proposals.mjs';
import { contractFor } from './accepted-contracts.mjs';
import { runProbes } from './accepted-probes.mjs';
import { buildScanner } from './accepted-scanner.mjs';

function fixture() {
  const repository = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-review-proposals-'));
  const workspace = path.join(repository, 'sdk-rs');
  const filename = path.join(workspace, 'example/src/lib.rs');
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  fs.writeFileSync(filename, 'pub struct A { value: bool }\npub fn helper(value: &str) -> bool { value.is_empty() }\npub type Recipes = String;\n');
  const span = (line = 1) => ({ filename: 'example/src/lib.rs', begin: [line, 1], end: [line, 20] });
  const generics = { params: [], where_predicates: [] };
  const item = (id, name, inner, line = 1) => ({ id, name, crate_id: 0, visibility: 'public', span: span(line), inner });
  const doc = { format_version: 61, root: '0', paths: {
    90: { path: ['serde', 'de', 'Deserialize'] },
    91: { path: ['core', 'clone', 'Clone'] },
    92: { path: ['alloc', 'string', 'String'] },
  }, index: {
    0: item('0', 'example', { module: { items: ['1', '2', '3', '6', '8'] } }),
    1: item('1', 'A', { struct: { kind: { plain: { fields: ['5'] } }, impls: ['2', '3'], generics } }),
    2: item('2', null, { impl: { for: { resolved_path: { id: '1', path: 'A' } }, trait: { id: '90' }, items: [] } }),
    3: item('3', null, { impl: { for: { resolved_path: { id: '1', path: 'A' } }, trait: { id: '91' }, items: [] } }),
    5: { ...item('5', 'value', { struct_field: { primitive: 'bool' } }), visibility: 'default' },
    6: item('6', 'helper', { function: { sig: { inputs: [['value', { borrowed_ref: { lifetime: null, is_mutable: false, type: { primitive: 'str' } } }]], output: { primitive: 'bool' }, is_c_variadic: false }, generics } }, 2),
    8: item('8', 'Recipes', { type_alias: { type: { resolved_path: { id: '92', path: 'String', args: null } }, generics } }, 3),
  } };
  const row = { crate: 'example', target: 'example', missingOwners: ['example::A', 'example::helper'], missingAliases: ['example::Recipes'], missingConstruction: ['example::A'], receiverDiagnostics: ['unreviewed external receiver'] };
  return { repository, workspace, doc, row, close: () => fs.rmSync(repository, { recursive: true, force: true }) };
}

test('review proposals retain compiler/source evidence and refusals without becoming contracts', () => {
  const f = fixture();
  try {
    const result = targetProposals(f.doc, f.row, f);
    assert.equal(result.proposals.length, 4);
    assert.deepEqual(result.receiverRefusals, ['unreviewed external receiver']);
    const fn = result.proposals.find(proposal => proposal.name === 'example::helper');
    assert.equal(fn.compiler.methods[0].signature.inputs[0][0], 'value');
    assert.equal(fn.classificationCandidate, 'review-required');
    assert.equal(fn.sourceEvidence[0].file, 'sdk-rs/example/src/lib.rs');
    assert.match(fn.sourceEvidence[0].excerpt, /value\.is_empty/);
    assert.match(fn.sourceEvidence[0].sourceSha256, /^[a-f0-9]{64}$/);
    for (const proposal of result.proposals) {
      assert.equal(proposal.status, 'needs-human-review');
      assert.equal(proposal.gatePermission, false);
      assert.equal(proposal.nativeEvidence.behaviorExecuted, false);
      assert.equal(Object.hasOwn(proposal, 'category'), false);
    }
    assert.throws(() => contractFor(fn.compiler, { [fn.name]: fn }), /unclassified or changed free-function decoder/);
    assert.equal(result.proposals.find(proposal => proposal.kind === 'construction').compiler.traits[0], 'core::clone::Clone');
  } finally { f.close(); }
});

test('stale, blocked and ambiguous compiler declarations cannot be presented as covered', () => {
  const f = fixture();
  try {
    assert.throws(() => targetProposals(f.doc, { ...f.row, error: 'reviewed source changed' }, f), /resolve blocked census inventory/);
    assert.throws(() => targetProposals(f.doc, { ...f.row, missingOwners: ['example::Missing'] }, f), /selected production owners must be inventoried/);
    assert.throws(() => targetProposals(f.doc, { ...f.row, missingAliases: ['example::Missing'] }, f), /stale census/);
    const doc = structuredClone(f.doc);
    doc.index['8'].name = 'helper';
    assert.throws(() => compilerNames(doc), /ambiguous compiler declaration/);
  } finally { f.close(); }
});

test('alias evidence resolves compiler identity and source evidence rejects paths outside the checkout', () => {
  const f = fixture();
  try {
    assert.deepEqual(normalizeCompiler(f.doc.index['8'].inner.type_alias.type, f.doc), { resolved_path: { path: 'alloc::string::String', args: null } });
    assert.throws(() => sourceEvidence({ filename: process.execPath, begin: [1, 1], end: [1, 1] }, f), /Rust file inside this checkout/);
    assert.throws(() => sourceEvidence({ filename: 'example/src/lib.rs', begin: [99, 1], end: [99, 2] }, f), /span outside file/);
    assert.equal(proposalKind({ deserialize: false, kind: 'record', rawEntrypoints: [false, true] }, 'owner').classificationCandidate, 'review-required');
    assert.equal(proposalKind({}, 'alias').classificationCandidate, 'review-required');
  } finally { f.close(); }
});

test('CLI refuses contract destinations and duplicate flags before running Cargo', () => {
  assert.throws(() => main(['--crate=x', '--crate=y', '--census=x']), /duplicate --crate/);
  assert.throws(() => main(['--crate=x', '--census=x', `--out=${fileURLToPath(new URL('../accepted-set/contracts/safe-cex-venue-example.json', import.meta.url))}`]), /output must be outside/);
  for (const jobs of ['0', '3', '-1', '1.5', '01', 'unlimited']) {
    assert.throws(() => main(['--crate=x', '--census=missing.json', `--jobs=${jobs}`]), /--jobs must be 1 or 2/);
    assert.throws(() => runProbes(undefined, undefined, undefined, undefined, { jobs }), /probe jobs must be 1 or 2/);
    assert.throws(() => buildScanner(jobs), /jobs must be 1 or 2/);
  }
  assert.throws(() => main(['--crate=x', '--census=x', '--jobs=1', '--jobs=2']), /duplicate --jobs/);
  for (const [args, message] of [[['--jobs=0'], /--jobs must be 1 or 2/], [['--jobs=1', '--jobs=2'], /duplicate --jobs/]]) {
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./accepted-set.mjs', import.meta.url)), ...args], { encoding: 'utf8' });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, message);
  }
});
