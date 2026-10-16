// Executes the real venue module. Type-only rows additionally use its fail-closed
// AST census and a compiler-checked, executed identity witness for each declaration.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/paradex/paradex.types.ts');
const text = fs.readFileSync(file, 'utf8');
const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const declarations = new Map();
for (const node of source.statements) {
  if (ts.isTypeAliasDeclaration(node) || ts.isInterfaceDeclaration(node)) declarations.set(node.name.text, node);
}
function shape(node, resolving = new Set()) {
  if (node.kind === ts.SyntaxKind.StringKeyword) return { kind: 'string' };
  if (node.kind === ts.SyntaxKind.NumberKeyword) return { kind: 'number' };
  if (node.kind === ts.SyntaxKind.BooleanKeyword) return { kind: 'boolean' };
  if (ts.isArrayTypeNode(node)) return { kind: 'array', item: shape(node.elementType, resolving) };
  if (ts.isTupleTypeNode(node)) return { kind: 'tuple', items: node.elements.map(n => shape(n, resolving)) };
  if (ts.isTypeReferenceNode(node)) {
    if (node.typeName.getText(source) === 'Array') {
      assert.equal(node.typeArguments.length, 1, 'array arity');
      return { kind: 'array', item: shape(node.typeArguments[0], resolving) };
    }
    const name = node.typeName.getText(source);
    assert.ok(!resolving.has(name), 'recursive declaration refused');
    assert.ok(declarations.has(name), `unknown declaration ${name}`);
    return declaration(declarations.get(name), new Set([...resolving, name]));
  }
  if (ts.isUnionTypeNode(node)) {
    assert.ok(node.types.every(n => ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal)), 'unsupported union refused');
    return { kind: 'enum', values: node.types.map(n => n.literal.text) };
  }
  if (ts.isTypeLiteralNode(node) || ts.isInterfaceDeclaration(node)) {
    // A member-only interface walk MUST NOT silently omit inherited properties.
    assert.ok(!node.heritageClauses?.length, 'inherited fields require explicit resolution');
    return { kind: 'object', fields: node.members.map(m => {
      assert.ok(ts.isPropertySignature(m) && m.type && ts.isIdentifier(m.name), 'unsupported member refused');
      return { name: m.name.text, optional: !!m.questionToken, shape: shape(m.type, resolving) };
    }) };
  }
  throw new Error(`unsupported AST ${ts.SyntaxKind[node.kind]}`);
}
function declaration(n, resolving) { return shape(ts.isTypeAliasDeclaration(n) ? n.type : n, resolving); }
const inherited = ts.createSourceFile('control.ts', 'interface Child extends Parent { own: string }',99,true).statements[0];
assert.throws(() => shape(inherited), /inherited fields require explicit resolution/, 'inherited_field_control');
const schemas = Object.fromEntries([...declarations].map(([name,n]) => [name,declaration(n,new Set([name]))]));
function execute(code, requireFn = () => { throw Error('unexpected import'); }) {
  const module = { exports: {} };
  vm.runInNewContext(ts.transpileModule(code, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, {module,exports:module.exports,require:requireFn});
  return module.exports;
}
const shared = ts.createSourceFile('shared.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8'),99,true);
const enumSource = shared.statements.filter(n => ts.isEnumDeclaration(n) && ['OrderSide','OrderType'].includes(n.name.text)).map(n=>n.getText(shared)).join('\n');
assert.equal(shared.statements.filter(n => ts.isEnumDeclaration(n) && ['OrderSide','OrderType'].includes(n.name.text)).length,2,'actual shared enum census');
const actualEnums = execute(enumSource);
const actual = execute(text, name => { assert.equal(name,'../../types'); return actualEnums; });
function fixture(s, p, minimal=false) {
  switch(s.kind) {
    case 'string': return `field:${p}: é😀 e\u0301 `;
    case 'number': return 101.25 + [...p].reduce((a,c)=>a+c.charCodeAt(0),0);
    case 'boolean': return true;
    case 'enum': return s.values[0];
    case 'tuple': return s.items.map((v,i)=>fixture(v,`${p}.${i}`,minimal));
    case 'array': return [fixture(s.item,`${p}.0`,minimal),fixture(s.item,`${p}.1`,minimal)];
    case 'object': return Object.fromEntries(s.fields.filter(f=>!minimal||!f.optional).map(f=>[f.name,fixture(f.shape,`${p}.${f.name}`,minimal)]));
    default: throw Error(s.kind);
  }
}
const fixtures = Object.fromEntries(Object.entries(schemas).map(([n,s])=>[n,fixture(s,n)]));
// The values are typechecked against the ACTUAL declarations, then that exact
// typed TypeScript witness is executed. No substitute response validator runs.
const probe = enumSource+'\n'+text.replace(/^import .*?;\s*/,'')+'\n'+Object.entries(fixtures).map(([n,v])=>`export const witness_${n} = (value: ${n}): ${n} => value;\nexport const value_${n}: ${n} = witness_${n}(${JSON.stringify(v)});`).join('\n');
const options = {types:[], strict:true, target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS};
const host = ts.createCompilerHost(options);
const defaultGet = host.getSourceFile.bind(host);
host.getSourceFile = (name, languageVersion, onError) => name==='probe.ts'?ts.createSourceFile(name,probe,99,true):defaultGet(name,languageVersion,onError);
const program = ts.createProgram(['probe.ts'], options, host);
assert.deepEqual(ts.getPreEmitDiagnostics(program).map(d=>ts.flattenDiagnosticMessageText(d.messageText,' ')), [], 'actual_declaration_typecheck');
const witnesses=execute(probe);
function bits(n) { const b=Buffer.alloc(8); b.writeDoubleBE(Number.isNaN(n)?NaN:n); return Number.isNaN(n)?'7ff8000000000000':b.toString('hex'); }
function observe(s,v,p='',out={}) {
  if(v===undefined){out[p]={type:'undefined'};return out;}
  if(s.kind==='object')for(const f of s.fields)observe(f.shape,v[f.name],p?`${p}.${f.name}`:f.name,out);
  else if(s.kind==='array')v.forEach((x,i)=>observe(s.item,x,`${p}.${i}`,out));
  else if(s.kind==='tuple')s.items.forEach((x,i)=>observe(x,v[i],`${p}.${i}`,out));
  else out[p]=s.kind==='number'?{type:typeof v,bits:bits(v)}:{type:typeof v,value:v};
  return out;
}
function paths(s,v,p=[]) {
  if(s.kind==='object')return s.fields.flatMap(f=>[{path:[...p,f.name],shape:f.shape,optional:f.optional},...paths(f.shape,v[f.name],[...p,f.name])]);
  if(s.kind==='array')return v.flatMap((x,i)=>[{path:[...p,i],shape:s.item,optional:false},...paths(s.item,x,[...p,i])]);
  if(s.kind==='tuple')return s.items.flatMap((x,i)=>[{path:[...p,i],shape:x,optional:false},...paths(x,v[i],[...p,i])]);
  return [];
}
function changed(v,p,value,remove=false){v=structuredClone(v);let o=v;for(const k of p.slice(0,-1))o=o[k];if(remove)delete o[p.at(-1)];else o[p.at(-1)]=value;return v;}
const records=[];
const numberTokens=['-0','-0.0','-1e-9999','1e400','-1e400','9007199254740993','1.0000000000000002'];
for(const [name,s] of Object.entries(schemas)){
 const value=fixtures[name], cases=[];
 const push=(label,raw)=>{const v=witnesses[`witness_${name}`](JSON.parse(raw));cases.push({label,raw,observed:observe(s,v),serialized:JSON.parse(JSON.stringify(v))});};
 assert.equal(JSON.stringify(witnesses[`value_${name}`]),JSON.stringify(value),'executed_typed_fixture');
 push('all_fields',JSON.stringify(value));push('minimal',JSON.stringify(fixture(s,name,true)));
 const invalid=[];
 if(s.kind==='enum'){
  for(const v of s.values)push(`literal:${v}`,JSON.stringify(v));
  for(const v of [null,1,true,[],{},'unknown',...s.values.map(v=>({[v]:null}))])invalid.push(JSON.stringify(v));
 } else {
  invalid.push('[]','null','1','"record"');
  for(const entry of paths(s,value)){
   const {path:p,shape:f,optional}=entry;
   if(optional)push(`absent:${p.join('.')}`,JSON.stringify(changed(value,p,null,true)));
   else invalid.push(JSON.stringify(changed(value,p,null,true)));
   invalid.push(JSON.stringify(changed(value,p,null)));
   if(f.kind==='boolean'){push(`false:${p.join('.')}`,JSON.stringify(changed(value,p,false)));for(const x of [0,'false'])invalid.push(JSON.stringify(changed(value,p,x)));}
   if(f.kind==='string')for(const x of [0,true,[]])invalid.push(JSON.stringify(changed(value,p,x)));
   if(f.kind==='number')for(const token of numberTokens)push(`numeric:${p.join('.')}:${token}`,JSON.stringify(changed(value,p,'__TOKEN__')).replace('"__TOKEN__"',token));
   if(f.kind==='enum'){
    for(const literal of f.values)push(`literal:${p.join('.')}:${literal}`,JSON.stringify(changed(value,p,literal)));
    for(const v of ['unknown',1,{[f.values[0]]:null}])invalid.push(JSON.stringify(changed(value,p,v)));
   }
   if(f.kind==='object')invalid.push(JSON.stringify(changed(value,p,[])));
  }
 }
 const surrogate=[];
 for(const e of paths(s,value).filter(e=>e.shape.kind==='string')){
  const raw=JSON.stringify(changed(value,e.path,'__SURROGATE__')).replace('"__SURROGATE__"','"\\ud800"');
  const v=witnesses[`witness_${name}`](JSON.parse(raw));let leaf=v;for(const p of e.path)leaf=leaf[p];
  assert.equal(typeof leaf,'string','surrogate_typeof_before_stringify');assert.equal(leaf.charCodeAt(0),0xd800,'surrogate_code_unit');
  surrogate.push(raw);
 }
 records.push({name,cases,invalid,surrogate});
}
const keys=[...Object.keys(actual.ORDER_TYPE),...Object.keys(actual.ORDER_SIDE),'','unknown','market','STOP_MARKET ', 'é😀',...Object.getOwnPropertyNames(Object.prototype)];
const constants=Object.fromEntries(['RECV_WINDOW','BASE_URL','BASE_WS_URL','ENDPOINTS','ORDER_TYPE','ORDER_SIDE'].map(n=>[n,typeof actual[n]==='number'?{type:typeof actual[n],bits:bits(actual[n])}:Object.entries(actual[n]).map(([k,v])=>[k,typeof v==='object'?Object.entries(v):v])]));
const lookups=Object.fromEntries(['ORDER_TYPE','ORDER_SIDE'].map(n=>[n,keys.map(k=>{const v=actual[n][k];return {key:k,type:typeof v,string:String(v),own:Object.hasOwn(actual[n],k)};})]));
const predicates=keys.map(key=>({key,trigger:actual.isParadexTriggerOrderType(key),market:actual.isParadexMarketOrderType(key)}));
const lone = JSON.parse('"\\ud800"');
assert.equal(lone.charCodeAt(0),0xd800,'predicate_surrogate_code_unit');
const surrogatePredicates = {trigger:actual.isParadexTriggerOrderType(lone),market:actual.isParadexMarketOrderType(lone)};
process.stdout.write(JSON.stringify({header:{arch:process.arch,platform:process.platform,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),v8:process.versions.v8},constants,lookups,predicates,records,surrogatePredicates}));
