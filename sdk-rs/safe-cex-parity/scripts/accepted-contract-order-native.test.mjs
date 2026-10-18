import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { loadContracts } from './accepted-contract-files.mjs';
import { runProbes } from './accepted-probes.mjs';

const fixture = fileURLToPath(new URL('../fixtures/accepted-set/', import.meta.url));
const name = 'accepted_set_control::private::Record';
function cargo(args, extra = {}) {
  const result = spawnSync(process.env.CARGO ?? 'cargo', args, {
    cwd: fixture, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...extra },
  });
  assert.equal(result.status, 0, `native preparation must compile: ${result.stderr}`);
  return result.stdout;
}

test('authored integer-key order reaches the maintained native decoder probe', { timeout: 300000 }, () => {
  const metadata = JSON.parse(cargo(['metadata', '--locked', '--no-deps', '--format-version=1']));
  const pkg = metadata.packages.find(row => row.name === 'accepted-set-control');
  const target = pkg.targets.find(row => row.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs', '1', '--',
    '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
    '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
  const found = inventory(doc, { onlyOwners: [name] });
  assert.deepEqual(found.types.map(row => row.name), [name]);
  const contractPath = path.join(metadata.target_directory, 'authored-key-contract.json');
  fs.writeFileSync(contractPath, String.raw`{"${name}":{
    "shape":"map","sample":{"value":true},"evidence":"Observe admission order and the real native Record decoder together.",
    "entrypoints":[],"observation":{
      "call":"match <$type as serde::Deserialize>::deserialize($value.clone()) { Ok(decoded) => serde_json::json!({\"keys\":$value.as_object().unwrap().keys().collect::<Vec<_>>(),\"value\":decoded.value}), Err(_) => serde_json::Value::Null }",
      "cases":[
        {"input":{"9":"nine","2":"two","x":"last","value":true},"output":{"keys":["9","2","x","value"],"value":true}},
        {"input":{"10":"ten","1":"one","value":false},"output":{"keys":["10","1","value"],"value":false}}
      ]
    }
  }}`);
  try {
    const results = runProbes(fixture, metadata, [{
      ...found, package: pkg.name, packageId: pkg.id, target, features: [],
      publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
    }], loadContracts(contractPath), { target: 'native', features: [], jobs: 1 });
    assert.equal(results.length, 1);
    assert.deepEqual(results[0].failures, [], 'authored key order must reach the actual Rust decoder host');
  } finally {
    fs.rmSync(contractPath, { force: true });
  }
});
