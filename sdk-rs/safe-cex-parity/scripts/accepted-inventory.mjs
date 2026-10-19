import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { defaultTraitInventory } from './accepted-defaults.mjs';
import { hyperliquidProjectionCompilerIdentity } from './accepted-hyperliquid-projection.mjs';

// Rustdoc coordinates are one-based Unicode scalar columns, with an exclusive
// end. Resolve them against the current source on every inventory; coordinates
// themselves are deliberately not stored in policy, so preceding edits can move
// an unchanged item. Hash exact item text, including internal whitespace.
export function receiverItemSha256(span, source) {
  const label = 'accepted-set: reviewed receiver item span';
  assert.ok(typeof source === 'string' || Buffer.isBuffer(source), `${label}: source bytes required`);
  const text = typeof source === 'string' ? source : new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(source);
  const lines = text.split('\n');
  function offset(position) {
    assert.ok(Array.isArray(position) && position.length === 2 && position.every(value => Number.isSafeInteger(value) && value > 0), `${label}: invalid coordinates`);
    const [line, column] = position;
    assert.ok(line <= lines.length, `${label}: line outside source`);
    const characters = Array.from(lines[line - 1]);
    assert.ok(column <= characters.length + 1, `${label}: column outside source`);
    return lines.slice(0, line - 1).reduce((size, value) => size + value.length + 1, 0) + characters.slice(0, column - 1).join('').length;
  }
  const begin = offset(span?.begin), end = offset(span?.end);
  assert.ok(end > begin, `${label}: empty or reversed item`);
  return createHash('sha256').update(text.slice(begin, end)).digest('hex');
}

// The pinned compiler, rather than derive syntax, owns the implementation inventory.
export function inventory(doc, { onlyOwners, onlyDefaultTraits, defaultTraits, receiverTraits = {}, readSource } = {}) {
  assert.ok(!(onlyOwners && onlyDefaultTraits), 'accepted-set: regression scopes cannot be combined');
  assert.equal(doc.format_version, 61, 'accepted-set: review changed rustdoc JSON format');
  const index = doc.index;
  const get = id => index[String(id)];
  const paths = new Map();
  const modules = new Map();
  function walk(id, path, owner) {
    const item = get(id);
    assert.ok(item, `accepted-set: missing compiler item ${id}`);
    paths.set(String(id), path);
    modules.set(String(id), owner);
    if (item.inner.module) {
      for (const child of item.inner.module.items) {
        const value = get(child);
        if (value && !value.inner.use) walk(child, [...path, value.name], item);
      }
    }
  }
  walk(doc.root, [get(doc.root).name], get(doc.root));
  function normalize(value) {
    if (Array.isArray(value)) return value.map(normalize);
    if (!value || typeof value !== 'object') return value;
    const result = Object.fromEntries(Object.entries(value).filter(([key]) => key !== 'id').map(([key, member]) => [key, normalize(member)]));
    if (value.id != null && value.path) result.path = doc.paths[String(value.id)]?.path.join('::') ?? paths.get(String(value.id))?.join('::') ?? value.path;
    return result;
  }
  function returnsOwner(value, id, seen = new Set()) {
    if (!value || typeof value !== 'object') return false;
    if (value.generic === 'Self') return true;
    const resolved = value.resolved_path;
    if (resolved) {
      if (String(resolved.id) === String(id)) return true;
      const alias = get(resolved.id)?.inner.type_alias;
      if (alias && !seen.has(String(resolved.id))) {
        const visited = new Set(seen).add(String(resolved.id));
        if (returnsOwner(alias.type, id, visited)) return true;
      }
    }
    return Object.values(value).some(child => returnsOwner(child, id, seen));
  }
  const reviewedDefaults = onlyOwners ? { owners: [], methodIds: new Set() } : defaultTraitInventory(doc, paths, normalize, { classifications: defaultTraits, readSource });
  if (onlyDefaultTraits) {
    assert.ok(Array.isArray(onlyDefaultTraits) && onlyDefaultTraits.length, 'accepted-set: default regression scope cannot be empty');
    assert.deepEqual(reviewedDefaults.owners.map(owner => owner.name).sort(), [...new Set(onlyDefaultTraits)].sort(), 'accepted-set: selected default traits must be inventoried and reviewed');
    return { sources: reviewedDefaults.owners.map(owner => owner.span.filename), construction: [], declarations: reviewedDefaults.owners.map(owner => ({ name: owner.name, kind: 'trait' })), types: [], externalReexports: [], externalTypeAliases: [], publicAliases: [], defaultWitnesses: reviewedDefaults.owners, scope: { onlyDefaultTraits } };
  }
  const traitPath = impl => doc.paths[String(impl.trait?.id)]?.path.join('::') ?? paths.get(String(impl.trait?.id))?.join('::');
  const deserializers = Object.values(index).filter(item => {
    const impl = item.inner.impl;
    return impl && !impl.is_negative && ['serde::de::Deserialize', 'serde_core::de::Deserialize'].includes(traitPath(impl));
  });
  const deserializerIds = new Set(deserializers.map(item => {
    const target = item.inner.impl.for.resolved_path;
    assert.ok(target, `accepted-set: unsupported Deserialize target ${JSON.stringify(item.inner.impl.for)}`);
    assert.ok(get(target.id)?.crate_id === 0 && paths.has(String(target.id)), `accepted-set: unresolved Deserialize owner ${target.path}`);
    return String(target.id);
  }));
  const types = new Map();
  const rawEntrypoints = new WeakMap();
  const construction = [];
  const coveredImpls = new Set();
  const coveredDefaults = new Set(reviewedDefaults.methodIds);
  const coveredTraits = new Set();
  function expanded(value, seen = new Set(), bindings = new Map()) {
    if (Array.isArray(value)) return value.map(child => expanded(child, seen, bindings));
    if (!value || typeof value !== 'object') return value;
    // Bound values are expanded in the caller's lexical environment before
    // entering an alias. Reapplying callee bindings would capture same-named
    // caller parameters, or recurse forever on a legitimate identity binding.
    if (value.generic && bindings.has(value.generic)) return bindings.get(value.generic);
    if (value.resolved_path) {
      const id = String(value.resolved_path.id);
      const alias = get(id)?.inner.type_alias;
      if (alias) {
        assert.ok(!seen.has(id), 'accepted-set assertion: recursive type alias needs explicit compiler resolution');
        const parameters = (alias.generics?.params ?? []).filter(param => param.kind?.type);
        const arguments_ = (value.resolved_path.args?.angle_bracketed?.args ?? []).filter(arg => arg.type);
        const supplied = arguments_.map(argument => expanded(argument.type, seen, bindings));
        const substituted = new Map(bindings);
        // Alias parameters shadow the caller's names. Rust defaults may refer
        // to earlier parameters; resolve them in declaration order.
        for (const parameter of parameters) substituted.delete(parameter.name);
        parameters.forEach((parameter, i) => {
          const value = supplied[i] ?? (parameter.kind.type.default == null
            ? { generic: parameter.name }
            : expanded(parameter.kind.type.default, seen, substituted));
          substituted.set(parameter.name, value);
        });
        return expanded(alias.type, new Set(seen).add(id), substituted);
      }
    }
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, expanded(child, seen, bindings)]));
  }
  // Native frame inputs must use the compiler's exact canonical identity. Source
  // spelling and an alias named Js cannot substitute for the actual target.
  const nativeJsInput = value => {
    if (Array.isArray(value)) return value.some(nativeJsInput);
    if (!value || typeof value !== 'object') return false;
    const id = value.resolved_path?.id;
    if (id != null && (doc.paths[String(id)]?.path ?? paths.get(String(id)))?.join('::')
      === 'safe_cex_exchange::js_runtime::value::Js') return true;
    return Object.values(value).some(nativeJsInput);
  };
  // PatchSource admits the raw serde_json Map, so its dyn entrypoints are raw input.
  const otherRawInput = value => /"primitive":"str"|"primitive":"u8"|serde_json::(?:value::)?(?:Value|Map)|alloc::string::String|safe_cex_types::retained_json::RetainedJsonInput|safe_cex_store::model_patch::PatchSource/.test(JSON.stringify(normalize(expanded(value))));
  const rawInput = value => nativeJsInput(expanded(value)) || otherRawInput(value);
  const apiSources = new Map();
  const decoderBounds = fn => /serde(?:_core)?::de::(?:DeserializeOwned|Deserializer|Deserialize)/.test(JSON.stringify(normalize(fn.generics)));
  const decodedOutput = value => {
    value = expanded(value);
    if (!value || typeof value !== 'object') return false;
    if (value.resolved_path && (deserializerIds.has(String(value.resolved_path.id)) || /(?:serde_json::(?:value::)?Value|safe_cex_types::retained_json::RetainedJsonInput)/.test(JSON.stringify(normalize(value.resolved_path))))) return true;
    return Object.values(value).some(decodedOutput);
  };
  for (const item of Object.values(index)) {
    const declaration = item.inner.struct ?? item.inner.enum ?? item.inner.union;
    if (!declaration || item.crate_id !== 0) continue;
    const target = { id: item.id };
    const path = paths.get(String(item.id));
    assert.ok(path, `accepted-set: unaddressable type ${item.name}`);
    if (onlyOwners && !onlyOwners.includes(path.join('::'))) continue;
    const deserialize = deserializerIds.has(String(item.id));
    // Discovery provenance enforces an exact pending ledger, never a filter.
    let otherwiseInventoried = deserialize;
    const methods = [];
    const conversions = [];
    const traitMethods = [];
    for (const id of declaration.impls) {
      const implementation = get(id)?.inner.impl;
      if (!implementation || implementation.is_synthetic || implementation.is_negative) continue;
      if (!implementation.blanket_impl && String(implementation.for?.resolved_path?.id) !== String(target.id)) continue;
      if (implementation.blanket_impl && get(implementation.trait?.id)?.crate_id !== 0) continue;
      coveredImpls.add(String(id));
      if (implementation.trait) coveredTraits.add(String(implementation.trait.id));
      const trait = traitPath(implementation);
      if (['core::convert::From', 'core::convert::TryFrom', 'core::str::traits::FromStr'].includes(trait)) {
        const api = { trait, args: normalize(implementation.trait.args) };
        conversions.push(api);
        otherwiseInventoried = true;
        rawEntrypoints.set(api, trait === 'core::str::traits::FromStr' || rawInput(implementation.trait.args));
      }
      if (!implementation.trait) {
        for (const id of implementation.items ?? []) {
          const method = get(id);
          if (method?.visibility === 'public' && method.inner.function && (deserialize || returnsOwner(method.inner.function.sig.output, item.id) || rawInput(method.inner.function.sig.inputs) || decodedOutput(method.inner.function.sig.output) || decoderBounds(method.inner.function))) {
            const fn = method.inner.function;
            otherwiseInventoried ||= deserialize || returnsOwner(fn.sig.output, item.id) || otherRawInput(fn.sig.inputs) || decodedOutput(fn.sig.output) || decoderBounds(fn);
            const api = { name: method.name, signature: normalize(fn.sig) };
            methods.push(api);
            apiSources.set(api, method.span);
            rawEntrypoints.set(api, rawInput(method.inner.function.sig.inputs) || decoderBounds(method.inner.function));
          }
        }
      }
      if (implementation.trait && !['serde::de::Deserialize', 'serde_core::de::Deserialize', 'core::convert::From', 'core::convert::TryFrom', 'core::str::traits::FromStr', 'core::clone::Clone', 'core::marker::Copy', 'core::default::Default'].includes(trait)) {
        const supplied = (implementation.items ?? []).map(get).filter(Boolean);
        const suppliedNames = new Set(supplied.map(method => method.name));
        const defaults = (get(implementation.trait.id)?.inner.trait?.items ?? []).map(get).filter(method => method?.inner.function?.has_body && !suppliedNames.has(method.name));
        for (const method of defaults) coveredDefaults.add(String(method.id));
        for (const method of [...supplied, ...defaults]) {
          if (reviewedDefaults.methodIds.has(String(method.id))) continue;
          const fn = method.inner.function;
          if (fn && (returnsOwner(fn.sig.output, item.id) || decodedOutput(fn.sig.output) || rawInput(fn.sig.inputs) || decoderBounds(fn))) {
            otherwiseInventoried ||= returnsOwner(fn.sig.output, item.id) || decodedOutput(fn.sig.output) || otherRawInput(fn.sig.inputs) || decoderBounds(fn);
            assert.ok(trait, `${path.join('::')}: accepted-set assertion: unresolved constructor trait`);
            const api = { trait, name: method.name, signature: normalize(fn.sig), generics: normalize(fn.generics), ...(implementation.blanket_impl ? { blanket: normalize(implementation.generics) } : {}) };
            traitMethods.push(api);
            if (!implementation.blanket_impl && suppliedNames.has(method.name)) apiSources.set(api, method.span);
            rawEntrypoints.set(api, rawInput(fn.sig.inputs) || decoderBounds(fn));
          }
        }
      }
    }
    const fields = declaration.kind?.plain?.fields ?? declaration.kind?.tuple ?? [];
    const privateFields = fields.some(id => get(id)?.visibility !== 'public');
    const duplicateTraits = declaration.impls.map(id => get(id)?.inner.impl).filter(impl => impl && !impl.is_negative && !impl.blanket_impl && ['core::clone::Clone', 'core::marker::Copy'].includes(traitPath(impl))).map(impl => traitPath(impl)).sort();
    if (privateFields && duplicateTraits.length) construction.push({ name: path.join('::'), traits: duplicateTraits });
    if (!deserialize && !conversions.length && !methods.length && !traitMethods.length) continue;
    types.set(String(target.id), {
      deserialize, name: path.join('::'), ident: item.name, span: item.span,
      ...(!otherwiseInventoried ? { nativeInputDiscovered: true } : {}),
      owner: modules.get(String(target.id))?.span,
      kind: item.inner.struct ? 'record' : 'enum',
      generics: declaration.generics,
      methods, conversions, traitMethods,
      rawEntrypoints: [...methods, ...conversions, ...traitMethods].map(api => rawEntrypoints.get(api)),
      entrypointSources: [...methods, ...conversions, ...traitMethods].map(api => apiSources.get(api) ?? null),
    });
  }
  for (const item of Object.values(index)) {
    const fn = item.inner.function;
    if (item.crate_id !== 0 || !fn || !paths.has(String(item.id))) continue;
    if (onlyOwners && !onlyOwners.includes(paths.get(String(item.id)).join('::'))) continue;
    if (!rawInput(fn.sig.inputs) && !decodedOutput(fn.sig.output) && !decoderBounds(fn)) continue;
    types.set(String(item.id), {
      deserialize: false, name: paths.get(String(item.id)).join('::'), ident: item.name,
      ...(!otherRawInput(fn.sig.inputs) && !decodedOutput(fn.sig.output) && !decoderBounds(fn) ? { nativeInputDiscovered: true } : {}),
      span: item.span, owner: modules.get(String(item.id))?.span, kind: 'function',
      generics: fn.generics, decodedOutput: decodedOutput(fn.sig.output),
      methods: [{ kind: 'function', name: item.name, signature: normalize(fn.sig), generics: normalize(fn.generics) }], conversions: [], traitMethods: [],
    });
  }
  if (onlyOwners) {
    assert.deepEqual([...types.values()].map(type => type.name).sort(), [...new Set(onlyOwners)].sort(), 'accepted-set: selected production owners must be inventoried');
    return {
      sources: [...new Set([...types.values()].map(type => type.span.filename))],
      construction, declarations: [...types.values()].map(type => ({ name: type.name, kind: type.kind })),
      types: [...types.values()], externalReexports: [], externalTypeAliases: [], publicAliases: [], defaultWitnesses: [],
      scope: { onlyOwners },
    };
  }
  // Receivers with no inventoried owner (smart-pointer forwarding, external
  // types) and never-instantiated defaults are reviewed against pinned source.
  function typeName(type) {
    if (!type || typeof type !== 'object') return String(type);
    if (type.generic) return type.generic;
    if (type.primitive) return type.primitive;
    if (type.borrowed_ref) return `&${type.borrowed_ref.is_mutable ? 'mut ' : ''}${typeName(type.borrowed_ref.type)}`;
    if (type.tuple) return `(${type.tuple.map(typeName).join(', ')})`;
    if (type.slice) return `[${typeName(type.slice)}]`;
    if (type.resolved_path) {
      const name = doc.paths[String(type.resolved_path.id)]?.path.join('::') ?? paths.get(String(type.resolved_path.id))?.join('::') ?? type.resolved_path.path;
      const args = (type.resolved_path.args?.angle_bracketed?.args ?? []).map(arg => arg.type ? typeName(arg.type) : arg.lifetime ?? JSON.stringify(normalize(arg)));
      return args.length ? `${name}<${args.join(', ')}>` : name;
    }
    return JSON.stringify(normalize(type));
  }
  function reviewedReceiver(key, span, contract) {
    const label = `${key}: accepted-set assertion: reviewed receiver classification`;
    assert.deepEqual(Object.keys(contract).sort(), ['category', 'file', 'itemSha256', 'reason'], `${label}: unsupported fields`);
    assert.ok(['non-decoder', 'delegated'].includes(contract.category), `${label}: unknown category`);
    assert.ok(contract.reason?.trim(), `${label}: specific reason required`);
    assert.equal(span?.filename, contract.file, `${label}: reviewed source identity changed`);
    assert.equal(typeof readSource, 'function', `${label}: production source reader required`);
    assert.match(contract.itemSha256, /^[a-f0-9]{64}$/, `${label}: item SHA-256 required`);
    assert.equal(receiverItemSha256(span, readSource(span.filename)), contract.itemSha256, `${label}: reviewed item source changed`);
  }
  const unreviewed = [];
  for (const item of Object.values(index)) {
    const impl = item.inner.impl;
    if (impl && !impl.is_negative && !impl.is_synthetic && !coveredImpls.has(String(item.id)) && get(impl.trait?.id)?.crate_id === 0) {
      const supplied = (impl.items ?? []).map(get).filter(Boolean);
      const names = new Set(supplied.map(method => method.name));
      const defaults = (get(impl.trait.id)?.inner.trait?.items ?? []).map(get).filter(method => method?.inner.function?.has_body && !names.has(method.name));
      for (const method of [...supplied, ...defaults]) {
        if (reviewedDefaults.methodIds.has(String(method.id))) continue;
        const fn = method?.inner.function;
        if (fn && (rawInput(fn.sig.inputs) || decodedOutput(fn.sig.output) || decoderBounds(fn))) {
          const key = `${traitPath(impl)}::${method.name} for ${typeName(impl.for)}`;
          if (!receiverTraits[key]) unreviewed.push(`${key}: accepted-set assertion: decoder trait on unsupported receiver needs explicit production coverage`);
          else reviewedReceiver(key, item.span, receiverTraits[key]);
        }
      }
    }
    if (item.crate_id === 0 && item.inner.trait && !coveredTraits.has(String(item.id))) {
      for (const id of item.inner.trait.items) {
        const method = get(id); const fn = method?.inner.function;
        if (fn?.has_body && !coveredDefaults.has(String(id)) && (rawInput(fn.sig.inputs) || decodedOutput(fn.sig.output) || decoderBounds(fn))) {
          const key = `${paths.get(String(item.id))?.join('::')}::${method.name}`;
          if (!receiverTraits[key]) unreviewed.push(`${key}: accepted-set assertion: uninstantiated default decoder requires production coverage`);
          else reviewedReceiver(key, method.span, receiverTraits[key]);
        }
      }
    }
  }
  assert.equal(unreviewed.length, 0, unreviewed.join('\n'));
  const externalTypeAliases = [];
  const publicAliases = [];
  function collectExternalTypePaths(value, output) {
    if (Array.isArray(value)) { for (const child of value) collectExternalTypePaths(child, output); return; }
    if (!value || typeof value !== 'object') return;
    if (value.resolved_path && (get(value.resolved_path.id)?.crate_id ?? doc.paths[String(value.resolved_path.id)]?.crate_id) !== 0) {
      const canonical = doc.paths[String(value.resolved_path.id)];
      const crateId = get(value.resolved_path.id)?.crate_id ?? canonical?.crate_id;
      const compilerCrate = doc.external_crates?.[String(crateId)];
      assert.ok(canonical?.path?.length && canonical.kind && compilerCrate?.path, `accepted-set: external alias target lacks compiler identity (${value.resolved_path.id})`);
      output.push({ canonical: canonical.path.join('::'), kind: canonical.kind, compilerPath: compilerCrate.path });
    }
    // dyn-trait paths are bare trait records, not resolved_path wrappers.
    // Preserve their actual compiler IDs until external crate identity resolves.
    for (const { trait } of value.dyn_trait?.traits ?? []) {
      const canonical = doc.paths[String(trait.id)];
      const crateId = get(trait.id)?.crate_id ?? canonical?.crate_id;
      if (crateId === 0) continue;
      const compilerCrate = doc.external_crates?.[String(crateId)];
      assert.ok(canonical?.path?.length && canonical.kind === 'trait' && compilerCrate?.path, `accepted-set: external alias trait lacks compiler identity (${trait.id})`);
      output.push({ canonical: canonical.path.join('::'), kind: canonical.kind, compilerPath: compilerCrate.path });
    }
    for (const child of Object.values(value)) collectExternalTypePaths(child, output);
  }
  for (const item of Object.values(index)) {
    if (item.visibility === 'public' && item.inner.use) assert.ok(item.inner.use.id != null, `accepted-set: unresolved public reexport ${item.inner.use.name ?? item.name ?? item.inner.use.source ?? '<unnamed use>'}`);
    // Public aliases can hide an owner implementation. Private aliases are not public surface.
    if (item.inner.type_alias && item.crate_id === 0 && item.visibility === 'public') {
      const projectionIdentity = hyperliquidProjectionCompilerIdentity(doc, item, paths);
      publicAliases.push({ name: paths.get(String(item.id))?.join('::') ?? item.name, ident: item.name, span: item.span, generics: normalize(item.inner.type_alias.generics), rawTarget: normalize(item.inner.type_alias.type), target: normalize(expanded(item.inner.type_alias.type)), ...(projectionIdentity ? { projectionIdentity } : {}) });
      const targets = [];
      collectExternalTypePaths(expanded(item.inner.type_alias.type), targets);
      for (const target of targets) externalTypeAliases.push({ name: paths.get(String(item.id))?.join('::') ?? item.name, ...target });
    }
  }
  // External aliases cannot silently escape the workspace-owned implementation inventory.
  const externalReexports = Object.values(index).filter(item => item.visibility === 'public' && item.inner.use && item.inner.use.id != null && get(item.inner.use.id)?.crate_id !== 0).map(item => {
    const use = item.inner.use;
    const canonical = doc.paths[String(use.id)];
    const crateId = get(use.id)?.crate_id ?? canonical?.crate_id;
    const compilerPath = doc.external_crates?.[String(crateId)]?.path;
    const name = use.name ?? item.name ?? use.source ?? `<use ${item.id}>`;
    const rootModule = canonical?.kind === 'module' && canonical.path?.length === 1
      && doc.external_crates?.[String(crateId)]?.name === canonical.path[0];
    assert.ok((canonical?.path?.length >= 2 || rootModule) && canonical.kind && compilerPath, `accepted-set: external reexport ${name} (${use.source}) has no compiler canonical owner`);
    return { name, source: use.source, canonical: canonical.path.join('::'), kind: canonical.kind, compilerPath, span: item.span };
  });
  const declarations = [...paths].map(([id, parts]) => ({ name: parts.join('::'), kind: Object.keys(get(id).inner)[0] }));
  const sources = [...new Set(Object.values(index).filter(item => item.crate_id === 0 && item.span).map(item => item.span.filename))];
  return { sources, construction, declarations, types: [...types.values()].sort((a, b) => a.name.localeCompare(b.name)), externalReexports, externalTypeAliases, publicAliases, defaultWitnesses: reviewedDefaults.owners };
}
