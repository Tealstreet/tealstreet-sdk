import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {load,root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const ast=ts.createSourceFile(filename,fs.readFileSync(`${root}/${filename}`,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPrivateWebsocket');
const initializer=owner.members.find(n=>n.name?.getText(ast)==='handleFill')?.initializer;assert(initializer);
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const context=vm.createContext({fromMexcSymbol:utils.fromMexcSymbol,normalizeExecution:utils.normalizeExecution});
vm.runInContext(ts.transpileModule(`class Receiver { constructor(store,parent){this.store=store;this.parent=parent;} handleFill=${initializer.getText(ast)}; } globalThis.Receiver=Receiver;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const base={symbol:'BTC_USDT',side:1,price:12.5,vol:2,createTime:1700000000000};
const fixtures=[
 ['missing-order-id',{...base,tradeId:'fill-2'},undefined],
 ['fallback-number',{...base,createTime:1e308},undefined],
 ['amount-overflow',{...base,vol:'1e308'},1e308],
 ['ignored-asset',{...base,feeCurrency:{token:7}},undefined],
 ['populated-sell',{...base,side:3,tradeId:'sell-fill',orderId:'sell-order',price:'13.25',vol:'3',fee:'-0.25',profit:'-1.5'},undefined],
 ['contract-only',{...base,symbol:undefined,contract:'BTC_USDT'},10],
 ['lone-symbol',{...base,tradeId:'lone',symbol:JSON.parse('"\\ud800"')},undefined],
 ['coercion-error',{...base,symbol:{toString:0,valueOf:0}},undefined],
 ['null-row',null,undefined],
 ['null-frame',null,undefined],
 ['emitter-error',[{...base,tradeId:'first'},{...base,tradeId:'second'}],undefined],
 ['skip-zero',[{...base,vol:0},{...base,tradeId:'after-zero'}],undefined],
];
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
const bits=value=>{const buffer=new ArrayBuffer(8),view=new DataView(buffer);view.setFloat64(0,value);return view.getBigUint64(0).toString(16).padStart(16,'0')};
const rows=[];let total=0;
for(const [label,raw,contractSize] of fixtures){
 const frame=label==='null-frame'?null:{data:Array.isArray(raw)?raw:[raw]},events=[],failure=new TypeError('fill listener failed');failure.code=197;
 let thrown;
 try{new context.Receiver({marketsBySymbol:contractSize?{BTCUSDT:{contractSize}}:{}},{emitter:{emit(name,event){
  assert.equal(name,'fill');total++;events.push(event);
  assert.deepEqual(Object.keys(event),['id','orderId','symbol','side','price','amount','fee','realizedPnl'],'FILL Source own fields');
  assert.equal(typeof event.amount,'number','FILL Source native amount');
  assert.equal(event.orderId,label==='populated-sell'?'sell-order':'','FILL Source retained orderId');
  assert.equal(event.side,label==='populated-sell'?'sell':'buy','FILL Source side');
  assert.equal(event.price,label==='populated-sell'?13.25:12.5);assert.equal(event.fee,label==='populated-sell'?0.25:0);assert.equal(event.realizedPnl,label==='populated-sell'?-1.5:0);
  if(label==='fallback-number')assert.equal(event.id,'BTCUSDT:1e+305','FILL Source number template');
  if(label==='amount-overflow')assert.equal(event.amount,Infinity,'FILL Source native overflow');else assert.equal(event.amount,label==='populated-sell'?3:2,'FILL Source market selection');
  if(label==='lone-symbol'){assert.equal(typeof event.symbol,'string');assert.deepEqual(units(event.symbol),[0xd800],'FILL Source retained lone symbol unit');}
  if(label==='emitter-error')throw failure;
 }}}).handleFill(frame);}catch(error){thrown=error;}
 const errorCase=['coercion-error','null-row','null-frame','emitter-error'].includes(label);
 assert.equal(thrown!==undefined,errorCase,'FILL Source error branch');
 assert.equal(events.length,['coercion-error','null-row','null-frame'].includes(label)?0:1,'FILL Source emit count');
 if(label==='emitter-error')assert.equal(thrown,failure,'FILL Source emitter identity/stopped second row');
 if(label==='coercion-error')assert.equal(thrown.name,'TypeError');
 rows.push({label,contractSize:contractSize??null,inputJson:JSON.stringify(frame),events:events.map(event=>({idUnits:units(event.id),orderIdUnits:units(event.orderId),symbolUnits:units(event.symbol),side:event.side,fields:['price','amount','fee','realizedPnl'].map(key=>({key,bits:bits(event[key]),nan:Number.isNaN(event[key])})),wire:JSON.stringify(event)})),error:thrown?{name:thrown.name,message:thrown.message,code:thrown.code}:null});
}
assert.equal(fixtures.length,12,'FILL independent scenario floor');assert.equal(total,9,'FILL independent emit-call count');
process.stdout.write(JSON.stringify({rows,total}));
