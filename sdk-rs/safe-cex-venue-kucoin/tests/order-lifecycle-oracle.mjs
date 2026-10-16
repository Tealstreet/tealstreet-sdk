import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const read=p=>ts.createSourceFile(p,fs.readFileSync(path.join(root,'packages/safe-cex/src',p),'utf8'),ts.ScriptTarget.Latest,true);
const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
function declaration(src,name){const n=src.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(src)===name));assert.ok(n,name);return n.getText(src);}
function module(text){const m={exports:{}};vm.runInNewContext(transpile(text),{module:m,exports:m.exports});return m.exports;}
const ENDPOINTS=module(declaration(read('exchanges/kucoin/kucoin.types.ts'),'ENDPOINTS')).ENDPOINTS;
const types=read('types.ts');
const {OrderStatus}=module(types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='OrderStatus').getText(types));
const exchange=read('exchanges/kucoin/kucoin.exchange.ts'),futures=read('exchanges/kucoin/kucoin.ws-private.ts'),spot=read('exchanges/kucoin/kucoinspot.ws-private.ts');
const names={place:'placeOrder',batch:'placeOrderBatch',algo:'placeAlgoOrders',replace:'updateVirtualTpSlOrder',futures:'handleOrderTopic',spot:'handleOrderTopic'};
const pack=v=>{if(v===undefined)return {u:true};if(typeof v==='number'){if(Number.isNaN(v))return {n:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return {n:b.toString('hex')};}if(Array.isArray(v))return Array.from(v,pack);if(v&&typeof v==='object')return {o:Object.entries(v).map(([k,v])=>[k,pack(v)])};return v;};
async function fixture(method,c){
 const events=[],freshErrors=new Set();let closes=0,posts=0;
 const original=Object.assign(new TypeError('DOWN'),{code:'WRITE',ids:[3,7],response:{data:{msg:'VENUE'}}});
 function TracedError(message){const e=new Error(message);freshErrors.add(e);return e;}
 const parent={id:'P',symbol:'SYM',side:'sell',type:'limit',price:10,amount:5,timeInForce:'GTC',reduceOnly:true};
 const sl={id:'P__stop_loss',price:8},tp={id:'P__take_profit',price:12};
 const order=c==='sl'?sl:c==='tp'?tp:parent;order.symbol='SYM';
 const opts=method==='replace'?{order,update:c==='undefined'?{price:undefined,amount:undefined}:{price:11,amount:6}}:{symbol:'SYM',stopLoss:c==='attached'?8:undefined};
 const payloads=[{tag:'A',stop:'up'},{tag:'B'}],orders=c==='empty-mapped'?[]:[parent,sl,tp],related=c==='no-parent'?[sl,tp]:[parent,sl,tp];
 const raw={orderId:c==='missing-data'?undefined:'RAW',symbol:'SYM',type:c==='cancel'&&method==='spot'?'canceled':['cancel','cancelled','filled','update','open'].includes(c)?c:['done','done-remaining'].includes(c)?'received':'match',status:['done','done-remaining','cancel'].includes(c)?'done':c==='cancelled'?'cancelled':'open',matchSize:c==='filled'?undefined:'2',filledSize:'9',matchPrice:'15',price:'14',remainSize:c==='done-remaining'?'1':'0',orderTypeV2:'limit',orderSize:'5',ts:'1234000'};
 const record=(name,args)=>events.push([name,...args.map(pack)]);
 const receiver={store:{marketsBySymbol:c==='missing-market'?{}:{SYM:{contractSize:c==='zero-contract'?0:0.1}}}};
 receiver.resolveCallerClientOrderId=v=>{assert.equal(v,opts);record('resolve',[v]);};
 receiver.isAlgoOrder=()=>c==='algo';
 receiver.formatAlgoOrder=(...args)=>{record('formatAlgo',args);return payloads[0];};
 receiver.formatCreateOrder=async(...args)=>{record('format',args);if(c==='format-reject')throw original;return payloads;};
 receiver.placeOrderBatch=async(...args)=>{record('batch',args);if(c==='batch-reject')throw original;return ['NEW'];};
 receiver.placeAlgoOrders=async(...args)=>{record('algo',args);return ['NEW'];};
 receiver.cancelOrders=async(...args)=>{assert.equal(args[0][0],parent);record('cancel',args);if(c==='cancel-reject')throw original;};
 receiver.nativeAmountForCancelReplace=v=>{assert.equal(v,parent);return 4;};
 receiver.emitAndLogError=(...args)=>record('logError',args);
 receiver.emitter={emit:(...args)=>{record('emit',args);if(c==='emit-reject')throw original;}};
 receiver.unlimitedXHR={post:async(...args)=>{record('post',args);const i=posts++;assert.equal(args[1],payloads[i]);if(c==='reject'||c==='late-reject'&&i===1)throw original;return {data:{code:c==='falsey-code'?'':c==='missing-code'?undefined:c==='bad'||c==='late-bad'&&i===1?'400000':'200000',msg:c==='bad'?'DENIED':'',data:c==='no-id'?{}:{orderId:i===0?'ID1':'ID2'}}};}};
 receiver.parent={store:receiver.store,mapOrder:(...args)=>{record('map',args);if(c==='map-reject')throw original;return method==='spot'?parent:orders;}};
 receiver.store.updateOrderStatus=(...args)=>{record('status',args);assert.ok(orders.includes(args[0]));return 'STORE';};
 receiver.store.addOrUpdateOrder=(...args)=>{record('add',args);assert.ok(orders.includes(args[0]));return 'STORE';};
 receiver.refreshOrdersSoon=()=>record('refresh',[]);
 const source=method==='futures'?futures:method==='spot'?spot:exchange;
 const cls=source.statements.find(n=>ts.isClassDeclaration(n));const selected=cls.members.find(n=>n.name?.getText(source)===names[method]);assert.ok(selected,names[method]);
 const context={ENDPOINTS,OrderStatus,Error:TracedError};
 let prefix='';if(method==='spot')prefix=declaration(spot,'normalizeMillis');
 const body=vm.runInNewContext(transpile(`(function(){${prefix};return (${selected.initializer.getText(source)});})`),context).call(receiver);
 let payloadInput=payloads;
 if(method==='batch'||method==='algo')payloadInput={[Symbol.iterator](){let i=0;const rows=c==='empty'?[]:payloads;return{next(){return i<rows.length?{value:rows[i++],done:false}:{done:true};},return(){closes++;return {done:true};}};}};
 const args=method==='place'?[opts]:method==='batch'?[payloadInput,c==='explicit'?'CUSTOM':undefined]:method==='algo'?[payloadInput]:method==='replace'?[opts,related]:[{data:method==='futures'&&c==='missing-data'?undefined:raw}];
 let result;try{result={ok:pack(await body(...args))};}catch(e){assert.ok(e===original||freshErrors.has(e),`${method}/${c}: ${e.stack}`);result={error:{name:e.name,message:e.message,fresh:freshErrors.has(e),code:e.code??null,ids:e.ids??null}};}
 return {method,case:c,result,closes,events};
}
let input='';for await(const part of process.stdin)input+=part;
const outputs=[];for(const c of JSON.parse(input).cases)outputs.push(await fixture(c.method,c.case));
assert.equal(outputs.length,50,'independent source fixture floor');process.stdout.write(JSON.stringify(outputs));
