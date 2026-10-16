import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
const parse = file => ts.createSourceFile(file,readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/'+file,import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const source=parse('bybit.exchange.ts'), tables=parse('bybit.types.ts');
const names=['usesBybitDemoEnvironment','getBybitRestBaseUrl','getBybitPublicWsUrl','getBybitPrivateWsUrl'];
const owner=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitExchange');
const methods=owner.members.filter(n=>names.includes(n.name?.getText(source)));
assert.equal(methods.length,4);
const declarations=tables.statements.filter(ts.isVariableStatement).flatMap(n=>n.declarationList.declarations).filter(n=>['BASE_URL','DEMO_URL','BASE_WS_URL','DEMO_WS_URL'].includes(n.name.getText(tables)));
assert.equal(declarations.length,4);
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(declarations.map(n=>'const '+n.getText(tables)+';').join('\n')+'\nclass Subject {'+methods.map(n=>n.getText(source)).join('\n')+'};globalThis.subject=new Subject();',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const result=[];
for(const name of ['bybit','bybitdemo','bybitinverse','bybitspot'])for(const isDemo of [false,true])for(const network of ['livenet','testnet'])for(const category of ['linear','inverse']) {
 const s=context.subject;s.name=name;s.isDemo=isDemo;
 result.push({name,isDemo,network,category,demo:s.usesBybitDemoEnvironment(),rest:s.getBybitRestBaseUrl(network),public:s.getBybitPublicWsUrl(category,network),private:s.getBybitPrivateWsUrl(network)});
}
assert.equal(result.length,32);process.stdout.write(JSON.stringify(result));
