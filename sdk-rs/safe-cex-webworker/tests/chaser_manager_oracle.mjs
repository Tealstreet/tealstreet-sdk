// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const text=fs.readFileSync(new URL(path,root),'utf8'),ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','PositionSide'].includes(n.name.text)).map(n=>n.getText(types).replace('export ','')).join('\n');
const context=vm.createContext({Date:{now:()=>0},nanoid:()=> 'id',structuredClone});
vm.runInContext(ts.transpileModule(enums+'\n'+source('utils/clone.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderInstance.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderManager.ts')+'\nglobalThis.Instance=ChaserOrderInstance;globalThis.Manager=ChaserOrderManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const encode=v=>typeof v==='number'?(Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
const rows=[];
for(const name of ['bybitdemo','bybit'])for(const side of ['buy','sell'])for(const reduceOnly of [false,true])for(const initial of [undefined,-3,0,NaN,Infinity,-Infinity])for(const size of [-0,1,NaN,Infinity]){
 const targetSide=side==='buy'?'long':'short';
 const positions=contracts=>[{symbol:'OTHER',side:targetSide,contracts:999},{symbol:'BTC',side:targetSide==='long'?'short':'long',contracts:999},...(contracts===undefined?[]:[{symbol:'BTC',side:targetSide,contracts},{symbol:'BTC',side:targetSide,contracts:999}])];
 const exchange={name,store:{positions:positions(initial)}},options={symbol:'BTC',size,side,maxAttempts:1,delay:100,reduceOnly};
 const instance=new context.Instance(exchange,options),manager=new context.Manager();
 const steps=[];
 for(const contracts of [undefined,-0,-3,0,1,3,4,instance.targetPositionContracts-5e-13,instance.targetPositionContracts-2e-12,instance.targetPositionContracts,NaN,Infinity,-Infinity])for(const currentSize of [size,3,-0]){
  exchange.store.positions=positions(contracts);options.size=currentSize;
  steps.push({contracts,currentSize,reached:manager.hasReachedTargetPosition(instance),filled:manager.bybitDemoPositionFill(instance)});
 }
 rows.push({name,side,reduceOnly,initial,size,steps});
}
process.stdout.write(JSON.stringify(encode(rows)));
