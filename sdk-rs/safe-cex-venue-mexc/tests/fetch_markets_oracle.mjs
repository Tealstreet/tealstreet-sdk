import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
import {fileURLToPath} from 'node:url';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_FETCH_MARKETS_CONTROL==='accept-hidden'){
 const before='.filter(isTradableMexcMarket)';
 assert(source.includes(before),'missing fetchMarkets mutation anchor');
 source=source.replace(before,'.filter(() => true)');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const variables=new Map();
for(const statement of ast.statements){
 if(!ts.isVariableStatement(statement))continue;
 for(const declaration of statement.declarationList.declarations){
  if(['extractData','extractRows'].includes(declaration.name.getText(ast)))variables.set(declaration.name.getText(ast),declaration);
 }
}
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange class not found');
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchMarkets');
assert(method?.initializer,'fetchMarkets initializer not found');
assert(ts.isArrowFunction(method.initializer),'fetchMarkets is no longer a class-field arrow');
const initializer=`async function() ${method.initializer.body.getText(ast)}`;
const code=[...['extractData','extractRows'].map(name=>`const ${variables.get(name).getText(ast)}`),`exports.fetchMarkets=${initializer};`].join('\n');
const js=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:{CONTRACT_DETAIL:'/api/v1/contract/detail'},isTradableMexcMarket:utils.isTradableMexcMarket,normalizeMarket:utils.normalizeMarket});
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
const view=markets=>markets.map(m=>({id:m.id,symbol:m.symbol,base:m.base,quote:m.quote,active:m.active,amount:bits(m.precision.amount),price:bits(m.precision.price),min:bits(m.limits.amount.min),max:bits(m.limits.amount.max),minLeverage:bits(m.limits.leverage.min),maxLeverage:bits(m.limits.leverage.max)}));
const tradable={symbol:'BTC_USDT',settleCoin:'USDT',contractSize:0.0001,volUnit:3,minVol:2,maxVol:100000,priceScale:5,minLeverage:4,maxLeverage:100};
const cases=[
 {payload:{code:0,data:[tradable,{...tradable,symbol:'BAD_BTC'},{...tradable,symbol:'HIDDEN_USDT',isHidden:true},{...tradable,symbol:'DISABLED_USDT',apiAllowed:false},{...tradable,symbol:'INACTIVE_USDC',state:1},{...tradable,symbol:'ETH_USDC',quoteCoin:'USDC'}]}},
 {payload:[{...tradable,symbol:'SOL_USDT',state:0},{...tradable,symbol:'XRP_USDT',state:1}]},
 {payload:{code:0,data:[]}},
];
const output=[];
for(const scenario of cases){
 const instance={xhr:{get:async url=>{assert.equal(url,'/api/v1/contract/detail');return {data:scenario.payload};}},store:{markets:[]},emitAndLogError(error){throw Error(`unexpected fetchMarkets error: ${error}`);}};
 const markets=await module.exports.fetchMarkets.call(instance);
 const rawRows=Array.isArray(scenario.payload)?scenario.payload:scenario.payload.data;
 const firstRaw=markets.length?rawRows.find(row=>row.symbol===markets[0].id):undefined;
 let alias={returnMutatesInput:false,inputMutatesReturn:false};
 if(firstRaw){
  markets[0].info.__fromMarket='market';
  alias.returnMutatesInput=firstRaw.__fromMarket==='market';
  firstRaw.__fromInput='input';
  alias.inputMutatesReturn=markets[0].info.__fromInput==='input';
 }
 output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),payload:scenario.payload,markets:view(markets),alias});
}
process.stdout.write(JSON.stringify(output));
