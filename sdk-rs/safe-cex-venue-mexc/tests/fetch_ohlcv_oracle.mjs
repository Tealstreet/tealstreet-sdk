import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
import {load,ts} from './source.mjs';

const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts',import.meta.url));
let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_FETCH_OHLCV_CONTROL==='reverse-sort'){
 const before='.sort((a, b) => a.timestamp - b.timestamp)';
 assert(source.includes(before),'missing fetchOHLCV sort mutation anchor');
 source=source.replace(before,'.sort((a, b) => b.timestamp - a.timestamp)');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(node=>ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange');
assert(klass,'MexcExchange class not found');
const method=klass.members.find(node=>ts.isPropertyDeclaration(node)&&node.name.getText(ast)==='fetchOHLCV');
assert(method?.initializer&&ts.isArrowFunction(method.initializer),'fetchOHLCV initializer not found');
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const omit=load('packages/safe-cex/src/utils/omitUndefined.ts').omitUndefined;
const omitUndefined=omit;
const initializer=`async function(opts) ${method.initializer.body.getText(ast)}`;
const js=ts.transpileModule(`exports.fetchOHLCV=${initializer};`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`,{module,ENDPOINTS:types.ENDPOINTS,INTERVAL:types.INTERVAL,omitUndefined,normalizeTimestampToSeconds:utils.normalizeTimestampToSeconds,normalizeKlineResponse:utils.normalizeKlineResponse,toMexcSymbol:utils.toMexcSymbol});
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
const localOffsetMs=new Date('2000-01-01T00:00:00').getTime()-Date.UTC(2000,0,1);
const cases=[
 {options:{symbol:'BTCUSDT',interval:'1m',from:1700000000.999,to:1700000123},payload:{data:[{time:1700000002,open:'2',high:'3',low:'1',close:'2.5',vol:'4'},{time:1700000000,open:'1',high:'2',low:'0.5',close:'1.5',vol:'8'}]}},
 {options:{symbol:'ETH_USDC',interval:'4h'},payload:{time:[1700000000,1700003600],open:[2,1],high:[3,2],low:[1,0],close:[2.5,1.25],amount:[5,6]}},
 {options:{symbol:'SOLUSDT',interval:'1d'},payload:{data:[{time:0,o:1,h:2,l:0.25,c:1.25,q:3}]}},
 {options:{symbol:'BTCUSDT',interval:'2h'},payload:{data:[]}},
 {options:{symbol:'BTCUSDT',interval:'constructor'},payload:{data:[]}},
];
const output=[];
for(const scenario of cases){
 const requests=[];
 const instance={xhr:{get:async(url,config)=>{requests.push({url,config});return {data:scenario.payload};}}};
 try{
  const candles=await module.exports.fetchOHLCV.call(instance,scenario.options);
  output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),options:scenario.options,payload:scenario.payload,localOffsetMs,error:null,requests:requests.map(r=>({url:r.url,method:'get',params:Object.fromEntries(Object.entries(r.config.params).map(([key,value])=>[key,typeof value==='function'?{typeof:typeof value,string:String(value)}:value])),timeout:r.config.timeout})),intervalType:typeof (types.INTERVAL[scenario.options.interval]),candles:candles.map(c=>({timestamp:bits(c.timestamp),open:bits(c.open),high:bits(c.high),low:bits(c.low),close:bits(c.close),volume:bits(c.volume),lastSide:c.lastSide??null}))});
 }catch(error){output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),options:scenario.options,payload:scenario.payload,localOffsetMs,error:String(error?.message??error),requests,candles:null});}
}
if(process.env.MEXC_FETCH_OHLCV_CONTROL==='empty-source')output.length=0;
process.stdout.write(JSON.stringify(output));
