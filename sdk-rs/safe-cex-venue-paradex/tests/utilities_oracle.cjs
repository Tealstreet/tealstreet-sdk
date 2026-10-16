const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const header = {arch:process.arch, platform:process.platform, node:process.version, nodeMajor:Number(process.versions.node.split('.')[0]), v8:process.versions.v8};
const shared = ts.createSourceFile('shared.ts',fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8'),99,true);
const names = ['OrderSide','OrderType','OrderStatus'];
const enums = shared.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text));
assert.equal(enums.length,3,'shared_enum_census');
function execute(code, imports) {
 const module={exports:{}};
 vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{module,exports:module.exports,require:n=>{assert.equal(n,'../../types');return imports;}});
 return module.exports;
}
const sharedEnums=execute(enums.map(n=>n.getText(shared)).join('\n'));
function actual(name, privateNames){
 const file=path.join(root,`packages/safe-cex/src/exchanges/paradex/paradex.${name}.ts`);
 return execute(fs.readFileSync(file,'utf8')+`\nexport { ${privateNames.join(', ')} };`,sharedEnums);
}
const income=actual('income',['toNumber']);
const history=actual('order-history',['toNumber','toOptionalNumber']);
function bits(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex');}
function observe(value){
 if(typeof value==='number')return {type:'number',bits:bits(value)};
 if(value===undefined)return {type:'undefined'};
 if(value===null)return {type:'null'};
 if(Array.isArray(value))return value.map(observe);
 if(typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return {type:typeof value,value};
}
function capture(fn){try{return {ok:true,value:observe(fn())};}catch(e){return {ok:false,name:e.name,message:e.message};}}
const scalarRaw=['null','true','false','0','-0','-0.0','-1e-9999','1e400','-1e400','9007199254740993','1.0000000000000002','[]','[null]','["-0"]','["7.5","2"]','{}','{"toString":null}','{"valueOf":3}','{"toString":{},"valueOf":1}'];
for(const s of ['', ' ', '\ufeff-0tail','\u00855','\n-1.25e2tail','Infinity','-Infinity','NaN','0x10','+3.5','-.25','1e','1e+','1_000','9007199254740993','1e400','-1e-9999','-0','é😀','12.5suffix'])scalarRaw.push(JSON.stringify(s));
const scalars=[{label:'undefined'},...scalarRaw.map((raw,i)=>({label:`scalar-${i}`,raw}))].map(c=>{
 const v=c.raw===undefined?undefined:JSON.parse(c.raw);
 return {...c,incomeNumber:capture(()=>income.toNumber(v)),historyNumber:capture(()=>history.toNumber(v)),optionalNumber:capture(()=>history.toOptionalNumber(v))};
});
const hBase={id:'order-A',market:'BTC-USD-PERP',side:'BUY',type:'LIMIT',status:'CLOSED',created_at:'1700000000123',last_updated_at:'1700000000456',size:'11.5',remaining_size:'3.25',price:'101.25',avg_fill_price:'99.75',flags:['REDUCE_ONLY']};
const iBase={id:'fill-A',market:'ETH-USD-PERP',created_at:1700000000789,order_id:'order-B',fee_currency:'USDT',realized_pnl:'12.5',fee:'1.25',realized_funding:'-2.75'};
const histories=[],incomes=[];
function h(label,value){const raw=typeof value==='string'?value:JSON.stringify(value);histories.push({label,raw,result:capture(()=>history.mapParadexHistoricalOrder(JSON.parse(raw)))});}
function i(label,value,mode='resolved'){const raw=typeof value==='string'?value:JSON.stringify(value);let calls=[];const result=capture(()=>income.mapParadexFillToIncomeRecords(JSON.parse(raw),market=>{calls.push(market);if(mode==='throw')throw new TypeError('resolver failure');return mode==='missing'?undefined:mode==='empty'?'':`resolved:${market}`;}));incomes.push({label,raw,mode,result,calls});}
h('distinct-fields-supplement',hBase);i('distinct-fields-supplement',iBase);
// Preserve baseline paths where production deliberately collapses values, then
// supplement them with different inputs. A zero-remaining baseline cannot see
// amount/filled swaps; zero or missing prices collapse to undefined.
h('production-collapse',{...hBase,remaining_size:'0',price:'0',avg_fill_price:'0'});
for(const field of Object.keys(hBase)){
 const omitted={...hBase};delete omitted[field];h(`missing-${field}`,omitted);
 for(const value of [null,false,0,'','-0','garbage',{toString:null}])h(`${field}-${JSON.stringify(value)}`,{...hBase,[field]:value});
}
for(const side of ['SELL','buy','sell','ſell','ß',' buy ',{},['BUY']])h(`side-${JSON.stringify(side)}`,{...hBase,side});
for(const type of [...Object.keys(sharedEnums.OrderType), 'LIMIT','MARKET','STOP_LIMIT','STOP_MARKET','STOP_LOSS_LIMIT','STOP_LOSS_MARKET','TAKE_PROFIT_LIMIT','TAKE_PROFIT_MARKET','UNKNOWN',...Object.getOwnPropertyNames(Object.prototype)])for(const flags of [[],['REDUCE_ONLY'],['reduce_only'],['REDUCE_ONLY',{toString:null}]])h(`type-${type}-${JSON.stringify(flags)}`,{...hBase,type,flags});
for(const status of ['NEW','OPEN','UNTRIGGERED','PENDING','CLOSED','FILLED','REJECTED','new','unknown'])for(const cancel_reason of ['', 'INSUFFICIENT_MARGIN',0,false])h(`status-${status}-${cancel_reason}`,{...hBase,status,cancel_reason});
for(const field of ['created_at','last_updated_at','size','remaining_size','price','avg_fill_price'])for(const raw of scalarRaw)h(`number-${field}-${raw}`,JSON.stringify({...hBase,[field]:'__RAW__'}).replace('"__RAW__"',raw));
h('filled-overflow',{...hBase,size:'1.7e308',remaining_size:'-1.7e308'});
h('filled-clamp',{...hBase,size:'3',remaining_size:'5'});
for(const mode of ['resolved','missing','empty','throw'])i(`resolver-${mode}`,iBase,mode);
for(const field of Object.keys(iBase)){
 const omitted={...iBase};delete omitted[field];i(`missing-${field}`,omitted);
 for(const value of [null,false,0,'','-0','garbage',{toString:null}])i(`${field}-${JSON.stringify(value)}`,{...iBase,[field]:value});
}
for(const field of ['created_at','realized_pnl','fee','realized_funding'])for(const raw of scalarRaw)i(`number-${field}-${raw}`,JSON.stringify({...iBase,[field]:'__RAW__'}).replace('"__RAW__"',raw));
i('rebate',{...iBase,fee:'-1.25'});
i('explicit-id-skips-bad-order-id',{...iBase,order_id:{toString:null}});
i('fallback-coerces-bad-order-id',{...iBase,id:null,order_id:{toString:null}});
i('resolver-before-bad-asset',{...iBase,fee_currency:{toString:null}},'throw');
i('empty-id-is-not-nullish',{...iBase,id:'',order_id:{toString:null}});
const lone=JSON.parse('"\\ud800"');
const surrogate={raw:'"\\ud800"',codeUnit:lone.charCodeAt(0),incomeNumber:capture(()=>income.toNumber(lone)),historyNumber:capture(()=>history.toNumber(lone)),optionalNumber:capture(()=>history.toOptionalNumber(lone)),historyIdCodeUnit:history.mapParadexHistoricalOrder({...hBase,id:lone}).id.charCodeAt(0),incomeIdCodeUnit:income.mapParadexFillToIncomeRecords({...iBase,id:lone},x=>x)[0].tradeId.charCodeAt(0)};
assert.equal(surrogate.codeUnit,0xd800,'surrogate_source_acceptance');
process.stdout.write(JSON.stringify({header,scalars,histories,incomes,surrogate}));
