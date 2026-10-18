// Prepare finite alias cohorts with the canonical compiler/artifact/claim checks.
// Classification and runtime probing remain in the maintained alias/probe engines.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { inventory } from './accepted-inventory.mjs';
import { aliasClaim, aliasTargetIdentity } from './accepted-aliases.mjs';
import { verifiedMacroOrigins } from './accepted-macro-origins.mjs';
import { buildScanner } from './accepted-scanner.mjs';
import { loadPolicy } from './accepted-policy-files.mjs';

export function acceptedAliasContext({ workspace, packageName, aliasNames, features = [], jobs = 1 }) {
  const root = fs.realpathSync(workspace);
  assert.ok([1, 2].includes(jobs));
  assert.ok(Array.isArray(aliasNames) && aliasNames.length &&
    aliasNames.every(name => typeof name === 'string') && new Set(aliasNames).size === aliasNames.length);
  assert.ok(Array.isArray(features) && features.every(feature => /^[a-zA-Z0-9_-]+$/.test(feature)));
  function run(command, args, env = {}) {
    const result = spawnSync(command, args, {
      cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, CARGO_BUILD_JOBS: String(jobs), ...env },
    });
    assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
    return result.stdout;
  }
  const featureArgs = features.length ? ['--features', features.join(',')] : [];
  const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--format-version=1',
    ...(features.length ? ['--features', features.map(feature => `${packageName}/${feature}`).join(',')] : [])]));
  const members = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id));
  const matches = members.filter(pkg => pkg.name === packageName);
  assert.equal(matches.length, 1, 'alias context requires one real workspace package');
  const pkg = matches[0];
  const targets = pkg.targets.filter(target => target.kind.includes('lib'));
  assert.equal(targets.length, 1, 'alias context requires one production library');
  const target = targets[0];
  const scanner = buildScanner(jobs);
  run(scanner, ['--doc-guards', target.src_path]);
  const discovery = path.join(metadata.target_directory, 'accepted-inventory');
  const discoveryEnv = { CARGO_TARGET_DIR: discovery };
  run('cargo', ['rustdoc', '--locked', '-p', packageName, '--jobs', String(jobs), '--lib',
    ...featureArgs, '--', '-Z', 'unstable-options', '--output-format=json',
    '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'],
  { ...discoveryEnv, RUSTC_BOOTSTRAP: '1' });
  const doc = JSON.parse(fs.readFileSync(path.join(discovery, 'doc', `${target.name.replaceAll('-', '_')}.json`)));
  const artifacts = run('cargo', ['check', '--locked', '-p', packageName, '--jobs', String(jobs),
    '--lib', ...featureArgs, '--message-format=json'], discoveryEnv).trim().split('\n')
    .filter(Boolean).map(line => JSON.parse(line)).filter(item => item.reason === 'compiler-artifact');
  assert.ok(artifacts.some(item => item.package_id === pkg.id && item.target.name === target.name),
    'alias context requires the production compiler artifact');
  const sysroot = run('rustc', ['--print', 'sysroot']).trim();
  const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
  const found = inventory(doc, {
    defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'),
    readSource: file => fs.readFileSync(path.resolve(root, file)),
  });
  // Authenticate every discovered external root before selecting the finite cohort.
  for (const alias of found.externalTypeAliases) {
    alias.artifact = aliasTargetIdentity(alias, artifacts, members, sysroot);
  }
  const aliases = found.publicAliases.filter(alias => aliasNames.includes(alias.name));
  assert.deepEqual(aliases.map(alias => alias.name).sort(), aliasNames.slice().sort());
  const aliasSources = [...new Set(aliases.map(alias =>
    path.relative(root, path.resolve(root, alias.span.filename)).split(path.sep).join('/')))];
  const claims = JSON.parse(run(scanner, ['--alias-claims-json', root, ...aliasSources]));
  for (const alias of aliases) {
    alias.claim = aliasClaim(alias, {
      packageName, crateRoot: path.dirname(pkg.manifest_path), workspace: root,
      claims, nativeTooling: false,
    });
  }
  return { root, metadata, pkg, target, found, aliases, macroOrigins, artifacts, sysroot };
}
