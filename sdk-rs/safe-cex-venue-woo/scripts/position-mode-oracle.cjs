'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,extras=[],imports={}){const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8')+(extras.length?'\nexport { '+extras.join(',')+' };':''),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(n){assert.ok(Object.hasOwn(imports,n));return imports[n];}});return exports;}
const contracts=load('woo.v3.contracts.ts'),api=load('woo.v3.account-writes.ts',['requirePerpetualSymbol','validateSnapshotEvidence','isExactDecimalZero','positionProvesNoExposure'],{'./woo.v3.contracts':contracts});
if(process.argv.includes('--choice-audit')) {
 const flat={symbol:'PERP_BTC_USDT',positionSide:'BOTH',holding:'-00.',pendingLongQty:'.000',pendingShortQty:'0',settlePrice:'1',averageOpenPrice:'2',pnl24H:'3',fee24H:'4',markPrice:'5',estLiqPrice:'6',adlQuantile:7,timestamp:11};
 const sources=['decoded-v3-futures-positions-complete-page','decoded-v3-pending-regular-orders-complete-pages','decoded-v3-pending-algo-orders-complete-pages'];
 const snapshot=(index,rows=[],id='audit-23')=>({source:sources[index],reconciliationId:id,complete:true,rows});
 const make=(id='audit-23')=>({targetPositionMode:'HEDGE_MODE',evidence:{account:{source:'decoded-v3-account-info',reconciliationId:id,accountMode:'MARGIN',positionMode:'ONE_WAY'},positions:snapshot(0,[{...flat}],id),regularOrders:snapshot(1,[],id),algoOrders:snapshot(2,[],id)}});
 assert.equal(api.requirePerpetualSymbol('PERP_01_USDT'),'PERP_01_USDT');
 assert.throws(()=>api.requirePerpetualSymbol('PERP_btc_USDT'),/WOO V3 symbol must use PERP_BASE_QUOTE form/);
 const coerced=['PERP_BTC_USDT'];assert.equal(api.requirePerpetualSymbol(coerced),coerced,'WOO-MODE-CHOICE source regex returns original array');
 const rows=[null],snap=api.validateSnapshotEvidence(snapshot(0,rows),sources[0],'audit');
 assert.equal(snap.rows,rows,'WOO-MODE-CHOICE source snapshot row alias');assert.equal(snap.rows[0],null);assert.ok(Object.isFrozen(snap));
 rows.push({value:37});assert.equal(snap.rows.length,2);assert.equal(snap.rows[1].value,37);
 assert.equal(api.validateSnapshotEvidence({...snapshot(0),complete:1},sources[0],'audit'),undefined);
 const thrown={origin:'snapshot'};assert.equal(api.validateSnapshotEvidence({get source(){throw thrown;}},sources[0],'audit'),undefined);
 for(const value of ['0','-00.','-.00','.000'])assert.equal(api.isExactDecimalZero(value),true);
 for(const value of ['.','-.','0e0','1e-9999','0\n',0])assert.equal(api.isExactDecimalZero(value),false);
 const accesses=[];const dynamic={get holding(){accesses.push('holding');return '1';},get pendingLongQty(){accesses.push('long');throw thrown;}};
 assert.equal(api.positionProvesNoExposure(dynamic),false);assert.deepEqual(accesses,['holding']);
 assert.equal(api.positionProvesNoExposure({get holding(){throw thrown;}}),false);
 assert.equal(api.positionProvesNoExposure(flat),true);assert.equal(api.positionProvesNoExposure({...flat,pendingShortQty:'1e-9999'}),false);
 const ready=api.buildWooV3SetPositionModeRequest(make());assert.equal(ready.ready,true);
 assert.equal(ready.currentPositionMode,'ONE_WAY');assert.equal(ready.targetPositionMode,'HEDGE_MODE');
 assert.equal(ready.request.method,'PUT');assert.equal(ready.request.path,'/v3/futures/positionMode');assert.equal(ready.request.body.positionMode,'HEDGE_MODE');
 assert.deepEqual(Object.keys(ready.request.body),['positionMode']);assert.equal(ready.preconditionEvidence,'complete-reconciled-v3-positions-zero-and-pending-orders-empty');
 assert.ok(Object.isFrozen(ready)&&Object.isFrozen(ready.request)&&Object.isFrozen(ready.request.body));
 const incomplete=make();incomplete.evidence.account=null;
 assert.equal(Object.hasOwn(api.buildWooV3SetPositionModeRequest(incomplete),'currentPositionMode'),false);
 const conflict=make();conflict.evidence.account.accountMode='SPOT';conflict.targetPositionMode='ONE_WAY';conflict.evidence.positions.reconciliationId='different';
 assert.equal(api.buildWooV3SetPositionModeRequest(conflict).blocker,'account-state-evidence-incomplete-or-mixed');
 const exposed=make();exposed.evidence.positions.rows=[{...flat,holding:'1'}];exposed.evidence.regularOrders.rows=[null];
 assert.equal(api.buildWooV3SetPositionModeRequest(exposed).blocker,'open-positions-must-be-closed');
 exposed.evidence.positions.rows.every=()=>true;
 assert.equal(api.buildWooV3SetPositionModeRequest(exposed).blocker,'pending-regular-orders-must-be-cancelled','WOO-MODE-CHOICE source live every override');
 exposed.evidence.positions.rows.every=()=>{throw thrown;};
 assert.throws(()=>api.buildWooV3SetPositionModeRequest(exposed),e=>e===thrown,'WOO-MODE-CHOICE source every thrown identity');
 const sparse=make();sparse.evidence.positions.rows=new Array(1);
 assert.equal(api.buildWooV3SetPositionModeRequest(sparse).ready,true);
 const unit=JSON.parse('"\\ud800"'),unitInput=make(unit);
 const unitSnapshot=api.validateSnapshotEvidence(unitInput.evidence.positions,sources[0],'audit');
 assert.equal(unitSnapshot.reconciliationId.length,1);assert.equal(unitSnapshot.reconciliationId.charCodeAt(0),0xd800,'WOO-MODE-CHOICE source snapshot returned lone ID');
 assert.equal(unitInput.evidence.account.reconciliationId.charCodeAt(0),0xd800);
 assert.equal(api.buildWooV3SetPositionModeRequest(unitInput).ready,true,'WOO-MODE-CHOICE source equal lone IDs ready');
 unitInput.evidence.algoOrders.reconciliationId=JSON.parse('"\\ud801"');
 assert.equal(api.buildWooV3SetPositionModeRequest(unitInput).blocker,'account-state-evidence-incomplete-or-mixed','WOO-MODE-CHOICE source distinct lone ID blocker');
 for(const id of ['\ud800\udc00',String.raw`\ud800`]) {
  const positive=make(id);assert.equal(api.buildWooV3SetPositionModeRequest(positive).ready,true);
  assert.equal(positive.evidence.account.reconciliationId,id);
 }
 const ordinary=make();const alias=ordinary.evidence;
 alias.positions.reconciliationId='later';assert.equal(ordinary.evidence.positions.reconciliationId,'later');
 const parity=api.WOO_V3_ACCOUNT_WRITE_PARITY;assert.ok(Object.isFrozen(parity));assert.equal(Object.keys(parity).length,11);
 assert.equal(parity.requestAndResponseFixtures,true);assert.equal(parity.runtimeImported,false);assert.equal(parity.credentialedDemoLifecycle,false);assert.equal(parity.readbackRequired,true);
 assert.equal(parity.leverageReadback,'strict-one-way-both-or-hedge-long-short');assert.equal(parity.defaultMarginMode,'excluded-official-response-unusable');
 assert.equal(parity.crossOneWayLeverage,'descriptor-ready-request-state-only');assert.equal(parity.crossHedgeLeverage,'blocked-resulting-leg-scope-requires-demo-evidence');
 assert.equal(parity.isolatedLeverage,'blocked-outside-safe-cex-cross-margin-contract');assert.equal(parity.positionModePreconditions,'complete-reconciled-v3-positions-zero-and-pending-orders-empty');assert.equal(parity.mutationResponseState,'request-target-only');
 assert.throws(()=>{parity.runtimeImported=true;},e=>e.name==='TypeError');
 const regexPrototype=api.isExactDecimalZero.constructor('return RegExp.prototype')();
 const originalTest=regexPrototype.test;
 try {regexPrototype.test=()=>true;assert.equal(api.isExactDecimalZero('1'),true,'WOO-MODE-CHOICE source live regexp test');}
 finally {regexPrototype.test=originalTest;}
 process.stdout.write(JSON.stringify({groups:8,loneIdRows:4,revisitedRows:1}));process.exit(0);
}
function native(v){if(typeof v==='number')return {$number:Object.is(v,-0)?'-0':String(v)};if(Array.isArray(v))return v.map(native);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>[k,native(x)]));return v;}
function raw(v){if(v===undefined)return undefined;if(typeof v==='number')return Object.is(v,-0)?'-0':v===Infinity?'1e999':v===-Infinity?'-1e999':String(v);if(Array.isArray(v))return '['+v.map(raw).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>JSON.stringify(k)+':'+raw(x)).join(',')+'}';return JSON.stringify(v);}
function capture(fn){try{const v=fn();return v===undefined?{absent:true}:{ok:native(v)};}catch(e){const error={name:e.name,message:e.message};if(e instanceof api.WooV3AccountConfigResponseError){error.contractStatus=e.contractStatus;error.gatewayResponse=native(e.gatewayResponse);if(e.code!==undefined)error.code=native(e.code);}return {error};}}
const row=(v,fn)=>({input:raw(v),...capture(fn)});
const zeroValues=['0','-0','000','.000','-.00','0.','-000.000','+0','0e0','0\n','0\r','0\u2028','0\u2029','-0.0\n','00.01','1','1e-99999','','.','-',' 0',null,[],0,-0,false,{}];assert.equal(zeroValues.length,27);
const zeros=zeroValues.map(v=>row(v,()=>api.isExactDecimalZero(v)));
const flat={holding:'0',pendingLongQty:'-0.',pendingShortQty:'.000'};
const positionValues=[flat,{...flat,extra:Infinity},null,[],false,{},...['holding','pendingLongQty','pendingShortQty'].flatMap(k=>[{...flat,[k]:'0.0001'},{...flat,[k]:0},{...flat,[k]:null},{...flat,[k]:undefined}]),{...flat,holding:'0\n'},{...flat,holding:'-.000'},{...flat,pendingLongQty:'1e-9999'}];assert.equal(positionValues.length,21);
const positions=positionValues.map(v=>row(v,()=>api.positionProvesNoExposure(v)));
const sources={positions:'decoded-v3-futures-positions-complete-page',regularOrders:'decoded-v3-pending-regular-orders-complete-pages',algoOrders:'decoded-v3-pending-algo-orders-complete-pages'};
const snapshot=(name,rows=[],id='atomic-17')=>({source:sources[name],reconciliationId:id,complete:true,rows});
const snapshots=[];
for(const name of Object.keys(sources)){
 const good=snapshot(name,[{field:Infinity,zero:-0},null,['retained']],name+'-91');
 const values=[good,snapshot(name,[]),{...good,extra:true},{...good,source:'bad'},{...good,complete:false},{...good,complete:1},{...good,rows:null},{...good,rows:{}},{...good,reconciliationId:''},{...good,reconciliationId:undefined},null,[]];
 for(const v of values)snapshots.push({name,...row(v,()=>api.validateSnapshotEvidence(v,sources[name],'probe'))});
}assert.equal(snapshots.length,36);
const account={source:'decoded-v3-account-info',reconciliationId:'atomic-17',accountMode:'MARGIN',positionMode:'ONE_WAY'};
const evidence={account,positions:snapshot('positions',[flat]),regularOrders:snapshot('regularOrders'),algoOrders:snapshot('algoOrders')};
const base={targetPositionMode:'HEDGE_MODE',evidence};
const copy=v=>structuredClone(v),values=[copy(base),{targetPositionMode:'ONE_WAY',evidence:{...copy(evidence),account:{...account,positionMode:'HEDGE_MODE'}}},{...copy(base),evidence:{...copy(evidence),positions:snapshot('positions')}}];
function add(f){const v=copy(base);f(v);values.push(v);}
add(v=>{v.targetPositionMode='BAD';v.evidence.account=null;});
for(const bad of [undefined,null,[],{},false])add(v=>{v.evidence=bad;});
for(const [k,bad] of [['source','bad'],['reconciliationId',''],['accountMode','bad'],['positionMode','bad']])add(v=>{v.evidence.account[k]=bad;});
for(const name of Object.keys(sources))for(const f of [v=>{v.source='bad';},v=>{v.complete=false;},v=>{v.rows=null;},v=>{v.reconciliationId='mixed';}])add(v=>f(v.evidence[name]));
add(v=>{v.evidence.account.accountMode='SPOT';v.targetPositionMode='ONE_WAY';v.evidence.positions.rows=[{holding:'1'}];v.evidence.regularOrders.rows=[null];});
add(v=>{v.targetPositionMode='ONE_WAY';v.evidence.positions.rows=[{holding:'1'}];v.evidence.regularOrders.rows=[null];v.evidence.algoOrders.rows=[null];});
add(v=>{v.evidence.positions.rows=[{...flat,pendingShortQty:'1'}];v.evidence.regularOrders.rows=[null];v.evidence.algoOrders.rows=[null];});
add(v=>{v.evidence.regularOrders.rows=[null];v.evidence.algoOrders.rows=[null];});
add(v=>{v.evidence.algoOrders.rows=[null];});
for(const k of ['holding','pendingLongQty','pendingShortQty'])add(v=>{v.evidence.positions.rows=[{...flat,[k]:1}];});
add(v=>{v.evidence.positions.rows=[null];});
add(v=>{v.evidence.positions.rows=[flat,{...flat,holding:'0\n'}];});
add(v=>{v.evidence.positions.rows=[flat,{...flat,holding:'-000.00'}];});
add(v=>{v.evidence.positions.rows=[{...flat,holding:'1e-99999'}];});
add(v=>{v.evidence.positions.complete=false;v.evidence.account.accountMode='SPOT';v.targetPositionMode='ONE_WAY';});
add(v=>{delete v.evidence.account;});
add(v=>{delete v.targetPositionMode;});
assert.equal(values.length,40);
const builds=values.map(v=>row(v,()=>api.buildWooV3SetPositionModeRequest(v)));
const ready=api.buildWooV3SetPositionModeRequest(base),reverse=api.buildWooV3SetPositionModeRequest(values[1]);assert.equal(ready.ready,true);assert.equal(reverse.ready,true);
const inputs=[{success:true,timestamp:-0},{success:true,timestamp:9007199254740991},{success:true,timestamp:-1},{success:true,timestamp:Infinity},{success:true,timestamp:0.5},{success:true,timestamp:1,extra:true},{success:false},{success:false,code:-0,message:'mode denied',nested:{x:Infinity,zero:-0}},{success:false,code:-1,message:''},{success:false,code:2,message:''},{success:null},null,[]];
const responses=inputs.map(v=>({target:'HEDGE_MODE',...row(v,()=>api.decodeWooV3SetPositionModeResponse(v,ready))}));responses.push({target:'ONE_WAY',...row({success:true,timestamp:29},()=>api.decodeWooV3SetPositionModeResponse({success:true,timestamp:29},reverse))});
for(const overrides of [{method:'GET'},{path:'/v3/futures/leverage'}])responses.push({target:'HEDGE_MODE',overrides,...row(null,()=>api.decodeWooV3SetPositionModeResponse(null,{...ready,request:{...ready.request,...overrides}}))});assert.equal(responses.length,16);
// Full declaration rows have distinct native numeric fields, deliberately outside runtime validator bounds.
const position={symbol:'PERP_BTC_USDT',holding:'1.25',positionSide:'LONG',pendingLongQty:'2.5',pendingShortQty:'3.75',settlePrice:'11.5',averageOpenPrice:'12.5',pnl24H:'13.5',fee24H:'14.5',markPrice:'15.5',estLiqPrice:'16.5',adlQuantile:Infinity,timestamp:-0};
const regular={symbol:'PERP_ETH_USDT',status:'NEW',side:'SELL',positionSide:'SHORT',createdTime:-0,orderId:Infinity,orderTag:'regular-41',price:'42.5',type:'LIMIT',quantity:'43.5',amount:null,visible:'44.5',executed:'45.5',totalFee:'46.5',feeAsset:'USDT',totalRebate:-Infinity,rebateCurrency:'BTC',clientOrderId:47,reduceOnly:true,realizedPnl:'48.5',averageExecutedPrice:'49.5',leverage:-0,marginMode:'CROSSED',bidAskLevel:Infinity};
const algo={algoOrderId:Infinity,clientAlgoOrderId:-0,rootAlgoOrderId:61,parentAlgoOrderId:62,symbol:'PERP_SOL_USDT',algoOrderTag:'algo-63',algoType:'STOP',side:'BUY',positionSide:'BOTH',quantity:Infinity,isTriggered:false,triggerStatus:'USELESS',type:'MARKET',rootAlgoStatus:'NEW',algoStatus:'NEW',triggerPriceType:'MARKET_PRICE',triggerTime:'64',triggerPrice:'65.5',price:'66.5',visibleQuantity:'67.5',totalExecutedQuantity:'68.5',averageExecutedPrice:'69.5',totalFee:'70.5',feeAsset:'USDT',totalRebate:-Infinity,rebateCurrency:'ETH',reduceOnly:false,createdTime:-0,updatedTime:73,isActivated:true,callbackRate:'74.5',activatedPrice:'75.5',leverage:Infinity,marginMode:'CROSSED',childOrders:[]};
const declarationEvidence={account:{...account,reconciliationId:'declaration-77',positionMode:'ONE_WAY'},positions:snapshot('positions',[position,{...position,symbol:'PERP_ETH_USDT',adlQuantile:-0,timestamp:Infinity}],'position-78'),regularOrders:snapshot('regularOrders',[regular,{...regular,orderId:-0,clientOrderId:81,createdTime:Infinity}],'regular-79'),algoOrders:snapshot('algoOrders',[{...algo,childOrders:[{...algo,algoOrderId:83,quantity:-0}]},{...algo,algoOrderId:85,clientAlgoOrderId:86,quantity:-0}],'algo-80')};
const declarations=[{type:'evidence',value:declarationEvidence},{type:'input',value:{targetPositionMode:'HEDGE_MODE',evidence:declarationEvidence}},{type:'input',value:{targetPositionMode:'ONE_WAY',evidence:{account:{...account,positionMode:'HEDGE_MODE'},positions:snapshot('positions'),regularOrders:snapshot('regularOrders'),algoOrders:snapshot('algoOrders')}}}].map(r=>({type:r.type,input:raw(r.value),expected:native(r.value)}));assert.equal(declarations.length,3);
const refusals=[];
for(const name of Object.keys(sources))for(const f of [s=>{s.source='bad';},s=>{s.complete=false;},s=>{s.rows=null;}]){const v=copy(declarationEvidence);f(v[name]);refusals.push({type:'evidence',input:raw(v)});}
refusals.push({type:'input',input:raw({targetPositionMode:'BAD',evidence:declarationEvidence})},{type:'evidence',input:'[]'});assert.equal(refusals.length,11);
const cancelRequest=(n)=>({method:'POST',path:contracts.WOO_V3_ENDPOINTS.TRADE_CANCEL_ALL_AFTER,body:{triggerAfter:n}});
// Pin actual declaration fields/types/requiredness and nested readonly/literal structure.
const ast=ts.createSourceFile('account.ts',fs.readFileSync(path.join(root,'woo.v3.account-writes.ts'),'utf8'),ts.ScriptTarget.Latest,true);
const alias=(a,n)=>a.statements.find(x=>ts.isTypeAliasDeclaration(x)&&x.name.text===n).type;
function record(a,t,schema){assert.equal(t.typeName.text,'Readonly');const fields=t.typeArguments[0].members;assert.deepEqual(fields.map(m=>m.name.text),schema.map(x=>x[0]));for(const [i,[n,type,optional=false]]of schema.entries()){assert.equal(!!fields[i].questionToken,optional,n);if(Array.isArray(type))record(a,fields[i].type,type);else assert.equal((ts.isUnionTypeNode(fields[i].type)?fields[i].type.types.map(t=>t.getText(a).replace(/\s+/g,'')).join('|'):fields[i].type.getText(a).replace(/\s+/g,'')),type,n);}}
record(ast,alias(ast,'WooV3CompletePositionModeEvidence'),[['account','WooV3AccountModeProvenance'],...Object.entries(sources).map(([n,s])=>[n,[['source',"'"+s+"'"],['reconciliationId','string'],['complete','true'],['rows','readonly'+({positions:'WooV3FuturesPosition',regularOrders:'WooV3RegularOrder',algoOrders:'WooV3AlgoOrder'})[n]+'[]']]])]);
record(ast,alias(ast,'WooV3SetPositionModeInput'),[['targetPositionMode','WooV3AccountPositionMode'],['evidence','WooV3CompletePositionModeEvidence']]);
const unions=alias(ast,'WooV3SetPositionModeBuildResult').types;assert.equal(unions.length,2);
record(ast,unions[0],[['ready','true'],['request','WooV3RequestInput'],['currentPositionMode','WooV3AccountPositionMode'],['targetPositionMode','WooV3AccountPositionMode'],['preconditionEvidence',"'complete-reconciled-v3-positions-zero-and-pending-orders-empty'"]]);
record(ast,unions[1],[['ready','false'],['blocker',"'account-state-evidence-incomplete-or-mixed'|'futures-margin-account-mode-not-proven'|'position-mode-already-active'|'open-positions-must-be-closed'|'pending-regular-orders-must-be-cancelled'|'pending-algo-orders-must-be-cancelled'"],['currentPositionMode','WooV3AccountPositionMode',true],['targetPositionMode','WooV3AccountPositionMode']]);
const aliasRows=[{x:1}];const aliasSnapshot=api.validateSnapshotEvidence(snapshot('positions',aliasRows),sources.positions,'probe');assert.equal(aliasSnapshot.rows,aliasRows);aliasRows[0].x=2;assert.equal(aliasSnapshot.rows[0].x,2);assert.ok(Object.isFrozen(aliasSnapshot));assert.ok(Object.isFrozen(ready)&&Object.isFrozen(ready.request)&&Object.isFrozen(ready.request.body));
console.log(JSON.stringify({zeros,positions,snapshots,builds,responses,declarations,refusals,sources}));
