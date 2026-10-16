const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/woo/woo.v3.public-parity.ts'),'utf8');
const exportsObject={};
vm.runInNewContext(ts.transpileModule(source+'\nexport { blocker,mismatch,compareFields,compareKeyed,result };',{
 compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,
 {exports:exportsObject,require:key=>{assert.equal(key,'./woo.v3.public');return {mapWooV3Candles:()=>{throw Error('out-of-scope mapper invoked');}};}});
const api=exportsObject; const bits=require('./number-bits.cjs');
let ids=new Map(); function value(v){if(v===undefined)return {kind:'undefined'};if(v===null)return {kind:'null'};
 if(typeof v==='number')return {kind:'number',bits:bits(v)};if(typeof v==='bigint')return {kind:'bigint',value:String(v)};
 if(typeof v==='string')return {kind:'string',value:v};if(typeof v==='boolean')return {kind:'bool',value:v};
 if(!ids.has(v))ids.set(v,ids.size+1);return {kind:'reference',id:ids.get(v)};}
function capture(m){assert.ok(Object.isFrozen(m));assert.ok(Object.isFrozen(m.provenance));return {kind:m.kind,path:m.path,legacy:value(m.legacyValue),v3:value(m.v3Value),legacyProvenance:m.provenance.legacy,v3Provenance:m.provenance.v3};}
const object={x:1}; const other={x:1}; ids.set(object,1);ids.set(other,2);
const pairs=[[NaN,NaN],[0,-0],[undefined,null],[object,object],[object,other],[5,7],[2n,2n],[2n,3n]];
const fields=pairs.map(([legacy,v3],index)=>{const trace=[];const f={path:'value',legacy:()=>{trace.push('legacy');return legacy;},v3:()=>{trace.push('v3');return v3;},legacyProvenance:'L',v3Provenance:'V'};
 const sparse=[,f];const out=api.compareFields({}, {},'root',sparse);assert.deepEqual(trace,['legacy','v3']);return {index,trace,out:out.map(capture)};});
assert.equal(fields.length,8);
let trace=[];const legacy=[{key:'OTHER',value:11},{key:'42',value:22},{key:'OTHER',value:99},{key:'missing',value:33}];
const v3=[{key:'42',value:23},{key:'new',value:44},{key:'42',value:88},{key:'OTHER',value:12}];
const field={path:'value',legacy:x=>{trace.push('L:'+x.key);return x.value;},v3:x=>{trace.push('V:'+x.key);return x.value;},legacyProvenance:'LF',v3Provenance:'VF'};
const keyed=api.compareKeyed({path:'rows',legacy,v3,legacyKey:x=>{trace.push('LK:'+x.key);return x.key;},v3Key:x=>{trace.push('VK:'+x.key);return x.key;},fields:[field],legacyProvenance:'LR',v3Provenance:'VR'});
assert.equal(keyed.length,6);const keyedCapture=keyed.map(capture);
const undefinedTrace=[];
const undef=api.compareKeyed({path:'rows',legacy:[{}],v3:[undefined],legacyKey:()=> 'K',v3Key:()=> 'K',fields:[{...field,legacy:()=>{undefinedTrace.push('L');return 1;},v3:()=>{undefinedTrace.push('V');return 2;}}],legacyProvenance:'LR',v3Provenance:'VR'});
assert.deepEqual(undefinedTrace,[]);assert.equal(undef.length,1);
const error={tag:'unchanged'};const errorTrace=[];let sameError=false;
try{api.compareFields({}, {},'x',[{...field,legacy:()=>{errorTrace.push('L');throw error;},v3:()=>{errorTrace.push('V');return 2;}}]);}catch(e){sameError=e===error;}
assert.equal(sameError,true);assert.deepEqual(errorTrace,['L']);
const missing=api.blocker('invalid-request-context','detail');
const present=api.blocker('request-symbol-mismatch','detail',{path:undefined,expected:undefined,actual:null});
const blockerCapture=[missing,present].map(b=>{assert.ok(Object.isFrozen(b));return {code:b.code,detail:b.detail,pathOwn:Object.hasOwn(b,'path'),path:value(b.path),expectedOwn:Object.hasOwn(b,'expected'),expected:value(b.expected),actualOwn:Object.hasOwn(b,'actual'),actual:value(b.actual)};});
const a={version:1},o={version:2};const cases=[{mismatches:[],blockers:undefined},{mismatches:[keyed[0]],blockers:[]},{mismatches:[keyed[0]],blockers:[present]},{mismatches:new Array(1),blockers:new Array(1)}];
const results=cases.map(c=>{const r=api.result({routeId:'markets',authoritative:a,observed:o,legacyCanonical:'LC',v3Canonical:'VC',...c});
 assert.equal(r.authoritative,a);assert.equal(r.observed,o);assert.ok(Object.isFrozen(r));assert.ok(Object.isFrozen(r.mismatches));assert.ok(Object.isFrozen(r.blockers));
 const len=r.mismatches.length;c.mismatches.push(keyed[1]);assert.equal(r.mismatches.length,len);
 return {status:r.status,routeId:r.routeId,legacyCanonical:r.legacyCanonical,v3Canonical:r.v3Canonical,storeAction:r.storeAction,mismatches:r.mismatches.length,blockers:r.blockers.length,shallowMismatch:r.mismatches[0]===undefined?'undefined':r.mismatches[0]===keyed[0]?'same':'different'};});
a.version=9;const aliasResult=api.result({routeId:'tickers',authoritative:a,observed:o,legacyCanonical:'LC',v3Canonical:'VC',mismatches:[]});a.version=10;assert.equal(aliasResult.authoritative.version,10);
const lone=JSON.parse('"\\ud800"');
const force=[api.mismatch('value',lone,lone,0,lone,'V'),api.blocker('invalid-request-context',lone,{path:lone,expected:lone}),api.result({routeId:'markets',authoritative:a,observed:o,legacyCanonical:lone,v3Canonical:'VC',mismatches:[]}),
 api.compareFields({}, {},lone,[{path:'value',legacy:()=>1,v3:()=>2,legacyProvenance:lone,v3Provenance:lone}])[0],
 api.compareKeyed({path:lone,legacy:[{}],v3:[],legacyKey:()=>lone,v3Key:()=>'',fields:[],legacyProvenance:lone,v3Provenance:'V'})[0]];
const slots=[['path','legacyValue','provenance.legacy'],['detail','path','expected'],['legacyCanonical'],['path','provenance.legacy','provenance.v3'],['path','legacyValue','provenance.legacy']];
const refusals=force.map((obj,i)=>{const values=slots[i].map(s=>s.split('.').reduce((x,k)=>x[k],obj));const units=values.map(v=>{assert.equal(v.charCodeAt(0),0xd800);return v.charCodeAt(0);});return {slots:slots[i],units,stringWires:values.map(v=>JSON.stringify(v)),wire:JSON.stringify(obj)};});
const alias=api.mismatch('value','x',object,other,'L','V');object.x=9;assert.equal(alias.legacyValue.x,9);
const liveLegacy=[{key:'first'}];const live=api.compareKeyed({path:'live',legacy:liveLegacy,v3:[],legacyKey:x=>{if(x.key==='first')liveLegacy.push({key:'appended'});return x.key;},v3Key:x=>x.key,fields:[],legacyProvenance:'L',v3Provenance:'V'});assert.equal(live.length,2);
const override=api.blocker('invalid-request-context','original',{code:'override',detail:'changed',extra:7});assert.equal(override.code,'override');assert.equal(override.detail,'changed');
const metadataField={...field,path:'before',legacy:()=>{metadataField.path='after';return 1;},v3:()=>2};
const metadata=api.compareFields({}, {},'metadata',[metadataField]);assert.equal(metadata[0].path,'metadata.after');
const custom=[];custom[Symbol.iterator]=function*(){yield keyed[0];};
const customResult=api.result({routeId:'markets',authoritative:a,observed:o,legacyCanonical:'LC',v3Canonical:'VC',mismatches:custom});assert.equal(customResult.status,'mismatch');assert.equal(customResult.mismatches.length,1);
process.stdout.write(JSON.stringify({fields,keyed:keyedCapture,trace,undefined:undef.map(capture),errorTrace,sameError,blockers:blockerCapture,results,refusals,chosen:{liveArrayPaths:live.map(m=>m.path),compoundAliasAfter:alias.legacyValue.x,contextOverride:override,metadataPath:metadata[0].path,customResultStatus:customResult.status,customResultCount:customResult.mismatches.length}}));
