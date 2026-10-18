const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),filename=path.join(root,'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts');
const source=fs.readFileSync(filename,'utf8'),ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
require('./interface-heritage.cjs')(ts,ast);
const definitions=new Map();for(const s of ast.statements)if(ts.isTypeAliasDeclaration(s)||ts.isInterfaceDeclaration(s)){const a=definitions.get(s.name.text)||[];a.push(s);definitions.set(s.name.text,a);}
const bases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/fixtures/types.json'),'utf8')).cases;
function nullable(n,seen=new Set()) {
 if(ts.isParenthesizedTypeNode(n))return nullable(n.type,seen);
 if(n.kind===ts.SyntaxKind.AnyKeyword||n.kind===ts.SyntaxKind.UnknownKeyword)return true;
 if(n.getText(ast)==='null')return true;
 if(ts.isUnionTypeNode(n))return n.types.some(t=>nullable(t,seen));
 if(ts.isTypeReferenceNode(n)){const name=(n.typeName.text ?? n.typeName.getText(ast));if(seen.has(name))return false;return (definitions.get(name)||[]).some(d=>d.type&&nullable(d.type,new Set([...seen,name])));}
 return false;
}
const cases=[],keys=new Set();
function add(name,input,steps,kind) {
 const changed=JSON.parse(JSON.stringify(input));let parent=changed;for(const k of steps.slice(0,-1))parent=parent[k];
 if(kind==='optional-null')parent[steps.at(-1)]=null;else delete parent[steps.at(-1)];
 const raw=JSON.stringify(changed),id=name+'\n'+raw;if(keys.has(id))return;keys.add(id);cases.push({name,path:'/'+steps.join('/'),kind,raw});
}
function walk(n,value,name,input,steps=[],seen=new Set()) {
 if(!n||value===null||value===undefined)return;
 if(ts.isParenthesizedTypeNode(n))return walk(n.type,value,name,input,steps,seen);
 if(ts.isTypeReferenceNode(n)){const id=(n.typeName.text ?? n.typeName.getText(ast));if(id==='Array'){if(Array.isArray(value))value.forEach((v,i)=>walk(n.typeArguments[0],v,name,input,[...steps,String(i)],seen));return;}if(seen.has(id))return;for(const d of definitions.get(id)||[])if(d.type)walk(d.type,value,name,input,steps,new Set([...seen,id]));else members(d.members,value,name,input,steps,new Set([...seen,id]));return;}
 if(ts.isUnionTypeNode(n))return n.types.forEach(t=>walk(t,value,name,input,steps,seen));
 if(ts.isTypeLiteralNode(n))return members(n.members,value,name,input,steps,seen);
 if(ts.isArrayTypeNode(n)&&Array.isArray(value))return value.forEach((v,i)=>walk(n.elementType,v,name,input,[...steps,String(i)],seen));
 if(ts.isTupleTypeNode(n)&&Array.isArray(value))return n.elements.forEach((t,i)=>walk(t,value[i],name,input,[...steps,String(i)],seen));
}
function members(ms,value,name,input,steps,seen) {
 if(!value||Array.isArray(value)||typeof value!=='object')return;
 for(const m of ms.filter(ts.isPropertySignature)) {
  const key=m.name.text;if(!Object.hasOwn(value,key))continue;
  if(m.questionToken&&!nullable(m.type))add(name,input,[...steps,key],'optional-null');
  if(!m.questionToken&&nullable(m.type)&&m.type.kind!==ts.SyntaxKind.AnyKeyword)add(name,input,[...steps,key],'required-nullable-missing');
  walk(m.type,value[key],name,input,[...steps,key],seen);
 }
}
for(const b of bases)if(b.name!=='WsUserEvent'){const input=JSON.parse(b.raw);walk(ts.factory.createTypeReferenceNode(b.name),input,b.name,input);}
// Execute the compiler against the real declarations, rather than asserting our
// AST classification is right. Contextual annotations keep literals narrow.
const checkFile=path.join(root,'sdk-rs/safe-cex-venue-hyperliquid/tests/null-boundary-probes.ts');
const positives=bases.filter(b=>b.name!=='WsUserEvent');
const text='import type * as Source from "../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types";\n'+positives.map((b,i)=>`const positive${i}: Source.${b.name} = ${b.raw};`).join('\n')+'\n'+cases.map((c,i)=>`const probe${i}: ${process.argv.includes('--control-widened-context')?'any':'Source.'+c.name} = ${c.raw};`).join('\n');
const host=ts.createCompilerHost({});const original=host.readFile.bind(host);host.readFile=f=>f===checkFile?text:original(f);
const originalExists=host.fileExists.bind(host);host.fileExists=f=>f===checkFile||originalExists(f);
const program=ts.createProgram([filename,checkFile],{strict:true,noEmit:true,skipLibCheck:true,types:[],target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},host);
const diagnostics=ts.getPreEmitDiagnostics(program).filter(d=>d.file?.fileName===checkFile);
assert.ok(!diagnostics.some(d=>d.code===2307),'source import must resolve');
const probeAst=program.getSourceFile(checkFile), all=probeAst.statements.filter(ts.isVariableStatement);
const positiveDeclarations=all.slice(0,positives.length);
for(const d of positiveDeclarations)assert.ok(!diagnostics.some(e=>e.start>=d.pos&&e.start<d.end),'actual source positive must typecheck before rejection evidence '+d.getText(probeAst));
const declarations=all.slice(positives.length);
assert.equal(declarations.length,cases.length);
for(let i=0;i<cases.length;i++){const d=declarations[i],errors=diagnostics.filter(e=>e.start>=d.pos&&e.start<d.end);cases[i].sourceAccepts=errors.length===0;cases[i].sourceCodes=errors.map(e=>e.code);}
assert.ok(cases.some(c=>c.name==='OrderType'&&c.kind==='optional-null'&&!c.sourceAccepts),'OrderType optional null must be measured rejected');
const result=JSON.stringify({source:'actual TypeScript checker, strict null checks, contextual declared-source annotation',cases},null,2)+'\n',target=path.join(__dirname,'../tests/fixtures/typed-null-boundaries.json');
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'typed null accepted-set source drift');
console.log(`${cases.length} actual-source null/missing boundary cases; ${cases.filter(c=>!c.sourceAccepts).length} rejected`);
