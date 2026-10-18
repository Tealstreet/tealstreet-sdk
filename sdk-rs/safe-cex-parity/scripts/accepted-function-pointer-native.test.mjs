import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { registerHooks } from 'node:module';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { aliasTargetIdentity } from './accepted-aliases.mjs';
import { verifiedMacroOrigins } from './accepted-macro-origins.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['safe_cex_venue_bitunix::api::RetryCondition', 'safe_cex_venue_blofin::exchange::BlofinApiFactory'];
const packages = ['safe-cex-venue-bitunix', 'safe-cex-venue-blofin'];
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', ['--color', 'never', ...args], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extraEnv } });
  assert.equal(result.status, 0, `pointer preparation failed; compiler failures are not controls\n${result.stderr}`);
  return result.stdout;
}
async function probe(index, mutate) {
  const name = names[index];
  const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
  const pkg = metadata.packages.find(pkg => pkg.name === packages[index]);
  assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual SDK workspace owner');
  const target = pkg.targets.find(target => target.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs=1', '--', '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', target.name.replaceAll('-', '_') + '.json')));
  const artifacts = cargo(['check', '--locked', '-p', pkg.name, '--lib', '--jobs=1', '--message-format=json']).trim().split('\n').map(line => JSON.parse(line)).filter(row => row.reason === 'compiler-artifact');
  const sysrootResult = spawnSync('rustc', ['--print', 'sysroot'], { cwd: root, encoding: 'utf8' });
  assert.equal(sysrootResult.status, 0);
  const sysroot = sysrootResult.stdout.trim();
  // Discover all compiler declarations first. This later exact-alias selection
  // is a focused policy control, never a whole-package zero-missing claim.
  const found = inventory(doc, {
    defaultTraits: loadPolicy('default-traits'),
    receiverTraits: loadPolicy('receiver-traits'),
    readSource: file => fs.readFileSync(path.resolve(root, file)),
  });
  const aliases = found.publicAliases.filter(alias => alias.name === name);
  assert.equal(aliases.length, 1, 'exactly one actual production function-pointer alias');
  const alias = aliases[0];
  const members = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id));
  const externalTypeAliases = found.externalTypeAliases.filter(row => row.name === name).map(row => ({ ...row, artifact: aliasTargetIdentity(row, artifacts, members, sysroot) }));
  const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
  const contract = { category: 'non-decoder', evidence: 'Focused actual production alias invariant-equality and native trait control.', reason: 'Typed callback capability; no callback execution claim.', generics: alias.generics, rawTarget: alias.rawTarget, target: alias.target };
  const state = { count: 0, mutate };
  globalThis.__functionPointerNative = state;
  const url = new URL('./accepted-probes.mjs', import.meta.url).href;
  const hook = registerHooks({ load(moduleUrl, context, next) {
    const result = next(moduleUrl, context);
    if (moduleUrl !== url) return result;
    const source = String(result.source);
    const anchor = '  const alternate = `|value|';
    assert.equal(source.split(anchor).length, 2, 'one actual post-validation alias emitter');
    const injection = `  if (type.name === ${JSON.stringify(name)}) { const state = globalThis.__functionPointerNative; assert.equal(contract.aliasProbeKind, 'reviewed-function-pointer'); assert.equal(targetType, ${JSON.stringify('super::' + alias.ident)}); state.count++; ${mutate ? 'canonical = literal("wrong-native-function-pointer");' : ''} }\n`;
    return { ...result, source: source.replace('  const canonical = unsized', '  let canonical = unsized').replace(anchor, injection + anchor) };
  } });
  try {
    const { runProbes } = await import(pathToFileURL(path.join(root, 'safe-cex-parity/scripts/accepted-probes.mjs')));
    const rows = runProbes(root, metadata, [{ package: pkg.name, target, features: [], types: [], publicAliases: aliases, externalTypeAliases, macroOrigins, locals: [] }], { [name]: contract }, { target: 'native', features: [], jobs: 1 });
    console.log('POINTER_NATIVE ' + JSON.stringify({ count: state.count, rows }));
    assert.equal(rows.length, 1);
    assert.equal(rows[0].name, name);
    assert.deepEqual(rows[0].failures, [], 'actual function-pointer native identity and trait witness');
    assert.equal(rows[0].classificationOnly, true);
    assert.equal(rows[0].category, 'non-decoder-alias');
    assert.equal(rows[0].deserialize, false);
  } finally { hook.deregister(); delete globalThis.__functionPointerNative; }
}
function child(index, mutate) {
  // rustdoc requires this pinned-toolchain setting. Keep it consistent through
  // check/probes too: proc-macro2 rebuilds its dependency chain when it changes.
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--pointer-native', String(index), mutate ? 'mutant' : 'baseline'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, RUSTC_BOOTSTRAP: '1' } });
  assert.ok(!result.error, String(result.error));
  const output = result.stdout + '\n' + result.stderr;
  assert.doesNotMatch(output, /compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  const receipts = output.split('\n').filter(line => line.startsWith('POINTER_NATIVE ')).map(line => JSON.parse(line.slice('POINTER_NATIVE '.length)));
  assert.equal(receipts.length, 1, output);
  assert.equal(receipts[0].count, 1, 'one actual alias post-validation emitter');
  assert.deepEqual(receipts[0].rows.map(row => row.name), [names[index]]);
  return { ...result, output, rows: receipts[0].rows };
}
if (process.argv[2] === '--pointer-native') await probe(Number(process.argv[3]), process.argv[4] === 'mutant');
else test('reviewed function pointers execute actual SDK alias identity and trait probes', { timeout: 1800000 }, () => {
  for (let index = 0; index < names.length; index++) {
    let result = child(index, false); assert.equal(result.status, 0, result.output);
    result = child(index, true); assert.equal(result.status, 1, result.output);
    assert.match(result.output, /actual function-pointer native identity and trait witness/);
    assert.deepEqual(result.rows[0].failures, ['non-decoder alias compiler root identity mismatch']);
    console.log(`function pointer ${index === 0 ? 'retry' : 'factory'} native identity: named assertion; process exit 1`);
    result = child(index, false); assert.equal(result.status, 0, result.output);
  }
});
