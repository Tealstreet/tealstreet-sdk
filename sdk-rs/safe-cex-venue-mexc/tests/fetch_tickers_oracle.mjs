import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {load,ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_FETCH_TICKERS_CONTROL==='drop-last-duplicate'){
 const anchor="new Map(extractRows<Record<string, any>>(data).map((row) => [String(row.symbol), row]))";
 assert(source.includes(anchor),'fetchTickers duplicate-map mutation anchor missing');
 source=source.replace(anchor,"new Map(extractRows<Record<string, any>>(data).slice(0, 1).map((row) => [String(row.symbol), row]))");
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
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchTickers');
assert(method?.initializer&&ts.isArrowFunction(method.initializer),'fetchTickers initializer not found');
const initializer=`async function() ${method.initializer.body.getText(ast)}`;
const code=[...['extractData','extractRows'].map(name=>`const ${variables.get(name).getText(ast)}`),`exports.fetchTickers=${initializer};`].join('\n');
const js=ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:{TICKER:'/api/v1/contract/ticker'},normalizeTicker:utils.normalizeTicker});
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
const view=tickers=>tickers.map(t=>({id:t.id,symbol:t.symbol,bid:bits(t.bid),ask:bits(t.ask),last:bits(t.last),mark:bits(t.mark),index:bits(t.index),percentage:bits(t.percentage),openInterest:bits(t.openInterest),fundingRate:bits(t.fundingRate),fundingTime:bits(t.fundingTime),volume:bits(t.volume),quoteVolume:bits(t.quoteVolume)}));
const markets=[
 {symbol:'BTCUSDT',id:'BTC_USDT',base:'BTC',quote:'USDT',contractSize:0.001},
 {symbol:'ETHUSDT',id:'ETH_USDT',base:'ETH',quote:'USDT',contractSize:0.01},
];
const raw=[
 {symbol:'BTC_USDT',lastPrice:'10.25',bid1:'10',ask1:'10.5',indexPrice:'10.1',riseFallRate:'0.025',holdVol:'4',fundingRate:'0.0001',fundingTime:1700000000000,volume24:'5',amount24:'51'},
 {symbol:'OTHER_USDT',lastPrice:'99'},
 {symbol:'BTC_USDT',lastPrice:'11.25',bid1:'11',ask1:'11.5',indexPrice:'11.1',riseFallRate:'0.05',holdVol:'2',fundingTime:1700000001000,volume24:'8',amount24:'90'},
 {symbol:'ETH_USDT',lastPrice:'20',markPrice:'21',bidPrice:'19',askPrice:'22',openInterest:'3',nextSettleTime:1700000002000,volume:'6',quoteVolume:'120'},
];
const instance={xhr:{get:async url=>{assert.equal(url,'/api/v1/contract/ticker');return {data:raw};}},store:{markets:markets.map(m=>({...m}))},emitAndLogError(error){throw Error(`unexpected fetchTickers error: ${error}`);}};
const tickers=await module.exports.fetchTickers.call(instance);
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),markets,raw,tickers:view(tickers),alias:tickers[0].info===raw[2]}));
