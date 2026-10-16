'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH;assert(compiler&&path.isAbsolute(compiler),'absolute TypeScript compiler required');
const ts=require(compiler),root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt');
function ast(file){return ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);}
const tree=ast('grvt.ws-public.ts'),cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='GrvtPublicWebSocket');
const names=['connectAndSubscribe','onOpen','onMarketsChanged','resubscribe','subscribeToAllTickers','dispose','cancelTickerBatches','ensureTickerSubscription','subscribeToTicker'];
const members=names.map(name=>{const node=cls.members.find(n=>n.name?.getText(tree)===name);assert(node,'actual '+name);return node.getText(tree);});
const types=ast('grvt.types.ts');const urls=types.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(types)==='WSS_URLS'));assert(urls);
const compiled=ts.transpileModule([urls.getText(types),'class Base {dispose(){this.baseDispose();}}','export class Venue extends Base {',...members,'}'].join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0);
let schedule=()=>{},clear=()=>{},observe=()=>{};
const context={exports:{},setTimeout(...args){return schedule(...args);},clearTimeout(timer){return clear(timer);},JSON:{stringify(value){observe(value);return JSON.stringify(value);}}};vm.runInNewContext(compiled.outputText,context);const Venue=context.exports.Venue;
function tag(value){if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {bits:b.toString('hex'),nan:Number.isNaN(value)};}if(value===undefined)return {undefined:true};if(Array.isArray(value))return value.map(tag);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,tag(v)]));return value;}
function market(index){return {id:'m'+index,symbol:'SYM'+index,base:'BASE',quote:'QUOTE',active:true,precision:{amount:1,price:1},limits:{amount:{min:1,max:9},leverage:{min:1,max:9}}};}
function seed(id){return {jsonrpc:'2.0',id,method:'subscribe',params:{stream:'v1.ticker.d',selectors:['SEED'+id+'@500']}};}
function state(v){return tag({requestId:v.requestId,subscriptions:[...v.subscriptions],tickers:[...v.tickerSubscriptions],timers:[...v.tickerBatchTimers],generation:v.tickerBatchGeneration,disposed:v.isDisposed});}
function make(){const v=new Venue();v.requestId=1;v.subscriptions=new Map();v.tickerSubscriptions=new Map();v.tickerBatchTimers=new Set();v.tickerBatchGeneration=0;v.isDisposed=false;v.isConnected=true;v.ws={};v.parent={options:{testnet:false},store:{markets:[]}};return v;}
function caught(action){try{action();return null;}catch(e){return {name:e.name,message:e.message};}}
const connectRows=[];
for(const name of ['production','testnet','disposed','create-error','open-error','message-error','swap-socket','unset-socket','replace-handler']){
 const v=make(),events=[];v.ws={name:'old'};v.isDisposed=name==='disposed';v.parent.options.testnet=name==='testnet';
 function socket(which){return {name:which,addEventListener(type,fn){events.push(['listen',which,type,fn===v[type==='open'?'onOpen':type==='message'?'onMessage':'onClose']]);if(name===type+'-error')throw Error(type+'-fail');if(type==='open'&&name==='swap-socket')v.ws=socket('replacement');if(type==='open'&&name==='unset-socket')v.ws=undefined;if(type==='open'&&name==='replace-handler')v.onMessage=()=>{};}};}
 v.onMessage=()=>{};v.onClose=()=>{};v.createWebsocket=url=>{events.push(['create',url]);if(name==='create-error')throw Error('create-fail');return socket('created');};
 const error=caught(()=>v.connectAndSubscribe());connectRows.push({name,events,error,socket:v.ws?.name??null});
}
const openRows=[];
for(const name of ['ok','disposed','track-error','replay-error','all-error','off-error','on-error','dispose-during-track','replace-handler']){
 const v=make(),events=[];v.isDisposed=name==='disposed';const act=step=>()=>{events.push([step]);if(name===step+'-error')throw Error(step+'-fail');if(step==='track'&&name==='dispose-during-track')v.isDisposed=true;};
 v.trackLastReceived=act('track');v.resubscribe=act('replay');v.subscribeToAllTickers=act('all');v.parent.emitter={off(event,fn){events.push(['off',event,fn===v.onMarketsChanged]);if(name==='off-error')throw Error('off-fail');if(name==='replace-handler')v.onMarketsChanged=()=>{};},on(event,fn){events.push(['on',event,fn===v.onMarketsChanged]);if(name==='on-error')throw Error('on-fail');}};
 const error=caught(()=>v.onOpen());openRows.push({name,events,error,disposed:v.isDisposed});
}
const marketRows=[];
for(const name of ['ok','empty','no-socket','disposed','disconnected','send-error','emit-error','append','remove-first','live-symbol','dispose-after-first']){
 const v=make(),events=[],requests=[],wires=[];const added=name==='empty'?[]:[market(0),market(1)];v.ws=name==='no-socket'?undefined:{};v.isDisposed=name==='disposed';v.isConnected=name!=='disconnected';observe=request=>requests.push(tag(request));
 v.send=wire=>{wires.push(wire);events.push(['send']);if(name==='append'&&wires.length===1)added.push(market(2));if(name==='remove-first'&&wires.length===1)added.shift();if(name==='dispose-after-first')v.isDisposed=true;if(name==='live-symbol'&&wires.length===1)added[0].symbol='CHANGED';if((name==='send-error'||name==='emit-error'||name==='live-symbol')&&wires.length===1)throw Error('send-fail');};v.parent.emitAndLogError=message=>{events.push(['emit',message]);if(name==='emit-error')throw Error('emit-fail');};
 const error=caught(()=>v.onMarketsChanged({added}));marketRows.push({name,events,requests,wires,error,state:state(v)});
}
const replayRows=[];
for(const name of ['ok','empty','send-error','append','delete-next','clear-add','delete-reinsert','replace-next','nested-clear-add']){
 const v=make(),events=[],requests=[],wires=[];v.subscriptions=name==='empty'?new Map():new Map([[1,seed(1)],[2,seed(2)]]);observe=request=>requests.push(tag(request));let mutated=false,nested=false;
 v.send=wire=>{wires.push(wire);events.push(['send']);if(name==='send-error')throw Error('send-fail');if(!mutated){mutated=true;if(name==='append')v.subscriptions.set(3,seed(3));if(name==='delete-next')v.subscriptions.delete(2);if(name==='clear-add'){v.subscriptions.clear();v.subscriptions.set(3,seed(3));}if(name==='delete-reinsert'){v.subscriptions.delete(1);v.subscriptions.set(1,seed(1));}if(name==='replace-next')v.subscriptions.set(2,{...seed(2),params:{stream:'v1.ticker.s',selectors:['REPLACED@500']}});if(name==='nested-clear-add'&&!nested){nested=true;v.resubscribe();v.subscriptions.clear();v.subscriptions.set(3,seed(3));}}};
 const error=caught(()=>v.resubscribe());replayRows.push({name,events,requests,wires,error,state:state(v)});
}
const batchRows=[];
for(const name of ['forty-one','empty','disposed','disconnected','clear-error','schedule-error','send-error','emit-error','callback-disposed','callback-disconnected','stale-generation','generation-nan','inline-callback','live-market','replace-market','schedule-append']){
 const v=make(),events=[],requests=[],wires=[],callbacks=[],queued=[];v.parent.store.markets=Array.from({length:name==='empty'?0:name==='forty-one'?41:name==='schedule-append'?20:2},(_,i)=>market(i));v.isDisposed=name==='disposed';v.isConnected=name!=='disconnected';v.tickerBatchTimers=new Set([90]);if(name==='generation-nan')v.tickerBatchGeneration=NaN;observe=request=>requests.push(tag(request));
 clear=timer=>{events.push(['clear',timer]);if(name==='clear-error')throw Error('clear-fail');};schedule=(callback,delay)=>{const id=100+callbacks.length;events.push(['schedule',delay,id]);if(name==='schedule-error')throw Error('schedule-fail');if(name==='inline-callback')callback();callbacks.push({id,callback});queued.push({id,delay});if(name==='schedule-append'&&id===100)v.parent.store.markets.push(market(20));return id;};
 v.send=wire=>{wires.push(wire);events.push(['send']);if((name==='send-error'||name==='emit-error')&&wires.length===1)throw Error('send-fail');};v.parent.emitAndLogError=message=>{events.push(['emit',message]);if(name==='emit-error')throw Error('emit-fail');};
 const error=caught(()=>v.subscribeToAllTickers()),eager=events.slice(),before=state(v);if(name==='callback-disposed')v.isDisposed=true;if(name==='callback-disconnected')v.isConnected=false;if(name==='stale-generation')v.tickerBatchGeneration+=1;if(name==='live-market')v.parent.store.markets[0].id='CHANGED';if(name==='replace-market')v.parent.store.markets[0]=market(99);
 const callbackErrors=[];for(const {callback}of callbacks)callbackErrors.push(caught(callback));batchRows.push({name,events,eager,before,queued,callbackErrors,requests,wires,error,state:state(v)});
}
const disposeRows=[];
for(const name of ['ok','clear-error','off-error','base-error','clear-replaces-handler']){
 const v=make(),events=[];v.subscriptions.set(1,seed(1));v.tickerSubscriptions.set('m0',1);v.tickerBatchTimers.add(90);clear=timer=>{events.push(['clear',timer]);if(name==='clear-error')throw Error('clear-fail');if(name==='clear-replaces-handler')v.onMarketsChanged=()=>{};};v.parent.emitter={off(event,fn){events.push(['off',event,fn===v.onMarketsChanged]);if(name==='off-error')throw Error('off-fail');}};v.baseDispose=()=>{events.push(['base']);if(name==='base-error')throw Error('base-fail');v.isDisposed=true;};const error=caught(()=>v.dispose());disposeRows.push({name,events,error,state:state(v)});
}
const minimalVenue=make(),minimalRequests=[],minimalWires=[];observe=request=>minimalRequests.push(tag(request));minimalVenue.send=wire=>minimalWires.push(wire);minimalVenue.parent.emitAndLogError=message=>{throw Error(message);};minimalVenue.onMarketsChanged({added:[{id:'m0',symbol:'SYM0'},{id:'m1',symbol:'SYM1'}]});const minimalMarketRow={requests:minimalRequests,wires:minimalWires,state:state(minimalVenue)};
assert.equal(connectRows.length,9);assert.equal(openRows.length,9);assert.equal(marketRows.length,11);assert.equal(replayRows.length,9);assert.equal(batchRows.length,16);assert.equal(disposeRows.length,5);
process.stdout.write(JSON.stringify({connectRows,openRows,marketRows,replayRows,batchRows,disposeRows,minimalMarketRow}));
