import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { loadPolicy } from './accepted-policy-files.mjs';
import { loadContracts } from './accepted-contract-files.mjs';
import { genericAliasProbe } from './accepted-callable-alias.mjs';

const specs = loadPolicy('napi-aliases');
const contracts = loadContracts();
function fixture(name) {
  const spec = specs[name];
  return {
    alias: { name, ident: name.split('::').at(-1), span: { filename: spec.file }, generics: spec.generics, rawTarget: spec.rawTarget, target: spec.target },
    contract: contracts[name],
    context: { workspace: fileURLToPath(new URL('../..', import.meta.url)), macroOrigins: ['serde_json'],
      packageMetadata: { name: 'safe-cex-ffi', manifest_path: fileURLToPath(new URL('../../safe-cex-ffi/Cargo.toml', import.meta.url)) },
      externalTypeAliases: spec.roots.map(root => ({ name, canonical: root.canonical, kind: root.kind,
        artifact: { kind: root.artifactKind, packageId: root.packageId, features: root.features, compilerPath: fileURLToPath(import.meta.url) } })) },
  };
}

test('NAPI callbacks retain their exact compiler signatures and Cargo identities', () => {
  assert.deepEqual(Object.keys(specs), ['safe_cex_ffi::native_callback::Callback', 'safe_cex_ffi::native_callback::Request']);
  for (const name of Object.keys(specs)) {
    const { alias, contract, context } = fixture(name);
    const probe = genericAliasProbe(alias, contract, context);
    assert.equal(probe?.expectedType, specs[name].expectedType);
    assert.equal(probe.suffix, specs[name].suffix);
    assert.equal(probe.probeKind, 'reviewed-napi');
    for (const mutate of [
      row => { row.ident = 'Other'; },
      row => { row.span.filename = 'other.rs'; },
      row => { row.generics.params.push({ name: 'T' }); },
      row => { row.rawTarget.resolved_path.path = 'foreign::Function'; },
      row => { row.target.resolved_path.args.angle_bracketed.args.reverse(); },
    ]) {
      const row = structuredClone(alias); mutate(row);
      assert.throws(() => genericAliasProbe(row, contract, context), /reviewed NAPI alias/);
    }
    for (const key of ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical']) {
      assert.throws(() => genericAliasProbe(alias, { ...contract, [key]: 'redirect' }, context), /caller cannot supply/);
    }
    for (const mutate of [
      row => { row.externalTypeAliases.pop(); },
      row => { row.externalTypeAliases[0].artifact.kind = 'workspace'; },
      row => { row.externalTypeAliases[0].artifact.packageId += '-other'; },
      row => { row.externalTypeAliases[0].artifact.features.push('napi9'); },
      row => { row.externalTypeAliases[0].canonical = 'foreign::Function'; },
      row => { row.externalTypeAliases[0].kind = 'enum'; },
      row => { row.macroOrigins = []; },
      row => { row.packageMetadata.name = 'foreign'; },
      row => { row.packageMetadata.manifest_path = fileURLToPath(new URL('../../safe-cex-types/Cargo.toml', import.meta.url)); },
    ]) {
      const row = structuredClone(context); mutate(row);
      assert.throws(() => genericAliasProbe(alias, contract, row), /reviewed NAPI alias/);
    }
  }
});

test('unknown NAPI callback aliases keep the existing refusal', () => {
  const { alias, contract, context } = fixture('safe_cex_ffi::native_callback::Callback');
  assert.throws(() => genericAliasProbe({ ...alias, name: 'foreign::Callback' }, contract, context), /generic alias requires explicit compiler monomorphization coverage/);
});


test('NAPI alias compiler and artifact guards are load bearing', () => {
  const url = new URL('./accepted-napi-alias.mjs', import.meta.url).href;
  const runner = new URL('./accepted-callable-alias.mjs', import.meta.url).href;
  const cases = [
    ['NAPI alias source identity', '  assert.equal(alias.span?.filename, spec.file, `${label}: compiler source identity changed`);', row => { row.alias.span.filename = 'other.rs'; }, /compiler source identity changed/],
    ['NAPI alias expanded signature', '    assert.deepEqual(alias[key], spec[key], `${label}: compiler ${key} changed`);', row => { row.alias.target.resolved_path.args.angle_bracketed.args.reverse(); row.contract.target = row.alias.target; }, /compiler target changed/],
    ['NAPI alias nested Cargo identity', '    packageId: row.artifact?.packageId, features: row.artifact?.features })), spec.roots, `${label}: every nested NAPI Cargo artifact identity required`);', row => { row.context.externalTypeAliases[0].artifact.packageId += '-other'; }, /nested NAPI Cargo artifact identity/],
    ['NAPI alias caller redirection', '  for (const key of callerFields) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);', row => { row.contract.aliasProbeExpected = 'bool'; }, /caller cannot supply/],
  ];
  for (const [label, anchor, mutate, refusal] of cases) {
    const item = structuredClone(fixture('safe_cex_ffi::native_callback::Callback')); mutate(item);
    const execute = mutant => {
      const replacement = label === 'NAPI alias nested Cargo identity'
        ? "    packageId: row.artifact?.packageId, features: row.artifact?.features })), roots.map(row => ({ canonical: row.canonical, kind: row.kind, artifactKind: row.artifact?.kind, packageId: row.artifact?.packageId, features: row.artifact?.features })));"
        : '';
      const hook = mutant ? `import {registerHooks} from 'node:module';registerHooks({load(url,context,next){const r=next(url,context);if(url!==${JSON.stringify(url)})return r;const source=String(r.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...r,source:source.replace(anchor,${JSON.stringify(replacement)})};}});` : '';
      return spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';${hook}const {genericAliasProbe}=await import(${JSON.stringify(runner)});assert.throws(()=>genericAliasProbe(${JSON.stringify(item.alias)},${JSON.stringify(item.contract)},${JSON.stringify(item.context)}),${refusal},${JSON.stringify(label + ' must refuse')});`], { encoding: 'utf8' });
    };
    assert.equal(execute(false).status, 0);
    const mutant = execute(true); assert.equal(mutant.status, 1, mutant.stderr);
    assert.match(mutant.stderr, new RegExp(label + ' must refuse'));
    console.log(`${label}: named assertion; process exit 1`);
    assert.equal(execute(false).status, 0);
  }
});
