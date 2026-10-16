// Execute current TS constants and inspect its declared finite JSON wire shapes.
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitget/bitget.types.ts');
let source = fs.readFileSync(file, 'utf8');
// Maintained negative source control, not a synthetic shape bypassing the reader.
if (process.argv.includes('--probe-interface-heritage')) {
  const declaration = 'export interface FETCH_POSITIONS_V2_RESPONSE {';
  require('node:assert/strict').equal(source.split(declaration).length, 2, 'heritage control source target');
  source = 'namespace BitgetHeritageProbe { export interface ExtraNumeric { hiddenNumeric: number; } }\n' +
    source.replace(declaration, 'export interface FETCH_POSITIONS_V2_RESPONSE extends BitgetHeritageProbe.ExtraNumeric {');
}
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const shared = ts.createSourceFile('types.ts', fs.readFileSync(path.join(root, 'packages/safe-cex/src/types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['OrderSide', 'OrderStatus', 'OrderTimeInForce', 'OrderType', 'PositionSide', 'PositionMode', 'OrderStopTriggerType'];
const enums = shared.statements.filter(n => ts.isEnumDeclaration(n) && names.includes(n.name.text)).map(n => n.getText(shared)).join('\n');
if (enums.length === 0) throw Error('Missing shared enum declarations');
const body = ast.statements.filter(n => !ts.isImportDeclaration(n)).map(n => n.getText(ast)).join('\n');
const exportsObject = {};
new Function('exports', ts.transpileModule(enums + '\n' + body, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText)(exportsObject);
const acceptedSets=Object.fromEntries(names.map(name=>[name,Object.values(exportsObject[name])]));
for (const name of names) delete exportsObject[name];
// Explicit generation contract: adding an endpoint requires classifying its response family.
const generationByEndpoint = {
  "ACCOUNT_SPOT": "v2",
  "ACCOUNT": "v2",
  "BALANCE": "v2",
  "ACCOUNT_BILL": "v2",
  "MARKETS": "v2",
  "SYMBOL_LEVERAGE": "v2",
  "TICKERS": "v2",
  "POSITIONS": "v2",
  "POSITIONS_V2": "v2",
  "KLINE": "v2",
  "ORDERS": "v2",
  "CANCEL_ALGO_ORDER": "v2",
  "CANCEL_ORDERS": "v2",
  "CANCEL_ALL_ORDERS": "v2",
  "PLACE_ORDER": "v2",
  "BATCH_ORDERS": "v2",
  "ALGO_ORDERS": "v2",
  "PLACE_ALGO_ORDER": "v2",
  "UPDATE_ORDER": "v2",
  "UPDATE_ALGO_ORDER": "v2",
  "SET_LEVERAGE": "v2",
  "SET_LEVERAGE_V3": "v3",
  "ACCOUNT_SETTINGS_V3": "v3",
  "UTA_ACCOUNT_ASSETS": "v3",
  "UTA_POSITIONS": "v3",
  "UTA_ORDERS": "v3",
  "UTA_STRATEGY_ORDERS": "v3",
  "UTA_ORDER_FILLS": "v3",
  "UTA_PLACE_ORDER": "v3",
  "UTA_PLACE_STRATEGY_ORDER": "v3",
  "UTA_CANCEL_ORDER": "v3",
  "UTA_CANCEL_STRATEGY_ORDER": "v3",
  "UTA_CANCEL_BATCH": "v3",
  "UTA_MODIFY_ORDER": "v3",
  "UTA_SET_HOLD_MODE": "v3",
  "POSITION_LEVERAGE": "v2",
  "SET_POSITION_MODE": "v2",
  "SET_MARGIN_MODE": "v2",
  "PLACE_TRAILING_STOP_ORDER": "v2",
  "FETCH_SINGLE_ACCOUNT": "v2",
  "ORDER_HISTORY": "v2",
  "ORDER_FILLS": "v2"
};
require('node:assert/strict').deepEqual(Object.keys(generationByEndpoint).sort(), Object.keys(exportsObject.ENDPOINTS).sort(), 'endpoint generation inventory');
const declarations = Object.fromEntries(ast.statements.filter(n => ts.isTypeAliasDeclaration(n) || ts.isInterfaceDeclaration(n)).map(n => [n.name.text, n]));
function shape(n) {
  if (ts.isInterfaceDeclaration(n) && n.heritageClauses?.length) throw Error(`Bitget type inventory refuses interface heritage: ${n.name.text}`);
  if (n.kind === ts.SyntaxKind.StringKeyword) return {kind:'string'};
  if (n.kind === ts.SyntaxKind.NumberKeyword) return {kind:'number'};
  if (ts.isLiteralTypeNode(n)) return {kind:'literal', value:n.literal.kind === ts.SyntaxKind.NullKeyword ? null : n.literal.text};
  if (ts.isUnionTypeNode(n)) return {kind:'union', members:n.types.map(shape)};
  if (ts.isArrayTypeNode(n)) return {kind:'array', item:shape(n.elementType)};
  if (ts.isTypeReferenceNode(n)) {
    if (n.typeName.getText(ast) === 'Array') return {kind:'array', item:shape(n.typeArguments[0])};
    const d = declarations[n.typeName.getText(ast)];
    if (!d) throw Error(`Unsupported reference ${n.getText(ast)}`);
    return shape(d.type || d);
  }
  if (ts.isTypeLiteralNode(n) || ts.isInterfaceDeclaration(n)) return {kind:'object', fields:Object.fromEntries(n.members.map(m => [m.name.text, {optional:!!m.questionToken, shape:shape(m.type)}]))};
  throw Error(`Unsupported type ${n.getText(ast)}`);
}
const shapes = Object.fromEntries(Object.entries(declarations).map(([name,n]) => [name,shape(n.type || n)]));
acceptedSets.BITGET_UTA_CATEGORY=shapes.BITGET_UTA_CATEGORY.members.map(m=>m.value);
function sample(s, full) {
  switch(s.kind) {
    case 'string': return '17.25';
    case 'number': return 1714810266272;
    case 'literal': return s.value;
    case 'union': return sample(s.members[0],full);
    case 'array': return [sample(s.item,full)];
    case 'object': return Object.fromEntries(Object.entries(s.fields).filter(([,f]) => full || !f.optional).map(([k,f]) => [k,sample(f.shape,full)]));
  }
}
function valid(s,v) {
  switch(s.kind) {
    case 'string': return typeof v === 'string';
    case 'number': return typeof v === 'number';
    case 'literal': return v === s.value;
    case 'union': return s.members.some(m=>valid(m,v));
    case 'array': return Array.isArray(v) && v.every(x=>valid(s.item,x));
    case 'object': return v !== null && typeof v === 'object' && !Array.isArray(v) && Object.entries(s.fields).every(([k,f]) => Object.hasOwn(v,k) ? valid(f.shape,v[k]) : f.optional);
  }
}
function mutations(s,v,p=[]) {
  const out = [];
  for (const x of [null, false, 7, '', [], {}]) out.push({path:p, value:x});
  if(s.kind==='literal' && typeof s.value==='string') for(const x of [s.value.toLowerCase(),s.value.toUpperCase(),s.value+' ',s.value+'\uFEFF',s.value+'\u0085',{[s.value]:null},[s.value]]) out.push({path:p,value:x});
  if(s.kind==='union') for(const m of s.members) {out.push({path:p,value:sample(m,true)}); if(valid(m,v) || m.kind==='literal') out.push(...mutations(m,sample(m,true),p));}
  if(s.kind==='number') for(const x of [1.25, 1e-7, 1e21, -0, -1]) out.push({path:p,value:x});
  if(s.kind==='object') out.push({path:p,value:Object.keys(s.fields).map(k=>v[k])});
  if(s.kind==='object') out.push({path:p,value:{...v,unknownVenueField:{n:1.25, list:[null,'x'], empty:[]}}});
  if(s.kind==='object') for(const [k,f] of Object.entries(s.fields)) {out.push({path:[...p,k],missing:true}); out.push(...mutations(f.shape,v[k],[...p,k]));}
  if(s.kind==='array') {out.push({path:p,value:[]}); out.push(...mutations(s.item,v[0],[...p,0]));}
  return out;
}
const cases=[];
for(const [name,s] of Object.entries(shapes)) {
  for(const full of [false,true]) cases.push({name,label:full?'full':'minimal',value:sample(s,full),valid:true});
  const baseline=sample(s,true);
  for(const [i,m] of mutations(s,baseline).entries()) {
    let value=structuredClone(baseline);
    if(!m.path.length) value=m.value;
    else {let parent=value; for(const k of m.path.slice(0,-1)) parent=parent[k]; if(m.missing) delete parent[m.path.at(-1)]; else parent[m.path.at(-1)]=m.value;}
    cases.push({name,label:`${m.path.join('.') || '<root>'}:${m.missing?'missing':JSON.stringify(m.value)}:${i}`,value,valid:valid(s,value)});
  }
}
// JSON permits escaped lone UTF-16 surrogates; Rust String deliberately cannot carry them.
const utf16Cases = [];
for (const [name, s] of Object.entries(shapes)) {
  if (s.kind !== 'object') continue;
  const field = Object.entries(s.fields).find(([, f]) => f.shape.kind === 'string')[0];
  for (const value of ['\uD800', '\uDC00']) {
    const input = {...sample(s,true), [field]: value};
    utf16Cases.push({name,field,json:JSON.stringify(input),valid:valid(s,input)});
  }
}
// Preserve numeric bits before JSON.stringify would erase -0 and infinities.
const numericIngress=[];
for(const name of ['FETCH_UTA_ACCOUNT_ASSETS_RESPONSE','FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE','FETCH_UTA_POSITIONS_RESPONSE','FETCH_UTA_ORDERS_RESPONSE','FETCH_UTA_FILLS_RESPONSE','FETCH_POSITIONS_V2_RESPONSE']) {
 require('node:assert/strict').equal(shapes[name].fields.requestTime.shape.kind,'number',`${name}.requestTime numeric ingress source contract`);
}
for(const [name,s] of Object.entries(shapes)) {
 if(s.kind!=='object')continue;
 for(const [field,f] of Object.entries(s.fields)) {
  if(f.shape.kind!=='number')continue;
  for(const lexeme of ['-0','-0.0','-0e0','-0e-400','0','0.0','1e400','-1e400','1.7976931348623157e308','1.7976931348623159e308','1e-400','-1e-400','5e-324','-5e-324','9007199254740993','1.2345678901234567','-42','12e2']) {
   const raw=JSON.stringify({...sample(s,true),[field]:'__numeric_lexeme__'}).replace('"__numeric_lexeme__"',lexeme);
   const value=JSON.parse(raw);
   const bits=Buffer.alloc(8);bits.writeDoubleBE(value[field]);
   numericIngress.push({name,field,lexeme,raw,valid:valid(s,value),bits:bits.toString('hex')});
  }
 }
}
require('node:assert/strict').equal(numericIngress.length,108,'all six Bitget numeric wire fields inventoried');
// Separate native-field identity lane: never changes the frozen shape corpus.
if (process.argv.includes('--field-identity')) {
  let serial = 0;
  const numericBits = {};
  const fieldPolicies = {};
  function identity(s, p) {
    switch (s.kind) {
      case 'string': return `identity:${p}:Σ😀`;
      case 'number': {
        const value = ++serial + 0.125;
        const bits = Buffer.alloc(8); bits.writeDoubleBE(value);
        numericBits[p] = bits.toString('hex');
        return value;
      }
      case 'literal': return s.value;
      case 'union': return identity(s.members.find(m => m.kind !== 'literal' || m.value !== null), p);
      case 'array': return [identity(s.item, `${p}/0`), identity(s.item, `${p}/1`)];
      case 'object':
        fieldPolicies[p] = Object.fromEntries(Object.entries(s.fields).map(([key,f]) => [key,{optional:f.optional,nullable:valid(f.shape,null)}]));
        return {...Object.fromEntries(Object.entries(s.fields).map(([k,f]) => [k,identity(f.shape, `${p}/${k}`)])), __identityExtra: `extra:${p}`};
      default: throw Error(`unsupported identity shape ${s.kind}`);
    }
  }
  const samples = Object.fromEntries(Object.entries(shapes).filter(([,s]) => s.kind === 'object').map(([name,s]) => [name,identity(s,name)]));
  fs.writeFileSync(1, JSON.stringify({samples,numericBits,fieldPolicies,categories:acceptedSets.BITGET_UTA_CATEGORY}) + '\n');
  process.exit(0);
}
const result = JSON.stringify({numericIngress,constants:exportsObject,generations:generationByEndpoint,shapes,cases,utf16Cases,acceptedSets}) + '\n';
const fixture = path.join(__dirname, '../tests/types-oracle.json');
if (process.argv.includes('--write')) fs.writeFileSync(fixture, result);
else if (process.argv.includes('--check')) {
  require('node:assert/strict').equal(fs.readFileSync(fixture, 'utf8'), result, 'Bitget TS types oracle snapshot drift');
  console.log(`Bitget oracle: ${Object.keys(exportsObject).length} constants, ${Object.keys(shapes).length} wire types, ${cases.length} shape cases`);
} else process.stdout.write(result);
