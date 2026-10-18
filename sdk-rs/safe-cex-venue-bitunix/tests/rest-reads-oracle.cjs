'use strict';
const {ts,fs,path,vm,root,assert,load}=require('./oracle-loader.cjs');
const {createRequire}=require('node:module');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const classes=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name.text==='BitunixExchange');
assert.equal(classes.length,1,'REST actual owner');
const keyFile=path.join(root,'packages/safe-cex/src/utils/get-key.ts');
const keyModule={exports:{}};
const keyCode=ts.transpileModule(fs.readFileSync(keyFile,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext(`(function(require,module,exports){${keyCode}})`,{filename:keyFile})(createRequire(keyFile),keyModule,keyModule.exports);
const precisionFile=path.join(root,'packages/ccxt-compat/src/functions/ccxt_exchange.ts');
const precisionAst=ts.createSourceFile(precisionFile,fs.readFileSync(precisionFile,'utf8'),ts.ScriptTarget.Latest,true);
const precisionNodes=precisionAst.statements.filter(n=>ts.isFunctionDeclaration(n)&&n.name.text==='parsePrecision');
assert.equal(precisionNodes.length,1,'REST actual precision function');
const precisionCode=ts.transpileModule(precisionNodes[0].getText(precisionAst),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
const precisionModule={exports:{}};
vm.runInThisContext(`(function(module,exports){${precisionCode}})`,{filename:precisionFile})(precisionModule,precisionModule.exports);
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const endpoints=load(path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts')).ENDPOINTS;
const names=['isRecord','getResponseCandidates','getFirstValue','parseFiniteNumber','isApiTradingSupported','extractAccountRow','fetchMarkets','fetchTickers','fetchBalance','fetchPositionMode'];
const arrows={};
for(const name of names){
 const nodes=classes[0].members.filter(n=>n.name?.getText(ast)===name);
 assert.equal(nodes.length,1,`REST actual method ${name}`);assert(ts.isArrowFunction(nodes[0].initializer));
 arrows[name]=ts.transpileModule(`(function(){return (${nodes[0].initializer.getText(ast)});})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
}
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');};
function observe(v){
 if(v===undefined)return {type:'undefined'};
 if(v===null)return {type:'null'};
 if(typeof v==='number')return {type:'number',bits:bits(v)};
 if(typeof v==='string'||typeof v==='boolean')return {type:typeof v,value:v};
 if(Array.isArray(v))return {type:'array',items:v.map(observe)};
 return {type:'object',fields:Object.fromEntries(Object.keys(v).map(k=>[k,observe(v[k])]))};
}
const market={id:'BTC/USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.001,price:0.1},limits:{amount:{min:0,max:123},leverage:{min:1,max:100}}};
const ticker={id:'cached',symbol:'cached',bid:21,ask:22,last:23,mark:24,index:25,percentage:26,fundingRate:27,fundingTime:28,volume:29,quoteVolume:30,openInterest:31};
const balance={total:101,free:102,used:103,upnl:104};
const positions=[{id:'BTCUSDT:long',symbol:'BTCUSDT',side:'long',entryPrice:2,notional:3,leverage:4,unrealizedPnl:5,contracts:6,liquidationPrice:7}];
const base={symbol:'BTCUSDT',base:'BTC',quote:'USDT',symbolStatus:'OPEN',basePrecision:3,quotePrecision:1,maxLimitOrderVolume:'12345'};
const markets=[
 {data:[base]},
 {data:[base,{...base,symbol:'DUP',isApiSupported:true},{...base,isApiSupported:false},{...base,isApiSupported:' NO '},{...base,isApiSupported:0},{...base,quote:'BTC'},{...base,symbolStatus:'open'}]},
 {data:[{symbol:'SNAKE',base:'ETH',quote:'USDT',symbol_status:'OPEN',base_precision:'2',quote_precision:'0',max_limit_order_volume:200,is_api_supported:'yes'}]},
 {data:[{...base,basePrecision:'nonsense',quotePrecision:-2}]},
 {data:null},
 {fail:true},
 {data:[{...base,basePrecision:{toString:1}}]},
];
const tickers=[
 {data:[{symbol:'UNKNOWN',last:'999'},{symbol:'BTCUSDT',last:'12.5tail',open:'10',mark_price:'15',base_vol:'17',quote_vol:'19'},{symbol:'BTCUSDT',last:'-0',open:'0',markPrice:'8',baseVol:'2',quoteVol:'3'}]},
 {data:[{symbol:'BTCUSDT',last:'bad',open:'2',markPrice:'Infinity',baseVol:null,quoteVol:'-0'}]},
 {data:[{symbol:'BTCUSDT',last:{toString:1}}]},
 {data:[]},
 {data:{}},
 {fail:true},
];
const accounts=[
 {data:{marginCoin:'USDT',available:'10',margin:'2',frozen:'3',crossUnrealizedPNL:'4',isolationUnrealizedPNL:'5',positionMode:'ONE_WAY'}},
 {data:{marginCoin:'USDT',availableBalance:'12',usedMargin:'6',locked:'7',unrealizedPnl:'8',posMode:'one_way'}},
 {data:{margin:1,crossUnrealizedPNL:0,unrealizedPNL:99,positionMode:'HEDGE'}},
 {data:{margin:1,crossUnrealizedPNL:null,isolationUnrealizedPNL:'',unrealizedPNL:'9'}},
 {data:[{marginCoin:'BTC',available:'99'},{marginCoin:'usdt',available:'11',margin:'3'}]},
 {data:{data:{positionMode:'one_way',available:{toString:1}}}},
 {data:{positionMode:'HEDGE',available:'-0',margin:'0',frozen:'0'}},
 {data:[]},
 {fail:true},
 {data:{positionMode:{toString:1},available:2}},
];
assert.equal(markets.length,7,'REST market corpus');assert.equal(tickers.length,6,'REST ticker corpus');assert.equal(accounts.length,10,'REST account corpus');
async function run(method,fixture,observeMaximum=true){
 const calls=[],traces=[],emits=[];
 const owner={store:{markets:[market],marketsBySymbol:{BTCUSDT:market},tickers:[ticker],balance,positions},accountPositionMode:enums.PositionMode.OneWay,
 xhr:{get:async(url,config)=>{calls.push({url,config:config??{}});if(fixture.fail)throw {message:'wire failure',response:{data:{msg:'venue failure'}}};return {data:fixture};}},
 emitAndLogError:message=>emits.push(message)};
 const context={v:keyModule.exports.v,parsePrecision:precisionModule.exports.parsePrecision,PositionMode:enums.PositionMode,ENDPOINTS:endpoints,console:{trace:e=>traces.push(e),error:e=>traces.push(e)}};
 for(const name of names)owner[name]=vm.runInNewContext(arrows[name],context,{filename:file}).call(owner);
 let result,error;
 try{result=await owner[method]();}catch(e){error=e.name||e.message;}
 const numericMax=method==='fetchMarkets'&&!error&&observeMaximum?result.map(m=>observe(Number(m.limits.amount.max))):[];
 return {method,raw:JSON.stringify(fixture),result:observe(result),error:error??null,calls,traceCount:traces.length,emits,mode:owner.accountPositionMode,numericMax};
}
(async()=>{
 const rows=[];
 for(const f of markets)rows.push(await run('fetchMarkets',f));
 for(const f of tickers)rows.push(await run('fetchTickers',f));
 for(const f of accounts)rows.push(await run('fetchBalance',f));
 for(const f of accounts)rows.push(await run('fetchPositionMode',f));
 assert.equal(rows.length,33,'REST fixed source total');
 const assetRows=[];
 for(const value of [false,{asset:'wrong'},null,17,undefined]){
  const bad={...base,symbol:'BADUSDT',base:value};
  const result=await run('fetchMarkets',{data:[base,bad,{...base,symbol:'ETHUSDT',base:'ETH'}]});
  assert.equal(result.result.items.length,3,'source retains malformed base and siblings');
  assert.equal(result.traceCount,0,'source accepts the raw asset');
  assetRows.push(result);
 }
 assert.equal(assetRows.length,5,'asset boundary source corpus');
 const scalarRows=[];
 for(const value of [{symbol:'wrong'},false,null,17,undefined]){
  const result=await run('fetchMarkets',{data:[base,{...base,symbol:value},{...base,symbol:'ETHUSDT',base:'ETH'}]});
  assert.equal(result.result.items.length,3,'source keeps raw symbol and siblings');
  assert.equal(result.traceCount,0,'source accepts raw symbol');
  scalarRows.push(result);
 }
 const rawMaximum=await run('fetchMarkets',{data:[base,{...base,symbol:'BADUSDT',maxLimitOrderVolume:{toString:null}},{...base,symbol:'ETHUSDT',base:'ETH'}]},false);
 assert.equal(rawMaximum.result.items.length,3,'source copies maximum without coercion');
 assert.equal(rawMaximum.traceCount,0);
 scalarRows.push(rawMaximum);
 assert.equal(scalarRows.length,6,'strict scalar source corpus');
 process.stdout.write(JSON.stringify({rows,market,ticker,balance,positions,assetRows,scalarRows}));
})().catch(e=>{console.error(e);process.exitCode=1;});
