import assert from 'node:assert/strict';

// A pending row records classification debt. It grants no decoder/alias
// permission and never removes an owner from inventory or normal measurements.
export function requirePendingNativeInputs(packageName, owners, contracts, ledger) {
  const label = 'accepted-set: native input pending ledger';
  assert.equal(ledger?.version, 1, `${label}: unsupported version`);
  assert.ok(Array.isArray(ledger.owners), `${label}: exact owner rows required`);
  const rows = new Map();
  for (const row of ledger.owners) {
    assert.equal(typeof row.name, 'string', `${label}: owner name required`);
    assert.equal(typeof row.package, 'string', `${label}: package required`);
    assert.match(row.name, /^[a-z][a-z0-9_]*(?:::[A-Za-z_][A-Za-z_0-9]*)+$/, `${label}: exact compiler name required`);
    assert.equal(row.name.split('::')[0].replaceAll('_', '-'), row.package, `${label}: package identity differs`);
    assert.ok(!rows.has(row.name), `${label}: duplicate owner ${row.name}`);
    assert.ok(['pending', 'classified', 'removed'].includes(row.state), `${label}: unknown state ${row.name}`);
    if (row.state !== 'pending') assert.ok(typeof row.ruling === 'string' && row.ruling.trim(), `${label}: recorded ruling required for ${row.name}`);
    rows.set(row.name, row);
  }
  const actual = new Map(owners.map(owner => [owner.name, owner]));
  const hasContract = name => ['decoder', 'non-decoder'].includes(contracts[name]?.category) || contracts[name]?.shape === 'observed';
  for (const row of rows.values()) {
    if (row.package !== packageName) continue;
    if (row.state === 'removed') {
      assert.ok(!actual.has(row.name), `${label}: removed owner still exists ${row.name}`);
    } else {
      assert.ok(actual.has(row.name), `${label}: listed owner disappeared without removal ruling ${row.name}`);
      assert.equal(hasContract(row.name), row.state === 'classified', `${label}: classification state/ruling differs for ${row.name}`);
    }
  }
  for (const owner of owners) {
    if (!owner.nativeInputDiscovered || hasContract(owner.name)) continue;
    const row = rows.get(owner.name);
    assert.ok(row && row.package === packageName && row.state === 'pending', `${label}: unlisted native input owner ${owner.name}`);
  }
  return { package: packageName, pending: [...rows.values()].filter(row => row.package === packageName && row.state === 'pending').length };
}
