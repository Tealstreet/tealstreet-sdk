const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/lighter/lighter.utils.ts');
const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(compiled.diagnostics.length, 0);
const utils = {};
vm.runInNewContext(compiled.outputText, {exports:utils});
const fixtures = [
 ['resolveLighterCancelTargets', [[{id:'one',symbol:'BTC'},{id:'two',symbol:'missing'},{id:'one',symbol:'BTC'}],{BTC:{id:'12tail'},ignored:{}}]],
 ['resolveLighterCancelTargets', [[{id:{token:1},symbol:'BTC'}],{BTC:{id:'7'}}]],
 ['resolveLighterCancelTargets', [[{id:'unread',symbol:'BTC'}],{BTC:{id:'invalid'}}], 'skip-id'],
 ['resolveLighterCancelTargets', [[{id:'one',symbol:'BTC'}],{BTC:{}}]],
 ['resolveLighterCancelTargets', [[{symbol:'BTC'}],{BTC:{id:'3'}}]],
 ['getLighterAmountDecimals', [{info:{amountDecimals:4}}], 'early-decimal'],
 ['getLighterAmountDecimals', [{info:{quantityDecimals:'6'}}]],
 ['getLighterAmountDecimals', [{info:{sizeScale:1000}}]],
 ['getLighterAmountDecimals', [{precision:{amount:0.0001}}]],
 ['getLighterAmountDecimals', [{info:null,precision:{amount:0.01}}]],
 ['getLighterAmountDecimals', [{info:{amountDecimals:2}}], 'throw-read'],
 ['getLighterPriceDecimals', [{info:{priceDecimals:3}}]],
 ['getLighterPriceDecimals', [{info:{quoteScale:100}}]],
 ['getLighterPriceDecimals', [{precision:{price:0.01}}]],
 ['getLighterPriceDecimals', [{info:false,precision:{price:'1e-7'}}]],
 ['getLighterOrderMinimums', [{}]],
 ['getLighterOrderMinimums', [{info:{min_base_amount:'1.25tail',min_quote_amount:'5'}}]],
 ['getLighterOrderMinimums', [{info:{min_base_amount:null,minBaseAmount:'2',minQuoteAmount:'7'}}]],
 ['getLighterOrderMinimums', [{info:{min_base_amount:'-1',min_quote_amount:'Infinity'}}]],
 ['getLighterOrderMinimums', [{info:{minBaseAmount:'1'}}], 'throw-read'],
];
fixtures.push(
 ['resolveLighterCancelTargets', [[{id:'utf16',symbol:'BTC'}],{BTC:{id:'3'}}], 'utf16-id'],
 ['getLighterAmountDecimals', [{precision:{amount:{}}}], 'precision-coercion'],
 ['getLighterOrderMinimums', [{info:{minBaseAmount:null}}], 'symbol-minimum'],
);
const rows = fixtures.map(([helper,args,mode='plain']) => {
 const input=structuredClone(args);
 function poison(value) {
  if(value&&typeof value==='object') {
   for(const child of Object.values(value))poison(child);
   Object.defineProperty(value,'unused',{enumerable:true,get(){throw new Error('unread field');}});
  }
 }
 input.forEach(poison);
 if(mode==='early-decimal'){
  Object.defineProperty(input[0],'precision',{get(){throw new Error('unread precision');}});
  Object.defineProperty(input[0].info,'quantityDecimals',{get(){throw new Error('unread alias');}});
 }
 if(mode==='utf16-id')input[0][0].id=String.fromCharCode(0xd800);
 if(mode==='precision-coercion')input[0].precision.amount={valueOf(){return 7;},toString(){return 'not.012';}};
 if(mode==='symbol-minimum')input[0].info.minBaseAmount=Symbol.iterator;
 if(mode==='skip-id')Object.defineProperty(input[0][0],'id',{get(){throw new Error('unread order id');}});
 if(mode==='throw-read')Object.defineProperty(input[0].info,helper==='getLighterAmountDecimals'?'amountDecimals':'minBaseAmount',{get(){throw new Error('used getter');}});
 let expected;
 try {
  const result=utils[helper](...input);
  expected={ok:result};
  if(helper==='resolveLighterCancelTargets') {
   const indices=result.map(row=>input[0].findIndex(order=>Object.is(order.id,row.orderId)));
   expected.retained=indices.map(index=>index>=0);
   expected.idUnits=result.map(row=>typeof row.orderId==='string'?Array.from({length:row.orderId.length},(_,i)=>row.orderId.charCodeAt(i)):null);
   expected.ok=result.map((row,i)=>({...('id' in args[0][indices[i]]?{orderId:args[0][indices[i]].id}:{}),marketId:row.marketId}));
  }
 }catch(error){expected={error:{name:error.name,message:error.message}};}
 return {helper,args,mode,expected};
});
assert.equal(rows.length,23);
process.stdout.write(JSON.stringify(rows));
