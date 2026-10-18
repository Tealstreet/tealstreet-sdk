import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { verifiedMacroOrigins } from './accepted-macro-origins.mjs';

function fixture(check) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-macro-origin-'));
  try {
    const sysroot = path.join(root, 'toolchain');
    const std = path.join(sysroot, 'lib/rustlib/target/lib/libstd-hash.rmeta');
    const syn = path.join(root, 'libsyn-hash.rmeta');
    const json = path.join(root, 'libserde_json-hash.rmeta');
    fs.mkdirSync(path.dirname(std), { recursive: true });
    for (const file of [std, syn, json]) fs.writeFileSync(file, 'compiler fixture');
    const consumer = { id: 'consumer' };
    const doc = { external_crates: { 1: { name: 'std', path: std }, 2: { name: 'syn', path: syn }, 3: { name: 'serde_json', path: json } } };
    const packages = ['syn', 'serde_json'].map(name => ({ id: `registry:${name}`, name, version: name === 'syn' ? '3.0.7' : '1.0.152', source: 'registry+https://github.com/rust-lang/crates.io-index' }));
    const metadata = { packages, resolve: { nodes: [{ id: consumer.id, deps: packages.map(pkg => ({ name: pkg.name, pkg: pkg.id })) }] } };
    const artifacts = packages.map((pkg, index) => ({ package_id: pkg.id, filenames: [[syn, json][index]] }));
    check({ root, sysroot, std, syn, json, consumer, doc, metadata, artifacts });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}
test('expression macro origins require active registry bindings and exact compiler artifacts', () => fixture(({ root, doc, consumer, metadata, artifacts, sysroot }) => {
  assert.deepEqual(verifiedMacroOrigins(doc, consumer, metadata, artifacts, sysroot), ['serde_json', 'std', 'syn']);
  const changed = structuredClone(metadata);
  changed.packages[0].version = '999.0.0';
  assert.deepEqual(verifiedMacroOrigins(doc, consumer, changed, artifacts, sysroot), ['serde_json', 'std']);
  changed.packages[0].version = '3.0.7';
  changed.packages[0].source = null;
  assert.deepEqual(verifiedMacroOrigins(doc, consumer, changed, artifacts, sysroot), ['serde_json', 'std']);
  changed.packages[0].source = metadata.packages[0].source;
  changed.packages[0].name = 'vendor-syn';
  assert.deepEqual(verifiedMacroOrigins(doc, consumer, changed, artifacts, sysroot), ['serde_json', 'std']);
  assert.deepEqual(verifiedMacroOrigins(doc, consumer, metadata, [...artifacts, artifacts[0]], sysroot), ['serde_json', 'std']);
  assert.deepEqual(verifiedMacroOrigins(doc, consumer, metadata, [{ ...artifacts[0], package_id: 'substitution' }, artifacts[1]], sysroot), ['serde_json', 'std']);
  for (const [index, name] of ['serde', 'serde_core'].entries()) {
    const file = path.join(root, `lib${name}-hash.rmeta`); fs.writeFileSync(file, 'compiler fixture');
    doc.external_crates[index + 4] = { name, path: file };
    metadata.packages.push({ id: `registry:${name}`, name, version: '1.0.229', source: 'registry+https://github.com/rust-lang/crates.io-index' });
    artifacts.push({ package_id: `registry:${name}`, filenames: [file] });
  }
  metadata.resolve.nodes[0].deps.push({ name: 'serde', pkg: 'registry:serde' });
  metadata.resolve.nodes.push({ id: 'registry:serde', deps: [{ name: 'serde_core', pkg: 'registry:serde_core' }] });
  assert.ok(verifiedMacroOrigins(doc, consumer, metadata, artifacts, sysroot).includes('serde'));
  for (const mutate of [
    graph => { graph.packages.find(pkg => pkg.name === 'serde').version = '999.0.0'; },
    graph => { graph.packages.find(pkg => pkg.name === 'serde').source = null; },
    graph => { graph.packages.find(pkg => pkg.name === 'serde_core').version = '999.0.0'; },
    graph => { graph.packages.find(pkg => pkg.name === 'serde_core').source = null; },
    graph => { graph.resolve.nodes[1].deps[0].pkg = 'vendor'; },
    graph => { graph.resolve.nodes[0].deps.find(dep => dep.name === 'serde').name = 'renamed'; },
  ]) {
    const graph = structuredClone(metadata); mutate(graph);
    assert.ok(!verifiedMacroOrigins(doc, consumer, graph, artifacts, sysroot).includes('serde'), 'SERDE-FORWARD requires the actual reviewed facade and provider');
  }
  for (const compiled of [artifacts.slice(0, -1), [...artifacts, artifacts.at(-1)], artifacts.map(row => row.package_id === 'registry:serde_core' ? { ...row, package_id: 'vendor' } : row)]) {
    assert.ok(!verifiedMacroOrigins(doc, consumer, metadata, compiled, sysroot).includes('serde'), 'SERDE-FORWARD refuses missing, duplicate or substituted provider artifacts');
  }
}));
test('macro namespace capabilities reject missing, renamed and ambiguous bindings', () => fixture(({ doc, consumer, metadata, artifacts, sysroot }) => {
  for (const mutate of [
    value => { value.resolve.nodes[0].deps[0].name = 'parser'; },
    value => { value.resolve.nodes[0].deps.push(value.resolve.nodes[0].deps[0]); },
    value => { value.resolve.nodes = []; },
  ]) {
    const changed = structuredClone(metadata); mutate(changed);
    assert.ok(!verifiedMacroOrigins(doc, consumer, changed, artifacts, sysroot).includes('syn'));
  }
  const changedDoc = structuredClone(doc); changedDoc.external_crates[2].path = changedDoc.external_crates[3].path;
  assert.ok(!verifiedMacroOrigins(changedDoc, consumer, metadata, artifacts, sysroot).includes('syn'));
}));
test('builtin macro capabilities follow sysroot realpaths and refuse Cargo shadows', () => fixture(({ root, doc, consumer, metadata, artifacts, sysroot }) => {
  const changed = structuredClone(metadata); changed.resolve.nodes[0].deps.push({ name: 'std', pkg: 'vendor' });
  assert.ok(!verifiedMacroOrigins(doc, consumer, changed, artifacts, sysroot).includes('std'));
  const outside = path.join(root, 'libstd-outside.rmeta'); fs.writeFileSync(outside, 'outside');
  const link = path.join(sysroot, 'lib/rustlib/target/lib/libstd-link.rmeta'); fs.symlinkSync(outside, link);
  const changedDoc = structuredClone(doc); changedDoc.external_crates[1].path = link;
  assert.ok(!verifiedMacroOrigins(changedDoc, consumer, metadata, artifacts, sysroot).includes('std'));
}));

test('renamed same-name libraries require the active registry compiler artifact', () => fixture(({ root, doc, consumer, metadata, artifacts, sysroot }) => {
  const vendorPath = path.join(root, 'libserde_json-vendor.rmeta');
  fs.writeFileSync(vendorPath, 'untrusted compiler fixture');
  const compiler = structuredClone(doc);
  compiler.external_crates[4] = { name: 'serde_json', path: vendorPath };
  const graph = structuredClone(metadata);
  graph.packages.push({ id: 'vendor', name: 'vendor-serde-json', version: '0.1.0', source: null });
  graph.resolve.nodes[0].deps.push({ name: 'serde_json_vendor', pkg: 'vendor' });
  const compiled = [...artifacts, { package_id: 'vendor', filenames: [vendorPath] }];
  assert.deepEqual(verifiedMacroOrigins(compiler, consumer, graph, compiled, sysroot), ['serde_json', 'std', 'syn'], 'MACRO-ORIGIN active registry identity survives an unrelated renamed internal name');
  const shadow = structuredClone(graph);
  shadow.resolve.nodes[0].deps.find(dep => dep.name === 'serde_json').pkg = 'vendor';
  assert.ok(!verifiedMacroOrigins(compiler, consumer, shadow, compiled, sysroot).includes('serde_json'), 'MACRO-ORIGIN active vendor binding must not inherit registry permission');
  const duplicate = structuredClone(compiler);
  duplicate.external_crates[5] = duplicate.external_crates[3];
  assert.ok(!verifiedMacroOrigins(duplicate, consumer, graph, compiled, sysroot).includes('serde_json'), 'MACRO-ORIGIN duplicate selected compiler records must refuse');
  assert.ok(!verifiedMacroOrigins(compiler, consumer, graph, [...compiled, artifacts[1]], sysroot).includes('serde_json'), 'MACRO-ORIGIN duplicate selected artifacts must refuse');
  const substituted = compiled.map(artifact => artifact.package_id === 'registry:serde_json' ? { ...artifact, package_id: 'vendor' } : artifact);
  assert.ok(!verifiedMacroOrigins(compiler, consumer, graph, substituted, sysroot).includes('serde_json'), 'MACRO-ORIGIN substituted package identity must refuse');
}));
