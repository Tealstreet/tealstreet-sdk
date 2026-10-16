const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=path.resolve(__dirname,'../../..');
function execute(source,imports={}){const exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:name=>{assert.ok(name in imports,name);return imports[name];}});return exports;}
const typesSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8');
const typesAst=ts.createSourceFile('types.ts',typesSource,ts.ScriptTarget.Latest,true);
const types=execute(typesAst.statements.filter(ts.isEnumDeclaration).map(s=>s.getText(typesAst)).join('\n'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/woo/woo.order-history.ts');
const api=execute(fs.readFileSync(filename,'utf8')+'\nexport {toMilliseconds,toOptionalNumber,hasValue,toNumber,ORDER_TYPE,ORDER_STATUS};',{'../../types':types});
const base={order_id:123,symbol:'PERP_BTC_USDT',created_time:'1786536593.515',updated_time:'1786536594.125',side:'BUY',type:'LIMIT',status:'FILLED',price:'100',average_executed_price:'101',quantity:'3',executed:'2',reduce_only:false,position_side:'LONG',realized_pnl:'0',total_fee:'0.25',fee_asset:'USDT',total_rebate:'0',rebate_asset:'USDT'};
const values=[undefined,null,'',false,true,0,-1,0.5,'0','12abc','0x10','1e400','1e308','\ufeff2','\u00852',[],[1,2],{}, {toString:null},'constructor','__proto__','toString','BUY','SELL','short','true','MARKET','CANCELLED'];
const cases=[];
function capture(name,input){try{cases.push({name,input:input??null,missing:input===undefined,ok:api[name](input,id=>id==='PERP_BTC_USDT'?'BTCUSDT':undefined)??null});}catch(e){cases.push({name,input:input??null,missing:input===undefined,error:e.message});}}
for(const name of ['toMilliseconds','toOptionalNumber','hasValue','toNumber'])for(const value of values)capture(name,value);
capture('mapWooHistoricalOrder',base);
for(const key of Object.keys(api.ORDER_TYPE))capture('mapWooHistoricalOrder',{...base,type:key});
for(const key of Object.keys(api.ORDER_STATUS))capture('mapWooHistoricalOrder',{...base,status:key});
capture('mapWooHistoricalOrder',{...base,created_time:'1.0009'});

for(const key of Object.keys(base))for(const value of values){const row={...base};if(value===undefined)delete row[key];else row[key]=value;capture('mapWooHistoricalOrder',row);}
try { api.mapWooHistoricalOrder(base,()=>{throw new Error('resolver failed');});assert.fail('resolver throw was swallowed'); } catch(error) { assert.equal(error.message,'resolver failed');cases.push({name:'mapWooHistoricalOrder',input:base,resolveError:true,error:error.message}); }
// Observe the branch and resulting open string before JSON can erase Infinity.
const assetCases=[];
for(const [field, output] of [['fee_asset','feeAsset'],['rebate_asset','rebateAsset']])for(const lexeme of ['-0','-0.0','-1e-4000','1e400','-1e400','1','1e-4000']) {
 const input={...base,[field]:JSON.parse(lexeme)};
 const result=api.mapWooHistoricalOrder(input,()=> 'BTCUSDT');
 assetCases.push({field,lexeme,asset:result[output]??null});
}
const assetFile=path.join(__dirname,'../tests/fixtures/history-assets.json');
const assetBytes=JSON.stringify(assetCases,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(assetFile,'utf8'),assetBytes,'history asset truthiness/string source drift');else fs.writeFileSync(assetFile,assetBytes);
const data={tables:{ORDER_TYPE:Object.entries(api.ORDER_TYPE),ORDER_STATUS:Object.entries(api.ORDER_STATUS)},cases};
const file=path.join(__dirname,'../tests/fixtures/history.json'),bytes=JSON.stringify(data,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),bytes,'WOO history mapper source drift');else fs.writeFileSync(file,bytes);
console.log(`WOO history: ${cases.length} live cases`);
