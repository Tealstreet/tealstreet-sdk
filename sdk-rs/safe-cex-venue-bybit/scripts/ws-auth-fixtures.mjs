import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHmac} from 'node:crypto';
import ts from 'typescript';
import vm from 'node:vm';
const source=ts.createSourceFile('private.ts',readFileSync(new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts',import.meta.url),'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(ts.isClassDeclaration), member=klass.members.find(m=>m.name?.getText(source)==='auth');assert.ok(member);
const context=vm.createContext({createHmac});vm.runInContext(ts.transpileModule(`class Auth {${member.getText(source)}};globalThis.auth=new Auth();`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const timestamps=[0,1,-10000,-10000.5,-9999.5,-10002.5,1e12,1e15,1e20,1e21,-1e21,1000000000000000100,Infinity,-Infinity,NaN];
for(let i=0;i<100;i++)timestamps.push(1699999999999+(i-50)/8);
const cases=[];
for(const now of timestamps)for(const secret of ['', 'secret', '\uFEFF🔑\u0085'])for(const disposed of [false,true]) {
 const messages=[];context.virtualClock={getCurrentTimeMs:()=>now};context.auth.parent={options:{key:'fixture-key',secret}};
 context.auth.sendWithLog=message=>{if(!disposed)messages.push(message)};
 const result=await context.auth.auth();
 cases.push({now:Number.isFinite(now)?now:String(now),secret,disposed,result,messages});
}
if (process.argv.includes('--boundaries')) {
 const inputs = [
  {label:'numeric signature',response:{wsAuthSignature:7},refused:true},
  {label:'boolean signature',response:{wsAuthSignature:false},refused:true},
  {label:'array signature',response:{wsAuthSignature:['signed']},refused:true},
  {label:'object signature',response:{wsAuthSignature:{signed:true}},refused:true},
  {label:'falsy numeric error',response:{error:0},refused:true},
  {label:'truthy numeric error',response:{error:1},refused:true},
  {label:'boolean error',response:{error:true},refused:true},
  {label:'object error',response:{error:{reason:'unpaired'}},refused:true},
  {label:'array error',response:{error:[]},refused:true},
  {label:'primitive response',response:true,refused:true},
  {label:'array response',response:[],refused:true},
  {label:'null signature',response:{wsAuthSignature:null},refused:false},
  {label:'null error',response:{error:null,wsAuthSignature:'signed'},refused:false},
  {label:'scalar Unicode signature',response:{wsAuthSignature:'\uFEFF🔑\u0085'},refused:false},
  {label:'lone surrogate signature',response:JSON.parse('{"wsAuthSignature":"\\ud800"}'),refused:true},
 ];
 const boundaries=[];
 for(const input of inputs) {
  const messages=[],errors=[]; context.virtualClock={getCurrentTimeMs:()=>1000};context.nanoid=()=> 'fixture';
  context.auth.parent={options:{key:'',remoteSigner:{accountName:'account',sign:async()=>input.response}}};
  context.auth.sendWithLog=message=>messages.push(message);
  context.auth.emitAndLogError=message=>errors.push(message);
  const result=await context.auth.auth();
  const rejectsAtVenue=['truthy numeric error','boolean error','object error','array error'].includes(input.label);
  assert.equal(result,!rejectsAtVenue,input.label);
  assert.equal(messages.length,rejectsAtVenue?0:1,input.label);
  assert.equal(errors.length,rejectsAtVenue?1:0,input.label);
  boundaries.push({label:input.label,refused:input.refused,responseText:JSON.stringify(input.response),messages,errors,result});
 }
 // Capture before JSON.stringify: wire comparisons alone erase this difference.
 const expires=[];
 context.auth.sendWithLog=()=>{};
 for(const now of [Infinity,-Infinity,NaN]) {
  let seen;
  context.virtualClock={getCurrentTimeMs:()=>now};
  context.auth.parent={options:{key:'',remoteSigner:{sign:async request=>{seen=request.wsAuth.extraData.expires;return {wsAuthSignature:'signed'};}}}};
  await context.auth.auth();
  expires.push({now:String(now),kind:Number.isNaN(seen)?'NaN':String(seen),serialized:JSON.stringify(seen)});
 }
 assert.equal(boundaries.length,15);
 assert.deepEqual(expires.map(x=>x.kind),['Infinity','-Infinity','NaN']);
 process.stdout.write(JSON.stringify({boundaries,expires}));
} else if (process.argv.includes('--remote')) {
 const remote=[];
 for(const now of timestamps) for(const response of [{wsAuthSignature:'signed'}, {}, {error:''}, {error:'unpaired'}, {error:'\uFEFF'}]) for(const disposed of [false,true]) {
  const messages=[], errors=[], requests=[]; context.virtualClock={getCurrentTimeMs:()=>now}; context.nanoid=()=> 'fixture';
  context.auth.parent={options:{key:'fixture-key',remoteSigner:{accountName:'account',sign:async request=>{requests.push(request);return response;}}}};
  context.auth.sendWithLog=message=>{if(!disposed)messages.push(message)};
  context.auth.emitAndLogError=message=>errors.push(message);
  const result=await context.auth.auth();
  remote.push({now:Number.isFinite(now)?now:String(now), response, disposed, requests, messages, errors, result});
 }
 process.stdout.write(JSON.stringify(remote));
} else process.stdout.write(JSON.stringify(cases));
