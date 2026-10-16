import assert from 'node:assert/strict';
import fs from 'node:fs';import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const ast=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>['extractData','extractRows'].includes(d.name.getText(ast)))) selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(member.name?.getText(ast)==='fetchHistoricalFills')selected.push(`this.fetchHistoricalFills=${member.initializer.getText(ast)};`);
}
assert.equal(selected.length,3,'Source independent actual historical method/helper declarations');
const code=ts.transpileModule(selected.join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const types=load('packages/safe-cex/src/types.ts');const venueTypes=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');const {omitUndefined}=load('packages/safe-cex/src/utils/omitUndefined.ts');
const row={symbol:'BTC_USDT',vol:10,price:3,createTime:1700000000000,role:1};
const rejectionMode=process.argv.includes("--rejection");
const dateMode=process.argv.includes("--local-date");
const specs=dateMode ? [["local-date",[{...row,createTime:"2024-01-02T03:04:05"}]]] : rejectionMode ? [["rejected",[]]] : [['fresh-markets',[row,{...row,vol:20}]],['empty',[]],['null',[null]],['coercion',[{symbol:{toString:0,valueOf:0}}]],['primitives',[false,0,'row',[]]],['commission-number',[{...row,feeCurrency:42}]],['commission-then-null',[{...row,feeCurrency:42},null]],['commission-then-coercion',[{...row,feeCurrency:42},{symbol:{toString:0,valueOf:0}}]],['commission-profit-coercion',[{...row,feeCurrency:42,profit:{toString:0,valueOf:0}}]],['commission-role-coercion',[{...row,feeCurrency:42,role:{toString:0,valueOf:0}}]],['unicode',[{...row,symbol:'\uD800'}]],['scalar-bmp',[{...row,symbol:'é'}]],['scalar-astral',[{...row,symbol:'😃'}]],['scalar-space',[{...row,symbol:'a\uFEFF\u3000b'}]]];
const outputs=[];let calls=0,getters=0,requests=0;
for(const[label,rows]of specs){
 const inner=new DefaultStore();const market=utils.normalizeMarket({symbol:'BTC_USDT',contractSize:0.1});inner.update({markets:[market]});
 let reads=0,gets=0;const store={get marketsBySymbol(){reads++;getters++;if(label==='fresh-markets'&&reads===2)inner.update({markets:[{...market,contractSize:0.3}]});return inner.marketsBySymbol;}};
 const producer=Object.assign(new Error('history down'),{code:'HISTORY_FAILED',response:{status:418,data:{marker:7}}});
 let caught;
 const context={...types,...venueTypes,...utils,omitUndefined,store,isPublicDataOnly:false,xhr:{get:async(url,config)=>{gets++;requests++;if(rejectionMode){producer.config={url,...config};throw producer;}return{data:{data:rows}};}}};
 vm.runInNewContext(code,context,{filename});calls++;
 const dateArguments=[];const actualDateParse=Date.parse;
 if(dateMode)Date.parse=(value)=>{dateArguments.push(value);return actualDateParse(value);};
 let result,error;try{const pending=context.fetchHistoricalFills({symbol:'BTCUSDT',...(rejectionMode?{from:NaN,to:-0}:{})});assert.equal(gets,1,'Source eager GET prefix before promise settlement');result=await pending;}catch(e){caught=e;error={name:e.name,message:e.message};}finally{Date.parse=actualDateParse;}
 if(dateMode){assert.deepEqual(dateArguments,['2024-01-02T03:04:05'],'Source actual Date.parse call floor/argument');assert.equal(result.length,1);assert.equal(result[0].timestamp,1704157445,'Source fixed UTC+02 local timestamp');}
 assert.equal(gets,1,'Source independent historical request count');
 assert.equal(reads,dateMode?1:rejectionMode?0:{ 'fresh-markets':2,empty:0,null:1,coercion:1,primitives:4,'commission-number':1,'commission-then-null':2,'commission-then-coercion':2,'commission-profit-coercion':1,'commission-role-coercion':1,unicode:1,'scalar-bmp':1,'scalar-astral':1,'scalar-space':1}[label],'Source independent historical getter count');
 if(label==='fresh-markets'){assert.deepEqual(result.map(x=>x.qty),[1,6],'Source fresh row-local native market quantities');}
 if(['empty','primitives'].includes(label))assert.equal(result.length,0,'Source independent historical no-fill result count');
 if(['null','commission-then-null'].includes(label))assert.deepEqual(error,{name:'TypeError',message:"Cannot read properties of null (reading 'symbol')"},'Source null row exact rejection');
 if(['coercion','commission-then-coercion','commission-profit-coercion','commission-role-coercion'].includes(label))assert.deepEqual(error,{name:'TypeError',message:'Cannot convert object to primitive value'},'Source symbol coercion exact rejection');
 if(label==='commission-number'){assert.equal(result.length,1);assert.equal(result[0].commissionAsset,42,'Source native non-string commission asset survives actual method');}
 const symbol=result?.[0]?.symbol;const symbolUnits=symbol===undefined?null:Array.from({length:symbol.length},(_,i)=>symbol.charCodeAt(i));
 if(label==='unicode')assert.deepEqual(symbolUnits,[55296],'Source actual historical Execution program-readable lone-surrogate symbol');
 let rejection=null;
 if(rejectionMode){assert.equal(caught,producer,'Source historical retains original thrown object');assert(Number.isNaN(caught.config.params.start_time),'Source rejected request retains NaN');assert(Object.is(caught.config.params.end_time,-0),'Source rejected request retains signed zero');assert.deepEqual(Object.keys(caught.config.params),['symbol','start_time','end_time','page_num','page_size']);producer.response.data.marker=42;assert.equal(caught.response.data.marker,42,'Source error response remains aliased');rejection={same: caught===producer,code:caught.code,status:caught.response.status,marker:caught.response.data.marker};}
 outputs.push({label,gets,reads,dateArguments,timestamp:result?.[0]?.timestamp??null,rejection,symbolUnits,qty:result?.map(x=>x.qty)??null,error:error??null,commission:result?.[0]?.commissionAsset??null});
}
assert.equal(calls,dateMode?1:rejectionMode?1:14,'Source independent historical method call floor');assert.equal(requests,dateMode?1:rejectionMode?1:14,'Source independent historical GET floor');assert.equal(getters,dateMode?1:rejectionMode?0:19,'Source independent historical Store getter floor');
console.log(JSON.stringify(outputs));
