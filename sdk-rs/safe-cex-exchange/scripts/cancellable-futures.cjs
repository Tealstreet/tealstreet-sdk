// Reviewed inventory is NOT inferred from the presence of a guard name.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const fingerprint = source => crypto.createHash('sha256').update(source).digest('hex');
function validate(discovered, reviewed, sources = {}) {
  assert.equal(reviewed.version, 1, 'unsupported cancellation inventory version');
  const actual = new Map();
  for (const row of discovered) {
    assert.ok(!actual.has(row.owner), `duplicate discovered future owner: ${row.owner}`);
    assert.ok(row.boundaries.length, `future owner without a discovered boundary: ${row.owner}`);
    actual.set(row.owner, row);
  }
  const seen = new Set();
  for (const row of reviewed.owners) {
    assert.ok(!seen.has(row.owner), `duplicate reviewed future owner: ${row.owner}`);
    seen.add(row.owner);
    const source = actual.get(row.owner);
    assert.ok(source, `stale cancellation classification: ${row.owner}`);
    assert.deepEqual(row.boundaries, source.boundaries, `future boundary inventory changed: ${row.owner}`);
    assert.equal(row.producer_sha256, fingerprint(source.producer_tokens), `future creator changed; cancellation review required: ${row.owner}`);
    assert.ok(['guarded', 'delegated', 'no-owned-latch', 'immediate', 'finding'].includes(row.classification), `unclassified cancellation owner: ${row.owner}`);
    assert.ok(typeof row.reason === 'string' && row.reason.trim().length, `blank cancellation reason: ${row.owner}`);
    if (source.boundaries.includes('shared-waiter-registration')) {
      assert.ok(['guarded', 'finding'].includes(row.classification), `shared waiter registration needs cleanup or a routed FINDING: ${row.owner}`);
    }
    if (row.classification === 'finding') {
      assert.ok(typeof row.routed_to === 'string' && row.routed_to.trim(), `FINDING must name its core routing record: ${row.owner}`);
    }
    if (row.classification === 'guarded') {
      assert.ok(Array.isArray(row.source_dependencies) && row.source_dependencies.length, `guard implementation must be pinned: ${row.owner}`);
      for (const dependency of row.source_dependencies) {
        assert.ok(typeof sources[dependency.file] === 'string', `guard source dependency missing: ${dependency.file}`);
        assert.equal(dependency.sha256, fingerprint(sources[dependency.file]), `guard implementation changed; cancellation review required: ${dependency.file}`);
      }
      assert.ok(Array.isArray(row.witnesses) && row.witnesses.length, `guarded future needs TASK-drop witnesses: ${row.owner}`);
      for (const witness of row.witnesses) {
        assert.ok(typeof witness === 'string' && witness.includes('::'), `invalid TASK-drop witness: ${row.owner}`);
      }
    }
    if (row.classification === 'delegated') {
      assert.ok(typeof row.delegate === 'string' && row.delegate.trim(), `delegation must name its owner: ${row.owner}`);
    }
  }
  for (const owner of actual.keys()) assert.ok(seen.has(owner), `uncovered production future; route to core: ${owner}`);
  return actual.size;
}
function requireNoFindings(reviewed) {
  const findings = reviewed.owners.filter(row => row.classification === 'finding');
  assert.equal(findings.length, 0, `OPEN production cancellation FINDINGS must be repaired: ${findings.map(row => row.owner).join(', ')}`);
}
module.exports = {validate, fingerprint, requireNoFindings};
if (require.main === module) {
  try {
    assert.equal(process.argv[2], '--check-discovery', 'expected --check-discovery <AST-export>');
    assert.equal(process.argv.length, 4, 'expected one AST-export path');
    const discovered = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
    const reviewed = JSON.parse(fs.readFileSync(path.join(root, 'tests/cancellable-futures.json'), 'utf8'));
    const sources = {};
    for (const row of reviewed.owners) for (const dependency of row.source_dependencies || []) {
      assert.ok(dependency.file.startsWith('src/') && !dependency.file.split('/').includes('..'), 'source dependency must stay in production src');
      sources[dependency.file] = fs.readFileSync(path.join(root, dependency.file), 'utf8');
    }
    const count = validate(discovered, reviewed, sources);
    const findings = reviewed.owners.filter(row => row.classification === 'finding');
    for (const finding of findings) console.error(`OPEN CANCELLATION FINDING: ${finding.owner}: ${finding.reason}`);
    console.log(`Cancellation discovery: ${count} inventoried creators; ${count-findings.length} classified without a known finding; ${findings.length} OPEN FINDINGS. This is not a semantic proof of cancellation safety.`);
    requireNoFindings(reviewed);
  } catch (error) { console.error(error.stack); process.exitCode = 1; }
}
