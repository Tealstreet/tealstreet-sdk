// Field-identity supplements are post-audit evidence; original fixture bytes stay intact.
const mapperIdentity = process.argv.includes('--mapper-identity');
const identity = process.argv.includes('--field-identity') || mapperIdentity;
const numberBits = require('./number-bits.cjs');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=path.resolve(__dirname,'../../..');
function load(file,extra='',imports={}) {const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8')+extra,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:n=>{assert.ok(n in imports,n);return imports[n];}});return exports;}
const helpers=['isRecord','requireRecord','requireArray','requireString','optionalString','requireInteger','requireTimestamp','requireBoolean','requireDecimalString','requireInstrumentDecimal','optionalInstrumentDecimal','requireFiniteNumber'];
const api=load('packages/safe-cex/src/exchanges/woo/woo.v3.public.ts','\nexport { '+helpers.join(',')+' };',{'../../utils/orderbook':load('packages/safe-cex/src/utils/orderbook.ts')});
const cases=[];
function capture(name,input,extra) {let result;try{result={ok:api[name](input,...(extra||[]))??null};}catch(e){result={error:e.message};}cases.push({name,input:input??null,missing:input===undefined,extra:extra||[],...result});}
const values=[undefined,null,false,true,0,1,-1,1.5,9007199254740991,9007199254740992,'',' ','\ufeff','\u0085','\ufeff12\ufeff','\u008512','0x10','0b11','0o17','12abc','1e400','-0','5e-324','1e-400','Infinity',[],{},'text'];
for(const name of helpers)for(const v of values)capture(name,v,['probe']);
for(const value of [{toString:null},{toString:'x'},['0x10'],[null],[1,2]])capture('requireFiniteNumber',value,['probe']);
const instrument={symbol:'PERP_BTC_USDT',baseAsset:'BTC',quoteAsset:'USDT',baseMin:'0.01',baseMax:'100',baseTick:'0.001',quoteTick:'0.1',minNotional:'5',baseIMR:'0.02',baseAssetMultiplier:'1',fundingIntervalHours:8,isAllowedRpi:false,status:'TRADING'};
const futures={symbol:'PERP_BTC_USDT',indexPrice:'100',markPrice:'101',estFundingRate:'-0.001',lastFundingRate:'0.002',openInterest:'42','24hOpen':'100','24hClose':'110','24hHigh':'120','24hLow':'90','24hVolume':'3','24hAmount':'330',nextFundingTime:1700000000000};
const candle={open:'10',close:'11',low:'9',high:'12',volume:'3',amount:'33',symbol:'PERP_BTC_USDT',type:'1m',startTimestamp:120000,endTimestamp:180000};
const book={timestamp:1700000000000,asks:[{price:'103',quantity:'2'},{price:'101',quantity:'3'},{price:'101',quantity:'1'}],bids:[{price:'99',quantity:'4'},{price:'100',quantity:'5'}]};
function envelope(row){return {success:true,timestamp:1700000000000,data:{rows:[row]}};}
for(const [name,row] of [['decodeWooV3Instruments',instrument],['decodeWooV3Futures',futures],['decodeWooV3Klines',candle]]) {
 const base=envelope(row);capture(name,base);
 for(const key of Object.keys(row))for(const v of values){const input=structuredClone(base);if(v===undefined)delete input.data.rows[0][key];else input.data.rows[0][key]=v;capture(name,input);}
 for(const v of values){capture(name,v);for(const key of ['success','timestamp','data']){const x=structuredClone(base);if(v===undefined)delete x[key];else x[key]=v;capture(name,x);}capture(name,{...base,data:{rows:v}});}
}
const bookEnvelope={success:true,timestamp:book.timestamp,data:{asks:book.asks,bids:book.bids}};
capture('decodeWooV3OrderBook',bookEnvelope);capture('mapWooV3OrderBook',book);
for(const v of values){capture('decodeWooV3OrderBook',{...bookEnvelope,timestamp:v});for(const side of ['asks','bids']){capture('decodeWooV3OrderBook',{...bookEnvelope,data:{...bookEnvelope.data,[side]:v}});for(const key of ['price','quantity']){const b=structuredClone(bookEnvelope);b.data[side][0][key]=v;capture('decodeWooV3OrderBook',b);}}}
for(const v of ['0','-1','0x10','5e-324','1e-400','\ufeff2','\u00852','1e400'])for(const side of ['asks','bids'])for(const key of ['price','quantity']){const b=structuredClone(book);b[side][0][key]=v;capture('mapWooV3OrderBook',b);}
capture('mapWooV3Candles',[candle,{...candle,startTimestamp:60000},{...candle,startTimestamp:120000,close:'77'}]);
for(const v of ['0','-1','0x10','5e-324','1e-400','\ufeff2','\u00852','1e400'])for(const key of ['open','close','low','high','volume'])capture('mapWooV3Candles',[{...candle,[key]:v}]);
// Explicitly pin the engine-dependent NaN comparator separately from finite parity.
const nanRows=[3000,NaN,2000,1000].map((startTimestamp,i)=>({...candle,startTimestamp,close:String(i)}));
const nanSource=api.mapWooV3Candles(nanRows);
assert.deepEqual(Array.from(nanSource,r=>r.close),[0,1,3,2],'source NaN sort changed; revisit chosen divergence');
const nanRust=[3,2,0,1].map(i=>api.mapWooV3Candles([nanRows[i]])[0]);
cases.push({name:'mapWooV3Candles',input:nanRows,missing:false,extra:[],nonfinite:true,source:nanSource,ok:nanRust});
// Capture mapper boundaries before Rust implementation, including filtered indices.
capture('mapWooV3Markets',[{...instrument,symbol:'SPOT_BTC_USDT'},instrument,{...instrument,status:'HALTED'}]);
for(const status of [undefined,'TRADING','HALTED','']) {const row={...instrument};if(status===undefined)delete row.status;else row.status=status;capture('mapWooV3Markets',[row]);}
for(const key of ['baseMin','baseMax','baseTick','quoteTick','baseIMR'])for(const value of ['0','-1','5e-324','1e400','0x10','\ufeff0.1','\u00850.1',0.5,2])capture('mapWooV3Markets',[{...instrument,[key]:value}]);
capture('mapWooV3Markets',[{...instrument,symbol:'SPOT_BTC_USDT'},{...instrument,baseMin:'0'}]);
capture('mapWooV3Markets',[{...instrument,baseIMR:'0.03'}]);
const markets={BTCUSDT:{id:'btc-id',symbol:'BTCUSDT'},PERP_BTC_USDT:{id:'fallback',symbol:'FALLBACK'}};
capture('mapWooV3FuturesStats',[futures],[markets]);
capture('mapWooV3FuturesStats',[futures],[{PERP_BTC_USDT:markets.PERP_BTC_USDT}]);
capture('mapWooV3FuturesStats',[{...futures,symbol:'UNKNOWN'},futures],[markets]);
for(const key of Object.keys(futures).filter(k=>!['symbol','nextFundingTime'].includes(k)))for(const value of ['0','-1','5e-324','1e400','0x10','\ufeff2','\u00852'])capture('mapWooV3FuturesStats',[{...futures,[key]:value}],[markets]);
for(const symbol of ['constructor','toString','valueOf','hasOwnProperty','__proto__'])capture('mapWooV3FuturesStats',[{...futures,symbol}],[{}]);
for(const close of ['9999.5','9998.5','10000.5'])capture('mapWooV3FuturesStats',[{...futures,'24hOpen':'10000','24hClose':close}],[markets]);
// Source info retains its input alias; serialization is an explicit snapshot boundary.
const aliasInput={...instrument};
const aliasMarket=api.mapWooV3Markets([aliasInput])[0];
assert.equal(aliasMarket.info,aliasInput,'market info must retain source identity');
const aliasSnapshot=JSON.parse(JSON.stringify(aliasMarket.info));
aliasInput.symbol='PERP_RELABEL_USDT';
assert.equal(aliasMarket.info.symbol,aliasInput.symbol,'source alias observes later mutation');
assert.equal(aliasSnapshot.symbol,instrument.symbol,'owned snapshot stays frozen');
const aliasCase={input:instrument,mutatedSymbol:aliasInput.symbol,sourceAfter:aliasMarket.info.symbol,snapshotAfter:aliasSnapshot.symbol};
const aliasPath=path.join(__dirname,'../tests/fixtures/market-alias.json'),aliasBytes=JSON.stringify(aliasCase,null,2)+'\n';
if(!identity){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(aliasPath,'utf8'),aliasBytes,'market alias witness drift');else fs.writeFileSync(aliasPath,aliasBytes);}
const target=path.join(__dirname,'../tests/fixtures/public.json'),bytes=JSON.stringify(cases,null,2)+'\n';
if(!identity){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO V3 public live-source drift');else fs.writeFileSync(target,bytes);}
if(!identity) console.log(`WOO V3 public oracle: ${cases.length} live cases`);
const sourceFile=path.join(root,'packages/safe-cex/src/exchanges/woo/woo.v3.public.ts');
const ast=ts.createSourceFile(sourceFile,fs.readFileSync(sourceFile,'utf8'),ts.ScriptTarget.Latest,true);
const declarations=new Map(ast.statements.filter(ts.isTypeAliasDeclaration).map(s=>[s.name.text,s.type]));
let fieldIdentity=0;
function sample(type){
 if(ts.isTypeOperatorNode(type))return sample(type.type);
 if(ts.isTypeReferenceNode(type)) {const n=type.typeName.getText(ast);if(n==='Readonly')return sample(type.typeArguments[0]);if(n==='WooV3Decimal')return identity ? String(++fieldIdentity)+'.5' : '12.5';if(n==='OrderBook')return identity ? {bids:[{price:++fieldIdentity,amount:++fieldIdentity,total:++fieldIdentity}],asks:[{price:++fieldIdentity,amount:++fieldIdentity,total:++fieldIdentity}]} : {bids:[],asks:[]};return sample(declarations.get(n));}
 if(ts.isIntersectionTypeNode(type))return Object.assign({},...type.types.map(sample));
 if(ts.isTypeLiteralNode(type))return Object.fromEntries(type.members.map(m=>[m.name.text,sample(m.type)]));
 if(ts.isArrayTypeNode(type))return [sample(type.elementType)];
 if(type.kind===ts.SyntaxKind.StringKeyword)return identity ? String(++fieldIdentity)+'.5' : '12.5';
 if(type.kind===ts.SyntaxKind.NumberKeyword)return identity ? ++fieldIdentity+0.5 : 12.5;
 throw Error('unhandled public record '+type.getText(ast));
}
const records=Object.fromEntries([...declarations].filter(([name])=>name!=='UnknownRecord').map(([name,type])=>[name,sample(type)]));
const recordPath=path.join(__dirname,'../tests/fixtures/public-records.json'),recordBytes=JSON.stringify(records,null,2)+'\n';
function nativeNumbers(value) {
 if(typeof value === 'number') return {$bits:numberBits(value)};
 if(Array.isArray(value))return value.map(nativeNumbers);
 if(value && typeof value === 'object')return Object.fromEntries(Object.entries(value).map(([key,value])=>[key,nativeNumbers(value)]));
 return value;
}
if(mapperIdentity)process.stdout.write(JSON.stringify({capture:{label:'mapper native field-identity supplement',platform:process.platform,architecture:process.arch,node:process.version},fixture:cases.filter(c=>c.name.startsWith('mapWoo')).map(c=>({...c,ok:nativeNumbers(c.ok)}))}));
else if(identity)process.stdout.write(JSON.stringify({capture:{label:"field-identity supplement",platform:process.platform,architecture:process.arch,node:process.version},fixture:records}));else if(process.argv.includes('--check'))assert.equal(fs.readFileSync(recordPath,'utf8'),recordBytes,'WOO V3 public record drift');else fs.writeFileSync(recordPath,recordBytes);
