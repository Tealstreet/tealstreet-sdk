import assert from 'node:assert/strict';
import test from 'node:test';
import { inventory, receiverItemSha256 } from './accepted-inventory.mjs';

function fixture() {
  return {
    format_version: 61, root: 0,
    external_crates: {},
    paths: { 20: { path: ['serde_core', 'de', 'Deserialize'] }, 21: { path: ['core', 'str', 'traits', 'FromStr'] } },
    index: {
      0: { name: 'production', inner: { module: { items: [1, 2] } } },
      1: { id: 1, crate_id: 0, name: 'Private', visibility: 'default', span: { filename: 'src/lib.rs', begin: [1, 1], end: [1, 10] }, inner: { struct: { generics: { params: [] }, impls: [3, 4] } } },
      2: { name: 'Alias', visibility: 'public', inner: { use: { id: 1, name: 'Alias' } } },
      3: { attrs: [], inner: { impl: { trait: { id: 20 }, for: { resolved_path: { id: 1, path: 'Private' } }, is_negative: false } } },
      4: { inner: { impl: { trait: { id: 21 }, for: { resolved_path: { id: 1, path: 'Private' } }, items: [] } } },
    },
  };
}
test('compiler impl identity sees manual private Deserialize and deduplicates reexports', () => {
  const result = inventory(fixture());
  assert.equal(result.types.length, 1);
  assert.equal(result.types[0].name, 'production::Private');
  assert.deepEqual(result.types[0].conversions, [{ trait: 'core::str::traits::FromStr', args: undefined }]);
  const doc = fixture();
  doc.index[2].inner.use = { id: 80, name: 'RenamedSide', source: 'dependency::Side' };
  doc.paths[80] = { crate_id: 9, path: ['dependency', 'Side'], kind: 'enum' };
  doc.external_crates[9] = { path: '/deps/libdependency.rmeta' };
  assert.deepEqual(inventory(doc).externalReexports.map(({ name, canonical, kind }) => ({ name, canonical, kind })), [{ name: 'RenamedSide', canonical: 'dependency::Side', kind: 'enum' }]);
  const root = structuredClone(doc);
  root.index[2].inner.use = { id: 81, name: 'dependency', source: 'dependency', is_glob: true };
  root.paths[81] = { crate_id: 9, path: ['dependency'], kind: 'module' };
  root.external_crates[9].name = 'dependency';
  assert.deepEqual(inventory(root).externalReexports.map(({ name, canonical, kind }) => ({ name, canonical, kind })), [{ name: 'dependency', canonical: 'dependency', kind: 'module' }]);
  root.index[2].inner.use.is_glob = false;
  assert.equal(inventory(root).externalReexports[0].canonical, 'dependency');
  root.external_crates[9].name = 'unrelated';
  assert.throws(() => inventory(root), /no compiler canonical owner/);
  root.external_crates[9].name = 'dependency'; root.paths[81].kind = 'enum';
  assert.throws(() => inventory(root), /no compiler canonical owner/);
  delete doc.paths[80];
  assert.throws(() => inventory(doc), /RenamedSide.*no compiler canonical owner/);
  doc.index[2].inner.use.id = null;
  assert.throws(() => inventory(doc), /unresolved public reexport RenamedSide/);
  const aliases = fixture();
  aliases.index[0].inner.module.items.push(90);
  aliases.index[90] = { id: 90, crate_id: 0, name: 'Wire', visibility: 'public', inner: { type_alias: { type: { resolved_path: { id: 91, path: 'String' } } } } };
  aliases.paths[91] = { crate_id: 10, path: ['alloc', 'string', 'String'], kind: 'struct' };
  aliases.external_crates = { 10: { path: '/sysroot/liballoc.rmeta' }, 11: { path: '/deps/libdependency.rmeta' } };
  assert.doesNotThrow(() => inventory(aliases), 'standard raw String aliases retain ordinary decoder discovery');
  aliases.index[90].inner.type_alias.type.resolved_path.args = { angle_bracketed: { args: [{ type: { resolved_path: { id: 92, path: 'dependency::Side' } } }] } };
  aliases.paths[92] = { crate_id: 11, path: ['dependency', 'Side'], kind: 'enum' };
  assert.deepEqual(inventory(aliases).externalTypeAliases, [
    { name: 'production::Wire', canonical: 'alloc::string::String', kind: 'struct', compilerPath: '/sysroot/liballoc.rmeta' },
    { name: 'production::Wire', canonical: 'dependency::Side', kind: 'enum', compilerPath: '/deps/libdependency.rmeta' },
  ], 'public aliases expose exact external compiler identity for workspace resolution');
  aliases.index[90].visibility = 'default';
  assert.doesNotThrow(() => inventory(aliases), 'private implementation aliases are not public decoder boundaries');


});
test('an unrelated trait spelled Deserialize cannot impersonate serde', () => {
  const doc = fixture();
  doc.paths[20].path = ['other', 'Deserialize'];
  assert.equal(inventory(doc).types[0].deserialize, false);
});
test('compiler format changes and missing owners fail closed', () => {
  const doc = fixture();
  doc.format_version++;
  assert.throws(() => inventory(doc), /rustdoc JSON format/);
  doc.format_version--;
  delete doc.index[1];
  assert.throws(() => inventory(doc), /unresolved Deserialize owner/);
});
test('public inherent decoders are inventoried regardless of method spelling', () => {
  const doc = fixture();
  doc.index[1].inner.struct.impls.push(5);
  doc.index[5] = { inner: { impl: { trait: null, for: { resolved_path: { id: 1 } }, items: [6] } } };
  doc.index[6] = { name: 'anything', visibility: 'public', inner: { function: { sig: { inputs: [], output: { generic: 'Self' } } } } };
  assert.equal(inventory(doc).types[0].methods[0].name, 'anything');
});

test('alternate constructors returning a local type alias remain inventoried', () => {
  const doc = fixture();
  doc.index[1].inner.struct.impls = [5];
  delete doc.index[3];
  doc.index[5] = { inner: { impl: { trait: null, for: { resolved_path: { id: 1 } }, items: [6] } } };
  doc.index[6] = { name: 'decode', visibility: 'public', inner: { function: { sig: { inputs: [], output: { resolved_path: { id: 7, path: 'ResultAlias' } } } } } };
  doc.index[7] = { inner: { type_alias: { type: { resolved_path: { id: 1, path: 'Private' } } } } };
  assert.equal(inventory(doc).types[0].methods[0].name, 'decode');
});

test('explicit owner regression scope cannot turn the default matrix inventory green', () => {
  const doc = fixture();
  doc.index[0].inner.module.items.push(30);
  doc.index[30] = { id: 30, crate_id: 0, name: 'Unclassified', inner: { trait: { items: [31] } } };
  doc.index[31] = { id: 31, crate_id: 0, name: 'decode', inner: { function: { has_body: true, sig: { inputs: [['value', { borrowed_ref: { type: { primitive: 'str' } } }]], output: null } } } };
  assert.throws(() => inventory(doc), /Unclassified::decode.*uninstantiated default decoder/);
  const selected = inventory(doc, { onlyOwners: ['production::Private'] });
  assert.deepEqual(selected.types.map(owner => owner.name), ['production::Private']);
  assert.deepEqual(selected.scope, { onlyOwners: ['production::Private'] });
  assert.throws(() => inventory(doc, { onlyOwners: ['production::Missing'] }), /selected production owners must be inventoried/);
  assert.throws(() => inventory(doc, { onlyDefaultTraits: [] }), /default regression scope cannot be empty/);
  assert.throws(() => inventory(doc, { onlyDefaultTraits: ['production::Unclassified'] }), /selected default traits must be inventoried and reviewed/);
  assert.throws(() => inventory(doc, { onlyOwners: ['production::Private'], onlyDefaultTraits: ['production::Unclassified'] }), /regression scopes cannot be combined/);
});

test('inherent factories returning a different decoded owner remain inventoried', () => {
  const doc = fixture();
  doc.index[0].inner.module.items.push(40);
  doc.index[40] = { id: 40, crate_id: 0, name: 'Factory', span: doc.index[1].span, inner: { struct: { generics: { params: [] }, impls: [41] } } };
  doc.index[41] = { inner: { impl: { for: { resolved_path: { id: 40 } }, items: [42] } } };
  doc.index[42] = { name: 'decode_alias', visibility: 'public', inner: { function: { sig: { inputs: [['raw', { borrowed_ref: { type: { primitive: 'str' } } }]], output: { resolved_path: { id: 1, path: 'Private' } } }, generics: {} } } };
  const found = inventory(doc).types.find(owner => owner.name === 'production::Factory');
  assert.ok(found, 'factory decoder must have its own compiler owner');
  assert.equal(found.deserialize, false);
  assert.deepEqual(found.methods.map(method => method.name), ['decode_alias']);
  assert.deepEqual(found.rawEntrypoints, [true]);
});

test('alias defaults respect lexical parameter scopes and nested applications without weakening cycle refusal', async () => {
  const doc = fixture();
  const parameter = (name, fallback = null) => ({ name, kind: { type: { bounds: [], default: fallback, is_synthetic: false } } });
  const applied = (id, name, types) => ({ resolved_path: { id, path: name, args: { angle_bracketed: { args: types.map(type => ({ type })), constraints: [] } } } });
  const alias = (id, name, params, type) => {
    doc.index[0].inner.module.items.push(id);
    doc.index[id] = { id, name, crate_id: 0, visibility: 'public', span: { filename: 'src/lib.rs', begin: [id, 1], end: [id, 40] }, inner: { type_alias: { generics: { params, where_predicates: [] }, type } } };
  };
  alias(60, 'Pair', [parameter('T'), parameter('E', { generic: 'T' })], { tuple: [{ generic: 'T' }, { generic: 'E' }] });
  alias(61, 'Outer', [parameter('E')], applied(60, 'Pair', [{ generic: 'E' }]));
  alias(62, 'Wrap', [parameter('T')], { tuple: [{ generic: 'T' }] });
  alias(63, 'Nested', [], applied(62, 'Wrap', [applied(62, 'Wrap', [{ primitive: 'u16' }])]));
  alias(65, 'ConcretePair', [], applied(60, 'Pair', [{ primitive: 'u16' }]));
  const targets = () => new Map(inventory(doc).publicAliases.map(row => [row.ident, row.target]));
  assert.deepEqual(targets().get('Outer'), { tuple: [{ generic: 'E' }, { generic: 'E' }] }, 'alias defaults preserve caller E rather than recursively capturing it');
  assert.deepEqual(targets().get('Nested'), { tuple: [{ tuple: [{ primitive: 'u16' }] }] }, 'nested finite applications are not recursive definitions');
  const { spawnSync } = await import('node:child_process');
  const url = new URL('./accepted-inventory.mjs', import.meta.url).href;
  const before = ': expanded(parameter.kind.type.default, seen, substituted));';
  const check = `const {inventory}=await import(${JSON.stringify(url)});const doc=${JSON.stringify(doc)};const alias=inventory(doc).publicAliases.find(row=>row.ident==='ConcretePair');assert.deepEqual(alias.target,{tuple:[{primitive:'u16'},{primitive:'u16'}]},'alias default compiler substitution');`;
  const execute = mutate => spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';${mutate ? `import {registerHooks} from 'node:module';registerHooks({load(url,context,next){const result=next(url,context);if(url!==${JSON.stringify(url)})return result;const source=String(result.source);const before=${JSON.stringify(before)};assert.equal(source.split(before).length-1,1,'one actual default substitution control site');return {...result,source:source.replace(before,': {generic:parameter.name});')};}});` : ''}${check}`], { encoding: 'utf8' });
  let result = execute(false); assert.equal(result.status, 0, result.stderr);
  result = execute(true); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, /alias default compiler substitution/);
  console.log('alias default substitution removed: named assertion; process exit 1');
  result = execute(false); assert.equal(result.status, 0, result.stderr);
  alias(64, 'Cycle', [], applied(64, 'Cycle', []));
  assert.throws(() => inventory(doc), /recursive type alias needs explicit compiler resolution/);
});
test('unsupported receivers and uninstantiated defaults pass only with a reviewed source pin', async () => {
  const source = Buffer.from('impl Local for External {}\n');
  const span = { filename: 'src/lib.rs', begin: [1, 1], end: [1, source.toString().trimEnd().length + 1] };
  const doc = fixture();
  doc.index[0].inner.module.items.push(30);
  doc.paths[40] = { crate_id: 5, path: ['dependency', 'External'], kind: 'struct' };
  doc.index[30] = { id: 30, crate_id: 0, name: 'Local', visibility: 'public', inner: { trait: { items: [31], generics: { params: [] } } } };
  doc.index[31] = { id: 31, name: 'read', span, inner: { function: { has_body: false, sig: { inputs: [['raw', { borrowed_ref: { type: { primitive: 'str' } } }]], output: null }, generics: { params: [] } } } };
  doc.index[32] = { id: 32, crate_id: 0, span, inner: { impl: { trait: { id: 30, path: 'Local' }, for: { resolved_path: { id: 40, path: 'External' } }, items: [31] } } };
  const key = 'production::Local::read for dependency::External';
  assert.throws(() => inventory(doc, { readSource: () => source }), /production::Local::read for dependency::External: accepted-set assertion: decoder trait on unsupported receiver/);
  const reviewed = { category: 'non-decoder', file: 'src/lib.rs', reason: 'Reviewed fixture body.', itemSha256: receiverItemSha256(span, source) };
  assert.doesNotThrow(() => inventory(doc, { receiverTraits: { [key]: reviewed }, readSource: () => source }));
  assert.throws(() => inventory(doc, { receiverTraits: { [key]: reviewed }, readSource: () => Buffer.from('impl Local for External { }\n') }), /reviewed item source changed/);
  assert.throws(() => inventory(doc, { receiverTraits: { [key]: { ...reviewed, category: 'decoder' } }, readSource: () => source }), /unknown category/);
  doc.index[31].inner.function.has_body = true;
  delete doc.index[32];
  assert.throws(() => inventory(doc, { readSource: () => source }), /production::Local::read: accepted-set assertion: uninstantiated default decoder/);
  assert.doesNotThrow(() => inventory(doc, { receiverTraits: { 'production::Local::read': reviewed }, readSource: () => source }));
});

test('receiver item hashes preserve Unicode columns and refuse invalid spans', () => {
  const item = 'impl Local for () {\r\n fn read() {}\r\n}';
  const source = `// 😀\r\nconst É: &str = "😀"; ${item}\r\n// after\r\n`;
  const span = { begin: [2, Array.from('const É: &str = "😀"; ').length + 1], end: [4, 2] };
  assert.equal(receiverItemSha256(span, source), receiverItemSha256({ begin: [1, 1], end: [3, 2] }, item));
  assert.equal(receiverItemSha256(span, Buffer.from(source)), receiverItemSha256(span, source));
  for (const bad of [undefined, {}, { begin: [0, 1], end: [1, 2] }, { begin: [1, 0.5], end: [1, 2] }, { begin: [99, 1], end: [99, 2] }, { begin: [1, 99], end: [1, 100] }, { begin: [1, 2], end: [1, 2] }, { begin: [2, 1], end: [1, 1] }]) {
    assert.throws(() => receiverItemSha256(bad, source), /reviewed receiver item span/);
  }
});

test('compiler receiver item pins survive unrelated edits and refuse changes inside the impl', async () => {
  const fs = await import('node:fs');
  const os = await import('node:os');
  const path = await import('node:path');
  const { spawnSync } = await import('node:child_process');
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'receiver-item-pins-'));
  const filename = path.join(directory, 'lib.rs');
  const header = 'pub trait Local { fn read(&self, raw: &str); }\n';
  const impl = 'impl crate::Local for () { fn read(&self, _raw: &str) {} }';
  const original = `${header}pub mod nested { const É: &str = "😀"; ${impl} }\n`;
  const sources = [original, '// unrelated prefix\n'+original+'\n// unrelated suffix\n', original.replace('_raw: &str) {}', '_raw: &str) { let _ = 1; }')];
  try {
    const docs = sources.map(source => {
      fs.writeFileSync(filename, source);
      const result = spawnSync('rustdoc', [filename, '--crate-name', 'receiver_pin_fixture', '--edition=2024', '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '-o', directory], { encoding: 'utf8', cwd: new URL('../../', import.meta.url), env: { ...process.env, RUSTC_BOOTSTRAP: '1' } });
      assert.equal(result.status, 0, result.stderr);
      return JSON.parse(fs.readFileSync(path.join(directory, 'receiver_pin_fixture.json')));
    });
    const span = Object.values(docs[0].index).find(item => item.inner.impl?.trait?.path === 'Local').span;
    // Non-ASCII and non-BMP text precedes the compiler-owned impl on its line.
    assert.equal(receiverItemSha256(span, original), receiverItemSha256({ begin: [1, 1], end: [1, impl.length + 1] }, impl));
    const key = 'receiver_pin_fixture::Local::read for ()';
    const reviewed = { category: 'non-decoder', file: filename, reason: 'Reviewed fixture discards the raw string.', itemSha256: receiverItemSha256(span, original) };
    const url = new URL('./accepted-inventory.mjs', import.meta.url).href;
    const check = `const {inventory}=await import(${JSON.stringify(url)});const docs=${JSON.stringify(docs)},sources=${JSON.stringify(sources)},key=${JSON.stringify(key)},reviewed=${JSON.stringify(reviewed)};const run=i=>inventory(docs[i],{receiverTraits:{[key]:reviewed},readSource:()=>sources[i]});assert.doesNotThrow(()=>run(0),'original impl admitted');assert.doesNotThrow(()=>run(1),'unrelated edits preserve impl pin');assert.throws(()=>run(2),/reviewed item source changed/,'inside impl edit must stale pin');assert.doesNotThrow(()=>run(0));const {itemSha256,...legacy}=reviewed;legacy.sourceSha256=itemSha256;assert.throws(()=>inventory(docs[0],{receiverTraits:{[key]:legacy},readSource:()=>sources[0]}),/unsupported fields/);`;
    const site = 'receiverItemSha256(span, readSource(span.filename))';
    const assertion = 'assert.equal(receiverItemSha256(span, readSource(span.filename)), contract.itemSha256, `${label}: reviewed item source changed`);';
    const execute = mutation => spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';${mutation ? `import {registerHooks} from 'node:module';registerHooks({load(url,context,next){const result=next(url,context);if(url!==${JSON.stringify(url)})return result;let source=String(result.source);const before=${JSON.stringify(mutation === 'whole-file' ? site : assertion)};assert.equal(source.split(before).length-1,1,'one receiver pin mutation site');source=source.replace(before,${JSON.stringify(mutation === 'whole-file' ? "createHash('sha256').update(readSource(span.filename)).digest('hex')" : '')});return {...result,source};}});` : ''}${check}`], { encoding: 'utf8' });
    let result = execute(); assert.equal(result.status, 0, result.stderr);
    result = execute('whole-file'); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, /original impl admitted|unrelated edits preserve impl pin/);
    console.log('receiver item pin widened to whole file: named assertion; process exit 1');
    result = execute('unchecked'); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, /inside impl edit must stale pin/);
    console.log('receiver item source check removed: named assertion; process exit 1');
    result = execute(); assert.equal(result.status, 0, result.stderr);
  } finally {
    fs.rmSync(directory, { recursive: true });
  }
});

test('compiled retained input consumers and producers remain decoder inventory owners', {timeout: 300000}, async () => {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const {spawnSync} = await import('node:child_process');
  const root = new URL('../../', import.meta.url);
  const cargo = args => {
    const result = spawnSync('cargo', args, {cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
      env: {...process.env, CARGO_BUILD_JOBS: '1', RUSTC_BOOTSTRAP: '1'}});
    assert.equal(result.status, 0, `actual retained consumer compilation: ${result.stderr}`);
    return result.stdout;
  };
  const metadata = JSON.parse(cargo(['metadata', '--locked', '--no-deps', '--format-version=1']));
  cargo(['rustdoc', '--locked', '-p', 'safe-cex-num', '--lib', '--jobs', '1', '--',
    '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all']);
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_num.json')));
  const names = ['safe_cex_num::coercion::js_retained_number', 'safe_cex_num::numeric_ingress::js_capture_output'];
  const found = inventory(doc, {onlyOwners: names});
  assert.deepEqual(found.types.map(row => row.name).sort(), [...names].sort(), 'canonical retained consumer and producer owners');
  assert.equal(found.types.find(row => row.name.endsWith('js_capture_output')).decodedOutput, true,
    'canonical retained output remains an admitted decoder output');
});
