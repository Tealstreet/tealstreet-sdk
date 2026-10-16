import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import {root,ts,load} from './source.mjs';
const require=createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../mexc-oracle.cjs'));
const uniqBy=require('lodash/uniqBy');
const filename=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';let source=fs.readFileSync(filename,'utf8');
if(process.env.MEXC_POSITION_MARKET_SOURCE_CONTROL==='contract-fallback'){
 const anchor='fromMexcSymbol(row.symbol)';assert(source.includes(anchor),'SOURCE market lookup control anchor');
 source=source.replace(anchor,'fromMexcSymbol(row.symbol ?? row.contract)');
}
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcExchange');const method=klass.members.find(n=>n.name?.getText(ast)==='fetchPositions');assert(method,'SOURCE actual fetchPositions body');
const helpers=[];for(const n of ast.statements)if(ts.isVariableStatement(n))for(const d of n.declarationList.declarations)if(['extractData','extractRows'].includes(d.name.getText(ast)))helpers.push('const '+d.getText(ast)+';');assert.equal(helpers.length,2);
const compiled=ts.transpileModule(helpers.join('\n')+'\nclass Holder{'+method.getText(ast)+'}\nglobalThis.Holder=Holder;',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');const enums=load('packages/safe-cex/src/types.ts');
const base={symbol:'BTC_USDT',holdVol:10,holdAvgPrice:11,positionType:1,leverage:3,createTime:1700000000000};
const cases=[['symbol',[base],2,[1,0]],['contract-only',[{...base,symbol:undefined,contract:'BTC_USDT'}],2,[10,0]],['symbol-null',[{...base,symbol:null,contract:'BTC_USDT'}],2,[10,0]],['symbol-empty',[{...base,symbol:'',contract:'BTC_USDT'}],2,[0,0]],['null-row',[null],1,[]],['fresh-market',[base,{...base,positionType:2}],3,[1,3]]];
const output=[];
for(const [label,input,expectedGetters,expectedContracts]of cases){
 const trace=[],messages=[],lookups=[];let getterCalls=0,normalizations=0,fakeCalls=0,gets=0,positionReads=0,uniqCalls=0;
 let market=api.normalizeMarket({symbol:'BTC_USDT',contractSize:0.1,priceScale:2});
 const context={exports:{},ENDPOINTS:types.ENDPOINTS,PositionMode:enums.PositionMode,MarginType:enums.MarginType,PositionSide:enums.PositionSide,fromMexcSymbol:api.fromMexcSymbol,
 normalizePosition:(...args)=>{normalizations++;trace.push('normalize');lookups.push(args[1]?.contractSize??null);return api.normalizePosition(...args);},fakePositionsForMarket:(...args)=>{fakeCalls++;trace.push('fake');return api.fakePositionsForMarket(...args);},uniqBy:(...args)=>{uniqCalls++;trace.push('uniq');return uniqBy(...args);}};
 vm.runInNewContext(compiled,context);const owner=new context.Holder();const fallback=[];
 Object.assign(owner,{isPublicDataOnly:false,accountPositionMode:enums.PositionMode.Hedged,symbolSettings:new Map(),xhr:{async get(url){gets++;trace.push('get');assert.equal(url,'/api/v1/private/position/open_positions');return{data:{data:structuredClone(input)}};}},store:{get marketsBySymbol(){getterCalls++;trace.push('markets');if(label==='fresh-market'&&getterCalls===2)market=api.normalizeMarket({symbol:'BTC_USDT',contractSize:0.3,priceScale:2});return{BTCUSDT:market};},get positions(){positionReads++;trace.push('positions');return fallback;}},emitAndLogError(value){trace.push('report-child');messages.push(value);}});
 const pending=owner.fetchPositions(['BTCUSDT']);assert.equal(gets,1,'SOURCE position getter eager GET');const positions=await pending;
 assert.equal(getterCalls,expectedGetters,'SOURCE independently fixed getter floor '+label);assert.equal(gets,1,'SOURCE independent position getter GET floor');assert.deepEqual(Array.from(positions,p=>p.contracts),expectedContracts,'SOURCE independent decoded contracts '+label);
 const expectedTrace=label==='null-row'?['get','markets','report-child','positions']:label==='fresh-market'?['get','markets','normalize','markets','normalize','markets','fake','uniq']:['get','markets','normalize','markets','fake','uniq'];
 assert.deepEqual(trace,expectedTrace,'SOURCE independent market evaluation order '+label);
 const failed=label==='null-row';assert.equal(normalizations,failed?0:label==='fresh-market'?2:1,'SOURCE independent actual normalizer calls '+label);assert.equal(fakeCalls,failed?0:1,'SOURCE independent actual fake-position helper calls '+label);assert.equal(uniqCalls,failed?0:1,'SOURCE independent actual uniqBy calls '+label);assert.equal(messages.length,failed?1:0,'SOURCE independent report child calls '+label);assert.equal(positionReads,failed?1:0,'SOURCE independent fallback getter calls '+label);
 if(label==='contract-only'||label==='symbol-null')assert.deepEqual(lookups,[null],'SOURCE caller intentionally resolves market from row.symbol only');
 if(failed)assert.equal(messages[0],"Cannot read properties of null (reading 'symbol')");
 output.push({label,gets,getterCalls,normalizations,fakeCalls,uniqCalls,positionReads,messages,lookups,contracts:Array.from(positions,p=>p.contracts),symbols:Array.from(positions,p=>p.symbol),trace});
}
assert.equal(output.length,6,'SOURCE independent position getter root floor');for(const [field,expected]of [['gets',6],['getterCalls',12],['normalizations',6],['fakeCalls',5],['uniqCalls',5],['positionReads',1]])assert.equal(output.reduce((n,r)=>n+r[field],0),expected,'SOURCE independent selected call floor '+field);
assert.equal(output.reduce((n,r)=>n+r.messages.length,0),1);assert.equal(output.reduce((n,r)=>n+r.trace.length,0),36,'SOURCE independent selected host-call floor');console.log(JSON.stringify(output));
