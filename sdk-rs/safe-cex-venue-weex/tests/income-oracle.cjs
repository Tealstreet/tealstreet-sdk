const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,header,capture,load}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['toNumber','toIntegerString','normalizeTimestampToMs','normalizeIncome'];
const nodes=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(nodes.length,4,'income_source_inventory');
const context={exports:{},...load(path.join(root,'packages/safe-cex/src/utils/incomeCategoryFromType.ts'))};
vm.runInNewContext(ts.transpileModule(nodes.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
process.env.TZ='UTC';
const inputs=[
 ['distinct','{"symbol":"BTC-USDT","incomeType":"REALIZED_PNL","income":"-13.75","asset":"USDC","time":1700000000123,"billId":"bill-a","tranId":"tran-b","id":"id-c","tradeId":"trade-d"}'],
 ['aliases','{"symbol":"ETH","incomeType":"funding_fee","income":3,"asset":null,"coin":"BTC","time":null,"createdTime":"2024-01-02T03:04:05","billId":null,"tranId":77,"tradeId":99}'],
 ['empty','{}'],['null','null'],['primitive','3'],
 ['zero','{"symbol":0,"incomeType":false,"income":"-0","asset":false,"coin":"","billId":0,"tradeId":0}'],
 ['numeric','{"income":9007199254740993.0,"time":1e18,"billId":9007199254740993.0,"tradeId":1e400}'],
 ['false_id','{"billId":false,"tranId":"not-used","tradeId":false}'],
 ['id_fallback','{"billId":null,"tranId":null,"id":"last-id","tradeId":[]}'],
 ['empty_id','{"billId":"","tranId":"not-used"}'],
 ['coercion','{"symbol":["A","B"],"incomeType":["COMMISSION"],"income":["7.5"],"asset":true,"billId":[1,2],"tradeId":{}}'],
 ['symbol_error','{"symbol":{"toString":null},"income":{"toString":null}}'],
 ['income_error','{"income":{"toString":null},"time":{"toString":null}}'],
 ['asset_error','{"asset":{"toString":null}}'],
 ['time_error','{"time":{"toString":null},"billId":{"toString":null}}'],
 ['id_error','{"billId":{"toString":null}}'],
 ['trade_error','{"tradeId":{"toString":null}}'],
];
for(const category of ['REALIZED_PNL','DELIVERED_SETTELMENT','FUNDING_FEE','COMMISSION','OPTIONS_PREMIUM_FEE','POSITION_LIMIT_INCREASE_FEE','COMMISSION_REBATE','REFERRAL_KICKBACK','API_REBATE','TRANSFER','INTERNAL_TRANSFER','CROSS_COLLATERAL_TRANSFER','COIN_SWAP_DEPOSIT','COIN_SWAP_WITHDRAW','AUTO_EXCHANGE','INSURANCE_CLEAR','UNKNOWN']) inputs.push(['category_'+category,JSON.stringify({incomeType:category})]);
const cases=inputs.map(([id,raw])=>{try{const n=context.exports.normalizeIncome(JSON.parse(raw));return {id,raw,result:{symbol:n.symbol,incomeType:n.incomeType,income:capture(n.income),asset:n.asset,timestamp:capture(n.timestamp),tranId:n.tranId,tradeId:n.tradeId,category:n.category}};}catch(e){return{id,raw,error:{name:e.name,message:e.message}};}});
const refusals=['{"symbol":"\\ud800"}','{"extra":'+'['.repeat(150)+'0'+']'.repeat(150)+'}'].map(raw=>({raw,accepted:context.exports.normalizeIncome(JSON.parse(raw)).category==='other'}));
const callable=context.exports.normalizeIncome({income:{toString:()=> '7.25'}});
assert.equal(callable.income,7.25,'income_callable');
process.stdout.write(JSON.stringify({header,cases,refusals,callable:capture(callable.income)}));
