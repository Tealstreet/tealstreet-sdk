import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import vm from 'node:vm';
import ts from 'typescript';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const read=p=>readFileSync(root+p,'utf8');
const source=ts.createSourceFile('bybit.ts',read('packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitExchange');
const formatter=klass.members.find(m=>m.name?.getText(source)==='formatOrderForBatch');
const helperNames=['getBybitTriggerBy','getBybitTriggerDirectionPrice'];
const helpers=source.statements.filter(s=>ts.isVariableStatement(s)&&helperNames.includes(s.declarationList.declarations[0].name.getText(source)));
assert.equal(helpers.length,2);assert.ok(formatter);
const models=ts.createSourceFile('types.ts',read('packages/safe-cex/src/types.ts'),ts.ScriptTarget.Latest,true);
const enumNames=['OrderType','OrderSide','OrderStopTriggerType','OrderTimeInForce'];
const enums=models.statements.filter(s=>ts.isEnumDeclaration(s)&&enumNames.includes(s.name.text)).map(s=>s.getText(models).replace(/^export /,''));
assert.equal(enums.length,4);
const tables=ts.createSourceFile('tables.ts',read('packages/safe-cex/src/exchanges/bybit/bybit.types.ts'),ts.ScriptTarget.Latest,true);
const tableNames=['ORDER_TYPE','ORDER_SIDE','ORDER_TIME_IN_FORCE_OUTBOUND'];
const declarations=tables.statements.filter(s=>ts.isVariableStatement(s)&&tableNames.includes(s.declarationList.declarations[0].name.getText(tables))).map(s=>s.getText(tables).replace(/^export /,''));
assert.equal(declarations.length,3);
const math=read('packages/safe-cex/src/utils/safe-math.ts').replace(/^import .*;$/mg,'').replace(/export /g,'');
const context=vm.createContext({inverseObj:o=>Object.fromEntries(Object.entries(o).map(([k,v])=>[v,k])),omitUndefined:o=>Object.fromEntries(Object.entries(o).filter(([,v])=>v!==undefined))});
vm.runInContext(ts.transpileModule([...enums,math,...declarations,...helpers.map(s=>s.getText(source)),`globalThis.trigger=getBybitTriggerBy;globalThis.direction=getBybitTriggerDirectionPrice;globalThis.venue=new class {${formatter.getText(source)}};`].join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const market={id:'BTCUSDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{price:0.01,amount:0.001},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
const ticker={id:'BTCUSDT',symbol:'BTCUSDT',bid:10,ask:12,last:11,mark:15,index:17,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const cases=[];
for(const type of ['market','limit','conditional','stop_market','take_profit_market','trailing_stop_market'])
 for(const triggerType of [undefined,'mark_price','last_price','index_price'])
 for(const price of [undefined,0,1.239,1e21])
 for(const triggerPrice of [undefined,0,0.001,12,20]) {
 const opts={symbol:'BTCUSDT',type,side:cases.length%2?'buy':'sell',amount:1.23456,...(price===undefined?{}:{price}),...(triggerPrice===undefined?{}:{triggerPrice}),...(triggerType===undefined?{}:{triggerType,slTriggerType:triggerType,tpTriggerType:triggerType}),reduceOnly:true};
 cases.push({opts,market,ticker,positionIdx:cases.length%3});
}
for(const timeInForce of ['GoodTillCancel','ImmediateOrCancel','FillOrKill','PostOnly']) for(const amount of [0,-0,1e-7,1e21,12.34567]) {
 cases.push({opts:{symbol:'BTCUSDT',type:'limit',side:'buy',amount,price:1.1234,stopLoss:0.00001,takeProfit:2.234,clientOrderId:'sample',timeInForce},market,ticker,positionIdx:1});
}
for(const present of [false,true])cases.push({opts:{symbol:'BTCUSDT',type:'conditional',side:'sell',amount:1,triggerPrice:12},market:present?market:null,ticker:null,positionIdx:0});
for(const type of ['conditional','stop_market','take_profit_market','limit']) for(const triggerType of [undefined,'mark_price','last_price','index_price']) {
 cases.push({opts:{symbol:'BTCUSDT',type,side:'buy',amount:1,triggerPrice:12,...(triggerType===undefined?{}:{triggerType,slTriggerType:triggerType,tpTriggerType:triggerType})},market,ticker:null,positionIdx:0});
}
for(const type of ['limit','stop_market','take_profit_market'])
 for(const slTriggerType of ['last_price','mark_price','index_price'])
  for(const tpTriggerType of ['last_price','mark_price','index_price'])
   cases.push({opts:{symbol:'BTCUSDT',type,side:'buy',amount:1,price:11,triggerPrice:12,stopLoss:9,takeProfit:20,triggerType:'index_price',slTriggerType,tpTriggerType},market,ticker,positionIdx:1});
const prototypeStart=cases.length;
for(const symbol of ['constructor','toString','__proto__']) cases.push({opts:{symbol,type:'limit',side:'buy',amount:1},market:null,ticker:null,positionIdx:0});
// Preserve the original corpus; the labeled supplement adds axis discrimination.
assert.equal(cases.length,548);
assert.equal(createHash('sha256').update(JSON.stringify(cases)).digest('hex'),'cb3c61eb5bf9b1f31526d02b0fdf3c994210deaa315c9cd4b52f0be701dd360b','original formatter corpus byte identity');
const expected=[];
for(const c of cases){const errors=[];context.venue.store={marketsBySymbol:c.market?{BTCUSDT:c.market}:{},tickersBySymbol:c.ticker?{BTCUSDT:c.ticker}:{}};context.venue.getOrderPositionIdx=async()=>c.positionIdx;context.venue.emitAndLogError=message=>errors.push(message);
 expected.push({request:await context.venue.formatOrderForBatch(c.opts),errors,triggerBy:context.trigger(c.opts)??null,directionPrice:c.ticker?context.direction(c.opts,c.ticker):null});}
// Owned typed-market lookup intentionally excludes Object.prototype keys.
// Pin the source's inherited-key failure as well as Rust's explicit absence.
for(let i=prototypeStart;i<cases.length;i++) {
 assert.deepEqual(expected[i].errors,["Failed to format order for batch: Cannot read properties of undefined (reading 'price')"]);
 assert.equal(expected[i].request,null);
 expected[i].errors=[`Market ${cases[i].opts.symbol} not found`];
}
const sourceAdjust=vm.runInContext('adjust',context);
for(const [priceStep,amountStep] of [[0.1,1],[0.05,0.001],[0.25,0.01],[0.001,0.1]])
 for(const side of ['buy','sell']) for(const type of ['limit','conditional','stop_market','take_profit_market']) {
  const opts={symbol:'BTCUSDT',type,side,amount:1.23457,price:12.37,triggerPrice:12.83,stopLoss:11.43,takeProfit:13.67,reduceOnly:false};
  const c={label:`precision:${priceStep}/${amountStep}:${side}:${type}`,opts,market:{...market,precision:{price:priceStep,amount:amountStep}},ticker,positionIdx:1};
  const errors=[];
  context.venue.store={marketsBySymbol:{BTCUSDT:c.market},tickersBySymbol:{BTCUSDT:ticker}};
  context.venue.getOrderPositionIdx=async()=>c.positionIdx;
  context.venue.emitAndLogError=message=>errors.push(message);
  const request=await context.venue.formatOrderForBatch(opts);
  assert.deepEqual(errors,[],c.label);
  for(const [field,input] of [['price',opts.price],['triggerPrice',opts.triggerPrice],['stopLoss',opts.stopLoss],['takeProfit',opts.takeProfit],['qty',opts.amount]]) {
    if(request[field]===undefined)continue;
    const rightStep=field==='qty'?amountStep:priceStep,wrongStep=field==='qty'?priceStep:amountStep;
    assert.notEqual(sourceAdjust(input,rightStep),sourceAdjust(input,wrongStep),`distinct derived outputs ${field} ${c.label}`);
  }
  c.nativeExpected=request;
  cases.push(c);
  expected.push({request,errors,triggerBy:context.trigger(opts)??null,directionPrice:context.direction(opts,ticker)});
 }
assert.equal(cases.length,580);
const run=spawnSync('cargo',['run','--quiet','--locked','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--example','order-format'],{cwd:root,encoding:'utf8',input:JSON.stringify(cases),maxBuffer:16*1024*1024});
assert.equal(run.status,0,run.error?.message??run.stderr);assert.deepEqual(JSON.parse(run.stdout),JSON.parse(JSON.stringify(expected)), "formatter decoded trigger field identity");
console.log(`Bybit batch formatter: ${cases.length} live-source request/trigger/error cases (548 original + 32 precision supplement; ${process.arch}, Node ${process.versions.node}).`);
