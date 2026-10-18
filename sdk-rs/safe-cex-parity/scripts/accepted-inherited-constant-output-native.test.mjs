import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { registerHooks } from 'node:module';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const ownerName = 'safe_cex_venue_grvt::binding::exchange::GrvtExchange';
const specs = Object.values(loadPolicy('inherited-constant-outputs'));
const fixtureUrl = new URL('../fixtures/accepted-inherited-methods.json', import.meta.url);
function cargo(args) {
  const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1' } });
  assert.equal(result.status, 0, `inherited refusal preparation failed; compiler failures are not controls\n${result.stderr}`);
  return result.stdout;
}
async function probe(mode) {
  const metadata = JSON.parse(cargo(['metadata', '--no-deps', '--locked', '--format-version=1']));
  const pkg = metadata.packages.find(pkg => pkg.name === 'safe-cex-venue-grvt');
  assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
  const target = pkg.targets.find(target => target.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs=1', '--', '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all']);
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_grvt.json')));
  const found = inventory(doc, { onlyOwners: [ownerName] });
  const owner = found.types[0];
  const contracts = JSON.parse(fs.readFileSync(fixtureUrl));
  const entries = contracts[ownerName].entrypoints;
  assert.equal(owner.methods.length + owner.conversions.length + owner.traitMethods.length, 11, 'all eleven actual compiler APIs remain required');
  assert.equal(entries.length, 11);
  assert.equal(entries.filter(entry => entry.constantOutput).length, 4);
  for (const entry of entries.filter(entry => entry.constantOutput)) {
    const spec = specs.find(spec => spec.owner === ownerName && spec.api.name === entry.api.name);
    assert.ok(spec); assert.deepEqual(entry.api, spec.api);
    entry.call = spec.call; entry.cases = spec.cases;
  }
  const control = { count: 0, mode };
  globalThis.__inheritedRefusalControl = control;
  if (mode !== 'restored') {
    assert.ok(['error', 'http'].includes(mode));
    control.needle = mode === 'error' ? '"error_name":error.name' : '"http":h.http.count()';
    control.replacement = mode === 'error' ? '"error_name":"MUTATED_NATIVE_ERROR"' : '"http":h.http.count()+1';
    const moduleUrl = new URL('./accepted-probes.mjs', import.meta.url).href;
    registerHooks({ load(url, context, next) {
      const loaded = next(url, context);
      if (url !== moduleUrl) return loaded;
      const source = String(loaded.source);
      const anchor = '      const body = `${supplemental} ${nativeBody}`;';
      assert.equal(source.split(anchor).length, 2, 'one post-validation native emitter');
      const injection = '      let body = `${supplemental} ${nativeBody}`; if (type.name === ' + JSON.stringify(ownerName) + ') { const control = globalThis.__inheritedRefusalControl; const count = body.split(control.needle).length - 1; assert.equal(count, 4, "all four actual refusal adapters mutated after validation"); body = body.replaceAll(control.needle, control.replacement); control.count += count; }';
      return { ...loaded, source: source.replace(anchor, injection) };
    } });
  }
  const { runProbes } = await import(pathToFileURL(path.join(root, 'safe-cex-parity/scripts/accepted-probes.mjs')));
  const rows = runProbes(root, metadata, [{ package: pkg.name, target, types: found.types, locals: [] }], contracts, { target: 'native', features: [], jobs: 1 });
  console.log('INHERITED_EMITTER ' + JSON.stringify(control));
  console.log('INHERITED_NATIVE ' + JSON.stringify(rows));
  assert.deepEqual(rows.map(row => row.name), [ownerName]);
  assert.deepEqual(rows[0].failures, [], 'reviewed inherited refusal native error and zero HTTP witnesses');
}
function child(mode) {
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--inherited-native', mode], {
    encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, RUSTC_BOOTSTRAP: '1' },
  });
  assert.ok(!result.error, String(result.error));
  const output = `${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output, /compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  const marker = prefix => output.split('\n').filter(line => line.startsWith(prefix)).map(line => JSON.parse(line.slice(prefix.length)));
  const emitters = marker('INHERITED_EMITTER '), native = marker('INHERITED_NATIVE ');
  assert.equal(emitters.length, 1, output); assert.equal(native.length, 1, output);
  assert.equal(emitters[0].count, mode === 'restored' ? 0 : 4, output);
  assert.deepEqual(native[0].map(row => row.name), [ownerName]);
  return { ...result, output, rows: native[0] };
}
if (process.argv[2] === '--inherited-native') await probe(process.argv[3]);
else test('reviewed inherited refusals execute the complete production owner and reject native error or HTTP drift', { timeout: 1200000 }, () => {
  let result = child('restored'); assert.equal(result.status, 0, result.output);
  for (const [mode, label] of [['error', 'inherited refusal native error identity'], ['http', 'inherited refusal native zero HTTP']]) {
    result = child(mode); assert.equal(result.status, 1, result.output);
    assert.match(result.output, /reviewed inherited refusal native error and zero HTTP witnesses/);
    assert.equal(result.rows[0].failures.length, 8, result.output);
    assert.ok(result.rows[0].failures.every(failure => failure.startsWith('function output mismatch')), result.output);
    console.log(`${label}: named assertion; process exit 1`);
  }
  result = child('restored'); assert.equal(result.status, 0, result.output);
});
