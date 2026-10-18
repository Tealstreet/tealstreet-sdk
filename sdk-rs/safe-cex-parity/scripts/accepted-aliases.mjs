import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { genericAliasProbe } from './accepted-callable-alias.mjs';
import { grvtFieldsContract } from './accepted-grvt-fields.mjs';
import { krakenNumericChangesContract } from './accepted-kraken-numeric-changes.mjs';

export const aliasAlternates = ['FromStr', 'From<String>', 'From<&str>', 'TryFrom<Value>', 'TryFrom<Map>', 'TryFrom<String>', 'TryFrom<&str>'];

// This exemption belongs to one reviewed native tooling declaration. It says
// nothing about Deserialize for Result<T, String> or any chosen T.
export function toolingAliasExemption(alias, contract, context) {
  if (contract?.category !== 'tooling-exemption') return undefined;
  const label = `${alias.name}: accepted-set assertion: native tooling alias exemption`;
  assert.equal(alias.name, 'safe_cex_parity::Result', `${label}: unreviewed tooling owner`);
  assert.equal(alias.ident, 'Result', `${label}: alias identifier changed`);
  assert.deepEqual(Object.keys(contract).sort(), ['category', 'declarationEnd', 'evidence', 'exemption', 'file', 'generics', 'rawTarget', 'rs', 'sourceSha256', 'target'], `${label}: unsupported exemption fields`);
  assert.ok(contract.evidence?.trim() && contract.exemption?.trim(), `${label}: reviewed scope evidence required`);
  assert.ok(context?.packageMetadata && context.metadata && context.nativePolicy, `${label}: actual compiler package context required`);
  assert.equal(context.target, 'native', `${label}: native execution scope required`);
  const pkg = context.packageMetadata;
  assert.equal(pkg.name, 'safe-cex-parity', `${label}: SDK package cannot use tooling scope`);
  const workspace = fs.realpathSync(context.workspace);
  const expectedManifest = fs.realpathSync(path.join(workspace, 'safe-cex-parity/Cargo.toml'));
  assert.equal(fs.realpathSync(pkg.manifest_path), expectedManifest, `${label}: compiler package source changed`);
  const packages = context.metadata.packages.filter(candidate => candidate.id === pkg.id && fs.realpathSync(candidate.manifest_path) === expectedManifest);
  assert.equal(packages.length, 1, `${label}: unique compiler package required`);
  assert.ok(context.metadata.workspace_members.includes(pkg.id), `${label}: compiler package must belong to workspace`);
  assert.equal(context.nativePolicy.nativeOnly.filter(entry => entry.package === pkg.name && entry.reason?.trim()).length, 1, `${label}: explicit native-only policy required`);
  assert.ok(!context.nativePolicy.wasm.some(entry => entry.package === pkg.name), `${label}: SDK wasm scope cannot be exempted`);
  assert.equal(contract.file, 'safe-cex-parity/src/lib.rs', `${label}: unreviewed source file`);
  assert.equal(alias.span?.filename, contract.file, `${label}: compiler source identity changed`);
  assert.deepEqual(alias.span.end, contract.declarationEnd, `${label}: declaration end changed`);
  assert.equal(contract.rs, 'safe-cex-parity/src/lib.rs::Result', `${label}: unreviewed declaration mapping`);
  assert.deepEqual(alias.claim, { rs: contract.rs, exemption: contract.exemption }, `${label}: verified declaration exemption required`);
  assert.deepEqual(alias.generics, contract.generics, `${label}: compiler generics changed`);
  assert.deepEqual(alias.rawTarget, contract.rawTarget, `${label}: compiler raw target changed`);
  assert.deepEqual(alias.target, contract.target, `${label}: compiler expanded target changed`);
  const source = fs.readFileSync(path.join(workspace, contract.file));
  assert.equal(createHash('sha256').update(source).digest('hex'), contract.sourceSha256, `${label}: reviewed source changed`);
  return contract;
}

export function aliasContract(alias, contracts, context) {
  const contract = contracts[alias.name];
  const label = `${alias.name}: accepted-set assertion`;
  if (contract?.category === 'tooling-exemption') return toolingAliasExemption(alias, contract, context);
  assert.equal(contract?.nativeObservation, undefined, `${label}: aliases use their own direct native observation mechanism`);
  assert.equal(contract?.aliasProbeExpected, undefined, `${label}: expected compiler type cannot be supplied by the contract`);
  assert.equal(contract?.aliasProbeKind, undefined, `${label}: compiler alias probe kind cannot be supplied by the contract`);
  assert.ok(contract?.evidence?.trim(), `${label}: public alias needs an accepted-set contract`);
  assert.ok(['decoder', 'non-decoder'].includes(contract.category), `${label}: alias needs an explicit decoder classification`);
  const identity = genericAliasProbe(alias, contract, context);
  assert.equal(contract.instantiation, undefined, `${label}: alias cannot redirect its production type`);
  assert.ok(Array.isArray(contract.entrypoints ?? []), `${label}: alias entrypoints must be an array`);
  const entries = contract.entrypoints ?? [];
  assert.equal(new Set(entries.map(entry => entry.trait)).size, entries.length, `${label}: duplicate alias trait adapter`);
  for (const entry of entries) {
    assert.deepEqual(Object.keys(entry).sort(), ['nativeCases', 'negative', 'positive', 'reason', 'trait'], `${label}: alias adapter must use an executed trait path`);
    assert.ok(aliasAlternates.includes(entry.trait) && entry.reason?.trim() && Array.isArray(entry.positive) && entry.positive.length && Array.isArray(entry.negative), `${label}: alias trait needs explicit positive/negative witnesses`);
    assert.ok(Array.isArray(entry.nativeCases) && entry.nativeCases.length >= 2 && entry.nativeCases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${label}: alias adapter needs direct native output witnesses`);
    assert.deepEqual(entry.nativeCases.map(row => row.input), entry.positive, `${label}: native adapter witnesses must cover every declared positive input`);
    assert.ok(entry.negative.length || (contract.shape === 'json' && entry.trait === 'TryFrom<Value>'), `${label}: alias trait requires forbidden inputs unless it is the open JSON identity`);
  }
  if (entries.length) assert.ok(contract.nativeProjection?.includes('$decoded') && !/serde_json::(?:to_(?:value|string|vec)|from_)/.test(contract.nativeProjection), `${label}: alias alternate projection must inspect the actual decoded value`);
  assert.deepEqual(contract.target, alias.target, `${label}: alias compiler target changed`);
  const borrowed = grvtFieldsContract(alias, contract, context, identity) ?? krakenNumericChangesContract(alias, contract, context);
  if (borrowed) return borrowed;
  assert.equal(contract.borrowedRoutes, undefined, `${label}: borrowed routes require the exact reviewed alias`);
  if (contract.category === 'non-decoder') {
    assert.ok(contract.reason?.trim(), `${label}: non-decoder alias requires its specific reason`);
    assert.equal(entries.length, 0, `${label}: non-decoder alias cannot declare raw decoders`);
    assert.equal(contract.nativeProjection, undefined, `${label}: non-decoder alias cannot claim unexecuted native alternate observations`);
    assert.equal(contract.observation, undefined, `${label}: non-decoder alias cannot claim unexecuted observations`);
  } else {
    assert.ok(['string', 'map', 'union', 'uninhabited', 'json'].includes(contract.shape), `${label}: unknown alias shape classification`);
    if (contract.shape === 'string') assert.ok(contract.literals?.length && contract.literals.every(value => typeof value === 'string'), `${label}: string alias needs expected literals`);
    if (contract.shape === 'union') assert.ok(contract.positive?.length && contract.negative?.length, `${label}: union alias needs positive and negative fixtures`);
    const observation = contract.observation;
    assert.ok(observation?.call?.includes('$type') && observation.call.includes('$value') && observation.cases?.length >= 2 && observation.cases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${label}: alias requires direct decoded-value observations`);
    assert.ok(!/serde_json::to_(?:value|string|vec)/.test(observation.call), `${label}: alias observations must assert decoded native values`);
  }
  const canonical = identity?.canonical ?? alias.target?.resolved_path?.path ?? alias.target?.primitive;
  assert.ok(canonical, `${label}: alias target requires explicit compiler root identity coverage`);
  return { ...contract, canonical, aliasProbeSuffix: identity?.suffix ?? '', aliasProbeExpected: identity?.expectedType ?? null, aliasProbeKind: identity?.probeKind ?? null, ...(identity ? { monomorphization: identity.monomorphization } : {}) };
}

export function aliasClaim(alias, { packageName, crateRoot, workspace, claims, nativeTooling = false }, contract) {
  const source = path.resolve(workspace, alias.span.filename);
  const relative = path.relative(crateRoot, source);
  const label = `${alias.name}: accepted-set assertion`;
  assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative), `${label}: alias source must belong to its compiler package`);
  const prefix = `${packageName}/${relative.split(path.sep).join('/')}::`;
  const declarations = claims.declarations[path.relative(workspace, source).split(path.sep).join('/')] ?? [];
  const matches = declarations.filter(([scope, end]) => alias.name.endsWith(`::${scope}`) && JSON.stringify(end) === JSON.stringify(alias.span.end));
  assert.equal(matches.length, 1, `${label}: alias needs a unique public declaration at its compiler source span`);
  const rs = `${prefix}${matches[0][0]}`;
  const exemption = claims.exempt[rs];
  if (!claims.mapped.includes(rs) && !exemption?.trim() && nativeTooling) {
    assert.ok(contract?.exemption?.trim() && contract.rs === rs, `${label}: native tooling alias needs an exact exemption`);
    return { rs, exemption: contract.exemption };
  }
  assert.ok(claims.mapped.includes(rs) || exemption?.trim(), `${label}: public alias needs a mapped row or exemption: ${rs}`);
  return { rs, ...(exemption ? { exemption } : {}) };
}

export function aliasTargetIdentity(target, artifacts, workspaceMembers, sysroot) {
  const label = `${target.name} (${target.canonical}): accepted-set assertion`;
  const compilerPath = fs.realpathSync(target.compilerPath);
  const actualSysroot = fs.realpathSync(sysroot);
  const relative = path.relative(actualSysroot, compilerPath);
  if (relative && !relative.startsWith('..') && !path.isAbsolute(relative)) {
    assert.ok(relative.split(path.sep).includes('rustlib'), `${label}: sysroot alias must resolve to compiler target libraries`);
    return { kind: 'sysroot', compilerPath };
  }
  const matches = artifacts.filter(artifact => (artifact.filenames ?? []).some(file => {
    try { return fs.realpathSync(file) === compilerPath; }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  }));
  assert.equal(matches.length, 1, `${label}: external alias needs a unique Cargo compiler-artifact identity`);
  const owner = workspaceMembers.find(member => member.id === matches[0].package_id);
  return { kind: owner ? 'workspace' : 'registry', compilerPath, packageId: matches[0].package_id, features: matches[0].features.slice().sort(), ...(owner ? { owner } : {}) };
}
