import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';import {createRequire} from 'node:module';import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url),root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
function ast(relative){const file=path.join(root,relative);return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);}
function fields(relative,owner,names,label){const a=ast(relative),c=a.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===owner);return 'class '+label+' {'+names.map(name=>{const f=c.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(a)===name);assert(f?.initializer,'CANCEL actual '+owner+'.'+name);return name+'='+f.initializer.getText(a)+';';}).join('\n')+'}';}
const names=['cancelAllOrders','cancelOrders','cancelNormalOrders','cancelUtaNormalOrders','cancelAlgoOrders','cancelSymbolOrders'];
const bodies=fields('packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts','BitgetExchange',names,'Classic')+fields('packages/safe-cex/src/exchanges/bitget/bitgetinverse.exchange.ts','BitgetInverseExchange',['cancelAllOrders','cancelNormalOrders','cancelAlgoOrders'],'Inverse');
const types=ast('packages/safe-cex/src/exchanges/bitget/bitget.types.ts');const endpoints=types.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(types)==='ENDPOINTS');assert(endpoints?.initializer);
const omitAst=ast('packages/safe-cex/src/utils/omitUndefined.ts');const omit=omitAst.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(omitAst)==='omitUndefined');assert(omit?.initializer);
const storeAst=ast('packages/safe-cex/src/store/store.base.ts'),typesAst=ast('packages/safe-cex/src/types.ts');
const terminal=storeAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>['TERMINAL_STATUSES','isTerminalOrder'].includes(d.name.getText(storeAst))));assert.equal(terminal.length,2,'CANCEL actual terminal selector');
const statusEnum=typesAst.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderStatus').getText(typesAst).replace(/^export /,'');
const code=ts.transpileModule(statusEnum+';'+terminal.map(n=>n.getText(storeAst).replace(/^export /,'')).join('\n')+';const ENDPOINTS='+endpoints.initializer.getText(types)+';const omitUndefined='+omit.initializer.getText(omitAst)+';'+bodies,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const {Classic,Inverse}=new Function('partition','uniqBy','groupBy','chunk','omitBy',code+';return {Classic,Inverse};')(...['partition','uniqBy','groupBy','chunk','omitBy'].map(n=>require('lodash/'+n)));
const string=u=>String.fromCharCode(...u),units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const pack=v=>v===undefined?{undefined:true}:v===null?{null:true}:typeof v==='string'?{string:units(v)}:typeof v==='object'?{object:v.units}:{scalar:v};
const unpack=v=>v.undefined?undefined:v.null?null:v.string?string(v.string):v.object?{units:v.object}:v.scalar;
const mapping={'classic-all':'cancelAllOrders',partition:'cancelOrders',normal:'cancelNormalOrders','uta-normal':'cancelUtaNormalOrders',algo:'cancelAlgoOrders',symbol:'cancelSymbolOrders','inverse-all':'cancelAllOrders','inverse-normal':'cancelNormalOrders','inverse-algo':'cancelAlgoOrders'};
const cases=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(cases.length,88,'CANCEL independent input floor');
const outputs=[];
for(const c of cases){const trace=[],requests=[],delegates=[],errors=[];let primaryUsed=false,schedules=0;const primary={response:{data:{msg:'api failure'}},message:'fallback failure'},report={message:'report failure'};
 const orders=c.orders.map(o=>({...o,id:string(o.id),symbol:string(o.symbol),info:{orderId:unpack(o.orderId),clientOid:unpack(o.clientOid)}}));
 const extra={...c.extra,id:string(c.extra.id),symbol:string(c.extra.symbol),info:{orderId:unpack(c.extra.orderId),clientOid:unpack(c.extra.clientOid)}};
 if(c.mutation==='filter'){const originalSymbol=orders[0].symbol;Object.defineProperty(orders[0],'symbol',{get(){orders[1]=extra;return originalSymbol;},enumerable:true});}
 const target=c.op.startsWith('inverse')?new Inverse():new Classic();const current=mapping[c.op];
 const reject=()=>{if(c.failure!=='ok'&&!primaryUsed){primaryUsed=true;return true;}return false;};
 target.ensureAccountTypeDetected=()=>{trace.push('ensure');return Promise.resolve().then(()=>{if(c.mutation==='ensure')orders.push(extra);});};
 Object.defineProperties(target,{usesUtaRouting:{get(){trace.push('uta');return c.uta;}},apiProductType:{get(){trace.push('product');return 'product';}},apiMarginCoin:{get(){trace.push('margin');return 'margin';}},apiUtaCategory:{get(){trace.push('category');return 'category';}}});
 for(const [name,tag]of [['cancelNormalOrders','normal'],['cancelAlgoOrders','algo'],['cancelUtaNormalOrders','uta-normal'],['cancelUtaStrategyOrders','uta-strategy'],['cancelOrders','orders']])if(name!==current)target[name]=os=>{trace.push('delegate:'+tag);delegates.push({tag,ids:os.map(o=>units(o.id)),aliases:os.map(o=>orders.includes(o))});return reject()?Promise.reject(primary):Promise.resolve();};
 target.fetchUtaNormalOrders=()=>{trace.push('fetch-normal');if(c.failure==='fetch-get')throw primary;return c.failure==='fetch-reject'?Promise.reject(primary):Promise.resolve(orders.slice(0,1));};
 target.fetchUtaStrategyOrders=()=>{trace.push('fetch-strategy');return Promise.resolve(orders.length>3?orders.slice(3,4):orders.slice(0,1));};
 target.isVirtualTPSLOrder=o=>{if(o===undefined)throw primary;trace.push('virtual:'+o.tag);if(o.tag===0){if(c.mutation==='predicate')orders[1]=extra;else if(c.mutation==='delete')orders.splice(1);}return o.kind==='virtual';};target.isAlgoOrder=o=>{trace.push('algo:'+o.tag);return o.kind==='algo';};
 target.getCancelPlanType=o=>{trace.push('plan:'+o.tag);return 'plan/'+o.id;};
 target.getMarginCoinForSymbol=s=>{trace.push('coin');return 'coin/'+s;};
 target.selectPlanTypeAndAmount=(o,positions)=>{assert.equal(positions,target.store.positions);trace.push('inverse-plan:'+o.tag);return ['inverse-plan/'+o.id,0];};
 target.store={orders,positions:[],markets:c.coins.map(base=>({base:string(base)}))};
 target.schedulePrivateStreamWriteCheck=()=>{trace.push('write');schedules++;};
 target.emitAndLogError=m=>{trace.push('error');errors.push(pack(m));if(c.failure==='report')throw report;};
 target.unlimitedXHR={post(endpoint,payload){trace.push('post');requests.push({endpoint,fields:Object.entries(payload).map(([key,value])=>[key,key==='orderIdList'?{ids:value.map(o=>units(o.orderId))}:pack(value)])});if(payload.orderId&&typeof payload.orderId==='object'){assert(orders.some(o=>o.info.orderId===payload.orderId),'CANCEL source opaque info alias');}return reject()?Promise.reject(primary):Promise.resolve({data:{code:'00000'}}).then(v=>{if(c.mutation==='post'&&requests.length===1)orders.push(extra);return v;});}};
 let thrown=null;try{if(c.op==='symbol')await target[current](string(c.symbol));else if(c.op.endsWith('all'))await target[current]();else await target[current](orders);}catch(e){thrown=e===primary?'primary':e===report?'report':{name:e.name,message:units(e.message)};}
 outputs.push({trace,requests,delegates,errors,schedules,thrown});
}
assert.equal(outputs.length,88,'CANCEL independent source floor');console.log(JSON.stringify(outputs));
