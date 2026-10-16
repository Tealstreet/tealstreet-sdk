import assert from 'node:assert/strict';
import {loadSource} from './source-loader.mjs';
const base = '../../../packages/safe-cex/src/exchanges/okx/';
const income = loadSource(new URL(base+'okx.income.ts', import.meta.url), ['toNumber']);
const closed = loadSource(new URL(base+'okx.closed-pnl.ts', import.meta.url), ['toNumber']);
const history = loadSource(new URL(base+'okx.order-history.ts', import.meta.url), ['toNumber','toOptionalNumber']);
const cases = [];
for (const key of ['constructor','toString','valueOf','hasOwnProperty','__proto__']) {
  assert.notEqual(typeof income.okxBillCategory(key), 'string', 'ts-bug: okxBillCategory prototype lookup changed');
}
const encode = value => JSON.parse(JSON.stringify(value ?? null));
const push = (op, input, expected, corrected = false) => {
 const sourcePrototype=op==='history'&&corrected?{type:typeof expected.type,status:typeof expected.status,typePrototype:expected.type===Object.prototype,statusPrototype:expected.status===Object.prototype}:op==='income'&&corrected?expected.map(record=>({categoryType:typeof record.category,categoryPrototype:record.category===Object.prototype,incomeType:typeof record.incomeType})):undefined;
 if(sourcePrototype&&!Array.isArray(sourcePrototype))assert.ok(sourcePrototype.type==='function'||sourcePrototype.status==='function'||sourcePrototype.typePrototype||sourcePrototype.statusPrototype,'source inherited history value before serialization');
 cases.push({op,input,expected:encode(expected),corrected,sourcePrototype});
};
push('bill_type_codes',null,income.OKX_BILL_TYPE_BY_INCOME_TYPE);
const values = [null,false,true,0,1,-1,1e-7,1e21,'','-0','0','1.25suffix','-.5','1e+tail','1e-3tail','0x10','Infinity','-Infinity','NaN','\uFEFF42\uFEFF','\u008542\u0085',[],[null],[1],[1,2],[[2]],{},'9'.repeat(400)];
for (const value of values) {
  assert.equal(income.toNumber(value),closed.toNumber(value));
  assert.equal(income.toNumber(value),history.toNumber(value));
  push('number',value,{number:income.toNumber(value),optional:history.toOptionalNumber(value)});
}
const market = {id:'DOGE-USDT-SWAP',symbol:'DOGEUSDT',base:'DOGE',quote:'USDT',active:true,precision:{amount:0.001,price:0.00001},limits:{amount:{min:0.001,max:100},leverage:{min:1,max:100}},contractSize:1000,isInverse:false,info:{ctValCcy:'DOGE'}};
const inverse = {...market,id:'BTC-USD-SWAP',symbol:'BTCUSD',base:'BTC',quote:'USD',contractSize:100,isInverse:true};
// Conversion is the injected collaborator, shared input to both mapper engines.
const convert = (n,m,p) => m.isInverse ? (p > 0 ? n*m.contractSize/p : 0) : n*m.contractSize;
for (const type of ['1','2','5','6','8','9','12','999',null,true,{},'constructor','toString','valueOf','hasOwnProperty','__proto__']) {
  for (const amounts of [[-1,-0.1,-0.9],[5,0.2,2],[250000,-0.1234,250000.1234],[250000,0.25,249999.7499999999],[0,0,0]]) {
    const row = {ts:'1735689600000',billId:'9007199254740993',instId:'DOGE-USDT-SWAP',ccy:'USDT',tradeId:'10',type,balChg:String(amounts[0]),fee:String(amounts[1]),pnl:String(amounts[2])};
    push('income',row,income.mapOkxBillToIncomeRecords(row,()=> 'DOGEUSDT'),['constructor','toString','valueOf','hasOwnProperty','__proto__'].includes(type));
  }
}
for (const ts of values) {
  const row = {ts,instId:'UNKNOWN',ccy:'USDT',type:'8',balChg:'1suffix'};
  push('income',row,income.mapOkxBillToIncomeRecords(row,()=>undefined));
}
for (const closeAvgPx of ['0.07168','0','NaN','-1']) for (const m of [market,inverse,null]) for (const direction of ['short','SHORT','long',null,{},['short']]) for (const time of ['1735689600000','-1','0','garbage']) {
  const row = {cTime:time,uTime:'0',instId:'DOGE-USDT-SWAP',closeAvgPx,closeTotalPos:'0.14',openAvgPx:'0.08',realizedPnl:'-0.0114359',direction,fee:'-0.0100359',lever:'50'};
  push('closed',{row,market:m},closed.mapOkxClosedPosition(row,m??undefined,convert,'replay'));
}
for (const [px,avgPx] of [['0','0.07168'],['0.08',''],['','NaN'],['-1','-0.2']]) for (const m of [market,inverse,null]) for (const state of ['live','partially_filled','filled','canceled','mmp_canceled','new-state','constructor','__proto__']) for (const ordType of ['market','mmp','mmp_and_post_only','new-type','constructor']) {
  const row = {ordId:'9007199254740993',instId:'DOGE-USDT-SWAP',cTime:'1735689600000',uTime:'1735689600010',side:'BUY',ordType,state,px,avgPx,sz:'0.14',accFillSz:'0.07',reduceOnly:'TRUE',posSide:'short'};
  push('history',{row,market:m},history.mapOkxHistoricalOrder(row,m??undefined,convert),!!m && (['constructor','__proto__'].includes(state)||ordType==='constructor'));
}
for (const value of values) {
  const row = {ordId:value,instId:'DOGE-USDT-SWAP',cTime:'1735689600000',side:'sell',sz:'1',accFillSz:'0',posSide:'both'};
  push('history',{row,market},history.mapOkxHistoricalOrder(row,market,convert));
}
for (const side of ['buy','sell','BUY','unknown',null]) for (const posSide of ['long','short','both',null]) for (const cTime of ['1735689600000','0','garbage','-1']) {
  const row = {ordId:'10',instId:'DOGE-USDT-SWAP',cTime,side,posSide,sz:'1',accFillSz:'0'};
  push('history',{row,market},history.mapOkxHistoricalOrder(row,market,convert));
}
for (const value of [{toString:null},{toString:'12'},{toString:false,valueOf:1},{toString:{}},[{toString:null}]]) {
  const expected = (()=>{try {income.toNumber(value);throw Error('expected String TypeError');} catch(error) {assert.equal(error.name,'TypeError');return {name:error.name,message:error.message};}})();
  cases.push({op:'coercion_error',input:value,expected});
}
const throwing = [{toString:null},{toString:'12'},{toString:false,valueOf:1},{toString:{}},[{toString:null}]];
const seeds = {
 income:{ts:'1735689600000',billId:'1',instId:'DOGE-USDT-SWAP',ccy:'USDT',tradeId:'1',type:'2',balChg:'1',fee:'0.1',pnl:'0.9'},
 closed:{cTime:'1735689600000',uTime:'1735689660000',instId:'DOGE-USDT-SWAP',closeAvgPx:'1',closeTotalPos:'1',openAvgPx:'1',realizedPnl:'1',direction:'short',fee:'0.1',lever:'2'},
 history:{ordId:'1',instId:'DOGE-USDT-SWAP',cTime:'1735689600000',uTime:'1735689660000',side:'buy',px:'1',avgPx:'1',ordType:'limit',state:'live',posSide:'long',sz:'1',accFillSz:'1',reduceOnly:'false'}
};
for(const [op,seed] of Object.entries(seeds)) for(const field of Object.keys(seed)) for(const value of throwing) {
 const row={...seed,[field]:value};let error;
 try {if(op==='income')income.mapOkxBillToIncomeRecords(row,()=> 'DOGEUSDT');if(op==='closed')closed.mapOkxClosedPosition(row,market,convert,'replay');if(op==='history')history.mapOkxHistoricalOrder(row,market,convert);}catch(thrown){error=thrown;}
 assert.equal(error?.name,'TypeError',`${op}.${field} must throw`);
 cases.push({op:op+'_error',input:{row,market},field,expected:{name:error.name,message:error.message}});
}
for(const field of ['posSide','ordType','state','sz','accFillSz','reduceOnly','uTime'])for(const value of [{toString:null},[{toString:null}]]){
 const row={...seeds.history,sz:'3',accFillSz:'5',[field]:value};const calls=[];let error;
 try{history.mapOkxHistoricalOrder(row,market,(n,m,p)=>{calls.push({native:n,reference:p});return convert(n,m,p);});}catch(e){error={name:e.name,message:e.message};}
 assert.equal(error?.name,'TypeError','history throwing field must remain an error');
 assert.equal(calls.length,['reduceOnly','uTime'].includes(field)?2:field==='accFillSz'?1:0,'source history conversion before coercion error');
 cases.push({op:'history_effects',input:{row,market},field,expected:error,calls});
}
for(const inherited of ['constructor','__proto__'])for(const lookupField of ['ordType','state'])for(const laterField of ['sz','uTime']){
 const row={...seeds.history,sz:'3',accFillSz:'5',[lookupField]:inherited,[laterField]:{toString:null}};const calls=[];let error;
 try{history.mapOkxHistoricalOrder(row,market,(n,m,p)=>{calls.push({native:n,reference:p});return convert(n,m,p);});}catch(e){error={name:e.name,message:e.message};}
 assert.equal(error?.name,'TypeError','later coercion beats inherited typed-output refusal');
 assert.equal(calls.length,laterField==='uTime'?2:0,'source inherited history conversion count before later error');
 cases.push({op:'history_effects',input:{row,market},field:lookupField+':'+inherited+'/'+laterField,expected:error,calls});
}
for(const key of Object.getOwnPropertyNames(Object.prototype).filter(k=>!['constructor','__proto__'].includes(k)))for(const field of ['ordType','state']){
 const row={...seeds.history,[field]:key};const result=history.mapOkxHistoricalOrder(row,market,convert);
 assert.equal(result[field==='ordType'?'type':'status'],field==='ordType'?'limit':'closed','source lowercase key excludes mixed-case inherited members');
 push('history',{row,market},result);
}
assert.equal(cases.length,1220);
process.stdout.write(JSON.stringify(cases));
