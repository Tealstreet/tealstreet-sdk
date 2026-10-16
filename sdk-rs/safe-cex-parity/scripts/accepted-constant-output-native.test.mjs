import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { registerHooks } from 'node:module';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const specs = Object.values(JSON.parse(fs.readFileSync(new URL('../accepted-set/constant-outputs.json', import.meta.url), 'utf8')));
const ownerName = 'safe_cex_num::numeric_ingress::output::Capture';
// This is a focused control contract, never a production classification. Every
// compiler entry remains present. Ordinary typed arguments keep their normal
// compiler guards; raw serializer entries execute independently authored values.
const typed = ['serialize_i8','serialize_i16','serialize_i32','serialize_i64','serialize_i128','serialize_u16','serialize_u32','serialize_u64','serialize_u128','serialize_f32','serialize_f64','serialize_bool','serialize_char','serialize_none','serialize_some','serialize_unit'];
const dataWitnesses = {
  "serialize_u8": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_u8($value.as_u64().unwrap() as u8).expect(\"reviewed serializer input\"); match native.0 { super::Node::Float(v) => serde_json::json!({\"bits\":format!(\"{:016x}\",v.to_bits())}), _ => panic!(\"native numeric node required\") } }",
    "cases": [
      {
        "input": 0,
        "output": {
          "bits": "0000000000000000"
        }
      },
      {
        "input": 255,
        "output": {
          "bits": "406fe00000000000"
        }
      }
    ]
  },
  "serialize_str": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_str($value.as_str().unwrap()).expect(\"reviewed serializer input\"); match native.0 { super::Node::Text(v) => serde_json::Value::String(v), _ => panic!(\"native text node required\") } }",
    "cases": [
      {
        "input": "left",
        "output": "left"
      },
      {
        "input": "right",
        "output": "right"
      }
    ]
  },
  "serialize_bytes": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_bytes(&$value.as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect::<Vec<_>>()).expect(\"reviewed serializer input\"); match native.0 { super::Node::Array(values) => serde_json::json!(values.into_iter().map(|n|match n.0 {super::Node::Float(v)=>format!(\"{:016x}\",v.to_bits()),_=>panic!(\"native numeric byte\")}).collect::<Vec<_>>()), _ => panic!(\"native array node required\") } }",
    "cases": [
      {
        "input": [],
        "output": []
      },
      {
        "input": [
          1,
          2
        ],
        "output": [
          "3ff0000000000000",
          "4000000000000000"
        ]
      }
    ]
  },
  "serialize_unit_variant": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_unit_variant(\"Known\", 5, if $value.as_bool().unwrap() {\"Second\"} else {\"First\"}).expect(\"reviewed serializer input\"); match native.0 { super::Node::Text(v) => serde_json::Value::String(v), _ => panic!(\"native text node required\") } }",
    "cases": [
      {
        "input": false,
        "output": "First"
      },
      {
        "input": true,
        "output": "Second"
      }
    ]
  },
  "serialize_newtype_struct": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_newtype_struct(\"Known\", &$value.as_bool().unwrap()).expect(\"reviewed serializer input\"); match native.0 {super::Node::Bool(v)=>serde_json::Value::Bool(v),_=>panic!(\"native bool node required\")} }",
    "cases": [
      {
        "input": false,
        "output": false
      },
      {
        "input": true,
        "output": true
      }
    ]
  },
  "serialize_newtype_variant": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_newtype_variant(\"Known\", 5, \"Tagged\", &$value.as_bool().unwrap()).expect(\"reviewed serializer input\"); match native.0 {super::Node::Object(mut rows)=> {let (key,n)=rows.remove(0); match n.0 {super::Node::Bool(v)=>serde_json::json!({\"key\":key,\"value\":v}),_=>panic!(\"native bool node required\")}},_=>panic!(\"native object node required\")} }",
    "cases": [
      {
        "input": false,
        "output": {
          "key": "Tagged",
          "value": false
        }
      },
      {
        "input": true,
        "output": {
          "key": "Tagged",
          "value": true
        }
      }
    ]
  },
  "serialize_tuple_variant": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_tuple_variant(\"Known\", 5, if $value.as_bool().unwrap() {\"Second\"} else {\"First\"}, 3).expect(\"reviewed serializer input\"); serde_json::json!({\"values\":native.values.len(),\"variant\":native.variant}) }",
    "cases": [
      {
        "input": false,
        "output": {
          "values": 0,
          "variant": "First"
        }
      },
      {
        "input": true,
        "output": {
          "values": 0,
          "variant": "Second"
        }
      }
    ]
  },
  "serialize_struct": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_struct(if $value.as_bool().unwrap() {\"Second\"} else {\"First\"},3).expect(\"reviewed serializer input\"); serde_json::json!({\"values\":native.values.len(),\"key\":native.key,\"name\":native.name,\"variant\":native.variant}) }",
    "cases": [
      {
        "input": false,
        "output": {
          "values": 0,
          "key": null,
          "name": "First",
          "variant": null
        }
      },
      {
        "input": true,
        "output": {
          "values": 0,
          "key": null,
          "name": "Second",
          "variant": null
        }
      }
    ]
  },
  "serialize_struct_variant": {
    "call": "{ use serde::Serializer as _; let native = $type.serialize_struct_variant(\"Known\",5, if $value.as_bool().unwrap() {\"Second\"} else {\"First\"},3).expect(\"reviewed serializer input\"); serde_json::json!({\"values\":native.values.len(),\"key\":native.key,\"name\":native.name,\"variant\":native.variant}) }",
    "cases": [
      {
        "input": false,
        "output": {
          "values": 0,
          "key": null,
          "name": null,
          "variant": "First"
        }
      },
      {
        "input": true,
        "output": {
          "values": 0,
          "key": null,
          "name": null,
          "variant": "Second"
        }
      }
    ]
  }
};
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_BUILD_JOBS: '2', ...env } });
  assert.equal(result.status, 0, `constant serializer preparation failed; compiler failures are not controls\n${result.stderr}`);
  return result.stdout;
}
async function probe(mode) {
  const metadata = JSON.parse(cargo(['metadata','--no-deps','--locked','--format-version=1']));
  const pkg = metadata.packages.find(pkg => pkg.name === 'safe-cex-num');
  assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
  const target = pkg.targets.find(target => target.kind.includes('lib'));
  cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs=2','--','-Z','unstable-options','--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc/safe_cex_num.json')));
  const found = inventory(doc, { onlyOwners: [ownerName] });
  assert.deepEqual(found.types.map(type => type.name), [ownerName]);
  const owner = found.types[0];
  assert.deepEqual(owner.traitMethods.map(m => m.name).sort(), [...typed, ...Object.keys(dataWitnesses), ...specs.map(s => s.api.name)].sort(), 'all 27 actual compiler entrypoints remain required');
  const entrypoints = owner.traitMethods.map(method => {
    const api = { kind: 'trait', ...method };
    const fixed = specs.find(s => s.api.name === method.name);
    if (fixed) return { api, input: 'observed', constantOutput: true, reason: 'Reviewed metadata-only serializer constructor.', call: fixed.call, cases: fixed.cases };
    if (dataWitnesses[method.name]) return { api, input: 'observed', reason: 'Actual native serializer data or retained metadata varies.', ...dataWitnesses[method.name] };
    assert.ok(typed.includes(method.name), 'unknown typed API cannot enter the control');
    return { api, input: 'typed', reason: 'Focused control: existing typed Rust data without raw string/JSON/byte ingress. No decoder-runtime claim for this typed entry.' };
  });
  const contract = { shape: 'observed', evidence: 'Focused actual-production serializer constant-output control.', entrypoints };
  const hookState = { count: 0, mode };
  globalThis.__constantSerializerEmitter = hookState;
  if (mode !== 'restored') {
    const selected = specs.find(s => s.api.name === mode);
    assert.ok(selected);
    const needle = mode === 'serialize_unit_struct'
      ? selected.call.match(/match native\.0 \{ .* \}/)[0].slice(0, -2)
      : selected.call.match(/let native = .*?;/)[0];
    hookState.needle = needle;
    hookState.replacement = mode === 'serialize_unit_struct'
      ? needle.replace('serde_json::Value::Null', 'serde_json::Value::Bool(false)')
      : needle + ' let _discarded_native = native; let native = super::Sequence { values: Vec::new(), variant: Some("MUTATED_NATIVE".to_owned()) };';
    const targetUrl = new URL('./accepted-probes.mjs', import.meta.url).href;
    registerHooks({ load(url, context, next) {
      const loaded = next(url, context);
      if (url !== targetUrl) return loaded;
      const source = String(loaded.source);
      const anchor = '      const body = `${supplemental} ${nativeBody}`;';
      assert.equal(source.split(anchor).length, 2, 'one post-validation native emitter');
      const injected = '      let body = `${supplemental} ${nativeBody}`; const control = globalThis.__constantSerializerEmitter; if (type.name === ' + JSON.stringify(ownerName) + ') { const needle = control.needle.replaceAll("$type", targetType); assert.equal(body.split(needle).length, 2, "one actual serializer result mutation"); body = body.replace(needle, control.replacement.replaceAll("$type", targetType)); control.count++; }';
      return { ...loaded, source: source.replace(anchor, injected) };
    } });
  }
  const { runProbes } = await import(pathToFileURL(path.join(root,'safe-cex-parity/scripts/accepted-probes.mjs')));
  const rows = runProbes(root, metadata, [{ package: pkg.name, target, types: found.types, locals: [] }], { [ownerName]: contract });
  console.log('CONSTANT_EMITTER ' + JSON.stringify({ mode, count: hookState.count }));
  console.log('CONSTANT_NATIVE ' + JSON.stringify(rows));
  assert.equal(rows.length, 1);
  assert.equal(rows[0].name, ownerName);
  assert.deepEqual(rows[0].failures, [], 'reviewed constant serializer native value witness');
}
function child(mode) {
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url),'--constant-native',mode], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  assert.ok(!result.error, String(result.error));
  const output = `${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output, /compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  const native = output.split('\n').filter(line => line.startsWith('CONSTANT_NATIVE ')).map(line => JSON.parse(line.slice('CONSTANT_NATIVE '.length)));
  const emitter = output.split('\n').filter(line => line.startsWith('CONSTANT_EMITTER ')).map(line => JSON.parse(line.slice('CONSTANT_EMITTER '.length)));
  assert.equal(native.length, 1, output);
  assert.equal(emitter.length, 1, output);
  assert.equal(emitter[0].count, mode === 'restored' ? 0 : 1, 'actual post-validation emitter mutation count');
  assert.deepEqual(native[0].map(row => row.name), [ownerName]);
  return { ...result, output, rows: native[0] };
}
if (process.argv[2] === '--constant-native') await probe(process.argv[3]);
else test('reviewed serializer constants execute actual production values and reject native drift', { timeout: 1200000 }, () => {
  let result = child('restored'); assert.equal(result.status, 0, result.output);
  for (const [method, label] of [['serialize_unit_struct','constant serializer unit native value'], ['serialize_tuple_struct','constant serializer tuple native state']]) {
    result = child(method);
    assert.equal(result.status, 1, result.output);
    assert.match(result.output, /reviewed constant serializer native value witness/);
    assert.ok(result.rows[0].failures.length && result.rows[0].failures.every(failure => failure.startsWith('function output mismatch')), result.output);
    console.log(`${label}: named assertion; process exit 1`);
  }
  result = child('restored'); assert.equal(result.status, 0, result.output);
});
