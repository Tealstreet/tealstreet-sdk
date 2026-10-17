import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

export function nodeProbeConfiguration(pkg, target, configuration) {
  if (pkg.package !== 'safe-cex-ffi' || !configuration.features.includes('node')) return false;
  assert.equal(configuration.target, 'native', 'accepted-set: Node probes require a native target');
  assert.deepEqual(configuration.features, ['node'], 'accepted-set: Node probe feature profile changed');
  assert.equal(target.name, 'safe_cex_ffi', 'accepted-set: actual FFI target required');
  assert.deepEqual(target.crate_types.slice().sort(), ['cdylib', 'rlib'], 'accepted-set: Node probes require the production addon target');
  return true;
}

export function nodeProbeHarness() {
  return `
thread_local! { static __ACCEPTED_SET_ENV: std::cell::Cell<Option<napi::Env>> = const { std::cell::Cell::new(None) }; }
fn __accepted_set_node_env() -> napi::Env { __ACCEPTED_SET_ENV.with(|slot| slot.get().expect("actual Node probe environment")) }
fn __accepted_set_native_value(value: &safe_cex_registry::native_value::NativeValue) -> serde_json::Value {
    use safe_cex_registry::native_value::NativeValue;
    match value {
        NativeValue::Undefined => serde_json::json!(["undefined"]),
        NativeValue::Null => serde_json::json!(["null"]),
        NativeValue::Bool(value) => serde_json::json!(["boolean", value]),
        NativeValue::Number(value) => serde_json::json!(["number", format!("{:016x}", value.to_bits())]),
        NativeValue::String(value) => serde_json::json!(["string", value]),
        NativeValue::BigInt { negative, words } => serde_json::json!(["bigint", negative, words]),
        NativeValue::Array(values) => serde_json::json!(["array", values.iter().map(__accepted_set_native_value).collect::<Vec<_>>()]),
        NativeValue::Object(fields) => serde_json::json!(["object", fields.iter().map(|(key, value)| (key, __accepted_set_native_value(value))).collect::<Vec<_>>()]),
    }
}
#[napi_derive::napi]
pub fn accepted_set_native_run(env: napi::Env) {
    __ACCEPTED_SET_ENV.with(|slot| { assert!(slot.replace(Some(env)).is_none()); });
    __accepted_set_run();
    __ACCEPTED_SET_ENV.with(|slot| slot.set(None));
}
`;
}

export function executeNodeProbes(build, pkg, directory, manifest) {
  assert.equal(build.status, 0, `accepted-set: Node addon compilation failed\n${build.stdout}\n${build.stderr}`);
  assert.equal(pkg.package, 'safe-cex-ffi', 'accepted-set: selected FFI package required');
  assert.equal(typeof manifest, 'string', 'accepted-set: actual sandbox manifest required');
  const artifacts = build.stdout.trim().split('\n').filter(Boolean).map(line => JSON.parse(line))
    .filter(row => row.reason === 'compiler-artifact' && row.manifest_path === manifest && row.target.name === 'safe_cex_ffi' && row.target.crate_types.includes('cdylib'));
  assert.equal(artifacts.length, 1, 'accepted-set: one actual FFI Cargo artifact required');
  const libraries = artifacts[0].filenames.filter(file => /\.(dylib|so|dll)$/.test(file));
  assert.equal(libraries.length, 1, 'accepted-set: one actual Node dynamic library required');
  const addon = path.join(directory, 'accepted-set-ffi.node');
  fs.copyFileSync(libraries[0], addon);
  return spawnSync(process.execPath, ['--eval', `const addon=require(${JSON.stringify(addon)});if(typeof addon.acceptedSetNativeRun!=='function')throw Error('accepted-set: missing actual Node probe entry');addon.acceptedSetNativeRun();`],
    { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 30000 });
}
