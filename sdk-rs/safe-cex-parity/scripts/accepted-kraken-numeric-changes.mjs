import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const name = 'safe_cex_venue_kraken::stream_updates::NumericChanges';
const expectedType = "::std::vec::Vec<(&'static str, f64)>";
const target = { resolved_path: { path: 'alloc::vec::Vec', args: { angle_bracketed: {
  args: [{ type: { tuple: [{ borrowed_ref: { lifetime: "'static", is_mutable: false, type: { primitive: 'str' } } }, { primitive: 'f64' }] } }], constraints: [],
} } } };
const generics = { params: [], where_predicates: [] };
const raw = ['[["bid",-0.0],["ask",19.25]]', '[["last",17.5],["last",-3.125],["fundingRate",0.003125]]', '[]',
  '[["b\\u0069d",12.5]]', 'null', '{}', 'true', '0', '"x"', '["bid",1]', '[["bid"]]',
  '[["bid",1,2]]', '[[7,1]]', '[["bid","1"]]', '[["bid",null]]', '[["bid",1],null]',
  '[["",5e-324],["💱",-2.75]]'];
const members = [[['bid', -0], ['ask', 19.25]], [['last', 17.5], ['last', -3.125], ['fundingRate', 0.003125]], [], [['bid', 12.5]]];
members[16] = [['', 5e-324], ['💱', -2.75]];
const offsets = { 0: [3, 16], 1: [3, 17, 33], 2: [], 16: [3, 15] };
const bits = number => { const bytes = Buffer.alloc(8); bytes.writeDoubleBE(number); return bytes.toString('hex'); };

export function krakenNumericChangesWitnesses() {
  return ['static-text', 'qualified-static-text', 'static-borrowed-value', 'owned-value'].flatMap(route => raw.map((text, fixture) => {
    const textRoute = route.endsWith('static-text');
    const accepted = route === 'owned-value' ? fixture === 2 : [0, 1, 2, 16].includes(fixture) || (!textRoute && fixture === 3);
    const output = { accepted, identity: true, alternate_traits: [] };
    if (accepted) {
      output.members = members[fixture].map(([key, value]) => [key, bits(value)]);
      if (textRoute) output.key_offsets = offsets[fixture].slice();
      if (route === 'static-borrowed-value') output.borrowed_keys = true;
    }
    return { input: { route, fixture, raw: text }, output };
  }));
}

// Permission is fixed to this actual production alias and these routes. It does
// not relax DetectAlias's general borrowed-Deserialize refusal or owned bound.
export function krakenNumericChangesContract(alias, contract, context) {
  if (alias.name !== name || contract.shape !== 'borrowed-sequence') return undefined;
  const label = `${name}: accepted-set assertion: reviewed borrowed NumericChanges`;
  assert.equal(alias.ident, 'NumericChanges', `${label}: identifier changed`);
  assert.deepEqual(alias.span, { filename: 'safe-cex-venue-kraken/src/stream_updates.rs', begin: [11, 1], end: [11, 52] }, `${label}: compiler declaration changed`);
  assert.deepEqual(alias.generics, generics, `${label}: compiler generics changed`);
  assert.deepEqual(alias.rawTarget, target, `${label}: compiler source target changed`);
  assert.deepEqual(alias.target, target, `${label}: compiler expanded target changed`);
  assert.deepEqual(Object.keys(contract).sort(), ['borrowedRoutes', 'category', 'entrypoints', 'evidence', 'generics', 'rawTarget', 'shape', 'target'], `${label}: route contract fields changed`);
  assert.deepEqual(contract.generics, generics, `${label}: contract generics changed`);
  assert.deepEqual(contract.rawTarget, target, `${label}: contract source target changed`);
  assert.deepEqual(contract.target, target, `${label}: contract expanded target changed`);
  assert.equal(contract.category, 'decoder', `${label}: actual Deserialize remains a decoder`);
  assert.deepEqual(contract.entrypoints, [], `${label}: alternate traits require review`);
  assert.deepEqual(contract.borrowedRoutes, krakenNumericChangesWitnesses(), `${label}: complete exact route witnesses required`);
  assert.equal(context.packageMetadata?.name, 'safe-cex-venue-kraken', `${label}: actual compiler package required`);
  assert.equal(context.packageMetadata?.manifest_path, fileURLToPath(new URL('../../safe-cex-venue-kraken/Cargo.toml', import.meta.url)), `${label}: actual compiler package source required`);
  assert.ok(context.macroOrigins?.includes('serde_json'), `${label}: verified serde_json origin required`);
  const roots = context.externalTypeAliases.filter(row => row.name === name && row.canonical === 'alloc::vec::Vec');
  assert.ok(roots.length === 1 && roots[0].kind === 'struct' && roots[0].artifact?.kind === 'sysroot', `${label}: authenticated compiler Vec root required`);
  return { ...contract, canonical: 'alloc::vec::Vec', aliasProbeSuffix: '', aliasProbeExpected: expectedType, aliasProbeKind: 'reviewed-kraken-numeric-changes' };
}

export function krakenNumericChangesBody(type, targetType, contract, alternate) {
  assert.equal(type.name, name);
  assert.equal(contract.aliasProbeKind, 'reviewed-kraken-numeric-changes');
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
        "qualified-static-text" => <${targetType} as serde::Deserialize>::deserialize(&mut serde_json::Deserializer::from_str(raw)),
        "static-borrowed-value" => <${targetType} as serde::Deserialize>::deserialize(&retained[index]),
        "owned-value" => <${targetType} as serde::Deserialize>::deserialize(retained[index].clone()),
        _ => unreachable!("unreviewed borrowed route"),
      };
      let traits: Vec<_> = alternate(&retained[index]).into_iter().map(|(name, _)| name).collect();
      let mut output = serde_json::json!({"accepted":decoded.is_ok(),"identity":std::any::type_name::<${targetType}>().split('<').next() == Some(${literal(contract.canonical)}),"alternate_traits":traits});
      if let Ok(decoded) = decoded {
        output["members"] = serde_json::Value::Array(decoded.iter().map(|(key, value)| serde_json::json!([*key, format!("{:016x}", value.to_bits())])).collect());
        if route.ends_with("static-text") {
          output["key_offsets"] = serde_json::json!(decoded.iter().map(|(key, _)| key.as_ptr() as usize - raw.as_ptr() as usize).collect::<Vec<_>>());
        }
        if route == "static-borrowed-value" {
          let rows = retained[index].as_array().unwrap();
          output["borrowed_keys"] = serde_json::json!(decoded.iter().zip(rows).all(|((key, _), row)| row[0].as_str().unwrap().as_ptr() == key.as_ptr()));
        }
      }
      output
    });`;
}
