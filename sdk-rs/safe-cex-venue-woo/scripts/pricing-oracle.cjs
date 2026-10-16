const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../..');
const names=['isPositiveFinitePrice','applyMarketIocSlippage','chooseMarketIocOrderPrice','getWooMarketPriceRangePct','isWooRateLimitError','isWooPositionModeAlreadySetError','isPostOnlyCrossingPrice','isPostOnlyCrossingFairPrice'];
function execute(source,context={}){const exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,...context});return exports;}
const typesSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8');const typesAst=ts.createSourceFile('types.ts',typesSource,ts.ScriptTarget.Latest,true);const types=execute(typesAst.statements.filter(ts.isEnumDeclaration).map(x=>x.getText(typesAst)).join('\n'));
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/woo/woo.exchange.ts'),'utf8'),ast=ts.createSourceFile('woo.ts',source,ts.ScriptTarget.Latest,true);
const selected=ast.statements.filter(ts.isVariableStatement).filter(s=>s.declarationList.declarations.some(d=>[...names,'DEFAULT_MARKET_IOC_PRICE_RANGE_PCT'].includes(d.name.getText(ast))));assert.equal(selected.length,names.length+1);
const api=execute(selected.map(s=>s.getText(ast)).join('\n')+'\nexport { '+names.join(',')+' };',{OrderSide:types.OrderSide});
if (process.argv.includes('--guard-audit')) {
 const poison={toString:null,valueOf:null};
 for(const response of [{status:429},{data:{code:-1003}},{data:{error_code:90003}}])
  assert.equal(api.isWooRateLimitError({response,message:poison}),true);
 for(const response of [{status:'429'},{data:{code:'-1003'}},{data:{error_code:'90003'}}])
  assert.equal(api.isWooRateLimitError({response,message:''}),false);
 assert.equal(api.isWooRateLimitError({response:{data:{message:0}},message:'RATE LIMIT'}),true);
 assert.equal(api.isWooRateLimitError({response:{data:{message:'other'}},message:'RATE LIMIT'}),false);
 assert.equal(api.isWooPositionModeAlreadySetError({response:{data:{message:false}},message:'POSITION MODE REMAINS UNCHANGED'}),true);
 assert.equal(api.isWooPositionModeAlreadySetError({response:{data:{message:'other'}},message:'POSITION MODE REMAINS UNCHANGED'}),false);
 const thrown={origin:'message'};
 assert.throws(()=>api.isWooPositionModeAlreadySetError({get message(){throw thrown;}}),e=>e===thrown);
 assert.equal(api.isWooRateLimitError({response:{status:429,get data(){throw thrown;}}}),true);
 const error=new Error('RATE LIMIT');
 assert.equal(api.isWooRateLimitError(error),true,'WOO-GUARD source Error message before projection');
 assert.equal(JSON.stringify(error),'{}');assert.equal(api.isWooRateLimitError(JSON.parse(JSON.stringify(error))),false);
 assert.throws(()=>api.isWooRateLimitError({message:poison}),e=>e.name==='TypeError');
 process.stdout.write(JSON.stringify({groups:2}));process.exit(0);
}
function encode(v){if(v===undefined)return {$undefined:true};if(typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0)))return {$number:Object.is(v,-0)?'-0':String(v)};if(Array.isArray(v))return v.map(encode);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)]));return v;}
const cases=[];function capture(name,...args){try{cases.push({name,args:encode(args),ok:encode(api[name](...args))});}catch(e){cases.push({name,args:encode(args),error:e.message,errorName:e.name});}}
const prices=[undefined,0,-0,-1,0.01,99,100,101,1e308,NaN,Infinity,-Infinity];
for(const value of prices)capture('isPositiveFinitePrice',value);
const tickers=[undefined,{mark:100,index:102,midPrice:103,last:104,bid:99,ask:101},{mark:0,index:100,bid:90,ask:110},{mark:NaN,index:0,midPrice:100,last:120,bid:99,ask:101},{mark:0,index:0,last:0,bid:99,ask:101},{mark:Infinity,index:-1,last:0,bid:NaN,ask:100}];
for(const side of Object.values(types.OrderSide)) {
 for(const price of prices.slice(1))for(const slip of [-1,0,0.5,2,NaN,Infinity])capture('applyMarketIocSlippage',price,side,slip);
 for(const ticker of tickers)for(const touch of [undefined,0,99,101,120,NaN])for(const slip of [-1,0,0.5,2,NaN])for(const cap of [0,1,30,NaN])capture('chooseMarketIocOrderPrice',ticker,side,slip,cap,touch);
 for(const price of prices.slice(1))for(const touch of prices.slice(1))capture('isPostOnlyCrossingPrice',side,price,touch);
 for(const price of prices.slice(1))for(const ticker of tickers)capture('isPostOnlyCrossingFairPrice',side,price,ticker);
}
for(const value of [undefined,null,false,true,'',0,-1,'0.01','0x10','\ufeff2','\u00852','12abc','1e400',[],[2],{}, {toString:null}])capture('getWooMarketPriceRangePct',{info:{price_range:value}});
for(const response of [undefined,null,{}, {status:429},{status:'429'},{data:{code:-1003}},{data:{code:'-1003'}},{data:{error_code:90003}},{data:{message:'RATE LIMIT'}},{data:{message:{toString:null}}},{data:{message:0}},{data:{message:'position mode remains unchanged'}}])for(const message of ['', 'RATE LIMIT', 'position mode remains unchanged',{toString:null}])for(const name of ['isWooRateLimitError','isWooPositionModeAlreadySetError'])capture(name,{response,message});
const file=path.join(__dirname,'../tests/fixtures/pricing.json'),bytes=JSON.stringify(cases,null,2)+'\n';if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),bytes,'WOO pricing/error source drift');else fs.writeFileSync(file,bytes);console.log(`WOO pricing/error oracle: ${cases.length} cases`);
