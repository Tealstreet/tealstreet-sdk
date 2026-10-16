const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
const cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file);
 let source=fs.readFileSync(file,'utf8');
 if(file===path.join(root,'types.ts')){const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);source=ast.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(ast)).join('\n');}
 const exports={};cache.set(file,exports);
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name)+'.ts'):name==='@tealstreet/math/src/isNotNumber'?load(path.resolve(root,'../../math/src/isNotNumber.ts')):runtime(name)},{filename:file});
 return exports;
}
const file=path.join(root,'exchanges/woo/woo.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const names=['parseTransactionHistoryRow','normalizeTransactionSymbol','normalizeTimestampMilliseconds'];
const members=names.map(name=>{const m=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,name);return m.getText(ast)});
const context={...load(path.join(root,'exchanges/woo/woo.types.ts')),...load(path.join(root,'types.ts')),...load(path.join(root,'utils/get-key.ts')),...load(path.join(root,'exchanges/woo/woo.utils.ts'))};
vm.runInNewContext(ts.transpileModule('class Witness {'+members.join('\n')+'}\nglobalThis.Witness=Witness;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const worker=new context.Witness();
function pack(value){
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {$bits:Number.isNaN(value)?'7ff8000000000000':b.toString('hex')};}
 if(Array.isArray(value))return value.map(pack);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,pack(v)]));
 return value;
}
const base={symbol:'PERP_BTC_USDT',side:'BUY',executedPrice:17.125,executedQuantity:3.75,executedTimestamp:1700000000123,orderId:'order-31',id:'fill-47',fee:0.625,feeAsset:'USDT',realizedPnl:-2.5,isMaker:1,positionSide:'SHORT'};
const fields=['symbol','side','executedPrice','executedQuantity','executedTimestamp','orderId','id','fee','feeAsset','realizedPnl','isMaker','positionSide'];
const values=['null','false','true','0','-0','-1','1.125','1e400','-1e400','-1e-400','1e308','""','"0x10"','"NaN"','"OPEN LONG"','"CLOSE SHORT"','[]','[2]','{}','{"toString":null}'];
const cases=[];
function capture(label,raw){try{cases.push({label,raw,ok:pack(worker.parseTransactionHistoryRow(JSON.parse(raw)))});}catch(e){cases.push({label,raw,error:e.message,name:e.name});}}
capture('base',JSON.stringify(base));
for(const field of fields){const rest={...base};delete rest[field];capture(field+' missing',JSON.stringify(rest));for(const raw of values)capture(field+' '+raw,JSON.stringify(rest).slice(0,-1)+','+JSON.stringify(field)+':'+raw+'}');}
for(const side of ['BUY','SELL','OPEN LONG','CLOSE LONG','OPEN SHORT','CLOSE SHORT','constructor','__proto__'])capture('side '+side,JSON.stringify({...base,side,positionSide:null}));
for(const value of [null,0,false,''])capture('alias '+JSON.stringify(value),JSON.stringify({...base,executedPrice:value,price:29.25,executedQuantity:value,quantity:4.5,executedTimestamp:value,timestamp:1700000000999,orderId:value,order_id:'alternate-order',id:value,tradeId:'alternate-fill',realizedPnl:value,realized_pnl:8.5,isMaker:value,is_maker:1}));
assert.equal(cases.length,265,'TRANSACTION original source corpus total');
for(const [label,patch] of [
 ['invalid price then qty throw',{executedPrice:0,executedQuantity:{toString:null}}],
 ['invalid qty then timestamp throw',{executedQuantity:0,executedTimestamp:{toString:null}}],
 ['invalid side skips numeric throw',{side:'unknown',executedPrice:{toString:null}}],
 ['ID and maker both fail coercion',{orderId:{toString:null},isMaker:{toString:null}}],
])capture(label,JSON.stringify({...base,...patch}));
const noFee={...base};delete noFee.fee;
capture('final fee null',JSON.stringify({...noFee,total_fee:null}));
capture('final fee absent',JSON.stringify(noFee));
const noPrice={...base};delete noPrice.executedPrice;
capture('final price null',JSON.stringify({...noPrice,lastPrice:null}));
assert.equal(cases.length,272,'TRANSACTION supplemented source corpus total');
const aliasInput={...base,feeAsset:{label:'before'}};
const aliasResult=worker.parseTransactionHistoryRow(aliasInput);
assert.equal(aliasResult.commissionAsset,aliasInput.feeAsset,'TRANSACTION source asset alias');
const before=pack(aliasResult.commissionAsset);
aliasInput.feeAsset.label='after';
const alias={before,after:pack(aliasResult.commissionAsset)};
process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:process.versions.node.split('.')[0],cases,alias}));
