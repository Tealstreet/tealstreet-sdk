// The compiler is required: no frozen fixtures or handwritten TS replacements.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const read = name => fs.readFileSync(path.join(root, 'packages/safe-cex/src', name), 'utf8');
const parse = text => ts.createSourceFile('oracle.ts', text, ts.ScriptTarget.Latest, true);
function execute(text, imports = {}) {
  const module = { exports: {} };
  vm.runInNewContext(ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText,
    { exports: module.exports, module, require: key => { assert(key in imports, `unexpected import ${key}`); return imports[key]; } });
  return module.exports;
}
const shared = read('types.ts');
const enums = execute(parse(shared).statements.filter(ts.isEnumDeclaration).map(n => n.getText()).join('\n'));
const source = read('exchanges/breakoutprop/breakoutprop.types.ts');
const constants = execute(source, { '../../types': enums });
const exchange = read('exchanges/breakoutprop/breakoutprop.exchange.ts');
const names = ['parseDxSessionTimeoutMs', 'toDxOrderType'];
const helperSource = parse(exchange).statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => names.includes(d.name.getText()))).map(n => n.getText()).join('\n');
assert.equal(parse(helperSource).statements.length, 2, 'actual helper declarations');
const helpers = execute('const OrderType = ' + JSON.stringify(enums.OrderType) + ';\n' + helperSource + '\nexports.toDxOrderType = toDxOrderType;');
function observe(value) {
  if (typeof value === 'number') { const b=Buffer.alloc(8); b.writeDoubleBE(Number.isNaN(value) ? NaN : value); return { number: Number.isNaN(value) ? '7ff8000000000000' : b.toString('hex') }; }
  if (Array.isArray(value)) return value.map(observe);
  if (value !== null && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k, observe(v)]));
  return value;
}
function fields(node) {
  // Fail closed instead of silently ignoring inherited required fields.
  assert(!node.heritageClauses?.length, 'inherited-field census refusal');
  assert(ts.isTypeLiteralNode(node) || ts.isInterfaceDeclaration(node), 'record schema required');
  return node.members.map(m => { assert(ts.isPropertySignature(m) && m.type && !m.questionToken, 'unsupported field schema'); return m; });
}
assert.throws(() => fields(parse('interface Child extends Parent { own: number }').statements[0]), /inherited-field census refusal/, 'inherited-field control');
let count = 0;
const numbers = ['-0', '-1e-9999', '1e400', '-1e400', '9007199254740993', '5e-324', '1.7976931348623157e308'];
function fixture(node, profile, p) {
  if (ts.isTypeLiteralNode(node)) return '{' + fields(node).map(m => JSON.stringify(m.name.text) + ':' + fixture(m.type, profile, p+'.'+m.name.text)).join(',') + '}';
  if (ts.isArrayTypeNode(node)) return '[' + fixture(node.elementType, profile, p+'[0]') + ',' + fixture(node.elementType, profile, p+'[1]') + ']';
  count++;
  switch (node.kind) {
    case ts.SyntaxKind.StringKeyword: return JSON.stringify(`${p}:é😀:${count}`);
    case ts.SyntaxKind.NumberKeyword: return profile === -1 ? String(count + 0.125) : numbers[profile];
    case ts.SyntaxKind.BooleanKeyword: return (count + profile) % 2 ? 'true' : 'false';
    case ts.SyntaxKind.AnyKeyword: return '{"negativeZero":-0,"overflow":1e400,"label":'+JSON.stringify(p)+',"null":null}';
    default: throw Error('unsupported schema '+node.getText());
  }
}
const types = parse(source).statements.filter(ts.isTypeAliasDeclaration);
assert.equal(types.length, 7, 'response alias inventory');
const cases=[];
for(const type of types) for(let profile=-1;profile<numbers.length;profile++) {
  count=0; const raw=fixture(type.type,profile,type.name.text);
  // These erased declarations have no runtime codec: JSON.parse is their actual ingress.
  cases.push({name:type.name.text,profile,raw,expected:observe(JSON.parse(raw))});
}
// Every declared boolean path must be observed in both states, independently of
// which integer counter happens to reach that field in a fixture.
const booleanStates=new Map();
function censusBooleans(value,path){
  if(typeof value==='boolean'){if(!booleanStates.has(path))booleanStates.set(path,new Set());booleanStates.get(path).add(value);}
  else if(Array.isArray(value))value.forEach(v=>censusBooleans(v,path+'[]'));
  else if(value && typeof value==='object')for(const [k,v] of Object.entries(value))censusBooleans(v,path+'.'+k);
}
for(const c of cases)censusBooleans(JSON.parse(c.raw),c.name);
for(const [path,states] of booleanStates)assert.equal(states.size,2,'boolean complement coverage: '+path);
const unknownCases=types.map(t=>{
  count=0; const original=JSON.parse(fixture(t.type,-1,t.name.text));
  let added=0;
  // Do not add inside opaque cashTransactions: those fields are retained in Rust too.
  function declaredAdd(node,value){
    if(ts.isTypeLiteralNode(node)){ for(const m of fields(node))declaredAdd(m.type,value[m.name.text]);value.unknownWireField='retained by TypeScript';added++; }
    else if(ts.isArrayTypeNode(node) && node.elementType.kind!==ts.SyntaxKind.AnyKeyword)value.forEach(v=>declaredAdd(node.elementType,v));
  }
  declaredAdd(t.type,original); const raw=JSON.stringify(original);
  assert.equal(JSON.parse(raw).unknownWireField,'retained by TypeScript','source unknown field retained');
  // Re-serialization above projects opaque infinity and negative zero, so expected uses
  // the corresponding input projection, while still observing native numeric bits.
  function strip(node,value){ if(ts.isTypeLiteralNode(node)){delete value.unknownWireField;for(const m of fields(node))strip(m.type,value[m.name.text]);} else if(ts.isArrayTypeNode(node)&&node.elementType.kind!==ts.SyntaxKind.AnyKeyword)value.forEach(v=>strip(node.elementType,v)); }
  const known=JSON.parse(raw);strip(t.type,known);
  return {name:t.name.text,profile:'unknown-field-discard',raw,expected:observe(known),added,sourceRetainsUnknown:JSON.parse(raw).unknownWireField};
});
const surrogates=types.map(t=>{count=0;const raw=fixture(t.type,-1,t.name.text).replace(/"[^"\\]*é😀[^"\\]*"/, '"\\ud800"'); const parsed=JSON.parse(raw);assert(JSON.stringify(parsed).includes('\\ud800'),'source surrogate retained');return {name:t.name.text,raw};});
function call(name,arg){try{return {ok:observe(helpers[name](arg))};}catch(e){return {error:e.message};}}
const timeouts=['00:30:00','1:02:03','::','-0:-0:-0','0x2:0b11:0o7','1.5:2.25:3.125','\ufeff1: 2 :3','1:60:0','1:0:60','-1:0:0','NaN:0:0','Infinity:0:0','1e308:0:0','1:2','1:2:3:4','1foo:0:0','0:0:5e-324','0:0:-1e-9999','é:0:0'];
const orderTypes=[...Object.values(enums.OrderType),'STOP','', 'constructor','__proto__','é😀'];
const helperSurrogates = names.map(name => ({name, raw: '"\\ud800"', errorRetainsSurrogate: call(name, '\ud800').error.includes('\ud800')}));
const tables={};
for(const name of ['ENDPOINTS','INTERVAL','ORDER_TYPE','ORDER_SIDE','ORDER_STATUS','POSITION_SIDE']){
 tables[name]=Object.fromEntries([...Object.keys(constants[name]),'missing','constructor','__proto__','toString'].map(key=>{const v=constants[name][key];return [key,{kind:typeof v,text:typeof v==='undefined'?null:String(v)}];}));
}
const orderedConstants=Object.fromEntries(Object.keys(tables).map(name=>[name,Object.entries(constants[name])]));
process.stdout.write(JSON.stringify({runtime:{node:process.version,major:process.versions.node.split('.')[0],arch:process.arch,v8:process.versions.v8},orderedConstants,constants,cases,unknownCases,surrogates,helperSurrogates,tables,timeouts:timeouts.map(arg=>({arg,result:call('parseDxSessionTimeoutMs',arg)})),orderTypes:orderTypes.map(arg=>({arg,result:call('toDxOrderType',arg)}))}));
