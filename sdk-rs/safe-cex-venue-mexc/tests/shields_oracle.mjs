import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const names=['markStopLikeOrderSeen','notePlacedStopLikeOrder','isRecentlyPlacedStopLikeOrder','noteRecentlyCanceledStopLikeOrders','isRecentlyCanceledStopLikeOrder','isRecentlyCanceledStopLikeOrderId','filterRecentlyCanceledStopLikeOrders','isRecentlyRejectedRegularOrder','isStopLikeOrder'];
const constants=['MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS','MEXC_RECENT_STOP_CANCEL_SHIELD_MS','MEXC_RECENT_REJECTED_ORDER_SHIELD_MS'];
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>constants.includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(names.includes(member.name?.getText(ast)))selected.push(`this.${member.name.getText(ast)}=${member.initializer.getText(ast)};`);
}
let input=selected.join('\n');
if(process.env.MEXC_TS_CONTROL==='shield-expiry'){
 const before='> MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS';
 if(!input.includes(before))throw Error('missing shield mutation anchor');
 input=input.replace(before,'>= MEXC_RECENT_STOP_LIKE_PLACE_SHIELD_MS');
}
const js=ts.transpileModule(input,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const traces=[];
for(const id of ['a', '','a__long','a__long__extra','__long','constructor','__proto__','😃'])for(const start of [0,100000]){
 let now=start;let clockCalls=0;let clockStep=0;
 // These are the venue's private Map fields, not a replacement Store fixture.
 const context=vm.createContext({...load('packages/safe-cex/src/types.ts'),virtualClock:{getCurrentTimeMs:()=>{clockCalls++;const value=now;now+=clockStep;return value;}},recentlyPlacedStopLikeOrderIds:new Map(),recentlyCanceledStopLikeOrderIds:new Map(),recentlyRejectedRegularOrderIds:new Map([[id.split('__')[0],start]])});
 vm.runInContext(js,context,{filename});
 const steps=[];
 const invoke=(name,args,time=now)=>{now=time;clockCalls=0;const output=context[name](...args);const state={};for(const [field,key]of [['recentlyPlacedStopLikeOrderIds','placed'],['recentlyCanceledStopLikeOrderIds','canceled'],['recentlyRejectedRegularOrderIds','rejected']])state[key]=Object.fromEntries([...context[field]].sort(([a],[b])=>a<b?-1:a>b?1:0));steps.push({name,args,time,clockStep,clockCalls,output:output===undefined?{$undefined:true}:output,state});};
 const order={id,type:'stop_market'};
 invoke('notePlacedStopLikeOrder',[id]);
 invoke('isRecentlyPlacedStopLikeOrder',[order],start-1);
 invoke('isRecentlyPlacedStopLikeOrder',[order],start+20000);
 invoke('isRecentlyPlacedStopLikeOrder',[order],start+20001);
 invoke('notePlacedStopLikeOrder',[id],start+21000);
 invoke('markStopLikeOrderSeen',[id]);
 invoke('isRecentlyPlacedStopLikeOrder',[order]);
 invoke('notePlacedStopLikeOrder',[id]);
 invoke('noteRecentlyCanceledStopLikeOrders',[[order]]);
 invoke('isRecentlyCanceledStopLikeOrderId',[id],start+36000);
 invoke('isRecentlyCanceledStopLikeOrder',[order]);
 invoke('filterRecentlyCanceledStopLikeOrders',[[order,{id,type:'limit'},{id:'unseen',type:'stop_market'}]]);
 invoke('isRecentlyCanceledStopLikeOrderId',[id],start+36001);
 invoke('isRecentlyRejectedRegularOrder',[id],start+30000);
 invoke('isRecentlyRejectedRegularOrder',[id],start+30001);
 invoke('noteRecentlyCanceledStopLikeOrders',[[]],start);
 invoke('isRecentlyPlacedStopLikeOrder',[{id:'missing',type:'stop_market'}]);
 invoke('noteRecentlyCanceledStopLikeOrders',[[{id:'first',type:'stop_market'},{id:'second',type:'stop_market'}]],start);
 clockStep=1;
 invoke('filterRecentlyCanceledStopLikeOrders',[[{id:'first',type:'stop_market'},{id:'unseen',type:'stop_market'},{id:'second',type:'stop_market'},{id:'first',type:'limit'}]],start+15000);
 traces.push({id,start,steps});
}
process.stdout.write(JSON.stringify(traces));
