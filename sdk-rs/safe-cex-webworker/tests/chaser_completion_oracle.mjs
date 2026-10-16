// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const text=fs.readFileSync(new URL(path,root),'utf8'),ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','PositionSide','OrderStatus'].includes(n.name.text)).map(n=>n.getText(types).replace('export ','')).join('\n');
const context=vm.createContext({Date:{now:()=>0},nanoid:()=> 'id',structuredClone});
vm.runInContext(ts.transpileModule(enums+'\n'+source('utils/clone.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderInstance.ts')+'\n'+source('webworker/chaserOrder/ChaserOrderManager.ts')+'\nglobalThis.Instance=ChaserOrderInstance;globalThis.Manager=ChaserOrderManager;globalThis.statuses=Object.values(OrderStatus);', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const order=(id,status)=>({id,status,symbol:'BTC',type:'limit',side:'buy',price:100,amount:3,filled:1,remaining:2,reduceOnly:false,lastUpdate:0,extra:{remainingNotional:200}});
const position=(symbol,side,notional)=>({id:'p',symbol,side,entryPrice:100,notional,leverage:1,unrealizedPnl:0,contracts:1,liquidationPrice:0});
const books=[[],...context.statuses.map(status=>[order('a',status)]),[order('b','closed'),order('a','closed')],[order('a','closed'),order('a','open')],[order('constructor','closed'),order('__proto__','closed')],[order('𐐀','closed'),order(' α ','closed')]];
const sets=[[],['missing'],['a'],['b','a','a'],['constructor','__proto__'],['𐐀',' α ']];
const positions=[[],...[0,-0,1,NaN,Infinity].flatMap(n=>['long','short'].map(side=>[position('BTC',side,n)])),[position('OTHER','long',1)],[position('BTC','long',0),position('BTC','long',1)]];
const encode=v=>typeof v==='number'?(Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':Object.is(v,-0)?'-0':v):Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;
function scenario(side,reduceOnly,orders,positions,ids,stopped=false,reentrant=false){
 const events=[],exchange={name:'bybitdemo',store:{orders:structuredClone(orders),positions:structuredClone(positions)},emitter:{emit:(event,message)=>{events.push({event,message});if(reentrant)exchange.store.orders[0].status='closed';}}};
 const instance=new context.Instance(exchange,{symbol:'BTC',size:3,side,maxAttempts:1,delay:100,reduceOnly}),manager=new context.Manager();instance.currentOrderIds=[...ids];if(stopped)instance.markComplete('prior');
 const selection=structuredClone(manager.getCurrentOrders(instance));manager.checkAndMarkIfComplete(instance);
 return encode({side,reduceOnly,orders,positions,ids,stopped,reentrant,selection,expected:{ids:instance.currentOrderIds,stopped:instance.isStopped,reason:instance.state.stopReason,events}});
}
const rows=[];for(const side of ['buy','sell'])for(const reduceOnly of [false,true])for(const orders of books)for(const ps of positions)for(const ids of sets)for(const stopped of [false,true])rows.push(scenario(side,reduceOnly,orders,ps,ids,stopped));
const reentrant=scenario('buy',false,[order('a','open')],[],['a'],false,true);
const aliased=order('a','open'),exchange={store:{orders:[aliased]},emitter:{emit(){}}},instance={exchange,currentOrderIds:['a']},manager=new context.Manager();const selected=manager.getCurrentOrders(instance);aliased.status='closed';selected[0].price=42;
const logs=[];for(const id of ['id','','𐐀'])for(const message of ['', 'x', ' α\n', '𐐀'])manager.log(message,{id,exchange:{emitter:{emit:(event,message)=>logs.push({id,event,message})}}});
const surrogate='\uD800';let surrogateOutput;manager.log(surrogate,{id:'id',exchange:{emitter:{emit:(_,message)=>{surrogateOutput=message;}}}});
const surrogateCase=scenario('buy',false,[order(surrogate,'closed')],[],[surrogate]);
process.stdout.write(JSON.stringify({surrogateCaseInput:JSON.stringify({orders:surrogateCase.orders,ids:surrogateCase.ids}),surrogateSelection:JSON.stringify(surrogateCase.selection),surrogateCompletion:JSON.stringify(surrogateCase.expected),rows,reentrant,alias:{selectedStatus:selected[0].status,storePrice:aliased.price},logs,surrogateInput:JSON.stringify(surrogate),surrogateOutput:JSON.stringify(surrogateOutput)}));
