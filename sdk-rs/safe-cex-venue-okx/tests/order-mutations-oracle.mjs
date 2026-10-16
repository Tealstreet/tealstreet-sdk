import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const read=p=>ts.createSourceFile(p,fs.readFileSync(path.join(root,'packages/safe-cex/src',p),'utf8'),ts.ScriptTarget.Latest,true);
const source=read('exchanges/okx/okx.exchange.ts');
const cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXExchange');
const names={limit:'updateOrder',algo:'updateAlgoOrder',trigger:'updateTriggerAlgoOrder',dispatch:'cancelOrders',symbol:'cancelSymbolOrders',normal:'cancelNormalOrders','algo-cancel':'cancelAlgoOrders',leverage:'setLeverage',mode:'changePositionMode'};
const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
function declaration(src,name){const n=src.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(src)===name));assert.ok(n,name);return n.getText(src);}
function module(text,context={}){const m={exports:{}};vm.runInNewContext(transpile(text),{...context,module:m,exports:m.exports});return m.exports;}
const ENDPOINTS=module(declaration(read('exchanges/okx/okx.types.ts'),'ENDPOINTS')).ENDPOINTS;
const types=read('types.ts');
const enums=module(types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide','OrderStopTriggerType','PositionMode'].includes(n.name.text)).map(n=>n.getText(types)).join('\n'));
const math=read('utils/safe-math.ts');
const adjust=module([declaration(math,'POW10_CACHE'),math.statements.find(ts.isForStatement).getText(math),...['pow10','afterDecimal','adjust'].map(n=>declaration(math,n))].join('\n')).adjust;
const getOKXTriggerPxType=module(declaration(source,'getOKXTriggerPxType')+'\nmodule.exports={getOKXTriggerPxType};',enums).getOKXTriggerPxType;
const storeBase=read('store/store.base.ts');
const statuses=module(types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderStatus').getText(types));
const isTerminalOrder=module(['TERMINAL_STATUSES','isTerminalOrder'].map(n=>declaration(storeBase,n)).join('\n'),statuses).isTerminalOrder;
const forEachSeries=require('p-iteration').forEachSeries;
const chunk=require('lodash/chunk'),partition=require('lodash/partition');
const pack=v=>{if(v===undefined)return {u:true};if(typeof v==='number'){if(Number.isNaN(v))return {n:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return {n:b.toString('hex')};}if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return {o:Object.entries(v).map(([k,v])=>[k,pack(v)])};return v;};
const order=(id,symbol,type)=>({id,symbol,type,side:'sell',amount:4,price:11,reduceOnly:true,extra:{conditionalOrderPrice:12.6},triggerType:'index_price',tag:29});
async function fixture(method,c){
 const events=[];const selected=names[method];
 const original=Object.assign(new TypeError('DOWN'),{code:'WRITE',ids:[8,6]});
 const freshErrors=new Set();
 function TracedError(message){events.push(['newError',pack(message)]);const e=new Error(message);freshErrors.add(e);return e;}
 const market={id:'INST',precision:{amount:0.25,price:0.5},limits:{leverage:{min:1,max:9}}};
 const kind=method==='limit'&&c!=='delegate'?'limit':method==='trigger'||c==='conditional'?'conditional':'stop_market';
 const initial=order('ROOT_tp','SYM',kind),update={price:c==='invalid'?0:17.3,amount:c==='tiny'?0.01:2.2};
 if(c==='no-price')delete update.price;
 if(c==='zero-original')initial.amount='0';
 if(c==='market-order')initial.extra=null;
 let orders;
 if(method==='normal'||method==='algo-cancel'){
  const algo=method==='algo-cancel',count=c==='batch-boundary'?(algo?21:41):2;
  orders=Array.from({length:count},(_,i)=>order(`O${i}_tp`,c==='unknown'?'UNKNOWN':'SYM',algo?'stop_market':'limit'));
  orders.push(order('WRONG','SYM',algo?'limit':'conditional'));
 }else orders=c==='empty'?[]:[order('ROOT_sl','SYM','stop_market'),order('N1','SYM','limit'),order('ROOTX_tp','OTHER','take_profit_market'),order('OTHER','SYM','stop_market')];
 const positions=[{symbol:'SYM',leverage:c==='same'?9:3,contracts:c==='open'?1:0}];
 if(c==='guard-error')Object.defineProperty(positions[0],'contracts',{enumerable:true,get(){throw original;}});
 const refreshed=[{id:'REFRESHED',contracts:0}];
 const receiver={_accountPositionMode:'oneway',store:{orders,positions,marketsBySymbol:c==='missing'||c==='missing-market'?{}:{SYM:market}}};
 let storedMode='oneway';Object.defineProperty(receiver,'_accountPositionMode',{get:()=>storedMode,set:v=>{events.push(['mode',pack(v)]);storedMode=v;}});
 let fetches=0;
 receiver.emitAndLogError=v=>{events.push(['emit',pack(v)]);if(c==='late-error')original.message='AFTER';};
 receiver.fetchPositions=async()=>{fetches++;events.push(['fetch']);if(c==='recovery-reject'||c==='refresh-reject'&&fetches===1)throw original;return refreshed;};
 receiver.store.update=v=>{assert.equal(v.positions,refreshed);events.push(['this.store.update',pack(v)]);};
 receiver.store.addOrUpdatePositions=v=>{assert.equal(v,refreshed);events.push(['this.store.addOrUpdatePositions',pack(v)]);};
 receiver.isAlgoOrder=type=>['conditional','stop_market','take_profit_market','trailing_stop_market'].includes(type);
 receiver.toFiniteNumber=v=>Number.isFinite(Number(v))?Number(v):0;
 receiver.xhr={post:async(endpoint,payload)=>{
  const route=['UPDATE_ORDER','UPDATE_ALGO_ORDER','CANCEL_ORDERS','CANCEL_ALGO_ORDERS','SET_LEVERAGE','SET_POSITION_MODE'].find(k=>ENDPOINTS[k]===endpoint);assert.ok(route,endpoint);
  events.push(['post',route,pack(payload)]);
  if(['reject','late-error','recovery-reject'].includes(c))throw original;
  let body={};
  if(method==='trigger')body=c==='empty-body'?undefined:{code:c==='code-error'?'51000':'0',msg:c==='code-error'?'CODE':'',data:[{sCode:c==='row-error'?'51001':'0',sMsg:c==='row-error'?'ROW':''}]};
  if(method==='trigger'&&c==='raw-predicate'){const row={sCode:'0',sMsg:'ROW-RAW'};body.data={find(predicate){const returned=predicate(row);assert.equal(returned,'ROW-RAW');return row;}};}
  if(method==='mode')body={msg:c==='msg-error'?'DENIED':''};
  if(method==='normal'||method==='algo-cancel')body={data:[{sMsg:''},{sMsg:'WARN'}]};
  return {data:body};
 }};
 const fakeMath=Object.create(Math);
 if(c==='callee-reenter')fakeMath.max=(a,b)=>{fakeMath.min=()=>777;market.limits.leverage.max=7;return Math.max(a,b);};
 const context={...enums,ENDPOINTS,adjust,getOKXTriggerPxType,isTerminalOrder,chunk,partition,forEachSeries,Error:TracedError,Math:fakeMath};
 for(const name of Object.values(names)){
  const member=cls.members.find(n=>n.name?.getText(source)===name);assert.ok(member,name);
  receiver[name]=vm.runInNewContext(transpile(`(function(){return (${member.initializer.getText(source)});})`),context).call(receiver);
 }
 const delegates={
  updateAlgoOrder:async opts=>{assert.equal(opts.order,initial);assert.equal(opts.update,update);events.push(['this.updateAlgoOrder',pack(opts)]);return ['ALGO'];},
  updateTriggerAlgoOrder:async opts=>{assert.equal(opts.order,initial);assert.equal(opts.update,update);events.push(['this.updateTriggerAlgoOrder',pack(opts)]);return ['TRIGGER'];},
  cancelAlgoOrders:async rows=>{events.push(['this.cancelAlgoOrders',pack(rows)]);if(c==='cancel-reject')throw original;},
  cancelNormalOrders:async rows=>{events.push(['this.cancelNormalOrders',pack(rows)]);if(c==='normal-reject')throw original;},
  cancelOrders:async rows=>{events.push(['this.cancelOrders',pack(rows)]);if(method==='symbol'&&c==='reject')throw original;},
  placeOrder:async value=>{events.push(['this.placeOrder',pack(value)]);if(c==='place-reject')throw original;return ['PLACED'];},
 };
 for(const[name,binding]of Object.entries(delegates))if(name!==selected)receiver[name]=binding;
 let args;
 if(['limit','algo','trigger'].includes(method))args=[{order:initial,update}];
 else if(method==='dispatch')args=[c==='algo-only'?[order('A','SYM','conditional')]:orders];
 else if(method==='symbol')args=[c==='empty'?'NO':'SYM'];
 else if(method==='leverage')args=['SYM',50];
 else if(method==='mode')args=[c!=='oneway'];
 else args=[orders];
 let result;
 try{result={ok:pack(await receiver[selected](...args))};}
 catch(e){assert.ok(e===original||freshErrors.has(e),`${method}/${c}: ${e.stack}`);result={error:{name:e.name,message:e.message,fresh:freshErrors.has(e),code:e.code??null,ids:e.ids??null}};}
 return {method,case:c,result,events,mode:pack(receiver._accountPositionMode)};
}
let input='';for await(const part of process.stdin)input+=part;
const cases=JSON.parse(input).cases;const outputs=[];for(const c of cases)outputs.push(await fixture(c.method,c.case));
assert.equal(outputs.length,50);process.stdout.write(JSON.stringify(outputs));
