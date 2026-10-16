import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const read=file=>fs.readFileSync(new URL(file,root),'utf8');
const parse=file=>ts.createSourceFile(file,read(file),ts.ScriptTarget.Latest,true);
const runtime=id=>require(process.env.SAFE_CEX_TYPESCRIPT_PATH?path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',id):id);
const evaluate=(source,imports={})=>{const exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:id=>{if(id.startsWith('lodash/'))return runtime(id);assert.ok(id in imports,`unexpected import ${id}`);return imports[id];}});return exports;};
const types=parse('types.ts'),side=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='PositionSide');assert.ok(side);
const enums=evaluate(side.getText(types));
const helpers={...evaluate(read('utils/splitTimeRange.ts')),...evaluate(read('utils/omitUndefined.ts')),...evaluate(read('exchanges/binance/binance.closed-pnl.ts'),{'../../types':enums})};
const declaration=(file,name)=>{const ast=parse(file),node=ast.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.text===name);assert.ok(node);return node.initializer.getText(ast);};
const ast=parse('exchanges/binance/binance.exchange.ts'),klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BinanceExchange'),method=klass.members.find(n=>n.name?.getText(ast)==='deriveClosedPositions');assert.ok(method);
let methodSource=method.initializer.getText(ast);
if(process.env.BINANCE_READER_CONTROL==='skip-get')methodSource=methodSource.replace('await this.xhr.get','await this.skipped.get');
const exports={};let clockCalls=0;
vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${declaration('exchanges/binance/binance.types.ts','ENDPOINTS')};const USER_TRADES_MAX_WINDOW_MS=${declaration('exchanges/binance/binance.utils.ts','USER_TRADES_MAX_WINDOW_MS')};exports.fn=function(opts){return (${methodSource})(opts);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...helpers,Date:{now:()=>{clockCalls++;return 3000000000;}}});
const bits=v=>{const buffer=Buffer.alloc(8);buffer.writeDoubleBE(v);return {bits:buffer.toString('hex')};};
const norm=v=>typeof v==='number'?bits(v):Array.isArray(v)?v.map(norm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,norm(x)])):v;
const base={realizedPnl:'5',time:123,symbol:'BTCUSDT',price:'12',qty:'2',quoteQty:'25',side:'SELL',commission:'0.1'};
const cap=7*24*60*60*1000;
const bounds=[{}, {from:0,to:0},{from:0,to:cap},{from:0,to:cap+1},{from:0,to:cap*2+2},{from:2,to:1},{from:-0,to:1},{from:-2,to:2},{from:0.5,to:cap+1.5}];
const pages=[[],[base],[{...base,time:4,symbol:'first'}, {...base,time:8}, {...base,time:4,symbol:'last'}],[null,base],[false,12,'x',[],{},base],[{...base,price:{toString:null}}],[{...base,realizedPnl:0,time:{toString:null}},base],null,{},'123',[{...base,time:'Infinity'},{...base,time:'-Infinity'}],[{...base,price:'1e308',qty:'1e308',quoteQty:0}]];
const cases=[];
for(const opts of bounds)for(let page=0;page<pages.length;page++)for(const reject of [-1,0,1])for(const emitThrows of [false,true]){
 const input={opts:{symbol:'BTCUSDT',accountName:'acct',limit:2,fromId:9,...opts},publicOnly:false,page:pages[page],reject,emitThrows,msg:page%3===0?'venue message':page%3===1?0:{detail:'venue'}};
 if(Object.is(opts.from,-0))input.special={bound:'from',value:'-0'};
 cases.push(input);
}
for(const publicOnly of [false,true])for(const symbol of ['', 'BTCUSDT'])cases.push({opts:{symbol},publicOnly,page:[base],reject:-1,emitThrows:false});
for(const bound of ['from','to'])for(const value of ['NaN','Infinity','-Infinity'])cases.push({opts:{symbol:'BTCUSDT',from:0,to:1},special:{bound,value},page:[base],reject:-1,emitThrows:false});
for(const input of cases){
 clockCalls=0;const trace=[],opts={...input.opts};if(input.special)opts[input.special.bound]=Number(input.special.value);
 let requestIndex=0,result=null,error=null;
 const emitFailure=Object.assign(SyntaxError('fixture emitter'),{code:'emit-code'});
 const instance={isPublicDataOnly:input.publicOnly,xhr:{get:async(endpoint,{params})=>{trace.push({get:endpoint,params:norm(params),keys:Object.keys(params)});const index=requestIndex++;if(index===input.reject)throw Object.assign(RangeError('fixture request'),{response:{data:{msg:input.msg}}});return {data:input.page};}},emitAndLogError:message=>{trace.push({emit:message});if(input.emitThrows)throw emitFailure;}};
 try{result=norm(await exports.fn.call(instance,opts));}catch(e){error={name:e.name,message:e.message,code:e.code??null,same:e===emitFailure};}
 input.expected={trace,result,error,clockCalls};
}
assert.equal(cases.length,658);
assert.equal(cases[0].expected.trace.filter(v=>v.get).length,5,'default thirty-day request count');
process.stdout.write(JSON.stringify(cases));
