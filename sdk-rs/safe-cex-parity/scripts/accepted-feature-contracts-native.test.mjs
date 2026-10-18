import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import test from 'node:test';
import {loadContracts, contractJson} from './accepted-contract-files.mjs';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const sdk = path.resolve(scripts, '../..');
const fixture = path.resolve(scripts, '../fixtures/accepted-set');
const driver = path.join(scripts, 'accepted-set.mjs');
const sourcePath = path.join(fixture, 'src/lib.rs');
const name = 'accepted_set_control::private::Record';

test('required features visibly gate the default owner and execute the real native decoder with controls', {timeout: 1200000}, () => {
  const target = path.join(sdk, 'target/accepted-feature-contracts');
  fs.mkdirSync(target, {recursive: true});
  const contractPath = path.join(target, 'feature-contract.json');
  const contracts = loadContracts(path.join(fixture, 'contracts.json'));
  contracts[name].requiredFeatures = ['native'];
  contracts[name].nativeObservation = {
    nativeFields: ['value', 'nativeEnabled'],
    call: '{ use serde::Deserialize; let decoded=$type::deserialize($value.clone()).unwrap(); serde_json::json!({"ok":true,"value":decoded.value,"nativeEnabled":crate::native_contract_enabled()}) }',
    cases: [true, false].map(value => ({input: {value}, output: {ok: true, value, nativeEnabled: true}})),
  };
  fs.writeFileSync(contractPath, contractJson(contracts));
  const source = fs.readFileSync(sourcePath, 'utf8');
  const driverSource = fs.readFileSync(driver, 'utf8');
  const run = () => {
    const result = spawnSync(process.execPath, [driver, `--workspace=${fixture}`,
      `--contracts=${contractPath}`, '--package=accepted-set-control', '--jobs=1'], {
      cwd: sdk, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: {...process.env, CARGO_BUILD_JOBS: '1', CARGO_TARGET_DIR: target},
    });
    const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
    assert.doesNotMatch(output, /error\[E\d+\]|could not compile|compile failure is not a control/, output);
    return {...result, output};
  };
  const positive = () => {
    const result = run();
    assert.equal(result.status, 0, result.output);
    const rows = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-results.json')));
    const own = rows.filter(row => row.name === name);
    assert.equal(own.length, 2, 'default gate and actual native measurement must both remain visible');
    assert.equal(own.filter(row => row.status === 'native-gated' && row.observed === false).length, 1);
    assert.equal(own.filter(row => !row.classificationOnly && row.configuration.features.includes('native')).length, 1);
    assert.ok(rows.every(row => row.failures.length === 0), result.output);
    assert.match(result.output, /native-gated; required features native/);
    const ordinary = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-inventory.json')));
    assert.deepEqual(ordinary.find(row => row.package === 'accepted-set-control').features, [], 'native child must not replace the default compiler inventory');
  };
  try {
    positive();
    const needle = 'cfg!(feature = "native")';
    assert.equal(source.split(needle).length, 2);
    fs.writeFileSync(sourcePath, source.replace(needle, 'false'));
    const wrongNative = run();
    assert.equal(wrongNative.status, 1, wrongNative.output);
    assert.match(wrongNative.output, /Record|nativeEnabled/);
    assert.match(wrongNative.output, /failed types/);
    console.log('required feature actual native output: named assertion; process exit 1');
    fs.writeFileSync(sourcePath, source);
    positive();
    const dispatch = 'for (const group of plan.groups) {';
    assert.equal(driverSource.split(dispatch).length, 2);
    fs.writeFileSync(driver, driverSource.replace(dispatch, 'for (const group of []) {'));
    const omitted = run();
    assert.equal(omitted.status, 1, omitted.output);
    assert.match(omitted.output, /required native feature observation did not execute/);
    console.log('required feature native dispatch removal: named assertion; process exit 1');
    fs.writeFileSync(driver, driverSource);
    positive();
  } finally {
    fs.writeFileSync(sourcePath, source);
    fs.writeFileSync(driver, driverSource);
    fs.rmSync(contractPath, {force: true});
  }
});

test('native feature compiler provenance follows real reexports without passing foreign feature names to dependencies', {timeout: 300000}, () => {
  const target = path.join(sdk, 'target/accepted-feature-contracts');
  const contractPath = path.join(target, 'reexport-feature-contract.json');
  fs.mkdirSync(target, {recursive: true});
  const contracts = loadContracts(path.join(fixture, 'contracts.json'));
  contracts[name].requiredFeatures = ['native'];
  contracts[name].nativeObservation = {
    nativeFields: ['value', 'nativeEnabled'],
    call: '{ use serde::Deserialize; let decoded=$type::deserialize($value.clone()).unwrap(); serde_json::json!({"ok":true,"value":decoded.value,"nativeEnabled":crate::native_contract_enabled()}) }',
    cases: [true, false].map(value => ({input: {value}, output: {ok: true, value, nativeEnabled: true}})),
  };
  fs.writeFileSync(contractPath, contractJson(contracts));
  try {
    const result = spawnSync(process.execPath, [driver, `--workspace=${fixture}`,
      `--contracts=${contractPath}`, '--package=accepted-set-reexport-control', '--features=native', '--jobs=1'], {
      cwd: sdk, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: {...process.env, CARGO_BUILD_JOBS: '1', CARGO_TARGET_DIR: target},
    });
    const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
    assert.equal(result.status, 0, output);
    const inventory = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-inventory.json')));
    assert.deepEqual(inventory.find(pkg => pkg.package === 'accepted-set-control').features, ['native']);
    assert.deepEqual(inventory.find(pkg => pkg.package === 'vendor-serde-json').features, []);
    const results = JSON.parse(fs.readFileSync(path.join(target, 'accepted-set-results.json')));
    assert.equal(results.filter(row => row.name === name && !row.classificationOnly && row.configuration.features.includes('native')).length, 1);
    assert.ok(results.every(row => row.status !== 'native-gated' && row.failures.length === 0));
  } finally { fs.rmSync(contractPath, {force: true}); }
});
