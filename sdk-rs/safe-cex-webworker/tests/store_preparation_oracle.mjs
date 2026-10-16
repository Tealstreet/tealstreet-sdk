// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import {records as identities} from './task_identity_oracle.mjs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
const worker=ast('webworker/ExchangeWorker.ts'),store=ast('store/store.base.ts');
const cls=worker.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='ExchangeWorker');
const names=['_sanitizeForPostMessage','_shouldSendUpdateIdBackedSnapshot','_recordSentUpdateIdBackedSnapshot','_copyTickerBaseline','_packTickerDeltasSinceBaseline','_prepareStorePayload'];
const methods=names.map(name=>{const member=cls.members.find(m=>m.name?.getText(worker)===name);if(!member)throw Error(name);return member.getText(worker);});
const decl=name=>store.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(store)===name)).getText(store).replace('export ','');
const stateNames=['_lastSentTickerBaseline','_lastSentOrdersUpdateId','_lastSentPositionsUpdateId','_lastSentMarketsVersion','_lastSentTickersVersion','_lastPayloadHadActiveChasers'];
const state=stateNames.map(name=>{const member=cls.members.find(m=>m.name?.getText(worker)===name);if(!member)throw Error(name);return member.getText(worker);}).join('\n');
const context=vm.createContext({});vm.runInContext(ts.transpileModule(decl('PACKED_TICKER_FIELDS')+'\n'+decl('defaultStore')+'\nclass Worker { '+state+'\n'+methods.join('\n')+' }\nglobalThis.Worker=Worker;globalThis.defaultStore=defaultStore;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const position={id:'p',symbol:'BTC',side:'long',entryPrice:100,contracts:2,notional:200,leverage:5,unrealizedPnl:0,liquidationPrice:0};
const order={id:'o',symbol:'BTC',status:'open',type:'limit',side:'buy',price:100,amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:0};
const ticker={id:'BTC',symbol:'BTC',bid:99,ask:101,last:100,mark:100,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const market={id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
const chaser={id:'c',symbol:'BTC',size:2,side:'buy',maxAttempts:7,attempts:3,isStopped:false,stopReason:'',currentOrderIds:['o']};
function base(info){return {...structuredClone(context.defaultStore),latency:7,orders:[{...order,info},{...order,id:'other',info:false}],positions:[{...position,info}],displayedPositions:[{...position,account:'a',info}],tickers:[{...ticker,info:{keep:'ticker'}}],markets:[{...market,info:{keep:'market'}}],starkConfig:{positionId:1},zkLinkConfig:{accountId:2},userData:{id:'account'}};}
const infos=[undefined,null,false,0,'',true,1,'raw',[],{},[1],{value:1}];
const sanitize=[];
for(const info of infos)for(const omitOrders of [false,true])for(const omitPositions of [false,true]){
 const data=base(info),worker=new context.Worker(),expected=worker._sanitizeForPostMessage(data,{omitOrders,omitPositions});
 sanitize.push({input:structuredClone(data),omitOrders,omitPositions,expected});
}
const scenarios=[];
for(const hostedMode of [false,true])for(const force of [undefined,false,true])for(const info of infos){
 const worker=new context.Worker();worker._options={hostedMode};let marketsVersion=0,tickersVersion=0;worker._store={getMarketsVersion:()=>marketsVersion,getTickersVersion:()=>tickersVersion};
 let data=base(info);const steps=[];
 const take=()=>steps.push({input:structuredClone(data),marketsVersion,tickersVersion,hostedMode:worker._options.hostedMode,force,expected:structuredClone(worker._prepareStorePayload(data,{forceFullStore:force}))});
 take();take();data.tickers[0].bid=98;take();data.orders[0].amount=3;take();data.activeChasers=[chaser];take();data.activeChasers=[{...chaser,attempts:4}];take();data.activeChasers=[];take();take();data.positions[0].contracts=3;data.updateIds.positions++;take();data.updateIds.orders++;take();marketsVersion++;take();tickersVersion++;take();data.tickers[0].id='new';take();data.tickers=[];take();worker._options.hostedMode=!hostedMode;data.tickers=[{...ticker,bid:97}];take();data.tickers[0].bid=96;take();worker._options.hostedMode=hostedMode;take();
 scenarios.push({steps});
}
const original=base({raw:true}),held=new context.Worker()._sanitizeForPostMessage(original);
original.updateIds.orders=99;original.loaded.orders=true;original.socketHealth.publicHealthy=true;original.balance.total=99;original.orders[1].amount=99;
const aliasWitness={updateId:held.updateIds.orders,loaded:held.loaded.orders,socketHealthy:held.socketHealth.publicHealthy,balance:held.balance.total,orderAmount:held.orders[1].amount};
const fieldIdentity=identities.TwapOptions.cases.filter(c=>c.path.startsWith('/activeMarket/')).map(c=>{
 const data=base(undefined);
 data.balance={...data.balance,used:1.25,free:2.5,total:3.75,upnl:5};
 data.markets=[structuredClone(c.input.activeMarket)];
 data.tickers=[{...ticker,id:'ticker-id',symbol:'ticker-symbol',bid:11,ask:12,last:13,mark:14,index:15,percentage:16,openInterest:17,fundingRate:18,fundingTime:19,volume:20,quoteVolume:21}];
 data.positions=[{...position,id:'position-id',symbol:'position-symbol',entryPrice:31,contracts:32,notional:33,leverage:34,unrealizedPnl:35,liquidationPrice:36}];
 data.orders=[{...order,id:'order-id',symbol:'order-symbol',price:41,amount:42,filled:43,remaining:44,lastUpdate:45}];
 data.displayedPositions=[{...data.positions[0],account:'display-account'}];
 const input=structuredClone(data),worker=new context.Worker();
 const sanitized=structuredClone(worker._sanitizeForPostMessage(data));
 worker._options={hostedMode:false};worker._store={getMarketsVersion:()=>1,getTickersVersion:()=>2};
 const prepared=structuredClone(worker._prepareStorePayload(data,{forceFullStore:true}));
 return {path:c.path,input,sanitized,prepared};
});
process.stdout.write(JSON.stringify({sanitize,scenarios,aliasWitness,fieldIdentity}));
