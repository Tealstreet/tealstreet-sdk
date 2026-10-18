// Expected bits come from live Node JSON.parse, before any JSON serialization.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts'), 'utf8');
const ast = ts.createSourceFile('types.ts', source, ts.ScriptTarget.Latest, true);
require('./interface-heritage.cjs')(ts,ast);
const definitions = new Map();
for (const node of ast.statements) if (ts.isInterfaceDeclaration(node) || ts.isTypeAliasDeclaration(node)) {
  const list = definitions.get(node.name.text) || []; list.push(node); definitions.set(node.name.text, list);
}
function numeric(node, visiting = new Set()) {
  if (!node) return false;
  if (node.kind === ts.SyntaxKind.NumberKeyword) return true;
  if (ts.isTypeReferenceNode(node)) {
    const name = node.typeName.getText(ast);
    if (name === 'Array') return numeric(node.typeArguments[0], visiting);
    if (visiting.has(name)) return false;
    return (definitions.get(name) || []).some(def => numeric(def.type, new Set([...visiting, name])) || (def.members || []).some(member => numeric(member.type, new Set([...visiting, name]))));
  }
  if (node.type && numeric(node.type, visiting)) return true;
  if (node.elementType && numeric(node.elementType, visiting)) return true;
  return [...(node.types || []), ...(node.elements || [])].some(child => numeric(child, visiting)) || (node.members || []).some(member => numeric(member.type, visiting));
}
const sourceNames = [...definitions].filter(([_, nodes]) => nodes.some(def => numeric(def.type) || (def.members || []).some(member => numeric(member.type)))).map(([name]) => name).filter(name => !require('./not-modeled-types.cjs').has(name));
const bases = JSON.parse(fs.readFileSync(path.join(__dirname, '../tests/fixtures/types.json'), 'utf8')).cases;
const escape = key => String(key).replaceAll('~', '~0').replaceAll('/', '~1');
function numbers(value, prefix = '', result = {}) {
  if (typeof value === 'number') {
    result[prefix] = require('./number-bits.cjs')(value);
  } else if (value && typeof value === 'object') for (const [key, child] of Object.entries(value)) numbers(child, `${prefix}/${escape(key)}`, result);
  return result;
}
function locations(value, steps = [], result = []) {
  if (typeof value === 'number') result.push(steps);
  else if (value && typeof value === 'object') for (const [key, child] of Object.entries(value)) locations(child, [...steps, key], result);
  return result;
}
const lexemes = ['1e400', '-1e400', '9007199254740993', '-9007199254740993', '-0', '0.000001', '1e20', '5e-324', '1e-400', '-1e-400'];
const cases = [], covered = [], seen = new Set();
for (const name of sourceNames) {
  // This original 53-root corpus excludes the intersection; its repaired core
  // variant/user/number assertions live in finding_user_event and spot_runtime.
  if (name === 'WsUserEvent') continue;
  let count = 0;
  for (const base of bases.filter(base => base.name === name)) {
    const input = JSON.parse(base.raw);
    for (const steps of locations(input)) for (const lexeme of lexemes) {
      let changed = JSON.parse(base.raw);
      if (steps.length) { let parent = changed; for (const key of steps.slice(0, -1)) parent = parent[key]; parent[steps.at(-1)] = '__RAW_NUMBER__'; }
      else changed = '__RAW_NUMBER__';
      const raw = JSON.stringify(changed).replace('"__RAW_NUMBER__"', lexeme);
      const key = name + '\n' + raw; if (seen.has(key)) continue; seen.add(key);
      const parsed = JSON.parse(raw);
      cases.push({name, path: '/' + steps.map(escape).join('/'), lexeme, raw, bits: numbers(parsed)}); count++;
    }
  }
  assert.ok(count > 0, `source numeric type requires actual numeric witnesses: ${name}`); covered.push(name);
}
const corpus = JSON.stringify({sourceSha256: crypto.createHash('sha256').update(source).digest('hex'), names: covered, cases}, null, 2) + '\n';
const arms = '// Generated source-derived numeric inventory; regeneration checks every arm.\nfn capture_typed(name: &str, raw: &str) -> Result<std::collections::BTreeMap<String, String>, serde_json::Error> {\n    match name {\n' + covered.map(name => `        "${name}" => capture::<safe_cex_venue_hyperliquid::types::${name}>(raw),`).join('\n') + '\n        _ => panic!("unrecognized source numeric type {name}"),\n    }\n}\n';
const fixture = path.join(__dirname, '../tests/fixtures/typed-numbers.json'), bindings = path.join(__dirname, '../tests/fixtures/typed-number-bindings.rs');
if (process.argv.includes('--capture')) {
  assert.ok(!fs.existsSync(fixture) && !fs.existsSync(bindings), 'exclusive capture requires both new paths');
  fs.writeFileSync(fixture, corpus, {flag:'wx'}); fs.writeFileSync(bindings, arms, {flag:'wx'});
} else {
  assert.equal(corpus, fs.readFileSync(fixture, 'utf8'), 'live Node typed-number oracle drift');
  assert.equal(arms, fs.readFileSync(bindings, 'utf8'), 'source numeric binding inventory drift');
}
console.log(`${covered.length} source numeric types; ${cases.length} pre-serialization bit cases`);
