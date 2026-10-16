'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,extras=[],imports={}){const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8')+(extras.length?'\nexport { '+extras.join(',')+' };':''),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(n){assert.ok(Object.hasOwn(imports,n),'actual import '+n);return imports[n];}});return exports;}
const helpers=['requireRecord','requireBoolean','requireInteger','requireString','requireEnum','requireDecimal','decimalParts','compareDecimals','requirePerpetualSymbol','validateTarget','requireArray','validateCommon','hasUnsupportedTrigger','targetMatchesOrder','decodePlaceRow','decodeSuccessData','decodeMutation'];
const api=load(path.join(root,'woo.v3.algo-writes.ts'),helpers,{'./woo.v3.contracts':load(path.join(root,'woo.v3.contracts.ts'))});
if (process.argv.includes('--declaration-audit')) {
 const entry=path.join(root,'__algo_declaration_probe.ts'),filename=path.join(root,'woo.v3.algo-writes.ts');
 const probe=`import type {WooV3PositionalTriggerInput,WooV3AlgoPlaceResponseRow} from './woo.v3.algo-writes';
const unit=String.fromCharCode(0xd800);
const makeTrigger=(value:string):WooV3PositionalTriggerInput=>({triggerPrice:value,triggerPriceType:'MARKET_PRICE'});
const makeRow=(value:string):WooV3AlgoPlaceResponseRow=>({algoOrderId:71,clientAlgoOrderId:83,algoType:'STOP',quantity:value});
export const readTrigger=(input:WooV3PositionalTriggerInput):string=>input.triggerPrice;
export const readRow=(input:WooV3AlgoPlaceResponseRow):string=>input.quantity;
export const values=[readTrigger(makeTrigger(unit)),readRow(makeRow(unit))];
export const positiveViews=[String.fromCodePoint(0x10000),String.fromCharCode(92,117,100,56,48,48)].map(value=>[value,readTrigger(makeTrigger(value)),readRow(makeRow(value))]);`;
 const options={noEmit:true,strict:true,skipLibCheck:true,target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,types:[],esModuleInterop:true,resolveJsonModule:true};
 const original=fs.readFileSync(filename,'utf8');
 function check(source){const host=ts.createCompilerHost(options),read=host.readFile.bind(host),exists=host.fileExists.bind(host);host.fileExists=p=>p===entry||exists(p);host.readFile=p=>p===entry?probe:p===filename?source:read(p);return ts.getPreEmitDiagnostics(ts.createProgram([entry],options,host));}
 const diagnostics=check(original);assert.equal(diagnostics.length,0,'WOO-ALGO-DECL source maintained aliases typecheck: '+ts.formatDiagnostics(diagnostics,{getCanonicalFileName:x=>x,getCurrentDirectory:()=>root,getNewLine:()=> '\n'}));
 const typed={};vm.runInNewContext(ts.transpileModule(probe,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:typed});
 for(const value of typed.values){assert.equal(typeof value,'string');assert.equal(value.length,1);assert.equal(value.charCodeAt(0),0xd800,'WOO-ALGO-DECL source typed returned lone field');}
 assert.equal(typed.positiveViews.length,2);for(const [expected,trigger,row]of typed.positiveViews){assert.equal(trigger,expected);assert.equal(row,expected);}
 let mutated=original;
 for(const [name,field]of [['WooV3PositionalTriggerInput','triggerPrice'],['WooV3AlgoPlaceResponseRow','quantity']]){
  const start=mutated.indexOf('export type '+name+' ='),end=mutated.indexOf('\n}>;',start)+4,block=mutated.slice(start,end),needle=field+': WooV3Decimal';assert.ok(start>=0&&end>start&&block.includes(needle));mutated=mutated.slice(0,start)+block.replace(needle,field+': number')+mutated.slice(end);
 }
 const negative=check(mutated).filter(d=>d.file&&d.file.fileName===entry);assert.equal(negative.length,4,'WOO-ALGO-DECL checker rejects both changed field declarations');for(const d of negative)assert.equal(d.code,2322);
 const common={symbol:'PERP_BTC_USDT',side:'BUY',positionMode:'HEDGE_MODE',clientAlgoOrderId:83,algoOrderTag:'decl-19'};
 const stop={...common,family:'STOP_MARKET',quantity:'3.25',triggerPrice:'101.25',triggerPriceType:'MARKET_PRICE'};
 const trailing={...common,family:'TRAILING_STOP',quantity:'4.125',callback:{source:'WOO_NATIVE_CALLBACK_VALUE',value:'.25'},activatedPrice:'97.5'};
 const positional={...common,family:'POSITIONAL_TP_SL',takeProfit:{triggerPrice:'121.25',triggerPriceType:'MARKET_PRICE'},stopLoss:{triggerPrice:'81.5',triggerPriceType:'INDEX_PRICE'}};
 assert.equal(api.buildWooV3AlgoPlaceRequest(positional).ready,false);positional.stopLoss.triggerPriceType='MARKET_PRICE';
 for(const input of [stop,trailing,positional]){
  const out=api.buildWooV3AlgoPlaceRequest({...input,algoOrderTag:String.fromCharCode(0xd800)});assert.equal(out.ready,true);assert.equal(typeof out.request.body.algoOrderTag,'string');assert.equal(out.request.body.algoOrderTag.length,1);assert.equal(out.request.body.algoOrderTag.charCodeAt(0),0xd800,'WOO-ALGO-DECL source family/result lone tag');
 }
 const ownUndefined={...stop,clientAlgoOrderId:undefined,algoOrderTag:undefined};assert.ok(Object.hasOwn(ownUndefined,'algoOrderTag'));const body=api.buildWooV3AlgoPlaceRequest(ownUndefined).request.body;assert.equal(Object.hasOwn(body,'algoOrderTag'),false);assert.equal(Object.hasOwn(body,'clientAlgoOrderId'),false);
 const result=api.buildWooV3AlgoPlaceRequest(positional);assert.ok(Object.isFrozen(result)&&Object.isFrozen(result.request)&&Object.isFrozen(result.request.body));assert.notEqual(result.request.body.childOrders[0],positional.takeProfit);positional.takeProfit.triggerPrice='999';assert.equal(result.request.body.childOrders[0].triggerPrice,'121.25');
 process.stdout.write(JSON.stringify({groups:8,typedLoneFields:2,checkerControlErrors:4}));process.exit(0);
}
if (process.argv.includes('--guard-audit')) {
 const policy=api.WOO_V3_ALGO_WRITE_PARITY;
 assert.equal(Object.keys(policy).length,10);assert.equal(policy.requestAndResponseFixtures,true);assert.equal(policy.runtimeImported,false);assert.equal(policy.credentialedDemoLifecycle,false);
 assert.equal(policy.triggerPriceType,'MARKET_PRICE-only');assert.equal(policy.trailingInput,'vendor-native-callbackValue-no-safe-cex-conversion');assert.equal(policy.positionalResponseIdentity,'blocked-pending-demo-evidence');assert.equal(policy.positionalUpdate,'blocked-official-edit-schema-has-no-childOrders');assert.equal(policy.partialFillAndQuantityAmend,'blocked-pending-demo-evidence');assert.equal(policy.symbolPendingCancel,'blocked-published-query-schema-conflicts-with-prose');assert.equal(policy.cancelResponseIdentity,'request-target-only');
 assert.ok(Object.isFrozen(policy));assert.throws(()=>{policy.runtimeImported=true;},TypeError);assert.equal(policy.runtimeImported,false);

 const input={kept:41,z:-0,nested:{count:43}},record=api.requireRecord(input,'probe');
 assert.equal(record,input,'WOO-ALGO-GUARD source record identity');assert.equal(record.kept,41);assert.ok(Object.is(record.z,-0));assert.equal(record.nested,input.nested);input.nested.count=47;assert.equal(record.nested.count,47);
 const inherited=Object.create({kept:61});assert.equal(api.requireRecord(inherited,'probe').kept,61);
 const sentinel=Error('original getter');const getter={get kept(){throw sentinel;}};assert.equal(api.requireRecord(getter,'probe'),getter);assert.throws(()=>api.requireRecord(getter,'probe').kept,e=>e===sentinel);
 const array=[-0,Infinity,{count:53}],out=api.requireArray(array,'probe');assert.equal(out,array);assert.equal(out.length,3);assert.ok(Object.is(out[0],-0));assert.equal(out[1],Infinity);assert.equal(out[2],array[2]);array[2].count=59;assert.equal(out[2].count,59);
 const sparse=Array(3);sparse[2]=67;assert.equal(api.requireArray(sparse,'probe'),sparse);assert.equal(0 in sparse,false);assert.equal(sparse[2],67);
 for(const value of [null,[],0])assert.throws(()=>api.requireRecord(value,'probe'),/probe must be an object/);
 assert.throws(()=>api.requireArray({},'probe'),/probe must be an array/);
 for(const value of [true,false])assert.equal(api.requireBoolean(value,'probe'),value);
 for(const value of [0,1,null,'true'])assert.throws(()=>api.requireBoolean(value,'probe'),/probe must be a boolean/);
 assert.ok(Object.is(api.requireInteger(-0,'probe'),-0));assert.equal(api.requireInteger(9007199254740991,'probe',1),9007199254740991);assert.equal(api.requireInteger(1,'probe',NaN),1);
 for(const value of [1.25,9007199254740992,Infinity,'1',NaN])assert.throws(()=>api.requireInteger(value,'probe'),/probe must be a safe integer of at least 0/);
 assert.throws(()=>api.requireInteger(0,'probe',1),/probe must be a safe integer of at least 1/);
 assert.equal(api.requireString('  ','probe'),'  ');for(const value of ['',0,null])assert.throws(()=>api.requireString(value,'probe'),/probe must be a non-empty string/);
 assert.equal(api.requireEnum('B',['A','B'],'probe'),'B');assert.throws(()=>api.requireEnum('C',['B','A'],'probe'),/probe must be one of B, A/);
 const allowed=['A'];let calls=0;allowed.includes=function(value){assert.equal(this,allowed);assert.equal(value,'C');calls++;return true;};assert.equal(api.requireEnum('C',allowed,'probe'),'C');assert.equal(calls,1);
 assert.equal(api.requirePerpetualSymbol('PERP_1_2'),'PERP_1_2');for(const symbol of ['SPOT_BTC_USDT','PERP_btc_USDT','PERP_BTC_USDT\n','PERP__USDT'])assert.throws(()=>api.requirePerpetualSymbol(symbol),/WOO V3 symbol must use PERP_BASE_QUOTE form/);
 for(const [value,expected]of [['MARKET_PRICE',false],['MARK_PRICE',true],['INDEX_PRICE',true],['LAST_PRICE',true],['UNKNOWN',true]])assert.equal(api.hasUnsupportedTrigger(value),expected);
 const unit=JSON.parse('"\\ud800"');
 function retained(value,label){assert.equal(typeof value,'string');assert.equal(value.length,1);assert.equal(value.charCodeAt(0),0xd800,'WOO-ALGO-GUARD source '+label+' retained lone value');}
 retained(api.requireRecord({kept:unit},'probe').kept,'record');retained(api.requireArray([unit],'probe')[0],'array');retained(api.requireString(unit,'probe'),'string');retained(api.requireEnum(unit,[unit],'probe'),'enum');
 for(const value of ['\ud800\udc00',String.raw`\ud800`]){assert.equal(api.requireRecord({kept:value},'probe').kept,value);assert.equal(api.requireArray([value],'probe')[0],value);assert.equal(api.requireString(value,'probe'),value);assert.equal(api.requireEnum(value,[value],'probe'),value);}
 const regex=api.requirePerpetualSymbol.constructor('return RegExp.prototype')(),oldTest=regex.test;
 try{regex.test=()=>true;assert.equal(api.requirePerpetualSymbol('NOT_PERP'),'NOT_PERP');}finally{regex.test=oldTest;}
 const number=api.requireInteger.constructor('return Number')(),oldInteger=number.isSafeInteger;
 try{number.isSafeInteger=()=>true;assert.equal(api.requireInteger(1.25,'probe'),1.25);}finally{number.isSafeInteger=oldInteger;}
 process.stdout.write(JSON.stringify({groups:8,returnedLoneRows:4}));process.exit(0);
}
if (process.argv.includes('--request-audit')) {
 const common={symbol:'PERP_BTC_USDT',side:'BUY',positionMode:'HEDGE_MODE',clientAlgoOrderId:83,algoOrderTag:'audit-19'};
 const stop={...common,family:'STOP_MARKET',quantity:'0003.2500',triggerPrice:'101.25',triggerPriceType:'MARKET_PRICE'};
 const trailing={...common,family:'TRAILING_STOP',quantity:'4.125',callback:{source:'WOO_NATIVE_CALLBACK_VALUE',value:'.25'},activatedPrice:'97.5'};
 const positional={...common,family:'POSITIONAL_TP_SL',takeProfit:{triggerPrice:'121.25',triggerPriceType:'MARKET_PRICE'},stopLoss:{triggerPrice:'81.5',triggerPriceType:'MARKET_PRICE'}};
 assert.equal(api.requireDecimal('0003.2500','probe'),'0003.2500');assert.equal(api.requireDecimal('0','probe',true),'0');
 assert.throws(()=>api.requireDecimal('0','probe'),/probe must be greater than zero/);
 assert.throws(()=>api.requireDecimal('1e3','probe'),/probe must be a finite non-negative decimal string/);
 const target=api.validateTarget({algoOrderId:71});assert.equal(target.algoOrderId,71);assert.ok(Object.isFrozen(target));
 const inherited=Object.create({clientAlgoOrderId:83});assert.equal(api.validateTarget(inherited).clientAlgoOrderId,83);
 assert.throws(()=>api.validateTarget({algoOrderId:0,clientAlgoOrderId:83}),/requires exactly one ID/);
 const normalized=api.validateCommon(common);assert.equal(normalized.positionSide,'SHORT');assert.equal(normalized.clientAlgoOrderId,83);assert.ok(Object.isFrozen(normalized));
 assert.equal(api.validateCommon({...common,side:'SELL'}).positionSide,'LONG');assert.equal(api.validateCommon({...common,positionMode:'ONE_WAY'}).positionSide,'BOTH');
 const objectTag={kept:41};assert.equal(api.validateCommon({...common,algoOrderTag:objectTag}).algoOrderTag,objectTag);
 assert.equal(api.validateCommon({...common,algoOrderTag:'😀'.repeat(32)}).algoOrderTag.length,64);
 assert.throws(()=>api.validateCommon({...common,algoOrderTag:'😀'.repeat(33)}),/algoOrderTag must contain 1 to 64/);
 const built=api.buildWooV3AlgoPlaceRequest(stop);assert.equal(built.ready,true);assert.equal(built.request.method,'POST');assert.equal(built.request.path,'/v3/trade/algoOrder');
 assert.equal(built.request.body.positionSide,'SHORT');assert.equal(built.request.body.quantity,'0003.2500');assert.equal(built.request.body.triggerPrice,'101.25');
 assert.ok(Object.isFrozen(built)&&Object.isFrozen(built.request)&&Object.isFrozen(built.request.body));
 assert.equal(api.buildWooV3AlgoPlaceRequest({...stop,triggerPriceType:'INDEX_PRICE',quantity:'0'}).blocker,'only-market-price-trigger-is-documented');
 const trail=api.buildWooV3AlgoPlaceRequest(trailing);assert.equal(trail.request.body.callbackValue,'.25');assert.equal(trail.request.body.activatedPrice,'97.5');
 const positionBuild=api.buildWooV3AlgoPlaceRequest(positional);
 for(const [value,kind,count]of [[built,'STOP',12],[trail,'TRAILING_STOP',12],[positionBuild,'POSITIONAL_TP_SL',7]]) {
  assert.equal(value.ready,true);assert.equal(value.request.method,'POST');assert.equal(value.request.path,'/v3/trade/algoOrder');assert.equal(Object.hasOwn(value.request,'query'),false);
  const body=value.request.body;assert.equal(Object.keys(body).length,count);assert.equal(body.algoType,kind,'WOO-ALGO-REQUEST source root type');assert.equal(body.symbol,'PERP_BTC_USDT');assert.equal(body.positionSide,'SHORT');assert.equal(body.clientAlgoOrderId,83);assert.equal(body.algoOrderTag,'audit-19');assert.equal(body.marginMode,'CROSS');
  if(kind==='POSITIONAL_TP_SL')assert.equal(Object.hasOwn(body,'side'),false);else {assert.equal(body.side,'BUY');assert.equal(body.type,'MARKET');assert.equal(body.reduceOnly,true);}
 }
 assert.equal(trail.request.body.quantity,'4.125');
 const children=positionBuild.request.body.childOrders;
 assert.equal(children.length,2);assert.equal(children[0].algoType,'TAKE_PROFIT');assert.equal(children[0].triggerPrice,'121.25');assert.equal(children[1].algoType,'STOP_LOSS');assert.equal(children[1].triggerPrice,'81.5');
 assert.ok(Object.isFrozen(children)&&Object.isFrozen(children[0]));
 assert.equal(api.targetMatchesOrder({algoOrderId:71},{algoOrderId:71,clientAlgoOrderId:83}),true);
 assert.equal(api.targetMatchesOrder({algoOrderId:71},{algoOrderId:'71',clientAlgoOrderId:83}),false);
 assert.equal(api.targetMatchesOrder({algoOrderId:7,clientAlgoOrderId:83},{algoOrderId:71,clientAlgoOrderId:83}),true);
 const order={algoOrderId:71,clientAlgoOrderId:83,rootAlgoOrderId:75,parentAlgoOrderId:77,symbol:'PERP_BTC_USDT',algoOrderTag:'algo-29',algoType:'STOP',side:'SELL',positionSide:'BOTH',quantity:'4.25',isTriggered:false,triggerStatus:'USELESS',type:'MARKET',rootAlgoStatus:'NEW',algoStatus:'NEW',triggerPriceType:'MARKET_PRICE',triggerTime:'1009.125',triggerPrice:'101.25',price:'89.5',visibleQuantity:'2.75',totalExecutedQuantity:'0',averageExecutedPrice:'90.5',totalFee:'-0.0625',feeAsset:'USDT',totalRebate:0,rebateCurrency:'USD',reduceOnly:true,createdTime:2001,updatedTime:3007,isActivated:false,callbackRate:'0.125',activatedPrice:'91.25',leverage:19,marginMode:'ISOLATED',childOrders:[]};
 const amend={target:{algoOrderId:71},currentOrder:order,update:{triggerPrice:'102.5',triggerPriceType:'MARKET_PRICE'}};
 const amended=api.buildWooV3AlgoAmendRequest(amend);assert.equal(amended.ready,true);assert.equal(amended.request.method,'PUT');assert.equal(amended.request.body.algoOrderId,71);assert.equal(amended.request.body.triggerPrice,'102.5');
 assert.notEqual(amended.request.body,amended.target);assert.ok(Object.isFrozen(amended.request.body)&&Object.isFrozen(amended.target));
 assert.equal(api.buildWooV3AlgoAmendRequest({...amend,currentOrder:{...order,totalExecutedQuantity:'1',algoStatus:'FILLED'}}).blocker,'partial-fill-algo-amend-semantics-require-demo-evidence');
 assert.equal(api.buildWooV3AlgoAmendRequest({...amend,currentOrder:{...order,algoOrderId:99,totalExecutedQuantity:'1'}}).blocker,'target-id-mismatch');
 assert.throws(()=>api.buildWooV3AlgoAmendRequest({...amend,update:{triggerPrice:'00101.2500'}}),/must change triggerPrice/);
 const cancelInput={clientAlgoOrderId:83},cancel=api.buildWooV3AlgoCancelRequest(cancelInput);
 assert.equal(cancel.method,'DELETE');assert.equal(cancel.path,'/v3/trade/algoOrder');assert.notEqual(cancel.query,cancelInput,'WOO-ALGO-REQUEST source fresh cancel query');assert.equal(cancel.query.clientAlgoOrderId,83);
 cancelInput.clientAlgoOrderId=89;assert.equal(cancel.query.clientAlgoOrderId,83,'WOO-ALGO-REQUEST source cancel owns validated target');assert.ok(Object.isFrozen(cancel)&&Object.isFrozen(cancel.query));
 const pending=api.buildWooV3AlgoCancelPendingRequest();assert.equal(pending.ready,true);assert.equal(pending.request.path,'/v3/trade/algoOrders');assert.equal(Object.hasOwn(pending.request,'query'),false);
 const scoped=api.buildWooV3AlgoCancelPendingRequest({symbol:'PERP_SOL_USDT'});assert.equal(scoped.ready,false);assert.equal(scoped.scope.symbol,'PERP_SOL_USDT');assert.ok(Object.isFrozen(scoped.scope));
 const unit=JSON.parse('"\\ud800"');const output=api.validateCommon({...common,algoOrderTag:unit});
 assert.equal(typeof output.algoOrderTag,'string');assert.equal(output.algoOrderTag.length,1);assert.equal(output.algoOrderTag.charCodeAt(0),0xd800,'WOO-ALGO-REQUEST source common returned lone tag');
 for(const input of [stop,trailing,positional]) {
  const value=api.buildWooV3AlgoPlaceRequest({...input,algoOrderTag:unit});assert.equal(value.ready,true);assert.equal(typeof value.request.body.algoOrderTag,'string');assert.equal(value.request.body.algoOrderTag.length,1);assert.equal(value.request.body.algoOrderTag.charCodeAt(0),0xd800,'WOO-ALGO-REQUEST source request returned lone tag');
 }
 const comparison={...amend,currentOrder:{...order,totalExecutedQuantity:unit,algoStatus:'FILLED'}};
 assert.equal(typeof comparison.currentOrder.totalExecutedQuantity,'string');assert.equal(comparison.currentOrder.totalExecutedQuantity.length,1);assert.equal(comparison.currentOrder.totalExecutedQuantity.charCodeAt(0),0xd800);assert.equal(api.compareDecimals(comparison.currentOrder.totalExecutedQuantity,'0'),1);
 assert.equal(api.buildWooV3AlgoAmendRequest(comparison).blocker,'partial-fill-algo-amend-semantics-require-demo-evidence','WOO-ALGO-REQUEST source lone stored decimal comparison');
 comparison.currentOrder.totalExecutedQuantity='0';assert.equal(api.buildWooV3AlgoAmendRequest(comparison).blocker,'terminal-or-triggered-algo-cannot-be-amended');
 for(const tag of ['\ud800\udc00',String.raw`\ud800`])assert.equal(api.buildWooV3AlgoPlaceRequest({...stop,algoOrderTag:tag}).request.body.algoOrderTag,tag);
 const regexPrototype=api.requireDecimal.constructor('return RegExp.prototype')(),originalTest=regexPrototype.test;
 try {regexPrototype.test=()=>true;assert.equal(api.requireDecimal('not-decimal','probe'),'not-decimal','WOO-ALGO-REQUEST source live decimal regex');}
 finally {regexPrototype.test=originalTest;}
 process.stdout.write(JSON.stringify({groups:8,loneUnitRows:3}));process.exit(0);
}
if (process.argv.includes('--boundary-audit')) {
  const nested={value:81};const gateway={nested};
  const error=new api.WooV3AlgoWriteResponseError('probe',gateway,-0);
  assert.equal(error.name,'WooV3AlgoWriteResponseError');assert.equal(error.message,'probe');
  assert.equal(error.contractStatus,'vendor-error-envelope-undocumented');assert.ok(Object.is(error.code,-0));
  assert.notEqual(error.gatewayResponse,gateway);assert.equal(error.gatewayResponse.nested,nested);
  assert.ok(Object.isFrozen(error.gatewayResponse));nested.value=89;assert.equal(error.gatewayResponse.nested.value,89);
  const envelope={success:true,timestamp:-0,data:{nested}};
  const success=api.decodeSuccessData(envelope,'probe');
  assert.equal(success.data,envelope.data);assert.ok(Object.isFrozen(success));assert.ok(Object.is(success.timestamp,-0));
  const row=api.decodePlaceRow({algoOrderId:81,clientAlgoOrderId:83,algoType:'STOP',quantity:'3.25000'},'probe');
  assert.ok(Object.isFrozen(row));assert.throws(()=>{row.quantity='0';},TypeError);
  for (const [name,fn] of [['amend',api.decodeWooV3AlgoAmendResponse],['cancel',api.decodeWooV3AlgoCancelResponse]]) {
    const target={algoOrderId:83};const value=fn(envelope,target);
    assert.equal(value.accepted,true,name);assert.ok(Object.is(value.timestamp,-0),name);
    assert.equal(value.identityEvidence,'request-target-only-response-does-not-echo-id-or-status');
    assert.notEqual(value.target,target);assert.ok(Object.isFrozen(value.target));assert.ok(Object.isFrozen(value));
    assert.ok(Object.isFrozen(value.gatewayData));assert.equal(value.gatewayData.nested,nested);
    nested.value++;assert.equal(value.gatewayData.nested.value,nested.value);
  }
  const pending=api.decodeWooV3AlgoCancelPendingResponse(envelope);
  assert.equal(pending.accepted,true);assert.equal(pending.scope,'all-algo-orders-all-symbols');
  assert.ok(Object.isFrozen(pending));assert.ok(Object.isFrozen(pending.gatewayData));
  assert.equal(pending.gatewayData.nested,nested);assert.ok(Object.is(pending.timestamp,-0));
  process.stdout.write(JSON.stringify({cases:6}));process.exit(0);
}
if (process.argv.includes('--membership-audit')) {
  const build = {ready:true, request:{method:'POST',path:'/v3/trade/algoOrder',body:{quantity:'3.25',clientAlgoOrderId:83}},family:'STOP_MARKET'};
  const first = {algoOrderId:81,clientAlgoOrderId:83,algoType:'STOP',quantity:'3.25000'};
  const second = {algoOrderId:89,clientAlgoOrderId:91,algoType:'STOP_LOSS',quantity:'0'};
  const response = rows => ({success:true,timestamp:97,data:{rows}});
  const known = new Set([81]); let calls = [];
  known.has = function(id) {assert.equal(this,known); calls.push(id); return false;};
  const value = api.decodeWooV3AlgoPlaceResponse(response([first]),build,known);
  assert.equal(value.ready,true); assert.equal(value.timestamp,97);
  assert.equal(value.identityEvidence,'response-echoed-algo-order-id');
  for (const [key,expected] of Object.entries(first)) assert.equal(value.row[key],expected,'WOO-ALGO-MEMBERSHIP source decoded '+key);
  assert.deepEqual(calls,[81]);
  calls=[];
  known.has = id => {calls.push(id); return calls.length===2;};
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([first,first]),build,known),/reused a known algoOrderId/);
  assert.deepEqual(calls,[81,81]);
  calls=[]; known.has = id => {calls.push(id);return false;};
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([first,first]),build,known),/repeated an algoOrderId/);
  assert.deepEqual(calls,[81,81]);
  const sentinel=Error('original algo membership'); calls=[];
  known.has = id => {calls.push(id);if(calls.length===2)throw sentinel;return false;};
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([first,second]),{...build,family:'POSITIONAL_TP_SL'},known),e=>e===sentinel);
  assert.deepEqual(calls,[81,89]);
  calls=[]; known.has = id => {calls.push(id);throw sentinel;};
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([first,{...second,quantity:'bad'}]),build,known),/rows\[1\].quantity/);
  assert.deepEqual(calls,[]);
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([]),build,known),/must not be empty/);
  assert.deepEqual(calls,[]);
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([first]),{...build,request:{...build.request,method:'GET'}},known),/matching place request/);
  assert.deepEqual(calls,[]);
  known.has = id => {calls.push(id);return false;};
  const blocked=api.decodeWooV3AlgoPlaceResponse({...response([first,second]),timestamp:-0},{...build,family:'POSITIONAL_TP_SL'},known);
  assert.equal(blocked.ready,false);assert.equal(blocked.blocker,'positional-response-parent-child-identity-requires-demo-evidence');
  assert.equal(blocked.rows.length,2);assert.deepEqual(calls,[81,89]);assert.ok(Object.is(blocked.timestamp,-0));
  for(const [row,expected] of [[blocked.rows[0],first],[blocked.rows[1],second]]) for(const [key,v]of Object.entries(expected)) assert.equal(row[key],v);
  let reads=0;calls=[];
  Object.defineProperty(known,'has',{get(){reads++;return function(id){assert.equal(this,known);calls.push(id);return reads===2;};}});
  assert.throws(() => api.decodeWooV3AlgoPlaceResponse(response([first,second]),build,known),/reused a known algoOrderId/);
  assert.equal(reads,2);assert.deepEqual(calls,[81,89]);
  process.stdout.write(JSON.stringify({cases:9}));process.exit(0);
}
function native(v){if(typeof v==='number')return {$number:Object.is(v,-0)?'-0':String(v)};if(Array.isArray(v))return v.map(native);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>[k,native(x)]));return v;}
function raw(v){if(v===undefined)return undefined;if(typeof v==='number')return Object.is(v,-0)?'-0':v===Infinity?'1e999':v===-Infinity?'-1e999':String(v);if(Array.isArray(v))return '['+v.map(raw).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>JSON.stringify(k)+':'+raw(x)).join(',')+'}';return JSON.stringify(v);}
function capture(fn){try{return {ok:native(fn())}}catch(e){return {error:{name:e.name,message:e.message,...(e.name==='WooV3AlgoWriteResponseError'?{code:native(e.code),gatewayResponse:native(e.gatewayResponse),contractStatus:e.contractStatus}:{})}};}}
const row=(args,fn)=>({args:args.map(raw),...capture(fn)});
const validators=[];
for(const value of [{x:-0},null,[],false])validators.push({name:'requireRecord',...row([value],()=>api.requireRecord(value,'probe'))});
for(const value of [true,false,0,null])validators.push({name:'requireBoolean',...row([value],()=>api.requireBoolean(value,'probe'))});
for(const [value,min]of [[-0,0],[9007199254740991,0],[9007199254740992,0],[1.5,0],[-1,0],[Infinity,0],[1,1],[0,1],['1',0],[null,0]])validators.push({name:'requireInteger',...row([value,min],()=>api.requireInteger(value,'probe',min))});
for(const value of ['x','',' ',3])validators.push({name:'requireString',...row([value],()=>api.requireString(value,'probe'))});
for(const value of ['A','B','C',1])validators.push({name:'requireEnum',...row([value],()=>api.requireEnum(value,['A','B'],'probe'))});
for(const value of ['1','000.0100','.25','1.','0','000.000','-0','1e3','1\n','', '.',1,null])for(const allow of [false,true])validators.push({name:'requireDecimal',...row([value,allow],()=>api.requireDecimal(value,'probe',allow))});
for(const value of ['0001.2300','.50','000.','000a','1.2.3','😀.00'])validators.push({name:'decimalParts',...row([value],()=>api.decimalParts(value))});
for(const [a,b]of [['0001.2300','1.23'],['.5','0.500'],['9007199254740992001','9007199254740992000'],['.0000000000000000001','.0000000000000000002'],['99','100'],['2','1'],['a','😀'],['000a','a']])validators.push({name:'compareDecimals',...row([a,b],()=>api.compareDecimals(a,b))});
for(const value of ['PERP_BTC_USDT','PERP_1_2','SPOT_BTC_USDT','PERP_btc_USDT','PERP_BTC_USDT\n'])validators.push({name:'requirePerpetualSymbol',...row([value],()=>api.requirePerpetualSymbol(value))});
for(const value of [{algoOrderId:7},{clientAlgoOrderId:9},{},{algoOrderId:1,clientAlgoOrderId:2},{algoOrderId:null},{algoOrderId:0},{clientAlgoOrderId:-0},{clientAlgoOrderId:9007199254740991}])validators.push({name:'validateTarget',...row([value],()=>api.validateTarget(value))});
for(const value of [[],[1,'2',-0],null,{},'x'])validators.push({name:'requireArray',...row([value],()=>api.requireArray(value,'probe'))});
const triggerTypes=['MARKET_PRICE','MARK_PRICE','INDEX_PRICE','LAST_PRICE'];for(const v of triggerTypes)validators.push({name:'hasUnsupportedTrigger',...row([v],()=>api.hasUnsupportedTrigger(v))});
const common={symbol:'PERP_BTC_USDT',side:'BUY',positionMode:'ONE_WAY',clientAlgoOrderId:73,algoOrderTag:'tag-17'};
for(const side of ['BUY','SELL'])for(const positionMode of ['ONE_WAY','HEDGE_MODE'])validators.push({name:'validateCommon',...row([{...common,side,positionMode}],()=>api.validateCommon({...common,side,positionMode}))});
for(const change of [{symbol:'BAD',side:'BAD'},{side:'BAD',positionMode:'BAD'},{positionMode:'BAD',clientAlgoOrderId:0},{clientAlgoOrderId:0,algoOrderTag:''},{algoOrderTag:''},{algoOrderTag:'😀'.repeat(32)},{algoOrderTag:'😀'.repeat(33)}])validators.push({name:'validateCommon',...row([{...common,...change}],()=>api.validateCommon({...common,...change}))});
for(const target of [{algoOrderId:71},{clientAlgoOrderId:73}])for(const order of [{algoOrderId:71,clientAlgoOrderId:73},{algoOrderId:72,clientAlgoOrderId:74},{algoOrderId:71,clientAlgoOrderId:74}])validators.push({name:'targetMatchesOrder',...row([target,order],()=>api.targetMatchesOrder(target,order))});assert.equal(validators.length,105);
const stop={...common,family:'STOP_MARKET',quantity:'0003.2500',triggerPrice:'101.25',triggerPriceType:'MARKET_PRICE'};
const trailing={...common,family:'TRAILING_STOP',quantity:'4.125',callback:{source:'WOO_NATIVE_CALLBACK_VALUE',value:'.25'},activatedPrice:'97.5'};
const positional={...common,family:'POSITIONAL_TP_SL',takeProfit:{triggerPrice:'121.25',triggerPriceType:'MARKET_PRICE'},stopLoss:{triggerPrice:'81.5',triggerPriceType:'MARKET_PRICE'}};
const placeInputs=[];for(const base of [stop,trailing,positional])for(const side of ['BUY','SELL'])for(const positionMode of ['ONE_WAY','HEDGE_MODE'])placeInputs.push({...base,side,positionMode});
for(const t of triggerTypes.slice(1))placeInputs.push({...stop,triggerPriceType:t,quantity:'0',triggerPrice:'0'});
placeInputs.push({...stop,quantity:'0',triggerPrice:'0'},{...stop,triggerPrice:'0'},{...stop,clientAlgoOrderId:undefined,algoOrderTag:undefined});
placeInputs.push({...trailing,callback:null,quantity:'0'},{...trailing,callback:{source:'BAD',value:'0'},quantity:'0'},{...trailing,quantity:'0',callback:{source:'WOO_NATIVE_CALLBACK_VALUE',value:'0'}},{...trailing,callback:{source:'WOO_NATIVE_CALLBACK_VALUE',value:'0'},activatedPrice:'0'},{...trailing,activatedPrice:'0'},{...trailing,activatedPrice:undefined});
placeInputs.push({...positional,takeProfit:undefined},{...positional,stopLoss:undefined},{...positional,takeProfit:undefined,stopLoss:undefined},{...positional,takeProfit:{triggerPrice:'0',triggerPriceType:'MARKET_PRICE'},stopLoss:{triggerPrice:'0',triggerPriceType:'INDEX_PRICE'}},{...positional,takeProfit:{triggerPrice:'0',triggerPriceType:'MARKET_PRICE'},stopLoss:{triggerPrice:'0',triggerPriceType:'MARKET_PRICE'}},{...positional,stopLoss:{triggerPrice:'0',triggerPriceType:'MARKET_PRICE'}},{...positional,symbol:'BAD',takeProfit:undefined,stopLoss:undefined});
const places=placeInputs.map(v=>row([v],()=>api.buildWooV3AlgoPlaceRequest(v)));assert.equal(places.length,31);
const algo={algoOrderId:71,clientAlgoOrderId:73,rootAlgoOrderId:75,parentAlgoOrderId:77,symbol:'PERP_BTC_USDT',algoOrderTag:'algo-29',algoType:'STOP',side:'SELL',positionSide:'BOTH',quantity:'4.25',isTriggered:false,triggerStatus:'USELESS',type:'MARKET',rootAlgoStatus:'NEW',algoStatus:'NEW',triggerPriceType:'MARKET_PRICE',triggerTime:'1009.125',triggerPrice:'101.25',price:'89.5',visibleQuantity:'2.75',totalExecutedQuantity:'0',averageExecutedPrice:'90.5',totalFee:'-0.0625',feeAsset:'USDT',totalRebate:-0,rebateCurrency:'USD',reduceOnly:true,createdTime:2001,updatedTime:3007,isActivated:false,callbackRate:'0.125',activatedPrice:'91.25',leverage:19,marginMode:'ISOLATED',childOrders:[]};
const amend={currentOrder:algo,target:{algoOrderId:71},update:{triggerPrice:'102.5',triggerPriceType:'MARKET_PRICE'}};
const amendInputs=[amend,{...amend,target:{clientAlgoOrderId:73}},{...amend,update:{triggerPrice:'102.5'}},{...amend,update:{triggerPrice:'00101.2500'}},{...amend,update:{}},{...amend,update:{triggerPrice:'0'}},{...amend,target:{algoOrderId:0}},{...amend,target:{algoOrderId:71,clientAlgoOrderId:73}}];
for(const change of [{algoOrderId:99,totalExecutedQuantity:'1',algoStatus:'FILLED'},{totalExecutedQuantity:'1',algoStatus:'FILLED'},{algoStatus:'FILLED'},{rootAlgoStatus:'FILLED'},{isTriggered:true},{triggerStatus:'TRIGGERED'},{algoType:'POSITIONAL_TP_SL'},{algoType:'TP_SL'},{algoType:'TAKE_PROFIT'},{algoType:'STOP_LOSS'},{algoType:'TRAILING_STOP'},{algoType:'OCO'},{triggerPriceType:'INDEX_PRICE'},{triggerPrice:undefined},{triggerPrice:''},{totalExecutedQuantity:'000.000'}])amendInputs.push({...amend,currentOrder:{...algo,...change}});
for(const update of [{quantity:'0',triggerPrice:'0'},{quantity:'4',callbackValue:'0'},{callbackValue:'0',triggerPrice:'0'},{callbackValue:'.5',triggerPrice:'0'},{triggerPriceType:'INDEX_PRICE',triggerPrice:'0'},{triggerPriceType:'MARKET_PRICE'}])amendInputs.push({...amend,update});
amendInputs.push({...amend,currentOrder:{...algo,algoType:'TAKE_PROFIT'},update:{quantity:'5'}},{...amend,currentOrder:{...algo,algoType:'TAKE_PROFIT'},update:{callbackValue:'0'}});
const amends=amendInputs.map(v=>row([v],()=>api.buildWooV3AlgoAmendRequest(v)));assert.equal(amends.length,32);
const cancels=[];for(const target of [{algoOrderId:71},{clientAlgoOrderId:73},{algoOrderId:0},{},{algoOrderId:71,clientAlgoOrderId:73}])cancels.push({name:'single',...row([target],()=>api.buildWooV3AlgoCancelRequest(target))});for(const scope of [undefined,'all',{symbol:'PERP_SOL_USDT'},{symbol:'BAD'}])cancels.push({name:'pending',...row([scope],()=>api.buildWooV3AlgoCancelPendingRequest(scope))});assert.equal(cancels.length,9);
const success=(data,timestamp=1009)=>({success:true,data,timestamp});const responseRow={algoOrderId:81,clientAlgoOrderId:73,algoType:'STOP',quantity:'3.25000'};
const placeRows=[];for(const change of [{},{algoOrderId:0,clientAlgoOrderId:-1},{clientAlgoOrderId:-1,algoType:'BAD'},{algoType:'BAD',quantity:'-1'},{quantity:'-1'},{quantity:'0',clientAlgoOrderId:-0}])placeRows.push(row([{...responseRow,...change}],()=>api.decodePlaceRow({...responseRow,...change},'probe')));placeRows.push(row([null],()=>api.decodePlaceRow(null,'probe')),row([[]],()=>api.decodePlaceRow([],'probe')));assert.equal(placeRows.length,8);
const stopBuild=api.buildWooV3AlgoPlaceRequest(stop),trailingBuild=api.buildWooV3AlgoPlaceRequest(trailing),positionalBuild=api.buildWooV3AlgoPlaceRequest(positional);
const responseCases=[
 [success({rows:[responseRow]},-0),stopBuild,[]],[success({rows:[responseRow]}),stopBuild,[81]],
 [success({rows:[responseRow,{...responseRow,algoOrderId:81}]}),stopBuild,[]],
 [success({rows:[responseRow,{...responseRow,algoOrderId:82,quantity:'bad'}]}),stopBuild,[81]],
 [success({rows:[]}),stopBuild,[]],[success({rows:[responseRow,{...responseRow,algoOrderId:82}]}),stopBuild,[]],
 [success({rows:[{...responseRow,algoType:'TRAILING_STOP',quantity:'4.125'}]}),trailingBuild,[]],
 [success({rows:[{...responseRow,algoType:'TRAILING_STOP',quantity:'bad'}]}),stopBuild,[]],
 [success({rows:[{...responseRow,algoType:'TRAILING_STOP'}]}),stopBuild,[]],
 [success({rows:[{...responseRow,quantity:'3.2500000000000001'}]}),stopBuild,[]],
 [success({rows:[{...responseRow,clientAlgoOrderId:74}]}),stopBuild,[]],
 [success({rows:[{...responseRow,algoType:'POSITIONAL_TP_SL',quantity:'0'},{...responseRow,algoOrderId:82,algoType:'STOP_LOSS',quantity:'0'}]},-0),positionalBuild,[]],
 [success({rows:[responseRow]}),positionalBuild,[81]],
 [success({rows:null},-1),stopBuild,[]],
 [null,{...stopBuild,request:{...stopBuild.request,method:'GET'}},[]],
 [success({rows:[responseRow]}),{...stopBuild,request:{...stopBuild.request,body:{quantity:3}}},[]],
 [success({rows:[responseRow]}),{...stopBuild,request:{...stopBuild.request,body:{quantity:'3.25'}}},[]],
];
const placeResponses=responseCases.map(([v,b,ids])=>row([v,b,ids],()=>api.decodeWooV3AlgoPlaceResponse(v,b,new Set(ids))));assert.equal(placeResponses.length,17);
const successes=[null,[],{success:0,data:null,timestamp:-1},{success:true,data:null,timestamp:-1},{success:true,data:{},timestamp:-1},{success:false,code:-1,message:''},{success:false,code:7,message:''},{success:false,code:-0,message:'gateway-47',data:{x:3},extra:'retained'},{success:false},success({x:-0},-0)].map(v=>row([v],()=>api.decodeSuccessData(v,'probe')));assert.equal(successes.length,10);
const mutations=[];for(const name of ['amend','cancel','direct'])for(const [v,t]of [[success({status:'anything',nested:{x:-0}},-0),{algoOrderId:71}],[success({},11),{clientAlgoOrderId:73}],[success(null,-1),{}],[success({x:1},-1),{}],[success({x:1}),{}],[{success:false,message:'vendor',code:12},{algoOrderId:0}]])mutations.push({name,...row([v,t],()=>name==='direct'?api.decodeMutation(v,'probe',t):api[name==='amend'?'decodeWooV3AlgoAmendResponse':'decodeWooV3AlgoCancelResponse'](v,t))});
for(const v of [success({status:'unrecognized',x:-0},-0),success({},19),success(null,-1),{success:false}])mutations.push({name:'pending',...row([v],()=>api.decodeWooV3AlgoCancelPendingResponse(v))});assert.equal(mutations.length,22);
const declarations=[
 ['stop',{...stop,clientAlgoOrderId:Infinity}],['trailing',{...trailing,clientAlgoOrderId:-0}],['positional',positional],['positional',{...positional,takeProfit:undefined}],
 ['trigger',positional.takeProfit],['row',{...responseRow,algoOrderId:Infinity,clientAlgoOrderId:-0}],
 ['amend',{...amend,currentOrder:{...algo,createdTime:Infinity},target:{clientAlgoOrderId:-0},update:{triggerPrice:'107.25',triggerPriceType:'INDEX_PRICE',quantity:'9.125',callbackValue:'.75'}}],
 ['mutation',{accepted:true,target:{algoOrderId:Infinity},timestamp:-0,identityEvidence:'request-target-only-response-does-not-echo-id-or-status',gatewayData:{x:Infinity,y:-0}}],
 ['pending',{accepted:true,scope:'all-algo-orders-all-symbols',timestamp:Infinity,gatewayData:{x:-0}}],
 ['target',{algoOrderId:-0}],['target',{clientAlgoOrderId:Infinity}],
 ['union',stop],['union',trailing],['union',positional],
].map(([type,value])=>({type,input:raw(value),expected:native(value)}));assert.equal(declarations.length,14);
const declarationRefusals=[['stop',{...stop,family:'TRAILING_STOP'}],['stop',{...stop,clientAlgoOrderId:null}],['trailing',{...trailing,activatedPrice:null}],['trailing',{...trailing,callback:{source:'BAD',value:'.25'}}],['positional',{...positional,stopLoss:null}],['mutation',{accepted:false,target:{algoOrderId:1},timestamp:1,identityEvidence:'request-target-only-response-does-not-echo-id-or-status',gatewayData:{}}],['pending',{accepted:true,scope:'wrong',timestamp:1,gatewayData:{}}],['target',{algoOrderId:1,clientAlgoOrderId:2}],['union',{...stop,family:'BAD'}]].map(([type,value])=>({type,input:raw(value)}));assert.equal(declarationRefusals.length,9);
const directGateway={message:'original',nested:{x:1},n:Infinity};const direct=new api.WooV3AlgoWriteResponseError('direct-59',directGateway,-0);assert.equal(direct.name,'WooV3AlgoWriteResponseError');assert.ok(Object.isFrozen(direct.gatewayResponse));assert.notEqual(direct.gatewayResponse,directGateway);assert.equal(direct.gatewayResponse.nested,directGateway.nested);
assert.deepEqual(Object.keys(api.WOO_V3_ALGO_WRITE_PARITY),['requestAndResponseFixtures','runtimeImported','credentialedDemoLifecycle','triggerPriceType','trailingInput','positionalResponseIdentity','positionalUpdate','partialFillAndQuantityAmend','symbolPendingCancel','cancelResponseIdentity']);
const ast=ts.createSourceFile('algo-writes.ts',fs.readFileSync(path.join(root,'woo.v3.algo-writes.ts'),'utf8'),ts.ScriptTarget.Latest,true);const alias=n=>ast.statements.find(x=>ts.isTypeAliasDeclaration(x)&&x.name.text===n).type;
assert.deepEqual(alias('WooV3AlgoTriggerPriceType').types.map(x=>x.literal.text),triggerTypes);
const schema={
 WooV3AlgoCommonInput:[['symbol',false,'string'],['side',false,"'BUY'|'SELL'"],['positionMode',false,"'ONE_WAY'|'HEDGE_MODE'"],['clientAlgoOrderId',true,'number'],['algoOrderTag',true,'string']],
 WooV3PositionalTriggerInput:[['triggerPrice',false,'WooV3Decimal'],['triggerPriceType',false,'WooV3AlgoTriggerPriceType']],
 WooV3AlgoPlaceResponseRow:[['algoOrderId',false,'number'],['clientAlgoOrderId',false,'number'],['algoType',false,"'STOP'|'TRAILING_STOP'|'POSITIONAL_TP_SL'|'TAKE_PROFIT'|'STOP_LOSS'"],['quantity',false,'WooV3Decimal']],
 WooV3AlgoAmendInput:[['currentOrder',false,'WooV3AlgoOrder'],['target',false,'WooV3AlgoWriteTarget'],['update',false,'Readonly<{triggerPrice?:WooV3Decimal;triggerPriceType?:WooV3AlgoTriggerPriceType;quantity?:WooV3Decimal;callbackValue?:WooV3Decimal;}>']],
 WooV3AlgoMutationResult:[['accepted',false,'true'],['target',false,'WooV3AlgoWriteTarget'],['timestamp',false,'number'],['identityEvidence',false,"'request-target-only-response-does-not-echo-id-or-status'"],['gatewayData',false,'Readonly<UnknownRecord>']],
 WooV3AlgoCancelPendingResult:[['accepted',false,'true'],['scope',false,"'all-algo-orders-all-symbols'"],['timestamp',false,'number'],['gatewayData',false,'Readonly<UnknownRecord>']],
};
const fields=n=>n.members.map(f=>[f.name.text,!!f.questionToken,f.type.getText(ast).replace(/\s+/g,'')]);
for(const[n,expected]of Object.entries(schema)){const a=alias(n);assert.equal(a.typeName.text,'Readonly');assert.deepEqual(fields(a.typeArguments[0]),expected,'schema '+n);}
const intersections={WooV3StopMarketPlaceInput:[['family',false,"'STOP_MARKET'"],['quantity',false,'WooV3Decimal'],['triggerPrice',false,'WooV3Decimal'],['triggerPriceType',false,'WooV3AlgoTriggerPriceType']],WooV3TrailingPlaceInput:[['family',false,"'TRAILING_STOP'"],['quantity',false,'WooV3Decimal'],['callback',false,"Readonly<{source:'WOO_NATIVE_CALLBACK_VALUE';value:WooV3Decimal;}>"],['activatedPrice',true,'WooV3Decimal']],WooV3PositionalPlaceInput:[['family',false,"'POSITIONAL_TP_SL'"],['stopLoss',true,'WooV3PositionalTriggerInput'],['takeProfit',true,'WooV3PositionalTriggerInput']]};
for(const[n,expected]of Object.entries(intersections)){const a=alias(n);assert.equal(a.types[0].getText(ast),'WooV3AlgoCommonInput');assert.equal(a.types[1].typeName.text,'Readonly');assert.deepEqual(fields(a.types[1].typeArguments[0]),expected,'intersection '+n);}
assert.equal(alias('WooV3AlgoWriteTarget').getText(ast).replace(/\s+/g,''),'Readonly<{algoOrderId:number}|{clientAlgoOrderId:number}>');assert.deepEqual(alias('WooV3AlgoPlaceInput').types.map(t=>t.getText(ast)),['WooV3StopMarketPlaceInput','WooV3TrailingPlaceInput','WooV3PositionalPlaceInput']);
// Literal operation-union bodies pinned independently; native variants are asserted on every output path.
const unionSchemas={
WooV3AlgoPlaceBuildResult:[[['ready',false,'true'],['request',false,'WooV3RequestInput'],['family',false,"WooV3AlgoPlaceInput['family']"],['triggerSemantics',false,"'official-last-price-trigger'|'vendor-native-callback-value-no-safe-cex-conversion'|'official-last-price-positional-children'"]],[['ready',false,'false'],['blocker',false,"'only-market-price-trigger-is-documented'"],['family',false,"WooV3AlgoPlaceInput['family']"]]],
WooV3AlgoPlaceDecodeResult:[[['ready',false,'true'],['row',false,'WooV3AlgoPlaceResponseRow'],['timestamp',false,'number'],['identityEvidence',false,"'response-echoed-algo-order-id'"]],[['ready',false,'false'],['blocker',false,"'positional-response-parent-child-identity-requires-demo-evidence'"],['rows',false,'readonlyWooV3AlgoPlaceResponseRow[]'],['timestamp',false,'number']]],
WooV3AlgoAmendBuildResult:[[['ready',false,'true'],['request',false,'WooV3RequestInput'],['target',false,'WooV3AlgoWriteTarget'],['updateSemantics',false,"'untriggered-zero-fill-stop-trigger-only'"]],[['ready',false,'false'],['blocker',false,"'target-id-mismatch'|'terminal-or-triggered-algo-cannot-be-amended'|'partial-fill-algo-amend-semantics-require-demo-evidence'|'quantity-amend-semantics-require-demo-evidence'|'positional-child-update-not-documented'|'trailing-callback-update-not-documented'|'only-market-price-trigger-is-documented'|'stop-trigger-provenance-incomplete'|'algo-family-update-not-supported'"],['target',false,'WooV3AlgoWriteTarget']]],
WooV3AlgoCancelPendingBuildResult:[[['ready',false,'true'],['request',false,'WooV3RequestInput'],['scope',false,"'all-algo-orders-all-symbols'"]],[['ready',false,'false'],['blocker',false,"'symbol-scoped-pending-cancel-query-schema-conflicts-with-description'"],['scope',false,'Readonly<{symbol:string}>']]],
};for(const[n,expected]of Object.entries(unionSchemas)){const variants=alias(n).types;assert.equal(variants.length,2);for(let i=0;i<2;i++){assert.equal(variants[i].typeName.text,'Readonly');assert.deepEqual(fields(variants[i].typeArguments[0]).map(([name,opt,type])=>[name,opt,type.replace(/^\|/,'')]),expected[i],'operation branch '+n+' '+i);}}
console.log(JSON.stringify({validators,places,amends,cancels,placeRows,placeResponses,successes,mutations,declarations,declarationRefusals,triggerTypes,parity:api.WOO_V3_ALGO_WRITE_PARITY,directError:{name:direct.name,message:direct.message,code:native(direct.code),gatewayResponse:native(direct.gatewayResponse),contractStatus:direct.contractStatus}}));
