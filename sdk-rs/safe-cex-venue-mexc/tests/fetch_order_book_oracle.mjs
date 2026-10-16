import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {load,ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_FETCH_ORDER_BOOK_TS_CONTROL==='replace-version'){
 const before='lastUpdateId: toIntegerString(extractData(data)?.version),';
 assert(source.includes(before),'fetchOrderBook version mutation anchor');
 source=source.replace(before,"lastUpdateId: 'broken-version',");
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange class exists');
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchOrderBook');
assert(method?.initializer&&ts.isArrowFunction(method.initializer),'fetchOrderBook initializer exists');
const variables=new Map();
for(const statement of ast.statements){
 if(!ts.isVariableStatement(statement))continue;
 for(const declaration of statement.declarationList.declarations){
  if(declaration.name.getText(ast)==='extractData')variables.set('extractData',declaration);
 }
}
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const code=`const ${variables.get('extractData').getText(ast)}\nexports.fetchOrderBook=async function(symbol,limit=100) ${method.initializer.body.getText(ast)};`;
const js=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:types.ENDPOINTS,normalizeOrderBook:utils.normalizeOrderBook,toIntegerString:utils.toIntegerString});
const marketRaw={symbol:'BTC_USDT',contractSize:0.1,volUnit:3,minVol:1,maxVol:100000,priceScale:2,minLeverage:4,maxLeverage:100};
const market=utils.normalizeMarket(marketRaw);
const payload={data:{version:1700000000456,bids:[[99,3],[100,2]],asks:[[102,5],[101,4]]}};
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
const view=book=>({bids:book.bids.map(row=>({price:bits(row.price),amount:bits(row.amount),total:bits(row.total)})),asks:book.asks.map(row=>({price:bits(row.price),amount:bits(row.amount),total:bits(row.total)}))});
const output=[];
for(const scenario of [{name:'default-limit',symbol:'BTCUSDT'},{name:'custom-limit',symbol:'BTCUSDT',limit:20},{name:'missing-market',symbol:'NOPE'}]){
 const requests=[];
 const instance={store:{marketsBySymbol:{BTCUSDT:market}},xhr:{get:async(url,config)=>{requests.push({url,config});return {data:payload};}}};
 try{
  const book=await module.exports.fetchOrderBook.call(instance,scenario.symbol,scenario.limit);
  assert.equal(book.lastUpdateId,'1700000000456','MEXC TypeScript lastUpdateId field identity');
  output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),name:scenario.name,symbol:scenario.symbol,limit:scenario.limit??null,marketRaw,book:view(book),lastUpdateId:book.lastUpdateId,requests:requests.map(request=>({url:request.url,params:request.config.params}))});
 }catch(error){
  if(error?.code==='ERR_ASSERTION')throw error;
  output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),name:scenario.name,symbol:scenario.symbol,limit:scenario.limit??null,error:String(error?.message??error),requests});
 }
}
if(process.env.MEXC_FETCH_ORDER_BOOK_TS_CONTROL==='empty-source') output.length=0;
process.stdout.write(JSON.stringify(output));
