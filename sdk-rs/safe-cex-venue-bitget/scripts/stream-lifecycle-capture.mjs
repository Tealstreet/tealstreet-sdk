import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BitgetExchange');
const names=['dispose','markPrivateStreamMessageReceived','startPrivateStreamDegradedReconcile','stopPrivateStreamDegradedReconcile'];
const bodies=names.map(name=>{
 const field=owner.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);
 assert(field?.initializer,'STREAM actual field '+name);
 // Preserve lexical super through a real class expression; dependencies are supplied.
 return `${name} = ${field.initializer.getText(ast)};`;
});
const code=ts.transpileModule(`class Source extends Base {${bodies.join('\n')}}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
class Base {dispose(){this._superDispose();}}
const Source=new Function('Base',code+';return Source;')(Base);
const units=[0x41,0xd800,0x42,0xdc00];
function run(input){
 const trace=[],cleared=[],sockets=[];const failure={units};const stamp={units,tag:'clock'};
 let degraded=true,warned=true,last=undefined,superDisposed=false,health=null,launched=0,stopped=0;
 const present=[input.write,input.reconcile],reads=[0,0];
 const handles=[0,1].map(slot=>({guard:{slot,kind:'guard',units},argument:{slot,kind:'argument',units}}));
 const slotName=slot=>slot===0?'write':'reconcile';
 const record=name=>{trace.push(name);if(trace.length-1===input.fail)throw failure;};
 const target=new Source();target._superDispose=()=>{record('super');superDisposed=true;};
 Object.defineProperties(target,{
  isPrivateStreamDegraded:{get:()=>degraded,set:v=>{record('degraded:'+v);degraded=v;}},
  hasWarnedPrivateStreamDegraded:{get:()=>warned,set:v=>{record('warned:'+v);warned=v;}},
  lastPrivateStreamMessageAt:{get:()=>last,set:v=>{record('messageAt');assert.equal(v,stamp);last=v;}},
  privateStreamDegradedReconcileRunning:{get:()=>{record('running');return input.running;}},
 });
 for(const [slot,key] of [[0,'privateStreamWriteCheckTimeout'],[1,'privateStreamDegradedReconcileTimeout']]){
  Object.defineProperty(target,key,{get(){reads[slot]++;record(`get:${slotName(slot)}:${reads[slot]}`);return present[slot]?(reads[slot]===1?handles[slot].guard:handles[slot].argument):0;},set(value){record('unset:'+slotName(slot));assert.equal(value,undefined);present[slot]=false;}});
 }
 target.store={updateSocketHealth(patch){health={own:Object.hasOwn(patch,'privateDegradedReason'),undefined:patch.privateDegradedReason===undefined};record('health');}};
 // Called SDK dependencies remain separate rows; their call sites and arguments
 // are exercised here, without claiming their implementations or the full class.
 if(input.op==='message')target.stopPrivateStreamDegradedReconcile=()=>{record('stop');stopped++;};
 target.reconcilePrivateStateFromRest=()=>{record('launch');launched++;return {then(){throw Error('STREAM must not await launch result');}};};
 for(const [key,name] of [['publicWebsocket','public'],['privateWebsocket','private'],['secondsTickersWebsocket','seconds'],['thirdTickersWebsocket','third']])target[key]={dispose(){record('socket:'+name);sockets.push(name);}};
 const oldNow=Date.now,oldClear=globalThis.clearTimeout;
 Date.now=()=>{record('now');return stamp;};
 globalThis.clearTimeout=handle=>{cleared.push({slot:slotName(handle.slot),argument:handle===handles[handle.slot].argument});record('clear:'+slotName(handle.slot));};
 let originalError=false,resultUndefined=false;
 try {resultUndefined=target[names[{dispose:0,message:1,start:2,stop:3}[input.op]]]()===undefined;}
 catch(error){originalError=error===failure;assert(originalError,'STREAM original source error');}
 finally{Date.now=oldNow;globalThis.clearTimeout=oldClear;}
 return {input,output:{trace,cleared,sockets,degraded,warned,lastSame:last===stamp,superDisposed,health,launched,stopped,present,originalError,resultUndefined}};
}
// Independent fixed scenario/event floor, never derived from captured output.
const scenarios=[['message',false,false,false,6]];
for(const write of [false,true])for(const reconcile of [false,true])scenarios.push(['dispose',write,reconcile,false,7+3*Number(write)+3*Number(reconcile)]);
for(const running of [false,true])for(const reconcile of [false,true])scenarios.push(['start',false,reconcile,running,running?1:reconcile?2:3]);
for(const reconcile of [false,true])scenarios.push(['stop',false,reconcile,false,reconcile?4:1]);
const cases=[];
for(const [op,write,reconcile,running,floor] of scenarios)for(let fail=-1;fail<floor;fail++)cases.push(run({op,write,reconcile,running,fail}));
assert.equal(cases.length,69,'STREAM independent source floor');
console.log(JSON.stringify({cases}));
