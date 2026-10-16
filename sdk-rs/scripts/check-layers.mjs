import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, realpathSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const workspace = fileURLToPath(new URL('../', import.meta.url));
export function readInputs() {
  const policy = JSON.parse(readFileSync(path.join(workspace, 'layers.json'), 'utf8'));
  const metadata = JSON.parse(execFileSync(process.env.CARGO ?? 'cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], { cwd: workspace, encoding: 'utf8' }));
  return { policy, metadata };
}

export function validateLayers(policy, metadata) {
  const classes = new Map();
  const classify = (name, kind, layer) => {
    assert.equal(typeof name, 'string', 'package classification must name a package');
    assert.ok(!classes.has(name), `duplicate package classification ${name}`);
    classes.set(name, { kind, layer });
  };
  assert.ok(Array.isArray(policy.layers) && policy.layers.length, 'layers must be declared');
  const layerNames = new Set();
  policy.layers.forEach((layer, index) => {
    assert.ok(typeof layer.name === 'string' && !layerNames.has(layer.name), `duplicate/invalid layer ${layer.name}`);
    layerNames.add(layer.name);
    assert.ok(Array.isArray(layer.packages) && layer.packages.length, `empty layer ${layer.name}`);
    layer.packages.forEach(name => classify(name, 'runtime', index));
  });
  policy.tooling.forEach(name => classify(name, 'tooling'));
  policy.privateDerivations.forEach(name => classify(name, 'private'));
  // The public SDK must never import private venue derivations, including transitively.
  assert.equal(classes.get('safe-cex')?.kind, 'runtime', 'safe-cex must be a runtime package');
  for (const name of ['venue-data', 'safe-cex-venue-data']) assert.equal(classes.get(name)?.kind, 'private', `${name} must remain private derivations`);
  const packages = metadata.workspace_members.map(id => {
    const pkg = metadata.packages.find(pkg => pkg.id === id);
    assert.ok(pkg, `metadata missing workspace member ${id}`);
    return pkg;
  });
  const members = new Map(packages.map(pkg => [pkg.name, pkg]));
  const graph = new Map();
  for (const pkg of packages) {
    const edges = [];
    graph.set(pkg.name, edges);
    if (!classes.has(pkg.name)) {
      const incoming = packages.flatMap(parent => parent.dependencies.filter(dep => dep.name === pkg.name).map(() => `${parent.name} -> ${pkg.name}`));
      throw new Error(`unclassified crate: ${incoming.join(', ') || `workspace -> ${pkg.name}`}`);
    }
    for (const dep of pkg.dependencies) {
      const canonical = directory => existsSync(directory) ? realpathSync(directory) : path.resolve(directory);
      if (dep.path) {
        const segments = canonical(dep.path).split(path.sep);
        assert.ok(!policy.privateDerivations.some(name => segments.includes(name)), `private derivation path forbidden: ${pkg.name} -> ${dep.name} (${dep.path})`);
      }
      if (members.has(dep.name)) {
        assert.ok(classes.has(dep.name), `unclassified dependency: ${pkg.name} -> ${dep.name}`);
        const member = members.get(dep.name);
        assert.ok(dep.path && canonical(dep.path) === canonical(path.dirname(member.manifest_path)), `workspace package identity mismatch: ${pkg.name} -> ${dep.name} (${dep.path ?? dep.source ?? 'non-path dependency'})`);
      }
      // Cargo names the actual package even when a dependency is renamed.
      if (members.has(dep.name) || dep.path || classes.get(dep.name)?.kind === 'private') {
        assert.ok(classes.has(dep.name), `unclassified dependency: ${pkg.name} -> ${dep.name}`);
        assert.ok(members.has(dep.name) || classes.get(dep.name).kind === 'private', `classified dependency absent from workspace: ${pkg.name} -> ${dep.name}`);
        edges.push(dep.name);
      }
    }
  }
  for (const [name, info] of classes) {
    if (info.kind !== 'private') assert.ok(members.has(name), `classified package absent from workspace: ${name}`);
  }
  const exceptions = new Map();
  for (const edge of policy.exceptions) {
    const key = `${edge.from} -> ${edge.to}`;
    assert.ok(typeof edge.reason === 'string' && edge.reason.trim(), `exception requires reason: ${key}`);
    assert.ok(!exceptions.has(key), `duplicate exception: ${key}`);
    assert.ok(graph.get(edge.from)?.includes(edge.to), `stale exception: ${key}`);
    const from = classes.get(edge.from), to = classes.get(edge.to);
    assert.ok(from?.kind === 'runtime' && to?.kind === 'runtime' && from.layer - to.layer > 1, `only existing downward skips can be excepted: ${key}`);
    exceptions.set(key, edge.reason);
  }
  // Layer grants authorize potential venue edges; pair exceptions still require live edges.
  const layerGrants = new Set();
  for (const grant of policy.layerGrants ?? []) {
    const key = `${grant.fromLayer} -> ${grant.to}`;
    assert.equal(grant.fromLayer, 'venues', `layer grant restricted to venues: ${key}`);
    const sourceLayer = policy.layers.findIndex(layer => layer.name === grant.fromLayer);
    const target = classes.get(grant.to);
    assert.ok(sourceLayer >= 0 && target?.kind === 'runtime' && sourceLayer - target.layer > 1,
      `layer grant requires a named downward runtime skip: ${key}`);
    assert.ok(typeof grant.reason === 'string' && grant.reason.trim(), `layer grant requires reason: ${key}`);
    assert.ok(!layerGrants.has(key), `duplicate layer grant: ${key}`);
    layerGrants.add(key);
  }
  // Registry -> venue constructor edges are an explicit list, not implied by layering.
  const constructors = policy.registryConstructors;
  assert.ok(constructors && typeof constructors.from === 'string' && Array.isArray(constructors.venues), 'registryConstructors must be declared');
  assert.ok(typeof constructors.reason === 'string' && constructors.reason.trim(), 'registryConstructors requires reason');
  const venueLayer = policy.layers.findIndex(layer => layer.name === 'venues');
  const listed = new Set(constructors.venues);
  assert.equal(listed.size, constructors.venues.length, 'duplicate registry constructor venue');
  for (const venue of listed) {
    assert.equal(classes.get(venue)?.layer, venueLayer, `registry constructor target is not a venue: ${constructors.from} -> ${venue}`);
    assert.ok(graph.get(constructors.from)?.includes(venue), `stale registry constructor: ${constructors.from} -> ${venue}`);
  }
  for (const to of graph.get(constructors.from) ?? []) {
    if (classes.get(to)?.layer === venueLayer) assert.ok(listed.has(to), `unlisted registry constructor: ${constructors.from} -> ${to}`);
  }
  const walk = (name, trail) => {
    for (const to of graph.get(name) ?? []) {
      const next = [...trail, to];
      assert.notEqual(classes.get(to)?.kind, 'private', `private derivations forbidden: ${next.join(' -> ')}`);
      assert.ok(!trail.includes(to), `dependency cycle: ${next.join(' -> ')}`);
      walk(to, next);
    }
  };
  walk('safe-cex', ['safe-cex']);
  for (const [from, targets] of graph) for (const to of targets) {
    const source = classes.get(from), target = classes.get(to), edge = `${from} -> ${to}`;
    if (source.kind === 'tooling' || source.kind === 'private') continue;
    assert.equal(target.kind, 'runtime', `non-runtime dependency: ${edge}`);
    const distance = source.layer - target.layer;
    assert.notEqual(distance, 0, `lateral dependency: ${edge}`);
    assert.ok(distance > 0, `upward dependency: ${edge}`);
    assert.ok(distance === 1 || exceptions.has(edge) || layerGrants.has(`${policy.layers[source.layer].name} -> ${to}`), `skip dependency: ${edge}`);
  }
  return packages.length;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { policy, metadata } = readInputs();
  console.log(`Layering: ${validateLayers(policy, metadata)} deliberately classified workspace crates.`);
}
