import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { inventory } from './accepted-inventory.mjs';
import { loadContracts, defaultContractDirectory } from './accepted-contract-files.mjs';
import { runProbes } from './accepted-probes.mjs';
import { reviewedLocalHelper, reviewedGenericDecoderHelper, requireNativeDecoderObservations } from './accepted-local-helpers.mjs';
import { aliasClaim, aliasTargetIdentity, toolingAliasExemption } from './accepted-aliases.mjs';
import { verifiedMacroOrigins } from './accepted-macro-origins.mjs';
import { verifiedNumberOutputOrigin, verifiedNumberOutputDefinition, numberOutputDefinitionCovers } from './accepted-number-output.mjs';
import { genericAliasProbe } from './accepted-callable-alias.mjs';
import { buildScanner } from './accepted-scanner.mjs';
import { measurementScope, scopedMeasurements } from './accepted-measure-scope.mjs';

const args = process.argv.slice(2);
const contractPath = args.find(arg => arg.startsWith('--contracts='))?.slice(12) ?? defaultContractDirectory;
const allContracts = loadContracts(contractPath);
const root = fs.realpathSync(path.resolve(args.find(arg => arg.startsWith('--workspace='))?.slice(12) ?? path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..')));
const jobFlags = args.filter(arg => arg.startsWith('--jobs='));
assert.ok(jobFlags.length <= 1, 'accepted-set: duplicate --jobs');
const jobs = jobFlags[0]?.slice(7) ?? '2';
assert.match(jobs, /^[12]$/, 'accepted-set: --jobs must be 1 or 2');
const configuration = { target: args.find(arg => arg.startsWith('--target='))?.slice(9) ?? 'native', features: args.find(arg => arg.startsWith('--features='))?.slice(11).split(',').filter(Boolean) ?? [], jobs: Number(jobs) };
assert.ok(['native', 'wasm32-unknown-unknown'].includes(configuration.target), 'accepted-set: unsupported execution target');
const targetArgs = configuration.target === 'native' ? [] : ['--target', configuration.target];
const featureArgs = configuration.features.length ? ['--features', configuration.features.join(',')] : [];
const selected = args.find(arg => arg.startsWith('--package='))?.slice(10);
assert.ok(args.every(arg => arg === '--inventory' || arg === '--census' || arg === '--measure' || arg.startsWith('--package=') || arg.startsWith('--workspace=') || arg.startsWith('--contracts=') || arg.startsWith('--target=') || arg.startsWith('--features=') || arg.startsWith('--jobs=') || arg.startsWith('--module=') || arg.startsWith('--owner=')), 'accepted-set: unsupported argument');
const scope = measurementScope(args, selected);
function cargo(args, options = {}) {
  const result = spawnSync('cargo', ['--color', 'never', ...args], {
    cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: jobs, ...options.env },
  });
  assert.equal(result.status, 0, `accepted-set: cargo ${args.join(' ')} did not complete\n${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version', '1', '--manifest-path', 'Cargo.toml', ...(configuration.features.length ? ['--features', configuration.features.map(feature => selected ? `${selected}/${feature}` : feature).join(',')] : [])]));
const workspaceMembers = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id));
const members = workspaceMembers.filter(pkg => !selected || pkg.name === selected);
assert.ok(members.length, `accepted-set: no workspace packages matched ${selected ?? 'workspace'}`);
const scanner = buildScanner(jobs);
const found = [];
const queued = new Set(members.map(pkg => pkg.id));
let compilerSysroot;
for (const pkg of members) {
  const targets = pkg.targets.filter(target => target.kind.some(kind => ['lib', 'cdylib', 'bin'].includes(kind)));
  assert.ok(targets.length, `accepted-set: missing production target for ${pkg.name}`);
  for (const target of targets) {
  const guards = spawnSync(scanner, ['--doc-guards', target.src_path], { encoding: 'utf8' });
  assert.equal(guards.status, 0, `accepted-set: production module discovery assertion: ${guards.stderr}`);
  const selector = target.kind.includes('bin') ? ['--bin', target.name] : ['--lib'];
  cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs', jobs, ...selector, ...targetArgs, ...featureArgs, '--', '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], { env: { RUSTC_BOOTSTRAP: '1' } });
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, ...(configuration.target === 'native' ? [] : [configuration.target]), 'doc', `${target.name.replaceAll('-', '_')}.json`), 'utf8'));
  const artifacts = cargo(['check', '--locked', '-p', pkg.name, '--jobs', jobs, ...selector, ...targetArgs, ...featureArgs, '--message-format=json']).trim().split('\n').filter(Boolean).map(line => JSON.parse(line)).filter(item => item.reason === 'compiler-artifact');
  const ownArtifact = artifacts.find(item => item.package_id === pkg.id && item.target.name === target.name);
  assert.ok(ownArtifact, `${pkg.name}: accepted-set assertion: missing production compiler artifact`);
  if (!compilerSysroot) {
    const sysroot = spawnSync('rustc', ['--print', 'sysroot'], { cwd: root, encoding: 'utf8' });
    assert.equal(sysroot.status, 0, sysroot.stderr);
    compilerSysroot = sysroot.stdout.trim();
  }
  const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, compilerSysroot);
  macroOrigins.push(...verifiedNumberOutputOrigin(doc, pkg, metadata, artifacts, scanner));
  const numberOutputDefinition = verifiedNumberOutputDefinition(doc, pkg, metadata, artifacts, scanner);
  if (numberOutputDefinition) macroOrigins.push(numberOutputDefinition.selfOrigin);
  const defaultTraits = args.some(arg => arg.startsWith('--workspace=')) ? {} : loadPolicy('default-traits');
  const receiverTraits = args.some(arg => arg.startsWith('--workspace=')) ? {} : loadPolicy('receiver-traits');
  const result = inventory(doc, { defaultTraits, receiverTraits, readSource: file => fs.readFileSync(path.resolve(root, file)) });
  let nativeEntry;
  if (target.kind.includes('bin') && configuration.target === 'native') {
    const mains = doc.index[String(doc.root)].inner.module.items.map(id => doc.index[String(id)]).filter(item => item?.name === 'main' && item.inner.function);
    assert.equal(mains.length, 1, `${pkg.name}: accepted binary: compiler root must name one active main`);
    assert.equal(fs.realpathSync(path.resolve(root, mains[0].span.filename)), fs.realpathSync(target.src_path), `${pkg.name}: accepted binary: main must belong to production root`);
    const files = [...new Set([target.src_path, ...result.sources.map(file => path.resolve(root, file))])];
    const entry = spawnSync(scanner, ['--binary-entry-json', target.src_path, JSON.stringify(mains[0].span.begin), JSON.stringify(mains[0].span.end), ...files.filter(file => file !== target.src_path)], { encoding: 'utf8' });
    assert.equal(entry.status, 0, `${pkg.name}: accepted binary: source entry proof failed\n${entry.stderr}`);
    nativeEntry = { ...JSON.parse(entry.stdout), sources: files.map(file => ({ file: path.relative(root, file), sha256: createHash('sha256').update(fs.readFileSync(file)).digest('hex') })) };
  }
  for (const alias of result.externalTypeAliases) {
    const label = `${pkg.name}::${alias.name} (${alias.canonical})`;
    if (!compilerSysroot) {
      const result = spawnSync('rustc', ['--print', 'sysroot'], { cwd: root, encoding: 'utf8' });
      assert.equal(result.status, 0, `accepted-set: compiler sysroot lookup failed: ${result.stderr}`);
      compilerSysroot = result.stdout.trim();
    }
    alias.artifact = aliasTargetIdentity(alias, artifacts, workspaceMembers, compilerSysroot);
    if (alias.artifact.kind === 'workspace' && !queued.has(alias.artifact.packageId)) {
      queued.add(alias.artifact.packageId);
      members.push(alias.artifact.owner);
    }
  }
  const aliasCompilerContext = { macroOrigins, externalTypeAliases: result.externalTypeAliases, packageMetadata: pkg, workspace: root };
  if (result.publicAliases.length) {
    const aliasSources = [...new Set(result.publicAliases.map(alias => path.relative(root, path.resolve(root, alias.span.filename)).split(path.sep).join('/')))];
    const claims = spawnSync(scanner, ['--alias-claims-json', root, ...aliasSources], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
    assert.equal(claims.status, 0, `accepted-set: public alias claim inventory failed: ${claims.stderr}`);
    const aliasClaims = JSON.parse(claims.stdout);
    const aliasContracts = allContracts;
    const nativePolicy = JSON.parse(fs.readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../rust-wasm.json'), 'utf8'));
    for (const alias of result.publicAliases) {
      const contract = aliasContracts[alias.name];
      const claimContext = { packageName: pkg.name, crateRoot: path.dirname(pkg.manifest_path), workspace: root, claims: aliasClaims, nativeTooling: nativePolicy.nativeOnly.some(entry => entry.package === pkg.name) };
      if (contract?.category === 'tooling-exemption') {
        alias.claim = aliasClaim(alias, claimContext, contract);
        toolingAliasExemption(alias, contract, { packageMetadata: pkg, metadata, workspace: root, nativePolicy, target: configuration.target });
        continue;
      }
      if (genericAliasProbe(alias, contract, aliasCompilerContext)) assert.ok(macroOrigins.includes('serde_json'), `${alias.name}: accepted-set assertion: fixed alias monomorphization requires its actual reviewed serde_json compiler dependency`);
      alias.claim = aliasClaim(alias, claimContext, contract);
    }
  }
  const construction = args.some(arg => arg.startsWith('--workspace=')) ? {} : loadPolicy('construction');
  for (const owner of result.construction) {
    assert.ok(construction[owner.name]?.reason?.trim(), `${owner.name}: accepted-set assertion: private-construction Clone/Copy must be classified`);
    assert.deepEqual(owner.traits, construction[owner.name].traits, `${owner.name}: accepted-set assertion: private-construction duplication traits changed`);
  }
  const local = spawnSync(scanner, ['--macro-origins-json', JSON.stringify(macroOrigins), ...[...new Set([target.src_path, ...result.sources.map(file => path.resolve(root, file))])].filter(file => !path.relative(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..'), file).startsWith('..'))], { encoding: 'utf8' });
  assert.equal(local.status, 0, `accepted-set: local type scope scan failed: ${local.stderr}`);
  const localFunctionContracts = args.some(arg => arg.startsWith('--workspace=')) ? {} : loadPolicy('local-functions');
  const localTypeHelpers = args.some(arg => arg.startsWith('--workspace=')) ? {} : loadPolicy('local-type-helpers');
  const delegatedLocals = [];
  const locals = local.stdout.trim().split('\n').filter(Boolean).map(line => {
    const [file, ident, row, column] = line.split('\t');
    if ((ident.startsWith('LOCAL_FUNCTION:') || ident.startsWith('LOCAL_TRAIT_IMPL:'))) {
      const label = `${path.relative(root, file)}::${ident}`;
      const enclosing = ident.startsWith('LOCAL_TRAIT_IMPL:') ? ident.match(/@([A-Za-z_][A-Za-z0-9_]*)$/)?.[1] : undefined;
      if (enclosing) {
        const owners = result.types.filter(owner => owner.deserialize && owner.ident === enclosing && fs.realpathSync(path.resolve(root, owner.span.filename)) === fs.realpathSync(file));
        assert.equal(owners.length, 1, `${label}: accepted-set assertion: serde helper needs its unique enclosing Deserialize owner`);
        requireNativeDecoderObservations(owners[0], allContracts[owners[0].name], label);
        delegatedLocals.push({ name: `${label}@${row}:${column}`, runtimeOwner: owners[0].name });
        return null;
      }
      const contract = localFunctionContracts[label];
      assert.ok(['non-decoder', 'decoder-helper'].includes(contract?.category) && contract.reason?.trim(), `${label}: accepted-set assertion: unclassified nested free function; move decoders to module scope`);
      if (contract.category === 'decoder-helper') {
        const owner = result.types.find(owner => owner.name === contract.runtimeOwner);
        const runtimeContract = allContracts[contract.runtimeOwner];
        assert.ok(owner && (owner.deserialize || (owner.kind === 'function' && runtimeContract?.category === 'decoder') || runtimeContract?.shape === 'observed'), `${label}: accepted-set assertion: decoder helper requires its inventoried production decoder`);
        requireNativeDecoderObservations(owner, runtimeContract, label);
      }
      assert.equal(contract.sourceSha256, createHash('sha256').update(fs.readFileSync(file)).digest('hex'), `${label}: accepted-set assertion: reviewed nested-function source changed`);
      return null;
    }
    if (numberOutputDefinitionCovers(numberOutputDefinition, file, ident, row, column)) return null;
    const helper = { file, ident, line: Number(row), column: Number(column), label: `${path.relative(root, file)}::${ident}` };
    if (reviewedGenericDecoderHelper(helper, localTypeHelpers, fs.readFileSync(file), result.types, allContracts)) {
      delegatedLocals.push({ name: helper.label, runtimeOwner: localTypeHelpers[helper.label].runtimeOwner });
      return null;
    }
    const scopeReason = ident.startsWith('UNSUPPORTED_MODULE_MACRO')
      ? 'module-scope macro may introduce decoder declarations and needs production coverage'
      : 'unsupported local scope needs module-scope production coverage';
    assert.ok(!ident.startsWith('UNSUPPORTED_'), `${file}::${ident}: accepted-set assertion: ${scopeReason}`);
    if (reviewedLocalHelper(helper, localTypeHelpers, fs.readFileSync(file), result.types, allContracts)) {
      delegatedLocals.push({ name: helper.label, runtimeOwner: localTypeHelpers[helper.label].runtimeOwner });
      return null;
    }
    return { file, ident, line: Number(row), column: Number(column), label: `${path.relative(root, file)}::${ident}` };
  }).filter(Boolean);
  // Same-name block-local types in different bodies are distinct declarations;
  // their declaration position names them. Identical positions still fail closed.
  const spellings = locals.reduce((counts, item) => counts.set(item.label, (counts.get(item.label) ?? 0) + 1), new Map());
  for (const item of locals) if (spellings.get(item.label) > 1) item.label = `${item.label}@${item.line}:${item.column}`;
  assert.equal(new Set(locals.map(item => item.label)).size, locals.length, `${pkg.name}: ambiguous local type names need explicit scope support`);
  // Follow compiler identity, never the spelling of an alias. Scoped runs include
  // the complete owning package so its decoders receive ordinary runtime probes.
  for (const alias of result.externalReexports) {
    const label = `${pkg.name}::${alias.name} (${alias.source} -> ${alias.canonical})`;
    const compilerPath = fs.realpathSync(alias.compilerPath);
    const dependencies = artifacts.filter(item => item.package_id !== pkg.id && (item.filenames ?? []).some(filename => {
      try { return fs.realpathSync(filename) === compilerPath; }
      catch (error) { if (error.code === 'ENOENT') return false; throw error; }
    }));
    assert.equal(dependencies.length, 1, `${label}: accepted-set assertion: external reexport requires a unique resolved compiler dependency`);
    alias.artifact = { packageId: dependencies[0].package_id, features: dependencies[0].features.slice().sort() };
    const owners = workspaceMembers.filter(member => member.id === alias.artifact.packageId);
    assert.equal(owners.length, 1, `${label}: accepted-set assertion: external reexport requires a unique workspace compiler owner`);

    if (!queued.has(owners[0].id)) { queued.add(owners[0].id); members.push(owners[0]); }
  }
  found.push({ package: pkg.name, packageId: pkg.id, features: ownArtifact.features.slice().sort(), target, ...result, macroOrigins, locals, delegatedLocals, ...(nativeEntry ? { nativeEntry } : {}) });
  console.log(`${pkg.name}: ${result.types.filter(type => type.deserialize).length} compiler-confirmed Deserialize types; ${result.types.filter(type => !type.deserialize).length} alternate-only owners; ${result.externalReexports.length} external reexports`);
  }
}
for (const pkg of found) for (const alias of pkg.externalReexports) {
  const owner = found.find(owner => owner.packageId === alias.artifact.packageId && owner.declarations.some(item => item.name === alias.canonical && item.kind === alias.kind));
  assert.ok(owner, `${pkg.package}::${alias.name} (${alias.source} -> ${alias.canonical}): accepted-set assertion: canonical reexport declaration absent from compiler owner inventory`);
  assert.deepEqual(owner.features, alias.artifact.features, `${pkg.package}::${alias.name} (${alias.canonical}): accepted-set assertion: consumer and canonical owner compiler features differ`);
}
for (const pkg of found) for (const alias of pkg.externalTypeAliases.filter(alias => alias.artifact.kind === 'workspace')) {
  const owner = found.find(owner => owner.packageId === alias.artifact.packageId && owner.declarations.some(item => item.name === alias.canonical && item.kind === alias.kind));
  assert.ok(owner, `${pkg.package}::${alias.name}: accepted-set assertion: canonical alias target absent from workspace compiler owner inventory`);
  assert.deepEqual(owner.features, alias.artifact.features, `${pkg.package}::${alias.name}: accepted-set assertion: alias consumer and canonical owner compiler features differ`);
}
const output = path.join(metadata.target_directory, 'accepted-set-inventory.json');
fs.writeFileSync(output, `${JSON.stringify(found, null, 2)}\n`);
console.log(`accepted-set: ${found.reduce((sum, pkg) => sum + pkg.types.filter(type => type.deserialize).length, 0)} Deserialize owners; inventory ${output}`);
if (!args.includes('--inventory')) {
  const contracts = args.includes('--census') ? undefined : allContracts;
  const measured = scopedMeasurements(found, scope, root);
  if (scope) console.log(`accepted-set: scoped --measure ${JSON.stringify(scope)}; full compiler inventory preserved`);
  const results = runProbes(root, metadata, measured, contracts,
    scope ? {...configuration, measurementScope: scope} : configuration);
  const failures = results.filter(row => row.failures.length);
  if (!args.includes('--measure')) assert.equal(failures.length, 0, `accepted-set: ${failures.length} failed types\n${failures.map(row => `${row.name}: ${row.failures.join('; ')}`).join('\n')}`);
}
