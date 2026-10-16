const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {captureHeader} = require('./special-numbers.cjs');
const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.types.ts');
const source = ts.createSourceFile(sourcePath, fs.readFileSync(sourcePath, 'utf8'), ts.ScriptTarget.Latest, true);
const aliases = new Map(source.statements.filter(ts.isTypeAliasDeclaration).map(node => [node.name.text, node.type]));
const roots = [
  {root: 'MARKET_RESPONSE', path: ['products', 0, 'product_id']},
  {root: 'INTX_INSTRUMENTS_RESPONSE', path: [0, 'instrument_id']},
  {root: 'PORTFOLIO_BREAKDOWN_RESPONSE', path: ['breakdown', 'portfolio', 'name']},
];
function resolve(node) {
  while (ts.isTypeReferenceNode(node)) node = aliases.get(node.typeName.getText(source));
  return node;
}
for (const {root, path: parts} of roots) {
  let node = aliases.get(root);
  for (const key of parts) {
    node = resolve(node);
    if (key === 0) { assert.ok(ts.isArrayTypeNode(node), 'live declared array'); node = node.elementType; }
    else { assert.ok(ts.isTypeLiteralNode(node), 'live declared record'); node = node.members.find(member => member.name.text === key)?.type; }
    assert.ok(node, `live declared string path ${root}.${parts.join('.')}`);
  }
  assert.equal(resolve(node).kind, ts.SyntaxKind.StringKeyword, 'live declared string surface');
}
const records = JSON.parse(execFileSync(process.execPath, [path.join(__dirname, 'unions.cjs')], {encoding: 'utf8'})).legacyRecords;
const templates = {MARKET_RESPONSE: records.MarketResponse, INTX_INSTRUMENTS_RESPONSE: [records.IntxInstrumentsResponseItem], PORTFOLIO_BREAKDOWN_RESPONSE: records.PortfolioBreakdownResponse};
const spellings = [
  {id: 'high', token: '"\\ud800"', units: [0xd800], refuses: true},
  {id: 'low', token: '"\\udc00"', units: [0xdc00], refuses: true},
  {id: 'ascii', token: '"alpha"', units: [97,108,112,104,97], refuses: false},
  {id: 'scalar', token: '"λ"', units: [0x3bb], refuses: false},
  {id: 'astral', token: '"\\ud83d\\ude80"', units: [0xd83d,0xde80], refuses: false},
  {id: 'whitespace', token: '" \\t\\n"', units: [32,9,10], refuses: false},
  {id: 'replacement', token: '"\\ufffd"', units: [0xfffd], refuses: false},
  {id: 'literal-ascii', token: '"\\\\ud800"', units: [92,117,100,56,48,48], refuses: false},
];
function at(value, parts) { for (const key of parts) value = value[key]; return value; }
const cases = [];
for (const item of roots) for (const spelling of spellings) {
  const value = structuredClone(templates[item.root]);
  at(value, item.path.slice(0, -1))[item.path.at(-1)] = '__coinbase_string_token__';
  const text = JSON.stringify(value);
  assert.equal(text.split('"__coinbase_string_token__"').length, 2, 'one string substitution');
  const raw = text.replace('"__coinbase_string_token__"', spelling.token);
  const native = at(JSON.parse(raw), item.path);
  assert.equal(typeof native, 'string', 'source returned field native type');
  assert.equal(native.length, spelling.units.length, 'source returned field UTF16 length');
  const units = Array.from({length: native.length}, (_, index) => native.charCodeAt(index));
  assert.deepEqual(units, spelling.units, 'source returned field units before encoding');
  // This comparison observes the materialized field, including lone units.
  assert.equal(native, String.fromCharCode(...spelling.units), 'source returned field comparison before encoding');
  cases.push({root: item.root, path: item.path, id: spelling.id, raw, units, refuses: spelling.refuses});
}
assert.equal(cases.length, 24, 'independent string witness floor');
const depths = [];
for (const item of roots) for (const depth of [3, 140]) {
  const value = structuredClone(templates[item.root]);
  let ignored = null;
  for (let index = 0; index < depth; index++) ignored = [ignored];
  const object = Array.isArray(value) ? value[0] : value;
  object.__ignored_depth = ignored;
  const raw = JSON.stringify(value);
  const native = JSON.parse(raw);
  assert.equal(at(native, item.path), at(value, item.path), 'source native string survives ignored depth');
  let decodedIgnored = (Array.isArray(native) ? native[0] : native).__ignored_depth;
  let decodedDepth = 0;
  while (Array.isArray(decodedIgnored)) { assert.equal(decodedIgnored.length, 1, 'source nested ignored array width'); decodedDepth++; decodedIgnored = decodedIgnored[0]; }
  assert.equal(decodedIgnored, null, 'source nested ignored terminal');
  assert.equal(decodedDepth, depth, 'source native parsed ignored depth');
  depths.push({root: item.root, depth, raw, field: at(native, item.path)});
}
assert.equal(depths.length, 6, 'independent depth witness floor');
const ignoredStrings=[];
for(const item of roots) for(const spelling of spellings.filter(item=>['high','low','ascii'].includes(item.id))) {
  const value=structuredClone(templates[item.root]);
  at(value,item.path.slice(0,-1))[item.path.at(-1)]='kept-field';
  const object=Array.isArray(value)?value[0]:value;object.__ignored_string='__coinbase_ignored_string_token__';
  const text=JSON.stringify(value);assert.equal(text.split('"__coinbase_ignored_string_token__"').length,2,'one ignored string substitution');
  const raw=text.replace('"__coinbase_ignored_string_token__"',spelling.token),native=JSON.parse(raw);
  assert.equal(at(native,item.path),'kept-field','source declared field survives ignored string');
  const ignored=(Array.isArray(native)?native[0]:native).__ignored_string;
  const units=Array.from({length:ignored.length},(_,index)=>ignored.charCodeAt(index));
  assert.deepEqual(units,spelling.units,'source ignored native string units before encoding');
  ignoredStrings.push({root:item.root,id:spelling.id,raw,units,field:at(native,item.path)});
}
assert.deepEqual(ignoredStrings.map(item=>item.root+':'+item.id),['MARKET_RESPONSE:high','MARKET_RESPONSE:low','MARKET_RESPONSE:ascii','INTX_INSTRUMENTS_RESPONSE:high','INTX_INSTRUMENTS_RESPONSE:low','INTX_INSTRUMENTS_RESPONSE:ascii','PORTFOLIO_BREAKDOWN_RESPONSE:high','PORTFOLIO_BREAKDOWN_RESPONSE:low','PORTFOLIO_BREAKDOWN_RESPONSE:ascii'],'independent ignored string witness content floor');
process.stdout.write(JSON.stringify({...captureHeader(), roots, cases, depths, ignoredStrings}));
