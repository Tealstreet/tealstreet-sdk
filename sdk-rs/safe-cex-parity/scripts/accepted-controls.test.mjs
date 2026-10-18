import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const fixture = path.resolve(scripts, '../fixtures/accepted-set');
const sourcePath = path.join(fixture, 'src/lib.rs');
const source = fs.readFileSync(sourcePath, 'utf8');

test('private Cargo targets cannot execute a stale default scanner in production or history', { timeout: 1200000 }, () => {
  const repo = path.resolve(scripts, '../../..');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-scanner-control-'));
  function command(program, args, options = {}) {
    const result = spawnSync(program, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...options });
    return { ...result, output: `${result.stdout ?? ''}\n${result.stderr ?? ''}` };
  }
  try {
    const archivePath = path.join(directory, 'source.tar');
    const archive = command('git', ['archive', '--format=tar', `--output=${archivePath}`, 'HEAD', 'sdk-rs', 'sdk-rs/rust-wasm.json', 'infra/tradetunnel', 'packages'], { cwd: repo });
    assert.equal(archive.status, 0, `scanner control must archive committed source\n${archive.error ?? ''}\n${archive.output}`);
    const extract = command('tar', ['-xf', archivePath, '-C', directory]);
    assert.equal(extract.status, 0, extract.output);
    const copiedScripts = path.join(directory, 'sdk-rs/safe-cex-parity/scripts');
    const copiedFixture = path.join(directory, 'sdk-rs/safe-cex-parity/fixtures/accepted-set');
    const workspaceLink = path.join(directory, 'fixture-link');
    fs.symlinkSync(copiedFixture, workspaceLink, 'dir');
    const selector = path.join(copiedScripts, 'accepted-scanner.mjs');
    const original = fs.readFileSync(selector, 'utf8');
    const stale = path.join(directory, 'sdk-rs/target/debug/accepted-source');
    fs.mkdirSync(path.dirname(stale), { recursive: true });
    const sentinel = path.join(directory, 'sentinel.rs');
    fs.writeFileSync(sentinel, 'fn main() { eprintln!("SCANNER_ARTIFACT_SENTINEL"); std::process::exit(17); }\n');
    const compile = command('rustc', [sentinel, '--crate-name', 'scanner_sentinel', '-o', stale]);
    assert.equal(compile.status, 0, `scanner sentinel must compile\n${compile.output}`);
    const targetDirectory = path.join(directory, 'private-target');
    const targetLink = path.join(directory, 'cargo-target-link');
    fs.mkdirSync(targetDirectory);
    fs.symlinkSync(targetDirectory, targetLink, 'dir');
    const env = { ...process.env, CARGO_TARGET_DIR: targetLink, CARGO_PROFILE_DEV_SPLIT_DEBUGINFO: 'off', CARGO_PROFILE_TEST_SPLIT_DEBUGINFO: 'off' };
    const production = () => command(process.execPath, [path.join(copiedScripts, 'accepted-set.mjs'), `--workspace=${workspaceLink}`, `--contracts=${path.join(copiedFixture, 'contracts.json')}`], { env });
    const declarationRepo = path.join(directory, 'declaration');
    const declaration = 'sdk-rs/control/src/lib.rs';
    fs.mkdirSync(path.dirname(path.join(declarationRepo, declaration)), { recursive: true });
    fs.writeFileSync(path.join(declarationRepo, declaration), 'pub enum Side { Buy }\n');
    const program = `import { birthVerifier } from ${JSON.stringify(pathToFileURL(path.join(copiedScripts, 'accepted-births.mjs')).href)}; const verify = birthVerifier(${JSON.stringify(declarationRepo)}, () => { throw Error('unexpected Git read'); }, new Map([['control::Side', ${JSON.stringify(declaration)}]])); verify({type:'control::Side',owner:{crate:'control'},declaration:${JSON.stringify(declaration)}}, 'WORKTREE', []);`;
    const history = () => command(process.execPath, ['--input-type=module', '--eval', program], { env });
    const baseline = () => {
      for (const execute of [production, history]) {
        const result = execute();
        assert.equal(result.status, 0, result.output);
        assert.doesNotMatch(result.output, /SCANNER_ARTIFACT_SENTINEL/);
      }
    };
    baseline();
    const returnStatement = 'return fs.realpathSync(receipt.executable);';
    assert.equal(original.split(returnStatement).length, 2, 'scanner mutation must select exactly one return');
    fs.writeFileSync(selector, original.replace(returnStatement, `return ${JSON.stringify(stale)};`));
    for (const [label, execute, assertion] of [
      ['scanner default-path production escape', production, /production module discovery assertion/],
      ['scanner default-path history escape', history, /outstanding defects: declaration parser failed/],
    ]) {
      const result = execute();
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, assertion);
      assert.match(result.output, /SCANNER_ARTIFACT_SENTINEL/);
      assert.doesNotMatch(result.output, /error\[E\d+\]|could not compile/);
      console.log(`${label}: named assertion; process exit 1`);
    }
    fs.writeFileSync(selector, original);
    baseline();
    const runner = path.join(copiedScripts, 'accepted-set.mjs');
    const originalRunner = fs.readFileSync(runner, 'utf8');
    for (const [label, before, after, assertion] of [
      ['symlinked audit source identity', 'fs.realpathSync(path.resolve(args.find', '(path.resolve(args.find', /alias source must belong to its compiler package/],
      ['symlinked reexport compiler identity', 'fs.realpathSync(filename) === compilerPath', 'path.resolve(filename) === compilerPath', /external reexport requires a unique resolved compiler dependency/],
    ]) {
      assert.equal(originalRunner.split(before).length, 2, `${label}: mutation must apply once`);
      fs.writeFileSync(runner, originalRunner.replace(before, after));
      const result = production();
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, assertion);
      assert.doesNotMatch(result.output, /error\[E\d+\]|could not compile/);
      console.log(`${label}: named assertion; process exit 1`);
      fs.writeFileSync(runner, originalRunner);
      baseline();
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
function run(extra = []) {
  const result = spawnSync(process.execPath, [path.join(scripts, 'accepted-set.mjs'), `--workspace=${fixture}`, `--contracts=${path.join(fixture, 'contracts.json')}`, ...extra], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const output = `${result.stdout}\n${result.stderr}`;
  assert.ok(!result.error, output);
  assert.doesNotMatch(output, /compile failure is not a control|error\[E\d+\]|could not compile/, output);
  return { ...result, output };
}
function negative(label, mutate, expected, extra = [], runtime = true) {
  try {
    const changed = mutate(source);
    assert.notEqual(changed, source, `${label}: mutation must apply`);
    fs.writeFileSync(sourcePath, changed);
    const result = run(extra);
    assert.equal(result.status, 1, `${label}: ${result.output}`);
    assert.match(result.output, /AssertionError/);
    assert.match(result.output, expected);
    if (runtime) assert.match(result.output, /production runtime results/);
    console.log(`${label}: named assertion; process exit ${result.status}`);
  } finally {
    fs.writeFileSync(sourcePath, source);
  }
}

test('invoked local macros cannot hide global decoders while private helpers remain scoped out', { timeout: 1200000 }, () => {
  const selected = ['--package=accepted-set-control'];
  const implementation = 'impl std::str::FromStr for Side { type Err = (); fn from_str(value: &str) -> std::result::Result<Self, ()> { match value { "buy" | "BUY" => Ok(Self::Buy), "sell" => Ok(Self::Sell), _ => Err(()) } } }';
  const native = '\n#[cfg(test)] mod macro_global_native_control { use super::Side; #[test] fn global_fromstr_accepts_uppercase_buy() { let decoded: Side = "BUY".parse().expect("MACRO-GLOBAL parse"); assert!(matches!(decoded, Side::Buy), "MACRO-GLOBAL NATIVE uppercase BUY decodes to Buy"); } }\n';
  const cargo = (command, exact = false) => {
    const result = spawnSync('cargo', [command, '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', 'accepted-set-control', '--lib', '--jobs', '2', ...(exact ? ['macro_global_native_control::global_fromstr_accepts_uppercase_buy', '--', '--exact', '--nocapture'] : [])], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
    const output = `${result.stdout}\n${result.stderr}`;
    assert.equal(result.status, 0, `local macro mutation must compile and execute\n${output}`);
    if (exact) assert.match(output, /1 passed; 0 failed/);
  };
  const baseline = () => { fs.writeFileSync(sourcePath, source); const result = run(selected); assert.equal(result.status, 0, result.output); };
  try {
    baseline();
    for (const [label, recipe] of [
      ['local macro unconditional global impl', `macro_rules! install_raw { () => { ${implementation} }; } install_raw!();`],
      ['local macro doc-hidden global impl', `macro_rules! install_raw { () => { #[cfg(not(doc))] ${implementation} }; } install_raw!();`],
      ['local macro doc-hidden forwarded impl', `macro_rules! install_raw { ($item:item) => { $item }; } install_raw! { #[cfg(not(doc))] ${implementation} }`],
      ['local macro cfg-disabled shadow', `macro_rules! install_raw { () => { #[cfg(not(doc))] ${implementation} }; } #[cfg(any())] macro_rules! install_raw { () => { let _ = 1; }; } install_raw!();`],
      ['local macro cfg-attr-disabled shadow', `macro_rules! install_raw { () => { #[cfg(not(doc))] ${implementation} }; } #[cfg_attr(all(), cfg(any()))] macro_rules! install_raw { () => { let _ = 1; }; } install_raw!();`],
      ['local macro inline-module shadow', `macro_rules! install_raw { () => { #[cfg(not(doc))] ${implementation} }; } mod inner { #[allow(unused_macros)] macro_rules! install_raw { () => { let _ = 1; }; } } install_raw!();`],
    ]) {
      fs.writeFileSync(sourcePath, `${source}\n#[allow(non_local_definitions)] pub fn install_local_macro_conversion() { ${recipe} }\n${native}`);
      // Observe the changed native enum before asking the gate to reject it.
      cargo('test', true);
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, /UNSUPPORTED_LOCAL_MACRO_EFFECTS:install_raw.*accepted-set assertion/);
      console.log(`${label}: named assertion; process exit 1`);
      baseline();
    }
    for (const recipe of [
      'macro_rules! make_helper { () => { #[allow(dead_code)] #[derive(serde::Deserialize)] struct Hidden { value: bool } }; } make_helper!();',
      'macro_rules! make_helper { () => { fn helper(value: &str) -> bool { value == "buy" } }; } make_helper!(); let _ = helper("buy");',
      `#[allow(unused_macros)] macro_rules! never_called { () => { #[cfg(not(doc))] ${implementation} }; }`,
    ]) {
      fs.writeFileSync(sourcePath, `${source}\npub fn private_macro_helper() { ${recipe} }\n`);
      cargo('check');
      const result = run(selected);
      assert.equal(result.status, 0, `function-local helper scope must remain accepted\n${result.output}`);
    }
  } finally { fs.writeFileSync(sourcePath, source); }
  baseline();
});

test('production runner rejects widening, new owners, and remote Self bypass at assertions', { timeout: 240000 }, () => {
  assert.equal(run().status, 0, 'strict baseline must pass');
  negative('doc-hidden production enum', value => `${value}\n#[cfg(not(doc))] #[derive(serde::Deserialize)] pub enum ShadowSide { Buy }\n`, /DOC_CONFIGURATION/, [], false);
  negative('doc-hidden production module', value => `${value}\n#[cfg(not(doc))] pub mod shadow { #[derive(serde::Deserialize)] pub struct ShadowRecord { pub value: bool } }\n`, /DOC_CONFIGURATION/, [], false);
  negative('doc-dependent attribute', value => `${value}\n#[cfg_attr(doc, cfg(any()))] #[derive(serde::Deserialize)] pub enum ShadowSide { Buy }\n`, /DOC_CONFIGURATION/, [], false);
  negative('raw doc predicate', value => `${value}\n#[cfg(not(r#doc))] #[derive(serde::Deserialize)] pub enum RawShadow { Buy }\n`, /DOC_CONFIGURATION/, [], false);
  negative('raw doc attribute predicate', value => `${value}\n#[cfg_attr(r#doc, cfg(any()))] #[derive(serde::Deserialize)] pub enum RawShadow { Buy }\n`, /DOC_CONFIGURATION/, [], false);
  const external = path.join(fixture, 'src/hidden.rs');
  try {
    fs.writeFileSync(external, '#![cfg(not(doc))]\n#[allow(dead_code)] #[derive(serde::Deserialize)] pub enum Hidden { Buy }\n');
    negative('external module inner doc predicate', value => `${value}\nmod hidden;\n`, /hidden.rs: accepted-set assertion: DOC_CONFIGURATION/, [], false);
    negative('local external module inner doc predicate', value => `${value}\n pub fn hidden_owner() { #[path = "hidden.rs"] mod hidden; }\n`, /UNSUPPORTED_LOCAL_EXTERNAL_MODULE.*accepted-set assertion/, [], false);
    negative('path module inner doc predicate', value => `${value}\n#[path = "hidden.rs"] mod other;\n`, /hidden.rs: accepted-set assertion: DOC_CONFIGURATION/, [], false);
    fs.writeFileSync(external, '#![r#cfg(not(doc))]\n#[allow(dead_code)] #[derive(serde::Deserialize)] pub enum Hidden { Buy }\n');
    negative('external raw cfg attribute', value => `${value}\nmod hidden;\n`, /hidden.rs: accepted-set assertion: DOC_CONFIGURATION/, [], false);
  } finally { fs.rmSync(external, { force: true }); }
  negative('enum re-widened', value => value.replace(/impl<'de> serde::Deserialize<'de> for Side \{[\s\S]*?\n\}\n\nmod private/, '\nmod private').replace('#[derive(Debug, serde::Serialize)]', '#[derive(Debug, serde::Serialize, serde::Deserialize)]'), /accepted_set_control::Side:.*Deserialize accepted.*buy/);
  negative('record re-widened', value => value.replace(/    impl<'de> serde::Deserialize<'de> for Record \{[\s\S]*?\n    \}\n/, '').replace('    #[derive(Debug, serde::Serialize)]', '    #[derive(Debug, serde::Serialize, serde::Deserialize)]'), /accepted_set_control::private::Record:.*Deserialize accepted \[true\]/);
  negative('unclassified public type', value => `${value}\n#[derive(serde::Deserialize)]\npub struct Unclassified { pub value: bool }\n`, /accepted_set_control::Unclassified: accepted-set assertion: unclassified deserializable type/);
  negative('remote Self bypass classification', value => value.replace('#[derive(Debug, serde::Serialize)]', '#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(remote = "Self")]'), /accepted_set_control::Side: accepted-set assertion: unclassified or changed decoder/);
  negative('remote Self concrete runtime bypass', value => value.replace('#[derive(Debug, serde::Serialize)]', '#[derive(Debug, serde::Serialize, serde::Deserialize)]\n#[serde(remote = "Self")]'), /accepted_set_control::Side:.*qualified decoder accepted.*buy/, ['--census']);
  negative('unclassified FromStr alias', value => `${value}\nimpl std::str::FromStr for Side { type Err = String; fn from_str(value: &str) -> Result<Self, Self::Err> { match value { "buy" | "BUY" => Ok(Self::Buy), "sell" => Ok(Self::Sell), _ => Err("unknown".into()) } } }\n`, /accepted_set_control::Side: accepted-set assertion: unclassified or changed decoder/);
  negative('unclassified free decoder', value => `${value}\npub fn decode_alias(value: &str) -> Option<Side> { if value == "BUY" { Some(Side::Buy) } else { None } }\n`, /accepted_set_control::decode_alias: accepted-set assertion: unclassified/);
  negative('aliased free decoder', value => `${value}\ntype Wire = String; pub fn decode_alias(value: Wire) -> bool { value == "BUY" }\n`, /accepted_set_control::decode_alias: accepted-set assertion: unclassified/);
  negative('generic free decoder', value => `${value}\npub fn decode_generic<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, serde_json::Error> { serde_json::from_value(value) }\n`, /accepted_set_control::decode_generic: accepted-set assertion: unclassified/);
  negative('custom trait decoder', value => `${value}\npub trait Decode { fn decode(value: &str) -> Option<Self> where Self: Sized; } impl Decode for Side { fn decode(value: &str) -> Option<Self> { if value == "BUY" { Some(Self::Buy) } else { None } } }\n`, /accepted_set_control::Side: accepted-set assertion: unclassified or changed decoder/);
  negative('default custom trait decoder', value => `${value}\npub trait Decode { fn decode(value: &str) -> Option<Side> { if value == "BUY" { Some(Side::Buy) } else { None } } } impl Decode for Side {}\n`, /accepted_set_control::Side: accepted-set assertion: unclassified or changed decoder/);
  negative('blanket custom trait decoder', value => `${value}\npub trait Decode { fn decode(value: &str) -> Option<Side>; } impl<T> Decode for T { fn decode(value: &str) -> Option<Side> { if value == "BUY" { Some(Side::Buy) } else { None } } }\n`, /Decode::decode for T: accepted-set assertion: decoder trait on unsupported receiver/, [], false);
  negative('custom trait returns another owner', value => `${value}\npub struct Factory; pub trait Decode { fn decode(value: &str) -> Option<Side>; } impl Decode for Factory { fn decode(value: &str) -> Option<Side> { if value == "BUY" { Some(Side::Buy) } else { None } } }\n`, /accepted_set_control::Factory: accepted-set assertion: unclassified/);
  negative('nested free decoder', value => `${value}\npub fn outer_decoder() { fn decode(value: &str) -> Side { if value == "BUY" { Side::Buy } else { Side::Sell } } let _ = decode("BUY"); }\n`, /LOCAL_FUNCTION:decode: accepted-set assertion: unclassified/, [], false);
  negative('local custom trait decoder', value => `${value}\npub fn outer_decoder() { trait Decode { fn decode(value: &str) -> Side; } impl Decode for Side { fn decode(_: &str) -> Side { Side::Buy } } let _ = <Side as Decode>::decode("BUY"); }\n`, /UNSUPPORTED_LOCAL_TRAIT:Decode.*accepted-set assertion/, [], false);
  negative('external receiver custom trait decoder', value => `${value}\npub trait Decode { fn decode(value: &str) -> Side; } impl Decode for () { fn decode(_: &str) -> Side { Side::Buy } }\n`, /Decode::decode for \(\): accepted-set assertion: decoder trait on unsupported receiver/, [], false);
  negative('uninstantiated default decoder', value => `${value}\npub trait Decode { fn decode(_: &str) -> Side { Side::Buy } }\n`, /Decode::decode: accepted-set assertion: uninstantiated default decoder/, [], false);
  negative('mutable output free decoder', value => `${value}\npub fn decode_into(value: &str, out: &mut Side) { if value == "BUY" { *out = Side::Buy; } }\n`, /accepted_set_control::decode_into: accepted-set assertion: unclassified/, [], true);
  negative('local Visitor bypass', value => `${value}\npub fn visitor_alias() { struct WireVisitor; impl<'de> serde::de::Visitor<'de> for WireVisitor { type Value = Side; fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("side") } fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Side, E> { Ok(Side::Buy) } } let _: Result<Side, serde_json::Error> = serde::de::Visitor::visit_str(WireVisitor, "BUY"); }\n`, /LOCAL_TRAIT_IMPL:serde::de::Visitor:WireVisitor: accepted-set assertion: unclassified/, [], false);
  negative('unsupported receiver inherits decoder default', value => `${value}\npub trait Decode { fn decode(_: &str) -> Side { Side::Buy } } impl Decode for Side {} impl Decode for () {}\n`, /Decode::decode for \(\): accepted-set assertion: decoder trait on unsupported receiver/, [], false);
  negative('function-local manual Deserialize', value => value.replace('    struct LocalHelper;', `    struct LocalHelper;
    impl<'de> serde::Deserialize<'de> for LocalHelper {
        fn deserialize<D: serde::Deserializer<'de>>(_decoder: D) -> Result<Self, D::Error> { Ok(Self) }
    }`), /LocalHelper: accepted-set assertion: unclassified function-local Deserialize/);
  negative('conversion-only owner', value => `${value}\npub struct ConversionOnly;\nimpl std::str::FromStr for ConversionOnly { type Err = String; fn from_str(_: &str) -> Result<Self, Self::Err> { Ok(Self) } }\n`, /accepted_set_control::ConversionOnly: accepted-set assertion: unclassified/);
  negative('const-local decoder', value => `${value}\nconst _: () = { #[allow(dead_code)] #[derive(serde::Deserialize)] struct Hidden { value: bool } };\n`, /Hidden: accepted-set assertion: unclassified function-local Deserialize/);
  negative('static-lifetime-only local decoder', value => value.replace('    struct LocalHelper;', `    struct LocalHelper; impl serde::Deserialize<'static> for LocalHelper { fn deserialize<D: serde::Deserializer<'static>>(_: D) -> Result<Self, D::Error> { Ok(Self) } }`), /LocalHelper: accepted-set assertion: borrowed local Deserialize/);
  negative('production differs from cfg-test decoder', value => value.replace("impl<'de> serde::Deserialize<'de> for Side", "#[cfg(test)]\nimpl<'de> serde::Deserialize<'de> for Side").replace('#[derive(Debug, serde::Serialize)]', '#[derive(Debug, serde::Serialize)]\n#[cfg_attr(not(test), derive(serde::Deserialize))]'), /accepted_set_control::Side:.*Deserialize accepted.*buy/);
  negative('local FromStr-only decoder', value => value.replace('    struct LocalHelper;', `    struct LocalHelper; impl std::str::FromStr for LocalHelper { type Err = String; fn from_str(_: &str) -> Result<Self, Self::Err> { Ok(Self) } }`), /LocalHelper: accepted-set assertion: local alternate decoder/);
  const macroBody = 'macro_rules! build_hidden { () => { #[allow(dead_code)] #[derive(serde::Deserialize)] struct Hidden { value: bool } }; }';
  try {
    fs.writeFileSync(sourcePath, `${source}\n#[allow(dead_code)] fn macro_owner() { ${macroBody} build_hidden!(); }\n`);
    const localMacro = run();
    assert.equal(localMacro.status, 0, `function-local macro is out of public scope: ${localMacro.output}`);
    fs.writeFileSync(sourcePath, `${source}\n${macroBody}\n#[allow(dead_code)] fn macro_owner() { build_hidden!(); }\n`);
    const hoistedMacro = run();
    assert.equal(hoistedMacro.status, 1, `module-scope macro must fail closed: ${hoistedMacro.output}`);
    assert.match(hoistedMacro.output, /UNSUPPORTED_MODULE_MACRO:build_hidden.*accepted-set assertion/);
    console.log('module-scope macro hoisted from local scope: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(sourcePath, source);
  }
  negative('qualified macro namespace shadow', value => `${value}\nmod std { pub use ::std::mem; macro_rules! make { () => {{ #[allow(dead_code)] #[derive(serde::Deserialize)] struct Hidden { value: bool } }} } pub(crate) use make as vec; } pub fn hidden_decoder() { std::vec!(); }\n`, /UNSUPPORTED_MACRO_NAMESPACE_SHADOW.*accepted-set assertion/, [], false);
  negative('local union decoder', value => value.replace('    struct LocalHelper;', `    #[allow(dead_code)] union LocalHelper { value: bool } impl<'de> serde::Deserialize<'de> for LocalHelper { fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> { Ok(Self { value: true }) } }`), /UNSUPPORTED_LOCAL_UNION:LocalHelper.*accepted-set assertion/, [], false);
  negative('private-construction Clone', value => `${value}\n#[derive(Clone)] pub struct Token { secret: bool }\nimpl Token { pub fn inspect(&self) -> bool { self.secret } }\n`, /Token: accepted-set assertion: private-construction Clone\/Copy/, [], false);
  assert.equal(run().status, 0, 'restored strict baseline must pass');
  assert.equal(run(['--target=wasm32-unknown-unknown']).status, 0, 'wasm strict baseline must pass');
});

test('classified alternate-only FromStr runs the string rejection corpus and positive witnesses', { timeout: 120000 }, () => {
  const contractPath = path.join(fixture, 'contracts.json');
  const original = fs.readFileSync(contractPath, 'utf8');
  try {
    fs.writeFileSync(sourcePath, `${source}\npub struct RawSide;\nimpl std::str::FromStr for RawSide { type Err = String; fn from_str(value: &str) -> Result<Self, Self::Err> { match value { "buy" | "BUY" | "sell" => Ok(Self), _ => Err("unknown".into()) } } }\n`);
    const inventoryRun = run(['--inventory']);
    assert.equal(inventoryRun.status, 0, inventoryRun.output);
    const inventory = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-inventory.json'), 'utf8'));
    const owner = inventory[0].types.find(type => type.ident === 'RawSide');
    const contracts = JSON.parse(original);
    contracts[owner.name] = { shape: 'string', literals: ['buy', 'sell'], evidence: 'Control closed string contract', entrypoints: [{ api: { kind: 'conversion', ...owner.conversions[0] }, input: 'string', reason: 'FromStr is a raw string decoder', positive: ['buy'], call: '$value.parse::<$type>().is_ok()' }] };
    fs.writeFileSync(contractPath, JSON.stringify(contracts));
    const widened = run();
    assert.equal(widened.status, 1, widened.output);
    assert.match(widened.output, /RawSide:.*entrypoint 0 accepted.*BUY/);
    console.log('alternate FromStr accepts alias: named assertion; process exit 1');
    contracts[owner.name].entrypoints[0].call = '{ let _ = $value; false }';
    fs.writeFileSync(contractPath, JSON.stringify(contracts));
    const inert = run();
    assert.equal(inert.status, 1, inert.output);
    assert.match(inert.output, /positive adapter witness rejected/);
    console.log('inert adapter rejects positive: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(contractPath, original);
  }
});

test('classified free decoder executes production output witnesses', { timeout: 120000 }, () => {
  const contractPath = path.join(fixture, 'contracts.json');
  const original = fs.readFileSync(contractPath, 'utf8');
  const strictSource = `${source}\npub fn decode_alias(value: &str) -> bool { value == "buy" }\n`;
  try {
    fs.writeFileSync(sourcePath, strictSource);
    const listed = run(['--inventory']);
    assert.equal(listed.status, 0, listed.output);
    const inventory = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-inventory.json'), 'utf8'));
    const owner = inventory[0].types.find(type => type.ident === 'decode_alias');
    assert.ok(owner, 'free decoder must be discovered');
    const contracts = JSON.parse(original);
    contracts[owner.name] = {
      api: owner.methods[0], category: 'decoder', evidence: 'Control accepts only the lowercase buy spelling',
      call: 'serde_json::json!($type($value.as_str().expect("string witness")))',
      cases: [{ input: 'buy', output: true }, { input: 'BUY', output: false }],
    };
    fs.writeFileSync(contractPath, JSON.stringify(contracts));
    const baseline = run();
    assert.equal(baseline.status, 0, baseline.output);
    const wasm = run(['--target=wasm32-unknown-unknown']);
    assert.equal(wasm.status, 0, wasm.output);
    fs.writeFileSync(sourcePath, strictSource.replace('value == "buy"', 'value == "buy" || value == "BUY"'));
    const widened = run();
    assert.equal(widened.status, 1, widened.output);
    assert.match(widened.output, /decode_alias:.*function output mismatch.*BUY/);
    console.log('classified free decoder alias widening: named assertion; process exit 1');
    contracts[owner.name].call = '{ let _ = $value; serde_json::json!(false) }';
    fs.writeFileSync(contractPath, JSON.stringify(contracts));
    const inert = run();
    assert.equal(inert.status, 1, inert.output);
    assert.match(inert.output, /decode_alias:.*function output mismatch.*buy/);
    console.log('inert free decoder adapter: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(contractPath, original);
  }
});

test('Deserialize output witnesses supplement strict shape probes', { timeout: 120000 }, () => {
  const contractPath = path.join(fixture, 'contracts.json');
  const original = fs.readFileSync(contractPath, 'utf8');
  try {
    const contracts = JSON.parse(original);
    contracts['accepted_set_control::Side'].observation = {
      call: 'match serde_json::from_value::<$type>($value.clone()).expect("valid side") { $type::Buy => serde_json::json!({"variant":"Buy"}), $type::Sell => serde_json::json!({"variant":"Sell"}) }',
      cases: [{ input: 'buy', output: { variant: 'Buy' } }, { input: 'sell', output: { variant: 'Sell' } }],
    };
    fs.writeFileSync(contractPath, JSON.stringify(contracts));
    const baseline = run();
    assert.equal(baseline.status, 0, baseline.output);
    const wasm = run(['--target=wasm32-unknown-unknown']);
    assert.equal(wasm.status, 0, wasm.output);
    assert.ok(source.includes('"buy" => Ok(Self::Buy)'), 'output control source must match');
    fs.writeFileSync(sourcePath, source.replace('"buy" => Ok(Self::Buy)', '"buy" => Ok(Self::Sell)'));
    const changed = run();
    assert.equal(changed.status, 1, changed.output);
    assert.doesNotMatch(changed.output, /error\[E\d+\]|could not compile/);
    assert.match(changed.output, /Side:.*Deserialize output mismatch.*buy/);
    console.log('Deserialize correct shape wrong value: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(contractPath, original);
  }
});

// A consumer-only invocation must still execute the canonical dependency owner.
test('external reexports retain canonical production decoder and feature coverage', { timeout: 1200000 }, () => {
  const consumer = path.join(fixture, 'reexport/src/lib.rs');
  const originalConsumer = fs.readFileSync(consumer, 'utf8');
  try {
    fs.writeFileSync(consumer, 'pub use accepted_set_control::*;\n');
    const selected = ['--package=accepted-set-reexport-control'];
    const baseline = () => {
      const result = run(selected);
      assert.equal(result.status, 0, result.output);
      assert.match(result.output, /accepted-set-control: [1-9]\d* production runtime results/, 'ROOT-REEXPORT canonical owner runtime executes');
    };
    baseline();
    const wasm = run([...selected, '--target=wasm32-unknown-unknown']);
    assert.equal(wasm.status, 0, wasm.output);
    assert.match(wasm.output, /accepted-set-control: [1-9]\d* production runtime results/, 'ROOT-REEXPORT wasm canonical owner runtime executes');
    negative('external reexport decoder widening', value => value.replace('"buy" => Ok(Self::Buy)', '"buy" | "BUY" => Ok(Self::Buy)'), /accepted_set_control::Side: Deserialize accepted "BUY"/, selected);
    baseline();
    const manifest = path.join(fixture, 'reexport/Cargo.toml');
    const original = fs.readFileSync(manifest, 'utf8');
    try {
      fs.writeFileSync(manifest, original.replace('path = ".."', 'path = "..", features = ["permissive"]'));
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, /accepted_set_control.*consumer and canonical owner compiler features differ/);
      console.log('external reexport feature mismatch: named assertion; process exit 1');
    } finally { fs.writeFileSync(manifest, original); }
    baseline();
  } finally { fs.writeFileSync(consumer, originalConsumer); }
  const restored = run(['--package=accepted-set-reexport-control']);
  assert.equal(restored.status, 0, restored.output);
});

test('public aliases classify targets by compiler package identity', { timeout: 180000 }, () => {
  const selected = ['--package=accepted-set-reexport-control', '--inventory'];
  const manifest = path.join(fixture, 'reexport/src/lib.rs');
  const original = fs.readFileSync(manifest, 'utf8');
  try {
    const baseline = run(selected);
    assert.equal(baseline.status, 0, baseline.output);
    const checkAlias = (label, alias, expected) => {
      try {
        fs.writeFileSync(manifest, `${original}\npub type ${alias};\n`);
        const result = run(selected);
        assert.equal(result.status, 1, `${label}: ${result.output}`);
        assert.doesNotMatch(result.output, /error\[E\d+\]|could not compile/);
        assert.match(result.output, expected);
        console.log(`${label}: named assertion; process exit 1`);
      } finally { fs.writeFileSync(manifest, original); }
    };
    checkAlias('public workspace alias', 'WorkspaceAlias = accepted_set_control::Side', /WorkspaceAlias.*public alias needs a mapped row or exemption/);
    checkAlias('renamed external workspace alias', 'VendoredAlias = serde_json_vendor::ValueLike', /VendoredAlias.*public alias needs a mapped row or exemption/);
  } finally { fs.writeFileSync(manifest, original); }
});

test('public alias probes execute native values and declared alternate traits', { timeout: 1200000 }, () => {
  const selected = ['--package=accepted-set-reexport-control'];
  const manifest = path.join(fixture, 'reexport/src/lib.rs');
  const contractPath = path.join(fixture, 'contracts.json');
  const original = fs.readFileSync(manifest, 'utf8');
  const originalContracts = fs.readFileSync(contractPath, 'utf8');
  // Remove the reexport: the workspace alias itself must enqueue its owner.
  const aliasOnly = original.replace('pub use accepted_set_control::Side as ExportedSide;\n', '');
  assert.notEqual(aliasOnly, original);
  const baseline = () => {
    fs.writeFileSync(manifest, aliasOnly);
    fs.writeFileSync(contractPath, originalContracts);
    const result = run(selected);
    assert.equal(result.status, 0, result.output);
    const receipts = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-results.json')));
    assert.deepEqual(receipts.map(row => row.name).sort(), [
      'accepted_set_control::Side', 'accepted_set_control::private::Record',
      'accepted_set_reexport_control::Callback', 'accepted_set_reexport_control::ClosedSide',
      'accepted_set_reexport_control::ExternalJson',
      'src/lib.rs::LocalHelper',
    ].sort());
    assert.equal(receipts.find(row => row.name.endsWith('::Callback')).classificationOnly, true);
  };
  try {
    baseline();
    fs.writeFileSync(manifest, aliasOnly.replace('ClosedSide = accepted_set_control::Side', 'ClosedSide = String'));
    const widened = run(selected);
    assert.equal(widened.status, 1, widened.output);
    assert.match(widened.output, /ClosedSide:.*alias compiler target changed/);
    console.log('public alias closed domain widened: named assertion; process exit 1');
    baseline();
    const contracts = JSON.parse(originalContracts);
    const adapter = contracts['accepted_set_reexport_control::ExternalJson'].entrypoints.find(entry => entry.trait === 'FromStr');
    adapter.positive[0] = 'invalid-json';
    adapter.nativeCases[0].input = 'invalid-json';
    fs.writeFileSync(contractPath, JSON.stringify(contracts));
    const inert = run(selected);
    assert.equal(inert.status, 1, inert.output);
    assert.match(inert.output, /ExternalJson: FromStr: native adapter witness rejected/);
    console.log('public alias declared adapter must execute: named assertion; process exit 1');
    baseline();
  } finally {
    fs.writeFileSync(manifest, original);
    fs.writeFileSync(contractPath, originalContracts);
  }
});

test('public aliases refuse unsupported static-only string conversions', { timeout: 1200000 }, () => {
  const selected = ['--package=accepted-set-reexport-control'];
  const baseline = run(selected);
  assert.equal(baseline.status, 0, baseline.output);
  const cases = [
    ['public alias static From', "impl From<&'static str> for Side { fn from(_: &'static str) -> Self { Self::Buy } }", /ClosedSide:.*unsupported From<&'static str>/],
    ['public alias static TryFrom', "impl TryFrom<&'static str> for Side { type Error = (); fn try_from(_: &'static str) -> Result<Self, ()> { Ok(Self::Buy) } }", /ClosedSide:.*unsupported TryFrom<&'static str>/],
  ];
  for (const [name, implementation, expected] of cases) {
    try {
      fs.writeFileSync(sourcePath, `${source}\n${implementation}\n`);
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, expected);
      console.log(`${name}: named assertion; process exit 1`);
    } finally { fs.writeFileSync(sourcePath, source); }
    const restored = run(selected);
    assert.equal(restored.status, 0, restored.output);
  }
});

test('production discovery rejects inherent factories returning another decoded type', { timeout: 1200000 }, () => {
  const selected = ['--package=accepted-set-control'];
  assert.equal(run(selected).status, 0, 'factory baseline must pass');
  negative('inherent factory returns another decoded owner', value => `${value}\npub struct Factory; impl Factory { pub fn decode_alias(value: &str) -> Side { if value == "BUY" { Side::Buy } else { Side::Sell } } }\n`, /Factory: accepted-set assertion: unclassified deserializable type/, selected);
  const restored = run(selected);
  assert.equal(restored.status, 0, restored.output);
});

test('production raw constructors cannot claim typed classifications without execution', { timeout: 1200000 }, () => {
  const selected = ['--package=accepted-set-control'];
  const contractPath = path.join(fixture, 'contracts.json');
  const originalContracts = fs.readFileSync(contractPath, 'utf8');
  const cases = [
    ['raw multi-input inherent typed claim', 'pub struct RawConstruct; impl RawConstruct { pub fn decode(_: &str, _: bool) -> Self { Self } }', 'inherent'],
    ['raw byte conversion typed claim', 'pub struct RawConstruct; impl From<Vec<u8>> for RawConstruct { fn from(_: Vec<u8>) -> Self { Self } }', 'conversion'],
    ['raw private alias typed claim', 'type PrivateWire = String; pub struct RawConstruct; impl RawConstruct { pub fn decode(_: PrivateWire, _: bool) -> Self { Self } }', 'inherent'],
  ];
  assert.equal(run(selected).status, 0, 'typed-constructor baseline must pass');
  for (const [name, implementation, kind] of cases) {
    try {
      fs.writeFileSync(sourcePath, `${source}\n${implementation}\n`);
      const discovered = run([...selected, '--inventory']);
      assert.equal(discovered.status, 0, discovered.output);
      const packages = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-inventory.json')));
      const owner = packages.find(item => item.package === 'accepted-set-control').types.find(item => item.ident === 'RawConstruct');
      assert.ok(owner, 'one actual RawConstruct declaration must be discovered');
      const entries = kind === 'inherent' ? owner.methods : owner.conversions;
      assert.equal(entries.length, 1, 'control declares exactly one entrypoint');
      assert.deepEqual(owner.rawEntrypoints, [true], 'compiler-expanded raw-input role must be retained');
      const contracts = JSON.parse(originalContracts);
      contracts[owner.name] = { shape: 'typed', evidence: 'Control deliberately misclassifies raw input.', entrypoints: [{ api: { kind, ...entries[0] }, input: 'typed', reason: 'Control typed bypass attempt.' }] };
      fs.writeFileSync(contractPath, JSON.stringify(contracts));
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, /RawConstruct: accepted-set assertion: raw decoder cannot be classified typed/);
      console.log(`${name}: named assertion; process exit 1`);
    } finally {
      fs.writeFileSync(sourcePath, source);
      fs.writeFileSync(contractPath, originalContracts);
    }
    const restored = run(selected);
    assert.equal(restored.status, 0, restored.output);
  }
});

test('opaque module item macros cannot hide declarations from doc inventory', { timeout: 1200000 }, () => {
  const included = path.join(fixture, 'src/opaque_include.rs');
  const consumer = path.join(fixture, 'reexport/src/lib.rs');
  const vendor = path.join(fixture, 'vendor-serde-json/src/lib.rs');
  const originalConsumer = fs.readFileSync(consumer, 'utf8');
  const originalVendor = fs.readFileSync(vendor, 'utf8');
  const compiles = packageName => {
    const result = spawnSync('cargo', ['check', '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', packageName, '--lib', '--jobs', '2'], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
    assert.equal(result.status, 0, `module macro must compile; compiler failures are not controls\n${result.stdout}\n${result.stderr}`);
  };
  const primary = ['--package=accepted-set-control'];
  assert.equal(run(primary).status, 0, 'module macro baseline must pass');
  try {
    fs.writeFileSync(included, '#[cfg(not(doc))] #[derive(serde::Deserialize)] pub enum IncludedShadow { Buy }\n');
    fs.writeFileSync(sourcePath, `${source}\ninclude!("opaque_include.rs");\n`);
    compiles('accepted-set-control');
    const result = run(primary);
    assert.equal(result.status, 1, result.output);
    assert.match(result.output, /UNSUPPORTED_MODULE_MACRO_INVOCATION:include/);
    console.log('opaque module include hides decoder: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.rmSync(included, { force: true });
  }
  const restoredPrimary = run(primary);
  assert.equal(restoredPrimary.status, 0, restoredPrimary.output);
  const selected = ['--package=accepted-set-reexport-control'];
  try {
    fs.writeFileSync(vendor, `${originalVendor}\n#[macro_export] macro_rules! hidden_decoder { () => { #[cfg(not(doc))] pub struct ImportedShadow; #[cfg(not(doc))] impl std::str::FromStr for ImportedShadow { type Err = (); fn from_str(_: &str) -> Result<Self, ()> { Ok(Self) } } }; }\n`);
    fs.writeFileSync(consumer, `${originalConsumer}\nserde_json_vendor::hidden_decoder!();\n`);
    compiles('accepted-set-reexport-control');
    const result = run(selected);
    assert.equal(result.status, 1, result.output);
    assert.match(result.output, /UNSUPPORTED_MODULE_MACRO_INVOCATION:serde_json_vendor::hidden_decoder/);
    console.log('opaque imported module macro hides decoder: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(consumer, originalConsumer);
    fs.writeFileSync(vendor, originalVendor);
  }
  const restoredConsumer = run(selected);
  assert.equal(restoredConsumer.status, 0, restoredConsumer.output);
});

test('expression macro arguments cannot hide nested decoder declarations', { timeout: 1200000 }, () => {
  const selected = ['--package=accepted-set-control'];
  const baseline = run(selected);
  assert.equal(baseline.status, 0, baseline.output);
  for (const [label, declaration, expected] of [
    ['expression macro nested decoder', 'pub fn macro_owner() { let _ = vec![{ fn decode(value: &str) -> Side { if value == "BUY" { Side::Buy } else { Side::Sell } } decode("BUY") }]; }', /UNSUPPORTED_LOCAL_MACRO:vec/],
    ['constant expression macro nested decoder', 'const _: bool = matches!({ const fn decode(_: &str) -> bool { true } decode("BUY") }, true);', /UNSUPPORTED_LOCAL_MACRO:matches/],
  ]) {
    try {
      fs.writeFileSync(sourcePath, `${source}\n${declaration}\n`);
      const compiled = spawnSync('cargo', ['check', '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', 'accepted-set-control', '--lib', '--jobs', '2'], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
      assert.equal(compiled.status, 0, `expression macro control must compile\n${compiled.stdout}\n${compiled.stderr}`);
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, expected);
      assert.match(result.output, /accepted-set assertion/);
      console.log(`${label}: named assertion; process exit 1`);
    } finally { fs.writeFileSync(sourcePath, source); }
    const restored = run(selected);
    assert.equal(restored.status, 0, restored.output);
  }
});

test('accepted outer expression macros cannot conceal opaque nested calls or imports', { timeout: 1200000 }, () => {
  const consumer = path.join(fixture, 'reexport/src/lib.rs');
  const vendor = path.join(fixture, 'vendor-serde-json/src/lib.rs');
  const originalConsumer = fs.readFileSync(consumer, 'utf8');
  const originalVendor = fs.readFileSync(vendor, 'utf8');
  const selected = ['--package=accepted-set-reexport-control'];
  const baseline = `${originalConsumer}\npub fn nested_expression_baseline() { let _ = format!(concat!("a", "b")); let _ = vec![serde_json::json!({"ok": true})]; }\n`;
  try {
    fs.writeFileSync(consumer, baseline);
    const strict = run(selected);
    assert.equal(strict.status, 0, strict.output);
    fs.writeFileSync(vendor, `${originalVendor}\n#[macro_export] macro_rules! hidden_decoder_expr { () => {{ fn decode(_: &str) -> bool { true } decode("BUY") }}; }\n`);
    for (const [label, expression, expected] of [
      ['nested opaque expression macro', 'vec![serde_json_vendor::hidden_decoder_expr!()]', /UNSUPPORTED_LOCAL_MACRO:hidden_decoder_expr/],
      ['nested expression macro import', 'vec![{ use serde_json_vendor::hidden_decoder_expr as json; json!() }]', /UNSUPPORTED_LOCAL_MACRO:vec/],
    ]) {
      try {
        fs.writeFileSync(consumer, `${baseline}\npub fn opaque_nested_owner() { let _ = ${expression}; }\n`);
        const compiled = spawnSync('cargo', ['check', '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', 'accepted-set-reexport-control', '--lib', '--jobs', '2'], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
        assert.equal(compiled.status, 0, `nested macro mutation must compile\n${compiled.stdout}\n${compiled.stderr}`);
        const result = run(selected);
        assert.equal(result.status, 1, result.output);
        assert.match(result.output, expected);
        assert.match(result.output, /accepted-set assertion/);
        console.log(`${label}: named assertion; process exit 1`);
      } finally { fs.writeFileSync(consumer, baseline); }
      const restored = run(selected);
      assert.equal(restored.status, 0, restored.output);
    }
  } finally {
    fs.writeFileSync(consumer, originalConsumer);
    fs.writeFileSync(vendor, originalVendor);
  }
  const restored = run(selected);
  assert.equal(restored.status, 0, restored.output);
});

test('trusted macro spellings cannot impersonate registry compiler identities', { timeout: 1200000 }, () => {
  const consumer = path.join(fixture, 'macro-consumer/src/lib.rs');
  const vendor = path.join(fixture, 'vendor-serde-json/src/lib.rs');
  const originalConsumer = fs.readFileSync(consumer, 'utf8');
  const originalVendor = fs.readFileSync(vendor, 'utf8');
  const selected = ['--package=accepted-set-macro-consumer-control'];
  const baseline = run(selected);
  assert.equal(baseline.status, 0, baseline.output);
  try {
    fs.writeFileSync(vendor, `${originalVendor}\n#[macro_export] macro_rules! json { ($($value:tt)*) => {{ fn decode(_: &str) -> bool { true } decode("BUY") }}; }\n`);
    for (const [label, declaration] of [
      ['qualified macro dependency substitution', 'pub fn opaque_owner() { let _ = serde_json::json!({}); }'],
      ['imported macro dependency substitution', 'use serde_json::json; pub fn opaque_owner() { let _ = json!({}); }'],
      ['nested macro dependency substitution', 'pub fn opaque_owner() { let _ = vec![serde_json::json!({})]; }'],
    ]) {
      try {
        fs.writeFileSync(consumer, `${originalConsumer}\n${declaration}\n`);
        const compiled = spawnSync('cargo', ['check', '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', 'accepted-set-macro-consumer-control', '--lib', '--jobs', '2'], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
        assert.equal(compiled.status, 0, `macro package substitution must compile\n${compiled.stdout}\n${compiled.stderr}`);
        const result = run(selected);
        assert.equal(result.status, 1, result.output);
        assert.match(result.output, /UNSUPPORTED_LOCAL_MACRO:json/);
        assert.match(result.output, /accepted-set assertion/);
        console.log(`${label}: named assertion; process exit 1`);
      } finally { fs.writeFileSync(consumer, originalConsumer); }
      const restored = run(selected);
      assert.equal(restored.status, 0, restored.output);
    }
  } finally {
    fs.writeFileSync(consumer, originalConsumer);
    fs.writeFileSync(vendor, originalVendor);
  }
  const restored = run(selected);
  assert.equal(restored.status, 0, restored.output);
});

test('verified builtin namespaces cannot rename opaque macros into accepted names', { timeout: 1200000 }, () => {
  const included = path.join(fixture, 'src/opaque_macro_expression.rs');
  const selected = ['--package=accepted-set-control'];
  const baseline = run(selected);
  assert.equal(baseline.status, 0, baseline.output);
  for (const [label, body, declaration] of [
    ['verified builtin opaque macro rename', '{ fn decode(_: &str) -> bool { true } decode("BUY") }', 'use std::include as vec; pub fn renamed_macro_owner() { let _ = vec!("opaque_macro_expression.rs"); }'],
    ['verified builtin diagnostic macro rename', '#[cfg(not(doc))] #[derive(serde::Deserialize)] pub enum IncludedDecoder { Buy }', 'use std::include as compile_error; compile_error!("opaque_macro_expression.rs");'],
  ]) {
    try {
      fs.writeFileSync(included, `${body}\n`);
      fs.writeFileSync(sourcePath, `${source}\n${declaration}\n`);
      const compiled = spawnSync('cargo', ['check', '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', 'accepted-set-control', '--lib', '--jobs', '2'], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
      assert.equal(compiled.status, 0, `renamed builtin macro mutation must compile\n${compiled.stdout}\n${compiled.stderr}`);
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, /UNSUPPORTED_MACRO_IMPORT_SCOPE/);
      assert.match(result.output, /accepted-set assertion/);
      console.log(`${label}: named assertion; process exit 1`);
    } finally {
      fs.writeFileSync(sourcePath, source);
      fs.rmSync(included, { force: true });
    }
    const restored = run(selected);
    assert.equal(restored.status, 0, restored.output);
  }
});

test('alias alternate native values reject swaps and inert projections before domain checks', { timeout: 1200000 }, () => {
  const contractPath = path.join(fixture, 'contracts.json');
  const originalContracts = fs.readFileSync(contractPath, 'utf8');
  const implementation = '\nimpl std::str::FromStr for Side { type Err = (); fn from_str(value: &str) -> Result<Self, ()> { match value { "buy" => Ok(Self::Buy), "sell" => Ok(Self::Sell), _ => Err(()) } } }\n';
  const selected = ['--package=accepted-set-reexport-control'];
  try {
    fs.writeFileSync(sourcePath, `${source}${implementation}`);
    const collected = run(['--package=accepted-set-control', '--inventory']);
    assert.equal(collected.status, 0, collected.output);
    const inventory = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-inventory.json')));
    const owner = inventory[0].types.find(type => type.ident === 'Side');
    assert.equal(owner.conversions.length, 1, 'one independently introduced FromStr implementation');
    const contracts = JSON.parse(originalContracts);
    contracts[owner.name].entrypoints = [{ api: { kind: 'conversion', ...owner.conversions[0] }, input: 'string', reason: 'Control executes the canonical FromStr accepted domain separately from alias native values.', positive: ['buy', 'sell'], call: '$value.parse::<$type>().is_ok()' }];
    const alias = contracts['accepted_set_reexport_control::ClosedSide'];
    alias.nativeProjection = 'serde_json::json!(match $decoded { accepted_set_control::Side::Buy => "Buy", accepted_set_control::Side::Sell => "Sell" })';
    alias.entrypoints = [{ trait: 'FromStr', reason: 'Control requires both actual native enum variants.', positive: ['buy', 'sell'], negative: ['BUY', 'invalid'], nativeCases: [{ input: 'buy', output: 'Buy' }, { input: 'sell', output: 'Sell' }] }];
    const baseline = () => {
      fs.writeFileSync(sourcePath, `${source}${implementation}`);
      fs.writeFileSync(contractPath, JSON.stringify(contracts));
      const result = run(selected);
      assert.equal(result.status, 0, result.output);
    };
    baseline();
    const swapped = implementation.replace('"buy" => Ok(Self::Buy)', '"buy" => Ok(Self::Sell)').replace('"sell" => Ok(Self::Sell)', '"sell" => Ok(Self::Buy)');
    assert.notEqual(swapped, implementation);
    fs.writeFileSync(sourcePath, `${source}${swapped}`);
    const result = run(selected);
    assert.equal(result.status, 1, result.output);
    assert.match(result.output, /ClosedSide: FromStr: native output mismatch/);
    const receipts = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-results.json')));
    const failures = receipts.find(row => row.name === 'accepted_set_reexport_control::ClosedSide').failures;
    assert.equal(failures.length, 2, 'both independent native variants must fail');
    assert.ok(failures.every(message => message.includes('FromStr: native output mismatch')));
    assert.deepEqual(receipts.find(row => row.name === 'accepted_set_control::Side').failures, [], 'canonical domain check must not detect the native-only swap');
    console.log('public alias alternate native variant swap: named assertion; process exit 1');
    baseline();
    const inertContracts = structuredClone(contracts);
    inertContracts['accepted_set_reexport_control::ClosedSide'].nativeProjection = '{ let _ = $decoded; serde_json::json!("Buy") }';
    fs.writeFileSync(contractPath, JSON.stringify(inertContracts));
    const inert = run(selected);
    assert.equal(inert.status, 1, inert.output);
    assert.match(inert.output, /ClosedSide: FromStr: native output mismatch/);
    console.log('public alias inert native projection: named assertion; process exit 1');
    baseline();
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(contractPath, originalContracts);
  }
  const restored = run(selected);
  assert.equal(restored.status, 0, restored.output);
});

test('public alias compiler targets pin concrete generic arguments', { timeout: 1200000 }, () => {
  const consumer = path.join(fixture, 'reexport/src/lib.rs');
  const original = fs.readFileSync(consumer, 'utf8');
  const selected = ['--package=accepted-set-reexport-control'];
  const baseline = run(selected);
  assert.equal(baseline.status, 0, baseline.output);
  try {
    const changed = original.replace('Callback = Box<dyn Fn(&str)>', 'Callback = Box<dyn Fn(&[u8])>');
    assert.notEqual(changed, original);
    fs.writeFileSync(consumer, changed);
    const result = run(selected);
    assert.equal(result.status, 1, result.output);
    assert.match(result.output, /Callback:.*alias compiler target changed/);
    console.log('public alias concrete generic target changed: named assertion; process exit 1');
  } finally { fs.writeFileSync(consumer, original); }
  const restored = run(selected);
  assert.equal(restored.status, 0, restored.output);
});

test('real generic identity aliases keep fixed monomorphization and reject target or redirect changes', { timeout: 1200000 }, () => {
  const consumer = path.join(fixture, 'reexport/src/lib.rs');
  const contractPath = path.join(fixture, 'contracts.json');
  const exemptionsPath = path.join(fixture, 'parity/exempt.toml');
  const original = fs.readFileSync(consumer, 'utf8');
  const originalContracts = fs.readFileSync(contractPath, 'utf8');
  const originalExemptions = fs.readFileSync(exemptionsPath, 'utf8');
  const selected = ['--package=accepted-set-reexport-control'];
  const identity = `${original}\npub type Identity<T> = T;\n`;
  const contracts = JSON.parse(originalContracts);
  contracts['accepted_set_reexport_control::Identity'] = { ...structuredClone(contracts['accepted_set_reexport_control::ExternalJson']), target: { generic: 'T' }, evidence: 'Real fixture identity alias; only its fixed JSON-value monomorphization is covered.' };
  try {
    fs.writeFileSync(exemptionsPath, `${originalExemptions}\n[[exempt]]\nrs = "accepted-set-reexport-control/src/lib.rs::Identity"\nreason = "Native gate fixture with no TypeScript source symbol."\n`);
    const baseline = () => {
      fs.writeFileSync(consumer, identity);
      fs.writeFileSync(contractPath, JSON.stringify(contracts));
      const result = run(selected);
      assert.equal(result.status, 0, result.output);
      const receipts = JSON.parse(fs.readFileSync(path.join(fixture, 'target/accepted-set-results.json')));
      assert.deepEqual(receipts.map(row => row.name).sort(), ['accepted_set_control::Side', 'accepted_set_control::private::Record', 'accepted_set_reexport_control::Callback', 'accepted_set_reexport_control::ClosedSide', 'accepted_set_reexport_control::ExternalJson', 'accepted_set_reexport_control::Identity', 'src/lib.rs::LocalHelper'].sort());
      assert.equal(receipts.find(row => row.name.endsWith('::Identity')).monomorphization, 'one fixed serde_json::Value identity-alias probe');
    };
    baseline();
    fs.writeFileSync(consumer, identity.replace('Identity<T> = T;', 'Identity<T> = Option<T>;'));
    const wrapped = run(selected);
    assert.equal(wrapped.status, 1, wrapped.output);
    assert.match(wrapped.output, /Identity:.*source must directly name its type parameter/);
    console.log('generic identity alias wrapped target: named assertion; process exit 1');
    baseline();
    const redirected = structuredClone(contracts);
    redirected['accepted_set_reexport_control::Identity'].instantiation = 'accepted_set_control::Side';
    fs.writeFileSync(contractPath, JSON.stringify(redirected));
    const redirect = run(selected);
    assert.equal(redirect.status, 1, redirect.output);
    assert.match(redirect.output, /Identity:.*alias cannot redirect its production type/);
    console.log('generic identity alias contract redirect: named assertion; process exit 1');
    baseline();
  } finally {
    fs.writeFileSync(consumer, original);
    fs.writeFileSync(contractPath, originalContracts);
    fs.writeFileSync(exemptionsPath, originalExemptions);
  }
});

test('verified thread local statics expose decoder initializers and reject opaque origins', { timeout: 1200000 }, () => {
  const included = path.join(fixture, 'src/thread_local_opaque.rs');
  const consumer = path.join(fixture, 'reexport/src/lib.rs');
  const vendor = path.join(fixture, 'vendor-serde-json/src/lib.rs');
  const originalConsumer = fs.readFileSync(consumer, 'utf8');
  const originalVendor = fs.readFileSync(vendor, 'utf8');
  const declaration = 'std::thread_local! { static SCANNER_CACHE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }';
  const valid = `${source}\n${declaration}\npub fn scanner_cache_read() -> bool { SCANNER_CACHE.with(|value| value.get()) }\npub fn scanner_local_cache() -> bool { thread_local! { static LOCAL_CACHE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; } LOCAL_CACHE.with(|value| value.get()) }\n#[cfg(test)] thread_local! { static TEST_ONLY: bool = { fn hidden(_: &str) -> bool { true } hidden("BUY") }; }\n`;
  const selected = ['--package=accepted-set-control'];
  const compiles = packageName => {
    const result = spawnSync('cargo', ['check', '--locked', '--manifest-path', path.join(fixture, 'Cargo.toml'), '-p', packageName, '--lib', '--jobs', '2'], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
    assert.equal(result.status, 0, `thread local mutation must compile\n${result.stdout}\n${result.stderr}`);
  };
  try {
    const baseline = () => {
      fs.writeFileSync(sourcePath, valid);
      const result = run(selected);
      assert.equal(result.status, 0, result.output);
    };
    baseline();
    fs.writeFileSync(sourcePath, valid.replace(declaration, `${declaration}\nstd::thread_local! { pub static EXPOSED_DECODER: fn(&str) -> Side = |value| if value == "BUY" { Side::Buy } else { Side::Sell }; }`));
    compiles('accepted-set-control');
    const callable = run(selected);
    assert.equal(callable.status, 1, callable.output);
    assert.match(callable.output, /UNSUPPORTED_THREAD_LOCAL_VISIBILITY/);
    assert.match(callable.output, /accepted-set assertion/);
    console.log('thread local visible callable static: named assertion; process exit 1');
    baseline();
    fs.writeFileSync(included, '{ fn opaque_decoder(_: &str) -> bool { true } opaque_decoder("BUY") }\n');
    for (const [label, replacement, expected] of [
      ['thread local initializer nested decoder', 'std::cell::Cell::new({ fn decode(value: &str) -> bool { value == "BUY" } decode("BUY") })', /LOCAL_FUNCTION:decode/],
      ['thread local initializer opaque macro', 'std::cell::Cell::new(include!("thread_local_opaque.rs"))', /UNSUPPORTED_LOCAL_MACRO:include/],
      ['thread local doc-dependent static', null, /DOC_CONFIGURATION/],
    ]) {
      const changed = replacement ? valid.replace('const { std::cell::Cell::new(false) }', replacement) : valid.replace('static SCANNER_CACHE:', '#[cfg(not(doc))] static SCANNER_CACHE:');
      assert.notEqual(changed, valid);
      fs.writeFileSync(sourcePath, changed);
      compiles('accepted-set-control');
      const result = run(selected);
      assert.equal(result.status, 1, result.output);
      assert.match(result.output, expected);
      assert.match(result.output, /accepted-set assertion/);
      console.log(`${label}: named assertion; process exit 1`);
      baseline();
    }
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(vendor, `${originalVendor}\n#[macro_export] macro_rules! thread_local { ($($tokens:tt)*) => { #[cfg(not(doc))] pub struct OpaqueThreadDecoder; #[cfg(not(doc))] impl std::str::FromStr for OpaqueThreadDecoder { type Err = (); fn from_str(_: &str) -> Result<Self, ()> { Ok(Self) } } }; }\n`);
    fs.writeFileSync(consumer, `${originalConsumer}\nuse serde_json_vendor::thread_local; thread_local! { static INERT_INPUT: bool = true; }\n`);
    compiles('accepted-set-reexport-control');
    const fake = run(['--package=accepted-set-reexport-control']);
    assert.equal(fake.status, 1, fake.output);
    assert.match(fake.output, /UNSUPPORTED_MACRO_IMPORT_SCOPE/);
    console.log('thread local fake imported provider: named assertion; process exit 1');
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(consumer, originalConsumer);
    fs.writeFileSync(vendor, originalVendor);
    fs.rmSync(included, { force: true });
  }
  const restored = run(['--package=accepted-set-reexport-control']);
  assert.equal(restored.status, 0, restored.output);
});

test('native binary probes retain production main without executing it or redirecting references', { timeout: 1200000 }, () => {
  const manifest = path.join(fixture, 'Cargo.toml');
  const originalManifest = fs.readFileSync(manifest);
  const binary = path.join(fixture, 'src/accepted_binary_control.rs');
  assert.equal(fs.existsSync(binary), false, 'control binary must be newly authored');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-binary-control-'));
  const rules = path.join(directory, 'contracts.json');
  const controlSource = 'use serde::Deserialize;\n#[derive(Deserialize)] enum Never {}\nfn main() { let _ = std::mem::size_of::<Never>(); panic!("ACCEPTED_BINARY_ORIGINAL_MAIN_TRIPWIRE"); }\n';
  const originalContracts = JSON.parse(fs.readFileSync(path.join(fixture, 'contracts.json')));
  originalContracts['accepted_binary_control::Never'] = { shape: 'uninhabited', evidence: 'Control enum has no inhabitants.', entrypoints: [] };
  fs.writeFileSync(rules, JSON.stringify(originalContracts));
  const loader = path.join(directory, 'loader.mjs');
  function execute(mutation = '') {
    fs.writeFileSync(loader, `import {registerHooks} from 'node:module'; registerHooks({load(url,context,next){const result=next(url,context);if(url.endsWith('/accepted-probes.mjs')){let source=String(result.source);const before=${JSON.stringify('let _ = __accepted_set_original_main; __accepted_set_run();')};if(!source.includes(before))throw Error('binary mutation must match actual emitter');source=source.replace(before,${JSON.stringify(mutation || 'let _ = __accepted_set_original_main; __accepted_set_run();')});return {...result,source};}return result;}});`);
    const result = spawnSync(process.execPath, ['--import', loader, path.join(scripts, 'accepted-set.mjs'), `--workspace=${fixture}`, `--contracts=${rules}`, '--package=accepted-set-control'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    const output = `${result.stdout}\n${result.stderr}`;
    assert.doesNotMatch(output, /error\[E\d+\]|could not compile/, `compiler failures are not binary controls\n${output}`);
    return { ...result, output };
  }
  try {
    fs.writeFileSync(manifest, `${originalManifest.toString()}\n[[bin]]\nname="accepted-binary-control"\npath="src/accepted_binary_control.rs"\n`);
    fs.writeFileSync(binary, controlSource);
    let result = execute(); assert.equal(result.status, 0, result.output);
    for (const [label, mutation, assertion] of [
      ['binary original main accidentally executed', '__accepted_set_original_main(); __accepted_set_run();', /ACCEPTED_BINARY_ORIGINAL_MAIN_TRIPWIRE/],
      ['binary probe driver omitted', 'let _ = __accepted_set_original_main;', /every compiler owner must produce a runtime result/],
    ]) {
      result = execute(mutation); assert.equal(result.status, 1, result.output); assert.match(result.output, assertion);
      console.log(`${label}: named assertion; process exit 1`);
      result = execute(); assert.equal(result.status, 0, result.output);
    }
    for (const [label, reference] of [['binary production main reference', 'main'], ['binary raw production main reference', 'r#main']]) {
      fs.writeFileSync(binary, `${controlSource}\nconst _: fn() = ${reference};\n`);
      result = execute(); assert.equal(result.status, 1, result.output); assert.match(result.output, /production main references cannot be redirected to harness/);
      console.log(`${label}: named assertion; process exit 1`);
      fs.writeFileSync(binary, controlSource);
      result = execute(); assert.equal(result.status, 0, result.output);
    }
    fs.writeFileSync(binary, controlSource);
    result = execute(); assert.equal(result.status, 0, result.output);
  } finally {
    fs.writeFileSync(manifest, originalManifest);
    fs.rmSync(binary, { force: true });
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('generated metadata and snapshot map guards reject actual production widening', { timeout: 1200000 }, () => {
  const sdk = path.resolve(scripts, '../..');
  const metadata = spawnSync('cargo', ['metadata', '--no-deps', '--locked', '--format-version', '1'], { cwd: sdk, encoding: 'utf8' });
  assert.equal(metadata.status, 0, metadata.stderr);
  const results = path.join(JSON.parse(metadata.stdout).target_directory, 'accepted-set-results.json');
  const guards = [
    ['ExchangeFeatures', 'safe_cex_types::exchange_metadata::generated::ExchangeFeatures', 'exchange_metadata_generated.rs'],
    ['ExchangeMetadata', 'safe_cex_types::exchange_metadata::generated::ExchangeMetadata', 'exchange_metadata_generated.rs'],
    ['Snapshot', 'safe_cex_types::exchange_metadata::Snapshot', 'exchange_metadata.rs'],
  ];
  const originals = new Map(guards.map(([, , file]) => {
    const source = path.join(sdk, 'safe-cex-types/src', file);
    return [source, fs.readFileSync(source, 'utf8')];
  }));
  const program = `import assert from 'node:assert/strict';import fs from 'node:fs';import {spawnSync} from 'node:child_process';const run=spawnSync(process.execPath,${JSON.stringify([path.join(scripts, 'accepted-set.mjs'), '--measure', '--package=safe-cex-types'])},{encoding:'utf8',maxBuffer:64*1024*1024});process.stdout.write(run.stdout??'');process.stderr.write(run.stderr??'');assert.equal(run.status,0,'metadata probe preparation must complete');const rows=JSON.parse(fs.readFileSync(${JSON.stringify(results)}));for(const name of ${JSON.stringify(guards.map(([, name]) => name))}){const selected=rows.filter(row=>row.name===name);assert.equal(selected.length,1,'metadata native receipt '+name);assert.deepEqual(selected[0].failures,[],'metadata map-only gate '+name);}`;
  const execute = () => {
    const run = spawnSync(process.execPath, ['--input-type=module', '--eval', program], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    const output = `${run.stdout}\n${run.stderr}`;
    assert.doesNotMatch(output, /error\[E\d+\]|could not compile|metadata probe preparation must complete/, 'compilation/setup failures are not metadata controls');
    return { ...run, output };
  };
  try {
    let run = execute(); assert.equal(run.status, 0, run.output);
    for (const [name, owner, file] of guards) {
      const source = path.join(sdk, 'safe-cex-types/src', file);
      const original = originals.get(source);
      const before = `${name}Wire::deserialize(crate::strict_decode::object(deserializer))`;
      assert.equal(original.split(before).length - 1, 1, 'metadata control selects one actual production guard');
      fs.writeFileSync(source, original.replace(before, `${name}Wire::deserialize(deserializer)`));
      run = execute(); assert.equal(run.status, 1, run.output);
      assert.ok(run.output.includes(`metadata map-only gate ${owner}`), run.output);
      assert.match(run.output, /Deserialize accepted \[/);
      console.log(`metadata ${name} positional-array widening: named assertion; process exit 1`);
      fs.writeFileSync(source, original);
      run = execute(); assert.equal(run.status, 0, run.output);
    }
  } finally {
    for (const [file, original] of originals) fs.writeFileSync(file, original);
  }
});
