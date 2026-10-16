import fs from 'node:fs';
import path from 'node:path';

const registry = 'registry+https://github.com/rust-lang/crates.io-index';
// Reviewed serde forwarding method arms, json/json_internal expression arms,
// and Token type-only arms.
// Updating one of these versions requires reviewing that macro's source again.
const reviewedVersions = { serde: ['1.0.229'], serde_json: ['1.0.152'], syn: ['2.0.119', '3.0.7'] };
function real(file) {
  if (!file) return undefined;
  try { return fs.realpathSync(file); }
  catch (error) { if (error.code === 'ENOENT') return undefined; throw error; }
}

// Return capabilities only for the active consumer binding and exact compiler
// artifact. A trusted spelling, target library name or path package is no proof.
export function verifiedMacroOrigins(doc, consumer, metadata, artifacts, sysroot) {
  const origins = [];
  const node = metadata.resolve?.nodes.find(node => node.id === consumer.id);
  const dependencies = node?.deps ?? [];
  const compilerCrates = Object.values(doc.external_crates ?? {});
  const actualSysroot = real(sysroot);
  for (const root of ['std', 'core']) {
    if (!actualSysroot || dependencies.some(dep => dep.name === root)) continue;
    const crates = compilerCrates.filter(crate => crate.name === root);
    if (crates.length !== 1) continue;
    const compilerPath = real(crates[0].path);
    if (!compilerPath) continue;
    const relative = path.relative(actualSysroot, compilerPath);
    if (relative && !relative.startsWith('..') && !path.isAbsolute(relative)
        && relative.split(path.sep).includes('rustlib')
        && path.basename(compilerPath).startsWith(`lib${root}-`)) origins.push(root);
  }
  for (const root of Object.keys(reviewedVersions)) {
    const bindings = dependencies.filter(dep => dep.name === root);
    if (bindings.length !== 1) continue;
    const dependency = metadata.packages.find(pkg => pkg.id === bindings[0].pkg);
    if (dependency?.name !== root || dependency.source !== registry || !reviewedVersions[root].includes(dependency.version)) continue;
    // Renamed dependencies may share a library's internal name. Resolve the
    // active binding through its actual package/artifact, not that name count.
    const selected = compilerCrates.filter(crate => crate.name === root).filter(crate => {
      const compilerPath = real(crate.path);
      if (!compilerPath) return false;
      const matching = artifacts.filter(artifact => artifact.filenames?.some(file => real(file) === compilerPath));
      return matching.length === 1 && matching[0].package_id === dependency.id;
    });
    if (root === 'serde') {
      // The facade reexports this macro from serde_core. Verify the provider
      // selected by that facade, including patched dependency substitutions.
      const providerBindings = metadata.resolve?.nodes.find(node => node.id === dependency.id)?.deps.filter(dep => dep.name === 'serde_core') ?? [];
      if (providerBindings.length !== 1) continue;
      const provider = metadata.packages.find(pkg => pkg.id === providerBindings[0].pkg);
      if (provider?.name !== 'serde_core' || provider.source !== registry || provider.version !== '1.0.229') continue;
      const providers = compilerCrates.filter(crate => crate.name === 'serde_core').filter(crate => {
        const compilerPath = real(crate.path);
        if (!compilerPath) return false;
        const matching = artifacts.filter(artifact => artifact.filenames?.some(file => real(file) === compilerPath));
        return matching.length === 1 && matching[0].package_id === provider.id;
      });
      if (providers.length !== 1) continue;
    }
    if (selected.length === 1) origins.push(root);
  }
  return origins.sort();
}
