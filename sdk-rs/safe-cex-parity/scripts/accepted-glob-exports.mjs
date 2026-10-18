// Exact active workspace dependency exports for the source import guards.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const prefix = 'accepted-set: dependency export assertion';
function real(file) {
  if (typeof file !== 'string') return undefined;
  try { return fs.realpathSync(file); }
  catch (error) { if (error.code === 'ENOENT') return undefined; throw error; }
}

export function dependencyExportProvider(root, { consumer, doc, metadata, artifacts }) {
  const bindings = metadata.resolve?.nodes.find(node => node.id === consumer.id)?.deps.filter(dep => dep.name === root) ?? [];
  if (!bindings.length) return undefined;
  assert.equal(bindings.length, 1, `${prefix}: ambiguous binding ${root}`);
  const matches = metadata.packages.filter(pkg => pkg.id === bindings[0].pkg);
  assert.equal(matches.length, 1, `${prefix}: missing or ambiguous package ${root}`);
  const pkg = matches[0];
  // Registry/sysroot capabilities keep their existing separate origin proof.
  if (!metadata.workspace_members.includes(pkg.id)) return undefined;
  const libraries = pkg.targets.filter(target => target.kind.includes('lib'));
  assert.equal(libraries.length, 1, `${prefix}: dependency needs one library ${root}`);
  const library = libraries[0];
  const source = real(library.src_path);
  assert.ok(source, `${prefix}: missing library source ${root}`);
  const selected = artifacts.filter(artifact => artifact.package_id === pkg.id
    && artifact.target?.name === library.name && artifact.target.kind?.includes('lib')
    && real(artifact.target.src_path) === source && artifact.profile?.test === false);
  assert.equal(selected.length, 1, `${prefix}: missing or ambiguous active compiler artifact ${root}`);
  const artifact = selected[0];
  const external = Object.values(doc.external_crates ?? {}).filter(crate => {
    const file = real(crate.path);
    return file && artifact.filenames.some(candidate => real(candidate) === file);
  });
  assert.equal(external.length, 1, `${prefix}: missing or ambiguous compiler dependency ${root}`);
  assert.equal(external[0].name, library.name.replaceAll('-', '_'), `${prefix}: compiler library name changed ${root}`);
  assert.ok(Array.isArray(artifact.features) && artifact.features.every(feature => typeof feature === 'string' && /^[A-Za-z0-9_-]+$/.test(feature))
    && new Set(artifact.features).size === artifact.features.length, `${prefix}: invalid active features ${root}`);
  return { pkg, library, features: [...artifact.features].sort() };
}

export function publicRootExportNames(doc, { workspace, library }) {
  const root = doc.index?.[doc.root];
  assert.ok(root?.inner?.module?.is_crate === true && root.inner.module.is_stripped === false
    && root.crate_id === 0 && root.name === library.name.replaceAll('-', '_'), `${prefix}: missing complete compiler root ${library.name}`);
  const source = real(library.src_path);
  assert.ok(source, `${prefix}: missing library source ${library.name}`);
  assert.equal(real(path.resolve(workspace, root.span?.filename ?? '')), source, `${prefix}: compiler root source changed ${library.name}`);
  const ids = root.inner.module.items;
  assert.ok(Array.isArray(ids) && new Set(ids).size === ids.length, `${prefix}: invalid root item inventory`);
  const names = [];
  for (const id of ids) {
    const item = doc.index[id];
    assert.ok(item && item.crate_id === 0, `${prefix}: missing root item ${id}`);
    assert.ok(['public', 'default', 'crate'].includes(item.visibility) || item.visibility?.restricted, `${prefix}: missing item visibility ${id}`);
    if (item.visibility !== 'public') continue;
    const imported = item.inner?.use;
    assert.ok(!imported?.is_glob, `${prefix}: unresolved public glob reexport ${library.name}`);
    const name = imported?.name ?? item.name;
    assert.ok(typeof name === 'string' && name.length, `${prefix}: missing public export name ${id}`);
    names.push(name.replace(/^r#/, ''));
  }
  assert.equal(new Set(names).size, names.length, `${prefix}: ambiguous public export names ${library.name}`);
  return names.sort();
}
