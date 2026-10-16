// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const text=fs.readFileSync(new URL(path,root),'utf8'),ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','PositionSide','OrderStatus'].includes(n.name.text)).map(n=>n.getText(types).replace('export ','')).join('\n');
const context=vm.createContext({Date:{now:()=>0},nanoid:()=> 'id',structuredClone});
vm.runInContext(ts.transpileModule(enums+'\n'+source('utils/clone.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderInstance.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderManager.ts')+'\nglobalThis.Instance=ChaserOrderInstance;globalThis.Manager=ChaserOrderManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
function fixture(mode='missing',listenerThrows=false){
 const events=[],fetches=[],cancels=[];let disposed=false;
 const known={id:'known',symbol:'BTC',status:'open',side:'buy',type:'limit',price:100,amount:3,filled:1,remaining:2,reduceOnly:false,lastUpdate:0};
 const exchange={name:'bybitdemo',store:{orders:[known],positions:[]},emitter:{emit(event,payload){events.push({event,payload});if(event==='chaser:complete'&&listenerThrows)throw new Error('listener fixture');}}};
 const manager=new context.Manager(),instance=new context.Instance(exchange,{symbol:'BTC',size:3,side:'buy',maxAttempts:1,delay:100,clientId:'client'});
 instance.currentOrderIds=['absent','known'];instance.registerDisposer(()=>{disposed=true;});manager._chaserOrderInstances.push(instance);
 exchange.store.positions=[{symbol:'BTC',side:'long',contracts:2}];
 exchange.cancelOrders=async orders=>{cancels.push({orders:structuredClone(orders),ids:[...instance.currentOrderIds],managerCount:manager.instances.length,disposed});};
 if(mode==='noncallable')exchange.fetchFillsForOrderIds=7;
 if(mode==='ok'||mode==='error'||mode==='empty')exchange.fetchFillsForOrderIds=async function(ids){fetches.push({ids:[...ids],bound:this===exchange});if(mode==='error')throw new Error('reconcile fixture');return mode==='empty'?new Map():new Map([['known',1.5]]);};
 return {manager,instance,exchange,events,fetches,cancels};
}
const rows=[];
for(const mode of ['missing','noncallable','ok','error','empty'])for(const listenerThrows of [false,true]){
 const f=fixture(mode,listenerThrows);await f.manager.cleanupInstance(f.instance);
 rows.push({mode,listenerThrows,cancels:f.cancels,fetches:f.fetches,events:f.events,completionClaimedAgain:f.instance.claimCompleteEmission(),ids:f.instance.currentOrderIds,managerCount:f.manager.instances.length});
}
process.stdout.write(JSON.stringify(rows));
