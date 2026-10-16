// Executed abstract-base constructors and concrete override dispatch; no worker pipeline is invoked.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/webworker/postProcessors/',import.meta.url);
const context=vm.createContext({});
const declarations=[];
for(const name of ['BasePostprocessor','ExchangeDependentPostprocessor']){
 const path=new URL(name+'.ts',root),ast=ts.createSourceFile(path.pathname,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===name);assert.ok(cls);
 const ctor=cls.members.find(ts.isConstructorDeclaration),method=cls.members.find(n=>ts.isMethodDeclaration(n)&&n.name.getText(ast)==='process');
 assert.ok(cls.modifiers.some(m=>m.kind===ts.SyntaxKind.AbstractKeyword));
 assert.ok(method.modifiers.some(m=>m.kind===ts.SyntaxKind.AbstractKeyword)&&!method.body);
 assert.equal(ctor.parameters.length,name==='BasePostprocessor'?3:4,'CONTEXT source constructor arity');
 const expected=['_accountName','_exchangeName','_options',...(name==='BasePostprocessor'?[]:['_exchange'])];
 const assignments=ctor.body.statements.map(n=>n.expression.left.name.text);
 assert.deepEqual(assignments,expected,'CONTEXT source stored-field inventory');
 declarations.push({name,arity:ctor.parameters.length,assignments,abstractProcess:true});
 const source=ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
 vm.runInContext(ts.transpileModule(source+'\nglobalThis.'+name+'='+name+';', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
}
vm.runInContext(`
class Independent extends BasePostprocessor { process(store) { store.value=this._options.buy;store.name=this._accountName; } }
class Dependent extends ExchangeDependentPostprocessor { process(store) { store.value=this._options.buy;store.exchange=this._exchange; } }
globalThis.Independent=Independent;globalThis.Dependent=Dependent;`,context);
const bytes=new DataView(new ArrayBuffer(8));
function bits(n){bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
function units(s){return Array.from({length:s.length},(_,i)=>s.charCodeAt(i));}
const names=['account','é🪙','',JSON.parse('"\\ud800"')],rows=[];
for(const kind of ['independent','dependent'])for(let index=0;index<4;index++){
 const options=JSON.parse('{"buy":1.25,"sell":7.5}'),exchange={marker:53};
 const args=[names[index],'bybit',options,...(kind==='dependent'?[exchange]:[])];
 const processor=new context[kind==='independent'?'Independent':'Dependent'](...args);
 const before={account:units(processor._accountName),exchangeName:processor._exchangeName,buy:bits(processor._options.buy),sell:bits(processor._options.sell),optionsSame:processor._options===options,exchangeSame:kind==='dependent'?processor._exchange===exchange:null};
 options.buy=-0;options.sell=99;exchange.marker=71;
 const store={};assert.equal(processor.process(store),undefined,'CONTEXT source void override');
 rows.push({kind,index,before,after:{buy:bits(processor._options.buy),sell:bits(processor._options.sell),processed:bits(store.value),exchangeMarker:kind==='dependent'?bits(processor._exchange.marker):null},processSame:kind==='dependent'?store.exchange===exchange:store.name===names[index]});
}
assert.equal(rows.length,8,'CONTEXT source case floor');
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),declarations,rows}));
