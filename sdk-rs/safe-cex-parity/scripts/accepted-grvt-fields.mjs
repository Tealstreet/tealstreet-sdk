import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const name = 'safe_cex_venue_grvt::composition::Fields';
const expectedType = "::std::rc::Rc<::std::cell::RefCell<::indexmap::IndexMap<&'static str, bool>>>";
const applied = (path, ...types) => ({ resolved_path: { path, args: { angle_bracketed: { args: types.map(type => ({ type })), constraints: [] } } } });
const target = applied('alloc::rc::Rc', applied('core::cell::RefCell', applied('indexmap::map::IndexMap',
  { borrowed_ref: { lifetime: "'static", is_mutable: false, type: { primitive: 'str' } } }, { generic: 'V' })));
const generics = { params: [{ name: 'V', kind: { type: { bounds: [], default: null, is_synthetic: false } } }], where_predicates: [] };

// Fixed text lives in the compiled program. Borrowed Value inputs are separately
// parsed and retained by OnceLock; neither path leaks or changes production keys.
const raw = ['{"z":false,"a":true}', '{"second":true,"first":false}', '{}', '{"\\u007a":false,"a":true}',
  'null', '[]', 'true', '0', '"x"', '{"z":0}', '{"z":null}', '{"z":"false"}'];
const members = [[['z', false], ['a', true]], [['second', true], ['first', false]], [], [['z', false], ['a', true]]];
const offsets = [[2, 12], [2, 16], []];

export function grvtFieldsWitnesses() {
  return ['static-text', 'static-borrowed-value', 'owned-value'].flatMap(route => raw.map((text, fixture) => {
    const accepted = route === 'static-text' ? fixture < 3 : route === 'static-borrowed-value' ? fixture < 4 : fixture === 2;
    const output = { accepted, identity: true, alternate_traits: [] };
    if (accepted) {
      output.members = structuredClone(members[fixture]);
      if (route === 'static-text') output.key_offsets = offsets[fixture].slice();
      if (route === 'static-borrowed-value') { output.borrowed_keys = true; output.keys_outside_text = true; }
    }
    return { input: { route, fixture, raw: text }, output };
  }));
}

export function grvtFieldsContract(alias, contract, context, identity) {
  if (alias.name !== name || contract.shape !== 'borrowed-map') return undefined;
  const label = `${name}: accepted-set assertion: reviewed borrowed Fields`;
  assert.equal(alias.ident, 'Fields', `${label}: identifier changed`);
  assert.deepEqual(alias.span, { filename: 'safe-cex-venue-grvt/src/composition.rs', begin: [12, 1], end: [12, 61] }, `${label}: compiler declaration changed`);
  assert.deepEqual(alias.generics, generics, `${label}: compiler generics changed`);
  assert.deepEqual(alias.rawTarget, target, `${label}: compiler source target changed`);
  assert.deepEqual(alias.target, target, `${label}: compiler expanded target changed`);
  assert.equal(identity?.expectedType, expectedType, `${label}: fixed native identity changed`);
  assert.equal(identity?.suffix, '<bool>', `${label}: only bool is reviewed`);
  assert.equal(identity?.canonical, 'alloc::rc::Rc', `${label}: compiler root changed`);
  assert.deepEqual(Object.keys(contract).sort(), ['borrowedRoutes', 'category', 'entrypoints', 'evidence', 'generics', 'rawTarget', 'shape', 'target'], `${label}: route contract fields changed`);
  assert.deepEqual(contract.generics, alias.generics, `${label}: contract generics changed`);
  assert.deepEqual(contract.rawTarget, alias.rawTarget, `${label}: contract source target changed`);
  assert.equal(contract.category, 'decoder', `${label}: actual Deserialize remains a decoder`);
  assert.deepEqual(contract.entrypoints ?? [], [], `${label}: alternate traits need independent review`);
  assert.equal(contract.observation, undefined, `${label}: route adapters are policy owned`);
  assert.equal(contract.nativeProjection, undefined, `${label}: route projections are policy owned`);
  assert.deepEqual(contract.borrowedRoutes, grvtFieldsWitnesses(), `${label}: all exact route witnesses required`);
  assert.equal(context.packageMetadata?.name, 'safe-cex-venue-grvt', `${label}: actual compiler package required`);
  assert.equal(context.packageMetadata?.manifest_path, fileURLToPath(new URL('../../safe-cex-venue-grvt/Cargo.toml', import.meta.url)), `${label}: actual compiler package source required`);
  for (const [canonical, kind] of [['alloc::rc::Rc', 'sysroot'], ['core::cell::RefCell', 'sysroot'], ['indexmap::map::IndexMap', 'registry']]) {
    const roots = context.externalTypeAliases.filter(row => row.name === name && row.canonical === canonical);
    assert.ok(roots.length === 1 && roots[0].kind === 'struct' && roots[0].artifact?.kind === kind, `${label}: authenticated ${canonical} root required`);
    if (kind === 'registry') assert.ok(context.metadata?.packages.some(pkg => pkg.id === roots[0].artifact.packageId && pkg.name === 'indexmap'), `${label}: actual indexmap package required`);
  }
  return { ...contract, canonical: identity.canonical, aliasProbeSuffix: identity.suffix, aliasProbeExpected: expectedType,
    aliasProbeKind: 'reviewed-grvt-fields', monomorphization: identity.monomorphization };
}

// Called only after exact compiler identity and the complete route matrix pass.
// The general DetectAlias borrowed guard and DeserializeOwned bound stay intact.
export function grvtFieldsBody(type, targetType, contract, alternate) {
  assert.equal(type.name, name);
  assert.equal(contract.aliasProbeKind, 'reviewed-grvt-fields');
  const literal = JSON.stringify;
  return `
    const RAW: &[&str] = &${literal(raw)};
    static VALUES: std::sync::OnceLock<Vec<serde_json::Value>> = std::sync::OnceLock::new();
    let retained: &'static Vec<serde_json::Value> = VALUES.get_or_init(|| RAW.iter().map(|raw| serde_json::from_str(raw).unwrap()).collect());
    let alternate = ${alternate};
    crate::__accepted_set_runtime::audit_function(${literal(name)}, ${literal(JSON.stringify({ category: 'decoder', cases: contract.borrowedRoutes }))}, |input| {
      let index = input["fixture"].as_u64().unwrap() as usize;
      let raw: &'static str = RAW[index];
      let route = input["route"].as_str().unwrap();
      let decoded: Result<${targetType}, _> = match route {
        "static-text" => serde_json::from_str::<${targetType}>(raw),
        "static-borrowed-value" => <${targetType} as serde::Deserialize>::deserialize(&retained[index]),
        "owned-value" => <${targetType} as serde::Deserialize>::deserialize(retained[index].clone()),
        _ => unreachable!("unreviewed borrowed route"),
      };
      let traits: Vec<_> = alternate(&retained[index]).into_iter().map(|(name, _)| name).collect();
      let mut output = serde_json::json!({"accepted":decoded.is_ok(),"identity":std::any::type_name::<${targetType}>().split('<').next() == Some(${literal(contract.canonical)}),"alternate_traits":traits});
      if let Ok(decoded) = decoded {
        let fields = decoded.borrow();
        output["members"] = serde_json::Value::Array(fields.iter().map(|(key, value)| serde_json::json!([*key, *value])).collect());
        if route == "static-text" {
          output["key_offsets"] = serde_json::json!(fields.keys().map(|key| key.as_ptr() as usize - raw.as_ptr() as usize).collect::<Vec<_>>());
        }
        if route == "static-borrowed-value" {
          let map = retained[index].as_object().unwrap();
          output["borrowed_keys"] = serde_json::json!(fields.keys().all(|key| map.get_key_value(*key).is_some_and(|(original, _)| original.as_ptr() == key.as_ptr())));
          output["keys_outside_text"] = serde_json::json!(fields.keys().all(|key| !(raw.as_ptr() as usize..raw.as_ptr() as usize + raw.len()).contains(&(key.as_ptr() as usize))));
        }
      }
      output
    });`;
}
