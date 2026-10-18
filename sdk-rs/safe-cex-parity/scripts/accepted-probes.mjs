import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { contractFor } from './accepted-contracts.mjs';
import { contractJson } from './accepted-contract-files.mjs';
import { aliasContract } from './accepted-aliases.mjs';
import { defaultWitnessContract, telemetryDefaultWitness } from './accepted-defaults.mjs';
import { grvtFieldsBody } from './accepted-grvt-fields.mjs';
import { nodeProbeConfiguration, nodeProbeHarness, executeNodeProbes } from './accepted-node-probes.mjs';

const literal = value => JSON.stringify(value);
const aliasTraitMethods = [
  ['AliasFromStr', 'probe_from_str', 'FromStr', 'native_from_str'], ['AliasFromString', 'probe_from_string', 'From<String>', 'native_from_string'],
  ['AliasFromStrRef', 'probe_from_str_ref', 'From<&str>', 'native_from_str_ref'], ['AliasFromValue', 'probe_from_value', 'TryFrom<Value>', 'native_from_value'],
  ['AliasFromMap', 'probe_from_map', 'TryFrom<Map>', 'native_from_map'], ['AliasTryString', 'probe_try_string', 'TryFrom<String>', 'native_try_string'],
  ['AliasTryStrRef', 'probe_try_str_ref', 'TryFrom<&str>', 'native_try_str_ref'],
];
function aliasBody(type, targetType, contract) {
  const observation = contract.observation;
  const unsized = ['unsized-income-fetcher', 'reviewed-unsized'].includes(contract.aliasProbeKind);
  const equality = contract.aliasProbeExpected
    ? unsized
      ? `let _: std::marker::PhantomData<fn(*mut (${contract.aliasProbeExpected}))->*mut (${contract.aliasProbeExpected})> = std::marker::PhantomData::<fn(*mut ${targetType})->*mut ${targetType}>;`
      : `let _: std::marker::PhantomData<fn(${contract.aliasProbeExpected})->${contract.aliasProbeExpected}> = std::marker::PhantomData::<fn(${targetType})->${targetType}>;`
    : '';
  // Dyn, tuple and function-pointer type names include nested signatures. Obtain
  // the compared prefix from the independently authored expected compiler type.
  const canonical = unsized || ['reviewed-hosted-signer-tuple', 'reviewed-function-pointer', 'reviewed-weex-kline-tuple'].includes(contract.aliasProbeKind)
    ? `std::any::type_name::<${contract.aliasProbeExpected}>().split('<').next().expect("reviewed unsized alias identity")`
    : literal(contract.canonical);
  const alternate = `|value| { let alternate = crate::__accepted_set_runtime::AliasAlternate::<${targetType}>(std::marker::PhantomData); let mut paths: Vec<_> = [${aliasTraitMethods.map(([, method, name]) => `(${literal(name)}, (&&alternate).${method}(value))`).join(',')}].into_iter().filter_map(|(name, accepted)| accepted.map(|accepted| (name, accepted))).collect(); if (&&alternate).has_static_from() && (&&alternate).probe_from_str_ref(value).is_none() { paths.push(("unsupported From<&'static str>", false)); } if (&&alternate).has_static_try() && (&&alternate).probe_try_str_ref(value).is_none() { paths.push(("unsupported TryFrom<&'static str>", false)); } paths }`;
  if (contract.aliasProbeKind === 'reviewed-grvt-fields') return `{ ${equality} use crate::__accepted_set_runtime::{AliasStaticFrom as _, AliasStaticTry as _, ${aliasTraitMethods.map(([trait]) => `${trait} as _`).join(', ')}}; ${grvtFieldsBody(type, targetType, contract, alternate)} }`;
  const native = `|path, value| { let alternate = crate::__accepted_set_runtime::AliasAlternate::<${targetType}>(std::marker::PhantomData); match path { ${aliasTraitMethods.map(([, , name, method]) => `${literal(name)} => (&&alternate).${method}(value, &|${contract.nativeProjection ? 'decoded' : '_decoded'}| ${contract.nativeProjection?.replaceAll('$decoded', 'decoded') ?? 'serde_json::Value::Null'}),`).join(' ')} _ => None } }`;
  return `{ ${equality} use crate::__accepted_set_runtime::{DetectAlias as _, AliasStaticFrom as _, AliasStaticTry as _, ${aliasTraitMethods.map(([trait]) => `${trait} as _`).join(', ')}}; let probe = crate::__accepted_set_runtime::AliasProbe::<${targetType}>(std::marker::PhantomData); (&&&probe).probe(${literal(type.name)}, ${canonical}, ${literal(contractJson(contract))}, |${observation ? 'value' : '_value'}| ${observation ? observation.call.replaceAll('$type', targetType).replaceAll('$value', 'value') : 'serde_json::Value::Null'}, ${native}, ${alternate}); }`;
}
function offset(source, [line, column]) {
  const lines = source.split('\n');
  assert.ok(line > 0 && line <= lines.length, 'accepted-set: compiler source line out of bounds');
  return Buffer.byteLength(lines.slice(0, line - 1).join('\n')) + (line > 1 ? 1 : 0) + column - 1;
}
export function runProbes(root, metadata, packages, contracts, configuration = { target: 'native', features: [] }) {
  const jobs = String(configuration.jobs ?? 2);
  assert.match(jobs, /^[12]$/, 'accepted-set: probe jobs must be 1 or 2');
  const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
  const workspaceRelative = path.relative(repo, root);
  assert.ok(!workspaceRelative.startsWith('..'), 'accepted-set: workspace must be inside this repository');
  const sandbox = path.join(metadata.target_directory, 'accepted-set-workspace');
  fs.rmSync(sandbox, { recursive: true, force: true });
  fs.mkdirSync(sandbox, { recursive: true });
  fs.mkdirSync(path.join(sandbox, workspaceRelative), { recursive: true });
  for (const entry of fs.readdirSync(root)) {
    if (['target', 'node_modules', '.git'].includes(entry)) continue;
    fs.cpSync(path.join(root, entry), path.join(sandbox, workspaceRelative, entry), { recursive: true, filter: source => !['target', 'node_modules', '.git'].includes(path.basename(source)) });
  }
  fs.cpSync(path.join(repo, 'infra/tradetunnel'), path.join(sandbox, 'infra/tradetunnel'), { recursive: true, filter: source => !['target', 'node_modules'].includes(path.basename(source)) });
  fs.symlinkSync(path.join(repo, 'packages'), path.join(sandbox, 'packages'), 'dir');
  const results = [];
  const wasm = configuration.target === 'wasm32-unknown-unknown';
  const nativePolicy = JSON.parse(fs.readFileSync(new URL('../../rust-wasm.json', import.meta.url), 'utf8'));
  for (const pkg of packages) {
    const owners = [...pkg.types, ...(pkg.publicAliases ?? []).map(alias => ({ ...alias, kind: 'alias' })), ...(pkg.defaultWitnesses ?? [])];
    if (!owners.length && !pkg.locals?.length) continue;
    const packageMetadata = metadata.packages.find(item => item.name === pkg.package);
    const crateRoot = path.dirname(packageMetadata.manifest_path);
    const target = pkg.target ?? packageMetadata.targets.find(item => item.kind.includes('lib') || item.kind.includes('cdylib'));
    const node = nodeProbeConfiguration(pkg, target, configuration);
    const targetArgs = target.kind.includes('bin') ? ['--bin', target.name] : ['--lib'];
    const originals = new Map();
    const remember = file => { if (!originals.has(file)) originals.set(file, fs.readFileSync(file)); };
    const copiedRoot = path.join(sandbox, path.relative(repo, target.src_path));
    const edits = new Map();
    let localTests = "";
    const nativeSymbols = [];
    const nativeBinary = !wasm && target.kind.includes('bin');
    if (nativeBinary) {
      assert.ok(pkg.nativeEntry?.sources?.length, `${pkg.package}: accepted binary: compiler/source entry proof required`);
      for (const source of pkg.nativeEntry.sources) assert.equal(createHash('sha256').update(fs.readFileSync(path.resolve(root, source.file))).digest('hex'), source.sha256, `${pkg.package}: accepted binary: production source changed after entry proof`);
      const at = offset(fs.readFileSync(target.src_path, 'utf8'), pkg.nativeEntry.ident);
      assert.equal(fs.readFileSync(target.src_path).subarray(at, at + 4).toString(), 'main', 'accepted binary: main identifier span changed');
      edits.set(copiedRoot, [{ at, remove: 4, added: '__accepted_set_original_main' }]);
    }
    for (const [i, type] of owners.entries()) {
      assert.ok(type.span, `accepted-set: ${type.name}: missing compiler source span`);
      const sourcePath = path.resolve(root, type.span.filename);
      const relative = path.relative(repo, sourcePath);
      assert.ok(!relative.startsWith('..'), `accepted-set: ${type.name}: source outside repository`);
      let destination = path.join(sandbox, relative);
      const bytes = fs.readFileSync(sourcePath);
      assert.deepEqual(fs.readFileSync(destination), bytes, `accepted-set: ${type.name}: sandbox source differs before instrumentation`);
      let at = offset(bytes.toString('utf8'), type.span.end);
      let contract;
      let problem;
      if (type.kind === 'trait-defaults') contract = defaultWitnessContract();
      if (contracts) {
        try { contract = type.kind === 'trait-defaults' ? defaultWitnessContract() : type.kind === 'trait-classifications' ? type.classificationContract : type.kind === 'alias' ? aliasContract(type, contracts, { packageMetadata, metadata, workspace: root, nativePolicy, target: configuration.target, macroOrigins: pkg.macroOrigins, externalTypeAliases: pkg.externalTypeAliases }) : contractFor(type, contracts, { packageMetadata, readSource(file) {
          const production = path.resolve(root, file);
          const copied = fs.readFileSync(path.join(sandbox, path.relative(repo, production)));
          assert.deepEqual(copied, fs.readFileSync(production), `${type.name}: accepted-set assertion: host source differs from compiled sandbox`);
          return copied;
        } }); }
        catch (error) { problem = error.message; }
      }
      if (!contracts && type.deserialize === false && type.kind !== 'trait-defaults') problem = `${type.name}: accepted-set assertion: alternate-only owner requires classification`;
      if (!contracts && type.kind === 'alias') problem = `${type.name}: accepted-set assertion: public alias requires classification`;
      if (!problem && type.kind !== 'alias' && type.deserialize !== false && type.generics?.params?.length && !contract?.instantiation) problem = `${type.name}: accepted-set assertion: generic type needs classified monomorphization`;
      const targetType = type.kind === 'alias' ? `super::${type.ident}${contract?.aliasProbeSuffix ?? ''}` : `super::${contract?.instantiation ?? type.ident}`;
      const alternate = (type.kind === 'alias' ? [] : (contract?.entrypoints ?? [])).filter(entry => !['typed', 'observed', 'non-decoder'].includes(entry.input)).map((entry, index) => {
        const input = entry.input === 'number' ? 'value.as_f64()' : entry.input === 'value' ? 'Some(value.clone())' : entry.input === 'map' ? 'value.as_object().cloned()' : entry.input === 'string' ? 'value.as_str().map(str::to_owned)' : 'Some(value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))';
        const call = entry.call.replaceAll('$type', targetType).replaceAll('$value', 'input');
        return `(${literal(`entrypoint ${index}`)}, (${input}).is_some_and(|input| ${call}))`;
      }).join(',');
      const auditBody = problem
        ? `crate::__accepted_set_runtime::unclassified(${literal(type.name)}, ${literal(problem)});`
        : type.kind === 'trait-defaults'
        ? `crate::__accepted_set_runtime::audit_function(${literal(type.name)}, ${literal(contractJson(contract))}, |value| ${telemetryDefaultWitness()});`
        : type.kind === 'alias' && contract.category === 'tooling-exemption'
        ? `crate::__accepted_set_runtime::audit_tooling_alias(${literal(type.name)}, ${literal(contractJson(contract))});`
        : type.kind === 'alias'
        ? aliasBody(type, targetType, contract)
        : type.kind === 'function' || type.kind === 'trait-classifications' || contract?.shape === 'observed'
        ? `crate::__accepted_set_runtime::audit_function(${literal(type.name)}, ${literal(contractJson(contract))}, |${contract.category !== 'non-decoder' ? 'value' : '_value'}| ${contract.category !== 'non-decoder' ? contract.call.replaceAll('$type', targetType).replaceAll('$value', 'value') : 'serde_json::Value::Null'});`
        : type.deserialize === false
        ? `crate::__accepted_set_runtime::audit_alternate(${literal(type.name)}, ${literal(contractJson(contract))}, |${alternate ? 'value' : '_value'}| vec![${alternate}]);`
        : `crate::__accepted_set_runtime::audit::<${targetType}>(${literal(type.name)}, ${literal(type.kind)}, ${contract ? `Some(${literal(contractJson(contract))})` : 'None'}, |value| { #[allow(unused_imports)] use serde::Deserialize as _; <${targetType}>::deserialize(value).is_ok() }, |${alternate ? 'value' : '_value'}| vec![${alternate}], |${contract?.observation ? 'value' : '_value'}| ${contract?.observation ? contract.observation.call.replaceAll('$type', targetType).replaceAll('$value', 'value') : 'serde_json::Value::Null'});`;
      const native = !problem && contract?.nativeObservation;
      const supplemental = !problem ? (contract?.entrypoints ?? []).filter(entry => entry.input === 'observed' && contract.shape !== 'observed').map(entry => `crate::__accepted_set_runtime::audit_native_observations(${literal(type.name)}, ${literal(contractJson(entry))}, |value| ${entry.call.replaceAll('$type', targetType).replaceAll('$value', 'value')});`).join(' ') : '';
      const nativeBody = native
        ? `crate::__accepted_set_runtime::audit_native_observations(${literal(type.name)}, ${literal(contractJson(native))}, |value| ${native.call.replaceAll('$type', targetType).replaceAll('$value', 'value')}); ${auditBody}`
        : auditBody;
      const body = `${supplemental} ${nativeBody}`;
      // Diagnostics and explicit non-decoder classifications do not reference a
      // private symbol. Put those receipts at the crate root: macro-generated
      // functions may point at an attribute span where insertion is invalid.
      if (problem || (type.kind === 'alias' && contract.category === 'tooling-exemption') || (['function', 'trait-classifications'].includes(type.kind) && contract.category === 'non-decoder')) {
        destination = copiedRoot;
        at = fs.readFileSync(target.src_path).length;
      }
      nativeSymbols.push(`__accepted_set_native_${i}`);
      const added = wasm
        ? `\nmod __accepted_set_${i} { #[unsafe(export_name = "__accepted_set_${i}")] pub extern "C" fn probe() { ${body} } }\n`
        : `\nmod __accepted_set_${i} { #[unsafe(export_name = "__accepted_set_native_${i}")] pub fn probe() { ${body} } }\n`;
      const list = edits.get(destination) ?? [];
      list.push({ at, added });
      edits.set(destination, list);
    }
    for (const [localIndex, local] of (pkg.locals ?? []).entries()) {
      const i = owners.length + localIndex;
      const sourcePath = path.resolve(root, local.file);
      const destination = path.join(sandbox, path.relative(repo, sourcePath));
      const bytes = fs.readFileSync(sourcePath);
      assert.deepEqual(fs.readFileSync(destination), bytes, `${local.label}: source changed before local probe`);
      const at = offset(bytes.toString('utf8'), [local.line, local.column + 1]);
      const contract = contracts?.[local.label];
      const body = `use crate::__accepted_set_runtime::{DetectLocal as _, LocalFromStr as _, LocalFromString as _, LocalFromStrRef as _, LocalFromValue as _, LocalFromMap as _}; let probe = crate::__accepted_set_runtime::LocalProbe::<${local.ident}>(std::marker::PhantomData); (&&probe).check_from_str(${literal(local.label)}); (&&probe).check_from_string(${literal(local.label)}); (&&probe).check_from_str_ref(${literal(local.label)}); (&&probe).check_from_value(${literal(local.label)}); (&&probe).check_from_map(${literal(local.label)}); (&&&probe).probe(${literal(local.label)}, ${contract ? `Some(${literal(contractJson(contract))})` : 'None'});`;
      const symbol = `__accepted_set_local_${i}`;
      const added = `\n\n#[allow(dead_code)] #[unsafe(export_name = "${wasm ? `__accepted_set_${i}` : symbol}")] fn ${symbol}() { ${body} }\n`;
      const list = edits.get(destination) ?? [];
      list.push({ at, added });
      edits.set(destination, list);
      nativeSymbols.push(symbol);
    }
    for (const [file, insertions] of edits) {
      remember(file);
      let source = fs.readFileSync(file);
      for (const { at, added, remove = 0 } of insertions.sort((a, b) => b.at - a.at)) source = Buffer.concat([source.subarray(0, at), Buffer.from(added), source.subarray(at + remove)]);
      fs.writeFileSync(file, source);
    }
    if (!wasm) {
      localTests = `\npub fn __accepted_set_run() { ${nativeSymbols.map(symbol => `{ unsafe extern "Rust" { fn ${symbol}(); } let _ = std::panic::catch_unwind(|| unsafe { ${symbol}(); }); }`).join('\n')} }\n`;
      if (nativeBinary) localTests += '\nfn main() { let _ = __accepted_set_original_main; __accepted_set_run(); }\n';
    }
    if (node) localTests += nodeProbeHarness();
    remember(copiedRoot);
    fs.appendFileSync(copiedRoot, localTests);
    fs.appendFileSync(copiedRoot, `\n\n#[allow(dead_code)]\n#[path = ${literal(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../accepted-set/runtime.rs'))}]\nmod __accepted_set_runtime;\n`);
    const manifest = path.join(sandbox, path.relative(repo, crateRoot), 'Cargo.toml');
    remember(manifest);
    let text = fs.readFileSync(manifest, 'utf8');
    const dependencies = packageMetadata.dependencies.filter(dependency => dependency.kind !== 'dev').map(dependency => dependency.name);
    const extra = ['serde', 'serde_json'].filter(name => !dependencies.includes(name)).map(name => `${name}.workspace = true`).join('\n');
    const section = '[dependencies]';
    if (extra) text = text.includes(section) ? text.replace(section, `${section}\n${extra}`) : `${text}\n${section}\n${extra}\n`;
    if (wasm && !target.kind.includes('bin')) {
      if (target.crate_types.includes('cdylib')) {
        assert.ok(target.crate_types.includes('rlib') && target.crate_types.includes('cdylib'), `accepted-set: ${pkg.package}: wasm probes need both rlib and cdylib outputs`);
      } else text = text.includes('[lib]') ? text.replace('[lib]', '[lib]\ncrate-type = ["rlib", "cdylib"]') : `${text}\n[lib]\ncrate-type = ["rlib", "cdylib"]\n`;
    }
    if (!wasm && !nativeBinary && !node) {
      const runner = path.join(path.dirname(manifest), '__accepted_set_runner.rs');
      fs.writeFileSync(runner, `fn main() { ${target.name.replaceAll('-', '_')}::__accepted_set_run(); }\n`);
      text += '\n[[bin]]\nname = "accepted-set-production-runner"\npath = "__accepted_set_runner.rs"\n';
    }
    fs.writeFileSync(manifest, text);
    const features = configuration.features.length ? ['--features', configuration.features.join(',')] : [];
    const command = node ? ['build', '--color', 'never', '-p', pkg.package, '--jobs', jobs, '--lib', '--message-format=json', ...features] : wasm ? ['build', '--color', 'never', '-p', pkg.package, '--jobs', jobs, ...targetArgs, '--target', configuration.target, ...features] : ['run', '--color', 'never', '-p', pkg.package, '--jobs', jobs, '--bin', nativeBinary ? target.name : 'accepted-set-production-runner', ...features];
    const build = spawnSync('cargo', command, {
      cwd: path.join(sandbox, workspaceRelative), encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, CARGO_BUILD_JOBS: jobs, CARGO_TARGET_DIR: path.join(metadata.target_directory, 'accepted-set-build') },
    });
    if (node) fs.writeFileSync(path.join(metadata.target_directory, `accepted-set-${pkg.package}-build.log`), `${build.stdout}\n${build.stderr}`);
    const result = node ? executeNodeProbes(build, pkg, metadata.target_directory, manifest) : build;
    const log = `${result.stdout}\n${result.stderr}`;
    fs.writeFileSync(path.join(metadata.target_directory, `accepted-set-${pkg.package}.log`), log);
    let receipts;
    if (wasm) {
      assert.equal(result.status, 0, `accepted-set: ${pkg.package}: wasm compile failure is not a control\n${log}`);
      const bytes = fs.readFileSync(path.join(metadata.target_directory, 'accepted-set-build', configuration.target, 'debug', `${target.name.replaceAll('-', '_')}.wasm`));
      receipts = [];
      let instance;
      const module = new WebAssembly.Module(bytes);
      const imports = {};
      for (const entry of WebAssembly.Module.imports(module)) {
        assert.equal(entry.kind, 'function', `accepted-set: unsupported wasm import ${entry.module}::${entry.name}`);
        (imports[entry.module] ??= {})[entry.name] = () => { throw new Error(`accepted-set: shape decoder unexpectedly called host import ${entry.module}::${entry.name}`); };
      }
      imports.accepted_set = { report(pointer, length) {
        const view = new Uint8Array(instance.exports.memory.buffer, pointer, length);
        receipts.push(JSON.parse(new TextDecoder().decode(view)));
      } };
      instance = new WebAssembly.Instance(module, imports);
      for (let i = 0; i < owners.length + (pkg.locals?.length ?? 0); i++) {
        const before = receipts.length;
        try { instance.exports[`__accepted_set_${i}`](); }
        catch (error) { assert.ok(error instanceof WebAssembly.RuntimeError && receipts.length === before + 1 && receipts.at(-1).failures.length, `accepted-set: unexpected wasm trap for ${owners[i]?.name ?? pkg.locals[i - owners.length].label}: ${error}`); }
        assert.equal(receipts.length, before + 1, `accepted-set: missing wasm runtime result for ${owners[i]?.name ?? pkg.locals[i - owners.length].label}`);
      }
    } else {
    assert.ok(!result.error && result.status === 0 && !/^error(?:\[|: could not compile)/m.test(log), `accepted-set: ${pkg.package}: runtime probes did not execute (compile failure is not a control)\n${log}`);
    receipts = result.stdout.split('\n').filter(line => line.includes('ACCEPTED_SET_RESULT ')).map(line => JSON.parse(line.slice(line.indexOf('ACCEPTED_SET_RESULT ') + 20)));
    }
    assert.equal(receipts.length, owners.length + (pkg.locals?.length ?? 0), `accepted-set: ${pkg.package}: every compiler owner must produce a runtime result`);
    assert.deepEqual(receipts.map(row => row.name).sort(), [...owners.map(owner => owner.name), ...(pkg.locals ?? []).map(local => local.label)].sort(), `accepted-set: ${pkg.package}: runtime receipt identities must match compiler owners`);
    for (const helper of pkg.delegatedLocals ?? []) {
      assert.ok(receipts.some(row => row.name === helper.runtimeOwner && !row.classificationOnly), `${helper.name}: accepted-set assertion: delegated helper parent did not execute`);
      receipts.push({ name: helper.name, classificationOnly: true, category: 'private-decoder-helper', runtimeOwner: helper.runtimeOwner, failures: [] });
    }
    console.log(`${pkg.package}: ${receipts.filter(row => !row.classificationOnly).length} production runtime results; ${receipts.filter(row => row.classificationOnly).length} explicit classifications; ${receipts.filter(row => row.failures.length).length} failed owners`);
    results.push(...receipts.map(row => ({ ...row, configuration })));
    for (const [file, bytes] of originals) fs.writeFileSync(file, bytes);
  }
  fs.writeFileSync(path.join(metadata.target_directory, 'accepted-set-results.json'), `${JSON.stringify(results, null, 2)}\n`);
  return results;
}
