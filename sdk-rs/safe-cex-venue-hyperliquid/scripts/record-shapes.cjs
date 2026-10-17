const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts'),'utf8');
const ast = ts.createSourceFile('hyperliquid.types.ts',source,ts.ScriptTarget.Latest,true);
require('./interface-heritage.cjs')(ts,ast);
const oracle = ts.createSourceFile('oracle.cjs',fs.readFileSync(path.join(__dirname,'oracle.cjs'),'utf8'),ts.ScriptTarget.Latest,true,ts.ScriptKind.JS);
const funcs = oracle.statements.filter(n=>ts.isFunctionDeclaration(n)&&['sample','members'].includes(n.name.text));
if(funcs.length!==2) throw Error('source sampler unavailable');
const samplerCode=funcs.map(n=>n.getText(oracle)).join('\n')+'\n({sample,members})';
const sampler = vm.runInNewContext(samplerCode,{ts,ast,selections:new Map()});
const records = new Map();
const pascal=s=>s.replace(/(^|[_:-])(\w)/g,(_,a,b)=>b.toUpperCase());
function shape(name,members) {
  if(members.length===1 && ts.isIndexSignatureDeclaration(members[0])) return;
  const data=sampler.members(members,[]);
  records.set(name,{name,raw:JSON.stringify(data),positional:JSON.stringify(Object.values(data))});
  for(const m of members.filter(ts.isPropertySignature)) visit(m.type,name+pascal(m.name.text));
}
function visit(n,name) {
 if(ts.isParenthesizedTypeNode(n)) return visit(n.type,name);
 if(ts.isTypeLiteralNode(n)) return shape(name,n.members);
 if(ts.isArrayTypeNode(n)) return visit(n.elementType,name+'Item');
 if(ts.isTupleTypeNode(n)) return n.elements.forEach((e,i)=>visit(e,name+'Item'+i));
 if(ts.isUnionTypeNode(n)) {
  const values=n.types.filter(t=>t.getText(ast)!=='null');
  if(values.length<n.types.length) {if(values.length===1)return visit(values[0],name);return visit(ts.factory.createUnionTypeNode(values),name);}
  if(values.every(t=>ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal)))return;
  values.forEach((t,i)=>visit(t,name+'Case'+i));
 }
}
const notModeled=require('./not-modeled-types.cjs');
for(const n of ast.statements) {
 if((ts.isTypeAliasDeclaration(n)||ts.isInterfaceDeclaration(n))&&notModeled.has(n.name.text)) continue;
 if(ts.isTypeAliasDeclaration(n)) visit(n.type,n.name.text);
 if(ts.isInterfaceDeclaration(n)) {
  const peers=ast.statements.filter(p=>ts.isInterfaceDeclaration(p)&&p.name.text===n.name.text), merged=new Map();
  for(const p of peers)for(const m of p.members)merged.set(m.name?.text??m.getText(ast),m);
  shape(n.name.text,[...merged.values()]);
 }
}
const signingSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.signing.ts'),'utf8');
const signingAst=ts.createSourceFile('hyperliquid.signing.ts',signingSource,ts.ScriptTarget.Latest,true);
require('./interface-heritage.cjs')(ts,signingAst);
// Use the actual signing source for all three handwritten split response records.
const cancel=signingAst.statements.find(n=>ts.isInterfaceDeclaration(n)&&n.name.text==='CancelOrderResponse');
if(!cancel)throw Error('CancelOrderResponse source missing');
const signingSampler=vm.runInNewContext(samplerCode,{ts,ast:signingAst,selections:new Map()});
function signingShape(name,members) {
 const data=signingSampler.members(members,[]);
 records.set(name,{name,raw:JSON.stringify(data),positional:JSON.stringify(Object.values(data))});
}
signingShape('CancelOrderResponse',cancel.members);
const response=cancel.members.find(m=>m.name.text==='response').type;
signingShape('CancelOrderResponseBody',response.members);
signingShape('CancelOrderResponseData',response.members.find(m=>m.name.text==='data').type.members);
const corpus=JSON.stringify([...records.values()],null,2)+'\n';
const fixture=path.join(__dirname,'../tests/fixtures/record-shapes.json');
if(process.argv.includes('--capture'))fs.writeFileSync(fixture,corpus,{flag:'wx'});
else if(fs.readFileSync(fixture,'utf8')!==corpus)throw Error('live record-shape oracle drift');
