'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,extras=[],imports={}){const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8')+(extras.length?'\nexport { '+extras.join(',')+' };':''),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(name){assert.ok(Object.hasOwn(imports,name));return imports[name];}});return exports;}
const api=load('woo.v3.account-writes.ts',['requireRecord','requireBoolean','requireInteger','requireString','requireEnum','requireExactKeys','requirePerpetualSymbol','validateAccountProvenance','decodeSuccess'],{'./woo.v3.contracts':load('woo.v3.contracts.ts')});
if (process.argv.includes('--guard-audit')) {
 const record={n:-0};const admitted=api.requireRecord(record,'guard');
 assert.equal(admitted,record,'WOO-GUARD source record alias');record.n=37;assert.equal(admitted.n,37);
 assert.throws(()=>api.requireRecord([],'guard'),/guard must be an object/);
 assert.equal(api.requireBoolean(false,'guard'),false);assert.equal(api.requireBoolean(true,'guard'),true);
 assert.throws(()=>api.requireBoolean(0,'guard'),/guard must be a boolean/);
 assert.ok(Object.is(api.requireInteger(-0,'guard',NaN),-0),'WOO-GUARD source NaN minimum keeps negative zero');
 assert.equal(api.requireInteger(9007199254740991,'guard'),9007199254740991);
 assert.throws(()=>api.requireInteger(9007199254740992,'guard'),/guard must be a safe integer of at least 0/);
 assert.throws(()=>api.requireInteger(1,'guard',Infinity),/guard must be a safe integer of at least Infinity/);
 assert.equal(api.requireString(' \t','guard'),' \t');
 assert.throws(()=>api.requireString('','guard'),/guard must be a non-empty string/);
 assert.equal(api.requireEnum('B',['B','A'],'guard'),'B');
 assert.throws(()=>api.requireEnum('C',['B','A'],'guard'),/guard must be one of B, A/);
 const allowed=['A'];allowed.includes=()=>true;
 assert.equal(api.requireEnum('B',allowed,'guard'),'B','WOO-GUARD source live includes override');
 const thrown={origin:'includes'};allowed.includes=()=>{throw thrown;};
 assert.throws(()=>api.requireEnum('B',allowed,'guard'),e=>e===thrown,'WOO-GUARD source includes error identity');
 let calls=0;allowed.includes=()=>{calls++;return false;};allowed.join=()=>{throw thrown;};
 assert.throws(()=>api.requireEnum(0,allowed,'guard'),e=>e===thrown);assert.equal(calls,0);
 const astral='\ud800\udc00',bmp='\ue000';
 assert.equal(api.requireExactKeys({[bmp]:1,[astral]:2},[bmp,astral],'guard'),undefined);
 assert.throws(()=>api.requireExactKeys({extra:1},[bmp,astral],'guard'),e=>e.message===`guard must contain exactly ${astral}, ${bmp}`);
 const unit=JSON.parse('"\\ud800"');
 const out=api.requireString(unit,'guard');
 assert.equal(typeof out,'string');assert.equal(out.length,1);assert.equal(out.charCodeAt(0),0xd800,'WOO-GUARD source returned lone unit');
 const enumOut=api.requireEnum(unit,[unit],'guard');assert.equal(enumOut.length,1);assert.equal(enumOut.charCodeAt(0),0xd800);
 const keyed=JSON.parse('{"\\ud800":1}'),objectOut=api.requireRecord(keyed,'guard');
 const keys=Object.keys(objectOut);assert.equal(keys.length,1);assert.equal(keys[0].length,1);assert.equal(keys[0].charCodeAt(0),0xd800);
 assert.equal(objectOut[unit],1,'WOO-GUARD source lone-key lookup');
 assert.equal(api.requireExactKeys(keyed,[unit],'guard'),undefined,'WOO-GUARD source lone-key equality');
 const ascii=String.raw`\ud800`;
 assert.equal(api.requireString(ascii,'guard'),ascii);assert.equal(ascii.length,6);
 assert.equal(api.requireEnum(ascii,[ascii],'guard'),ascii);
 assert.equal(api.requireString(astral,'guard').codePointAt(0),0x10000);
 process.stdout.write(JSON.stringify({groups:6,surrogateRows:4}));process.exit(0);
}
if (process.argv.includes('--choice-audit')) {
 const account={source:'decoded-v3-account-info',reconciliationId:'audit-17',accountMode:'MARGIN',positionMode:'ONE_WAY'};
 const provenance=api.validateAccountProvenance(account);
 assert.ok(Object.isFrozen(provenance));assert.notEqual(provenance,account);assert.equal(provenance.reconciliationId,'audit-17');
 const get=api.buildWooV3GetLeverageRequest({symbol:'PERP_BTC_USDT',marginMode:'ISOLATED',positionMode:'HEDGE_MODE'});
 assert.equal(get.request.query,get.target,'WOO-ACCOUNT-CHOICE source GET target alias');assert.ok(Object.isFrozen(get)&&Object.isFrozen(get.target));
 assert.equal(get.request.method,'GET');assert.equal(get.request.path,'/v3/futures/leverage');
 const setInput={symbol:'PERP_BTC_USDT',marginMode:'CROSS',leverage:31,account};
 const set=api.buildWooV3SetLeverageRequest(setInput);
 assert.ok(set.ready);assert.equal(set.request.body.leverage,31);assert.ok(Object.isFrozen(set.request.body));
 assert.equal(api.buildWooV3SetLeverageRequest({...setInput,marginMode:'ISOLATED',account:{...account,accountMode:'SPOT',positionMode:'HEDGE_MODE'}}).blocker,'futures-margin-account-mode-not-proven');
 assert.equal(api.buildWooV3SetLeverageRequest({...setInput,marginMode:'ISOLATED',account:{...account,positionMode:'HEDGE_MODE'}}).blocker,'isolated-margin-outside-safe-cex-contract');
 assert.equal(api.buildWooV3SetLeverageRequest({...setInput,account:{...account,positionMode:'HEDGE_MODE'}}).blocker,'cross-hedge-resulting-leg-scope-requires-demo-evidence');
 const nested={value:41};const error=new api.WooV3AccountConfigResponseError('audit-error',{nested},-0);
 assert.equal(error.name,'WooV3AccountConfigResponseError');assert.equal(error.message,'audit-error');assert.equal(error.contractStatus,'vendor-error-envelope-undocumented');
 assert.ok(Object.is(error.code,-0));assert.ok(Object.isFrozen(error.gatewayResponse));
 assert.equal(error.gatewayResponse.nested,nested);nested.value=43;assert.equal(error.gatewayResponse.nested.value,43);
 assert.ok(Object.is(api.decodeSuccess({success:true,timestamp:-0},'audit'),-0));
 assert.throws(()=>api.decodeSuccess({success:false,code:-1,message:''},'audit'),/audit.code must be a safe integer/);
 assert.throws(()=>api.decodeSuccess({success:true,timestamp:-1,extra:1},'audit'),/success response must contain exactly/);
 const read=api.decodeWooV3GetLeverageResponse({success:true,timestamp:97,data:{...get.target,details:[{positionSide:'SHORT',leverage:29},{positionSide:'LONG',leverage:17}]}},get);
 assert.equal(read.details.length,2);assert.equal(read.details[0].positionSide,'LONG');assert.equal(read.details[0].leverage,17);
 assert.equal(read.details[1].positionSide,'SHORT');assert.equal(read.details[1].leverage,29);assert.equal(read.timestamp,97);
 assert.ok(Object.isFrozen(read)&&Object.isFrozen(read.details)&&Object.isFrozen(read.details[0]));
 const mutated=api.decodeWooV3SetLeverageResponse({success:true,timestamp:-0},{...set,target:{...set.target,family:'source-extra',extra:89}});
 assert.equal(mutated.target.family,'source-extra');assert.equal(mutated.target.extra,89);assert.equal(mutated.target.leverage,31);
 assert.ok(Object.isFrozen(mutated)&&Object.isFrozen(mutated.target));assert.ok(Object.is(mutated.timestamp,-0));
 const mode=api.decodeWooV3SetPositionModeResponse({success:true,timestamp:-0},{ready:true,request:{method:'PUT',path:'/v3/futures/positionMode'},targetPositionMode:'HEDGE_MODE'});
 assert.equal(mode.accepted,true);assert.equal(mode.target.family,'position-mode');assert.equal(mode.target.positionMode,'HEDGE_MODE');
 assert.equal(mode.stateEvidence,'request-target-only-response-does-not-echo-resulting-state');
 assert.ok(Object.isFrozen(mode)&&Object.isFrozen(mode.target));assert.ok(Object.is(mode.timestamp,-0));
 process.stdout.write(JSON.stringify({groups:8}));process.exit(0);
}
function native(v){if(typeof v==='number')return {$number:Object.is(v,-0)?'-0':String(v)};if(Array.isArray(v))return v.map(native);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>[k,native(x)]));return v;}
function raw(v){if(v===undefined)return undefined;if(typeof v==='number')return Object.is(v,-0)?'-0':v===Infinity?'1e999':v===-Infinity?'-1e999':String(v);if(Array.isArray(v))return '['+v.map(raw).join(',')+']';if(v&&typeof v==='object')return '{'+Object.entries(v).filter(([,x])=>x!==undefined).map(([k,x])=>JSON.stringify(k)+':'+raw(x)).join(',')+'}';return JSON.stringify(v);}
function capture(fn){try{const out=fn();return {ok:native(out)};}catch(e){const error={name:e.name,message:e.message};if(e instanceof api.WooV3AccountConfigResponseError){error.contractStatus=e.contractStatus;error.gatewayResponse=native(e.gatewayResponse);error.gatewayKeys=Object.keys(e.gatewayResponse);if(e.code!==undefined)error.code=native(e.code);assert.ok(Object.isFrozen(e.gatewayResponse));}return {error};}}
const values=[undefined,null,[],{},false,true,0,-0,1,-1,0.5,9007199254740991,9007199254740992,Infinity,'',' ','ONE_WAY','CROSS'];assert.equal(values.length,18);
const helpers=[];for(const name of ['requireRecord','requireBoolean','requireInteger','requireString','requireEnum'])for(const value of values){helpers.push({name,input:raw(value),...capture(()=>name==='requireEnum'?api[name](value,['ONE_WAY','HEDGE_MODE'],'probe'):name==='requireInteger'?api[name](value,'probe',0):api[name](value,'probe'))});}assert.equal(helpers.length,90);
const exactKeys=[{b:1,a:2},{a:2},{a:2,b:1,extra:true},{'\ue000':1,'\ud800\udc00':2},{}].map((v,i)=>{const expected=i>=3?['\ue000','\ud800\udc00']:['b','a'];return {input:raw(v),keys:expected,...capture(()=>{api.requireExactKeys(v,expected,'probe');return true;})};});assert.equal(exactKeys.length,5);
const symbols=['PERP_BTC_USDT','PERP_1_2','SPOT_BTC_USDT','PERP_btc_USDT','PERP_BTC_USDT\n','PERP__USDT','PERP_A_B_C',''];const symbolCases=symbols.map(v=>({input:raw(v),...capture(()=>api.requirePerpetualSymbol(v))}));assert.equal(symbolCases.length,8);
const account={source:'decoded-v3-account-info',reconciliationId:'atomic-17',accountMode:'MARGIN',positionMode:'ONE_WAY'};
const provenanceValues=[account,{...account,accountMode:'SPOT',positionMode:'HEDGE_MODE'},{...account,source:'bad',reconciliationId:''},{...account,reconciliationId:'',accountMode:'bad'},{...account,accountMode:'bad',positionMode:'bad'},{...account,positionMode:'bad'},null,[],undefined];
const provenance=provenanceValues.map(v=>({input:raw(v),...capture(()=>api.validateAccountProvenance(v))}));assert.equal(provenance.length,9);
const getInput={symbol:'PERP_BTC_USDT',marginMode:'CROSS',positionMode:'ONE_WAY'},hedgeInput={...getInput,marginMode:'ISOLATED',positionMode:'HEDGE_MODE'};
const getValues=[getInput,hedgeInput,{...getInput,symbol:'BAD',marginMode:'BAD'},{...getInput,marginMode:'BAD',positionMode:'BAD'},{...getInput,positionMode:'BAD'},{}];
const getBuilds=getValues.map(v=>({input:raw(v),...capture(()=>api.buildWooV3GetLeverageRequest(v))}));assert.equal(getBuilds.length,6);
const setInput={symbol:'PERP_BTC_USDT',marginMode:'CROSS',leverage:12,account};
const setValues=[setInput,{...setInput,leverage:9007199254740991},{...setInput,account:{...account,accountMode:'SPOT'}},{...setInput,marginMode:'ISOLATED',account:{...account,accountMode:'SPOT',positionMode:'HEDGE_MODE'}},{...setInput,marginMode:'ISOLATED',account:{...account,positionMode:'HEDGE_MODE'}},{...setInput,account:{...account,positionMode:'HEDGE_MODE'}},{...setInput,symbol:'BAD',marginMode:'BAD',leverage:0},{...setInput,marginMode:'BAD',leverage:0},{...setInput,leverage:0,account:null},{...setInput,leverage:0.5},{...setInput,leverage:Infinity},{...setInput,account:null},{...setInput,account:{...account,reconciliationId:''}}];
const setBuilds=setValues.map(v=>({input:raw(v),...capture(()=>api.buildWooV3SetLeverageRequest(v))}));assert.equal(setBuilds.length,13);
const oneBuild=api.buildWooV3GetLeverageRequest(getInput),hedgeBuild=api.buildWooV3GetLeverageRequest(hedgeInput),setBuild=api.buildWooV3SetLeverageRequest(setInput);
const response=(target,details,time=17)=>({success:true,timestamp:time,data:{...target,details}}),detail=(side,leverage)=>({positionSide:side,leverage});
const one=response(getInput,[detail('BOTH',13)],-0),hedge=response(hedgeInput,[detail('SHORT',29),detail('LONG',17)],9007199254740991);
const getResponses=[];const add=(input,build,buildName,requestOverrides)=>getResponses.push({input:raw(input),build:buildName,requestOverrides,...capture(()=>api.decodeWooV3GetLeverageResponse(input,requestOverrides?{...build,request:{...build.request,...requestOverrides}}:build))});
add(one,oneBuild,'one');add(hedge,hedgeBuild,'hedge');
for(const v of [null,[],{success:'bad'},{success:true,timestamp:0.5,data:null}, {...one,extra:true,timestamp:-1}, {...one,data:{...one.data,extra:true}}, {...one,data:{...one.data,symbol:'BAD',marginMode:'BAD'}}, {...one,data:{...one.data,marginMode:'BAD',positionMode:'BAD'}}, {...one,data:{...one.data,symbol:'PERP_ETH_USDT',details:null}}, {...one,data:{...one.data,details:null}}, response(getInput,[{positionSide:'BAD',leverage:0}]), response(getInput,[{positionSide:'BOTH',leverage:0,extra:true}]), response(getInput,[detail('BOTH',0)]),response(getInput,[]),response(getInput,[detail('LONG',10)]),response(getInput,[detail('BOTH',10),detail('BOTH',11)]),{success:false},{success:false,code:-0,message:'denied',nested:{time:Infinity}}, {success:false,code:0.5,message:''},{success:false,message:''}])add(v,oneBuild,'one');
for(const v of [response(hedgeInput,[detail('LONG',17),detail('SHORT',29)]),response(hedgeInput,[detail('LONG',17),detail('LONG',29)]),response(hedgeInput,[detail('BOTH',17),detail('SHORT',29)]),response(hedgeInput,[detail('SHORT',29)]),response(hedgeInput,[detail('SHORT',29),detail('LONG',0)])])add(v,hedgeBuild,'hedge');
add(null,oneBuild,'one',{method:'PUT'});add(null,oneBuild,'one',{path:'/v3/futures/positionMode'});assert.equal(getResponses.length,29);
const successInputs=[{success:true,timestamp:-0},{success:true,timestamp:9007199254740991},{success:true,timestamp:0.5},{success:true,timestamp:-1},{success:true,timestamp:1,extra:true},{success:false},{success:false,code:-0,message:'setter denied',nested:[Infinity,-0]},{success:false,code:-1,message:''},{success:false,code:3,message:''},{success:null},null,[]];
const successes=successInputs.map(v=>({input:raw(v),...capture(()=>api.decodeSuccess(v,'set futures leverage'))}));assert.equal(successes.length,12);
const mutations=successInputs.map(v=>({input:raw(v),...capture(()=>api.decodeWooV3SetLeverageResponse(v,setBuild))}));for(const requestOverrides of [{method:'GET'},{path:'/v3/futures/positionMode'}])mutations.push({input:'null',requestOverrides,...capture(()=>api.decodeWooV3SetLeverageResponse(null,{...setBuild,request:{...setBuild.request,...requestOverrides}}))});assert.equal(mutations.length,14);
const gateway={code:-0,success:false,message:'constructor message',nested:{time:Infinity,zero:-0}};const directError=new api.WooV3AccountConfigResponseError('direct error',gateway,-0);const constructor={message:directError.message,name:directError.name,code:native(directError.code),gatewayResponse:native(directError.gatewayResponse),gatewayKeys:Object.keys(directError.gatewayResponse),contractStatus:directError.contractStatus};gateway.message='later outer';assert.equal(directError.gatewayResponse.message,'constructor message');gateway.nested.time=99;assert.equal(directError.gatewayResponse.nested.time,99,'source shallow error snapshot retains nested alias');
assert.equal(oneBuild.request.query,oneBuild.target,'GET query is the frozen target alias');assert.ok(Object.isFrozen(oneBuild)&&Object.isFrozen(oneBuild.target)&&Object.isFrozen(oneBuild.request));for(const x of [one,hedge]){const out=api.decodeWooV3GetLeverageResponse(x,x===one?oneBuild:hedgeBuild);assert.ok(Object.isFrozen(out)&&Object.isFrozen(out.details));for(const row of out.details)assert.ok(Object.isFrozen(row));}
const strictSymbolSource=api.buildWooV3GetLeverageRequest({...getInput,symbol:['PERP_BTC_USDT']});assert.ok(Array.isArray(strictSymbolSource.target.symbol));
const declarations=[
 {type:'WooV3AccountModeProvenance',value:account},
 {type:'WooV3SetLeverageInput',value:{...setInput,leverage:Infinity}},
 {type:'WooV3GetLeverageInput',value:hedgeInput},
 {type:'WooV3GetLeverageRequest',value:hedgeBuild},
 {type:'WooV3LeverageReadback',value:{symbol:'PERP_A_B',marginMode:'CROSS',positionMode:'ONE_WAY',details:[detail('BOTH',-0)],timestamp:Infinity}},
 {type:'WooV3LeverageReadback',value:{symbol:'PERP_C_D',marginMode:'ISOLATED',positionMode:'HEDGE_MODE',details:[detail('LONG',Infinity),detail('SHORT',-0)],timestamp:-0}},
].map(row=>({type:row.type,input:raw(row.value),expected:native(row.value)}));assert.equal(declarations.length,6);
const enumValues={positionMode:['ONE_WAY','HEDGE_MODE'],marginMode:['CROSS','ISOLATED']};
const ast=ts.createSourceFile('account-writes.ts',fs.readFileSync(path.join(root,'woo.v3.account-writes.ts'),'utf8'),ts.ScriptTarget.Latest,true),alias=n=>ast.statements.find(x=>ts.isTypeAliasDeclaration(x)&&x.name.text===n).type;
assert.deepEqual(alias('WooV3AccountPositionMode').types.map(x=>x.literal.text),enumValues.positionMode);assert.deepEqual(alias('WooV3FuturesMarginMode').types.map(x=>x.literal.text),enumValues.marginMode);
const recordSchemas={
 WooV3AccountModeProvenance:[['source',"'decoded-v3-account-info'"],['reconciliationId','string'],['accountMode',"'MARGIN'|'SPOT'"],['positionMode','WooV3AccountPositionMode']],
 WooV3SetLeverageInput:[['symbol','string'],['marginMode','WooV3FuturesMarginMode'],['leverage','number'],['account','WooV3AccountModeProvenance']],
 WooV3GetLeverageInput:[['symbol','string'],['marginMode','WooV3FuturesMarginMode'],['positionMode','WooV3AccountPositionMode']],
 WooV3GetLeverageRequest:[['request','WooV3RequestInput'],['target','WooV3GetLeverageInput']],
};
const typeText=node=>node.getText(ast).replace(/\s+/g,'');
for(const [name,expected] of Object.entries(recordSchemas)){const type=alias(name);assert.equal(type.typeName.text,'Readonly');const fields=type.typeArguments[0].members;assert.ok(fields.every(m=>!m.questionToken));assert.deepEqual(fields.map(m=>[m.name.text,typeText(m.type)]),expected,'declaration field types '+name);}
assert.equal(alias('WooV3LeverageReadback').types.length,2);
for(const [i,type] of alias('WooV3LeverageReadback').types.entries()){
 assert.equal(type.typeName.text,'Readonly');const fields=type.typeArguments[0].members;assert.ok(fields.every(m=>!m.questionToken));assert.deepEqual(fields.map(m=>m.name.text),['symbol','marginMode','positionMode','details','timestamp']);assert.equal(fields[0].type.kind,ts.SyntaxKind.StringKeyword);assert.equal(typeText(fields[1].type),'WooV3FuturesMarginMode');assert.equal(fields[2].type.literal.text,i===0?'ONE_WAY':'HEDGE_MODE');assert.equal(fields[3].type.operator,ts.SyntaxKind.ReadonlyKeyword);const tuple=fields[3].type.type;assert.equal(tuple.elements.length,i===0?1:2);
 for(const [index,detail] of tuple.elements.entries()){assert.equal(detail.typeName.text,'Readonly');const members=detail.typeArguments[0].members;assert.ok(members.every(m=>!m.questionToken));assert.deepEqual(members.map(m=>m.name.text),['positionSide','leverage']);assert.equal(members[0].type.literal.text,i===0?'BOTH':index===0?'LONG':'SHORT');assert.equal(members[1].type.kind,ts.SyntaxKind.NumberKeyword);}
 assert.equal(fields[4].type.kind,ts.SyntaxKind.NumberKeyword);
}
const parity=api.WOO_V3_ACCOUNT_WRITE_PARITY;assert.equal(Object.keys(parity).length,11);assert.ok(Object.isFrozen(parity));
console.log(JSON.stringify({helpers,exactKeys,symbolCases,provenance,getBuilds,setBuilds,getResponses,successes,mutations,constructor,declarations,enumValues,parity,strictSymbol:{input:raw({...getInput,symbol:['PERP_BTC_USDT']}),source:native(strictSymbolSource)}}));
