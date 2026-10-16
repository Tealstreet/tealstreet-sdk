// Audit WEEX's actual Cargo dependency closure against the shared layer policy.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const sdk = path.resolve(__dirname, '../..');
const cargo = process.env.CARGO || '/opt/homebrew/opt/rustup/bin/cargo';
const metadata = JSON.parse(execFileSync(cargo, ['metadata', '--manifest-path', path.join(sdk, 'Cargo.toml'), '--locked', '--no-deps', '--format-version', '1'], { encoding: 'utf8' }));
const policy = JSON.parse(fs.readFileSync(path.join(sdk, 'layers.json'), 'utf8'));
const packages = new Map(metadata.packages.filter(p => metadata.workspace_members.includes(p.id)).map(p => [p.name, p]));
const layers = new Map(policy.layers.flatMap((layer, i) => layer.packages.map(name => [name, { index: i, name: layer.name }])));
const seen = new Set();
let edges = 0;
function visit(name, trail = []) {
  assert.ok(!trail.includes(name), `cycle: ${[...trail, name]}`);
  if (seen.has(name)) return;
  seen.add(name);
  const pkg = packages.get(name);
  assert.ok(pkg, `missing workspace package ${name}`);
  assert.ok(layers.has(name), `unclassified runtime package ${name}`);
  for (const dep of pkg.dependencies) {
    assert.ok(!policy.privateDerivations.includes(dep.name), `private dependency ${name} -> ${dep.name}`);
    if (dep.path) {
      const segments = fs.realpathSync(dep.path).split(path.sep);
      assert.ok(!policy.privateDerivations.some(p => segments.includes(p)), `private path ${dep.path}`);
    }
    if (!dep.path && !packages.has(dep.name)) continue;
    const to = packages.get(dep.name);
    assert.ok(to && layers.has(dep.name), `non-runtime path dependency ${name} -> ${dep.name}`);
    assert.equal(fs.realpathSync(dep.path), fs.realpathSync(path.dirname(to.manifest_path)), `package identity ${name} -> ${dep.name}`);
    const source = layers.get(name), target = layers.get(dep.name);
    const distance = source.index - target.index;
    assert.ok(distance > 0, `upward/lateral dependency ${name} -> ${dep.name}`);
    assert.ok(distance === 1 || policy.exceptions.some(e => e.from === name && e.to === dep.name && e.reason.trim()) || (policy.layerGrants || []).some(g => g.fromLayer === source.name && g.to === dep.name && g.reason.trim()), `ungranted skip ${name} -> ${dep.name}`);
    edges++;
    visit(dep.name, [...trail, name]);
  }
}
visit('safe-cex-venue-weex');
console.log(`WEEX layer closure: ${seen.size} runtime crates, ${edges} workspace edges checked`);
